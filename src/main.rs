use std::io::stdout;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use anyhow::Result;
use ringbuf::producer::Producer;

use audio::device::{get_output_device, get_device_sample_rate};
use audio::engine::AudioEngine;
use audio::decoder::stream_decode;

use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use crate::audio::control::AudioControl;
use crate::playlist::{load_from_dir, Playlist};

mod audio;
mod playlist;
mod ui;

fn main() -> Result<()> {
    // 1. device
    let audio_device = get_output_device()?;

    // 2. sample rate
    let device_rate = get_device_sample_rate(&audio_device.device);

    // 3. control
    let control = AudioControl::new();

    // 4. engine (baseline)
    let engine = AudioEngine::new(audio_device, device_rate, control.clone())?;

    // 5. producer
    let mut producer = engine.producer;

    // 6. state keeper
    let running = Arc::new(AtomicBool::new(true));
    let track_id = Arc::new(AtomicU64::new(0));
    let finished_flag = Arc::new(AtomicBool::new(false));
    let follow = Arc::new(AtomicBool::new(true));

    // 7. playlist
    let tracks = load_from_dir("songs");

    if tracks.is_empty() {
        eprintln!("No audio files found in ./songs");
        return Ok(());
    }

    let playlist = Arc::new(Mutex::new(Playlist::new(tracks)));
    let selected = Arc::new(Mutex::new(0usize));

    let playlist_clone = playlist.clone();
    let track_id_clone = track_id.clone();
    let finished_clone = finished_flag.clone();
    let control_clone = control.clone();

    // 8. decoder thread (ISOLATED)
    thread::spawn(move || {
        loop {
            while !control_clone.is_started() {
                thread::sleep(Duration::from_millis(50));
            }

            let path = {
                let pl = playlist_clone.lock().unwrap();
                match pl.current() {
                    Some(p) => p,
                    None => break,
                }
            };

            // 🔥 start track
            let my_id = track_id_clone.fetch_add(1, Ordering::Relaxed) + 1;

            // reset finished untuk track ini
            finished_clone.store(false, Ordering::Relaxed);

            let _ = stream_decode(&path, device_rate, |sample| {
                // 🔥 interrupt check
                if track_id_clone.load(Ordering::Relaxed) != my_id {
                    return; // stop decode
                }

                loop {
                    if producer.try_push(sample).is_ok() {
                        break;
                    }
                    thread::yield_now();
                }
            });

            // 🔥 hanya tandai finished kalau TIDAK di-interrupt
            if track_id_clone.load(Ordering::Relaxed) == my_id {
                finished_clone.store(true, Ordering::Relaxed);
            }

            // 🔥 auto-next hanya kalau benar-benar finished
            if finished_clone.load(Ordering::Relaxed) {
                let mut pl = playlist_clone.lock().unwrap();
                pl.next();
            }
        }
    });

    let running_clone = running.clone();
    let track_id_clone = track_id.clone();
    let finished_clone = finished_flag.clone();
    let control_input = control.clone();
    let playlist_input = playlist.clone();
    let selected_input = selected.clone();
    let follow_input = follow.clone();

    // 9. INPUT THREAD (ISOLATED)
    thread::spawn(move || {
        loop {
            if event::poll(Duration::from_millis(50)).unwrap() {
                if let Event::Key(key) = event::read().unwrap() {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    match key.code {
                        KeyCode::Char(' ') => {
                            if !control_input.is_started() {
                                control_input.start(); // 🔥 start pertama kali
                            } else {
                                control_input.toggle_pause();
                            }
                        },

                        KeyCode::Char('+') | KeyCode::Char('=') => {
                            control_input.adjust_volume(0.1)
                        }

                        KeyCode::Char('-') => {
                            control_input.adjust_volume(-0.1)
                        }

                        KeyCode::Right => {
                            {
                                let mut pl = playlist_input.lock().unwrap();
                                pl.next();
                            }
                            finished_clone.store(false, Ordering::Relaxed);
                            track_id_clone.fetch_add(1, Ordering::Relaxed);
                        }

                        KeyCode::Left => {
                            {
                                let mut pl = playlist_input.lock().unwrap();
                                pl.prev();
                            }
                            finished_clone.store(false, Ordering::Relaxed);
                            track_id_clone.fetch_add(1, Ordering::Relaxed);
                        }

                        KeyCode::Up => {
                            let mut sel = selected_input.lock().unwrap();
                            if *sel > 0 {
                                *sel -= 1;
                            }
                            follow_input.store(false, Ordering::Relaxed);
                        },

                        KeyCode::Down => {
                            let mut sel = selected_input.lock().unwrap();
                            let pl = playlist_input.lock().unwrap();
                            if *sel + 1 < pl.tracks.len() {
                                *sel += 1;
                            }
                            follow_input.store(false, Ordering::Relaxed);
                        },

                        KeyCode::Enter => {
                            let mut pl = playlist_input.lock().unwrap();
                            let sel = *selected_input.lock().unwrap();

                            if sel < pl.tracks.len() {
                                pl.current = sel;
                            }

                            follow_input.store(true, Ordering::Relaxed);

                            finished_clone.store(false, Ordering::Relaxed);
                            track_id_clone.fetch_add(1, Ordering::Relaxed);

                            if !control_input.is_started() {
                                control_input.start();
                            }
                        },

                        KeyCode::Char('f') => {
                            let cur = follow_input.load(Ordering::Relaxed);
                            follow_input.store(!cur, Ordering::Relaxed);
                        },

                        KeyCode::Char('q') => {
                            running_clone.store(false, Ordering::Relaxed);
                            break;
                        }

                        _ => {}
                    }
                }
            }
        }
    });

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // 10. main loop stay alive as long state keeper is running true
    while running.load(Ordering::Relaxed) {
        // 🔥 SYNC selected → current (kalau follow aktif)
        if follow.load(Ordering::Relaxed) {
            let current = {
                let pl = playlist.lock().unwrap();
                pl.current
            };

            let mut sel = selected.lock().unwrap();
            *sel = current;
        }

        // 🔥 baru render
        terminal.draw(|f| {
            ui::draw(f, &playlist, &control, &selected);
        })?;
    }

    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;

    Ok(())
}