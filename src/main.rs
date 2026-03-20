use std::io::stdout;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use anyhow::Result;

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
use ringbuf::producer::Producer;
use crate::audio::control::AudioControl;
use crate::audio::decoder::probe_only;
use crate::playlist::{load_from_dir, Playlist};

mod audio;
mod playlist;
mod ui;

fn apply_track_metadata(control: &AudioControl, path: &str, device_rate: u32) {
    if let Ok(info) = probe_only(path) {
        control.set_sample_rate(info.sample_rate);

        let adjusted_total = if info.sample_rate > 0 {
            (info.total_samples as u128 * device_rate as u128 / info.sample_rate as u128) as u64
        } else {
            0
        };

        control.set_total_samples(adjusted_total);
    }
}

fn main() -> Result<()> {
    let audio_device = get_output_device()?;
    let device_rate = get_device_sample_rate(&audio_device.device);

    let control = AudioControl::new();
    control.set_elapsed(0);
    control.set_total_samples(0);
    control.set_sample_rate(device_rate);

    let engine = AudioEngine::new(audio_device, device_rate, control.clone())?;
    let mut producer = engine.producer;

    let running = Arc::new(AtomicBool::new(true));
    let track_id = Arc::new(AtomicU64::new(0));
    let finished_flag = Arc::new(AtomicBool::new(false));
    let follow = Arc::new(AtomicBool::new(true));
    let decoder_ready = Arc::new(AtomicBool::new(false));

    let prefetch_buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
    let prefetch_track_index = Arc::new(AtomicU64::new(u64::MAX));

    let tracks = load_from_dir("songs");
    if tracks.is_empty() {
        eprintln!("No audio files found in ./songs");
        return Ok(());
    }

    let playlist = Arc::new(Mutex::new(Playlist::new(tracks)));
    let selected = Arc::new(Mutex::new(0usize));

    // =========================
    // 🔥 DECODER THREAD
    // =========================
    {
        let playlist = playlist.clone();
        let track_id = track_id.clone();
        let finished_flag = finished_flag.clone();
        let control = control.clone();
        let decoder_ready = decoder_ready.clone();
        let prefetch_buffer = prefetch_buffer.clone();
        let prefetch_track_index = prefetch_track_index.clone();

        thread::spawn(move || {
            loop {
                while !control.is_started() {
                    thread::sleep(Duration::from_millis(50));
                }

                let (path, current_index) = {
                    let pl = playlist.lock().unwrap();
                    match pl.current() {
                        Some(p) => (p, pl.current as u64),
                        None => break,
                    }
                };

                let my_id = track_id.fetch_add(1, Ordering::Relaxed) + 1;

                decoder_ready.store(false, Ordering::Relaxed);
                finished_flag.store(false, Ordering::Relaxed);

                // 🔥 PREFETCH VALIDATION
                let mut skip_samples = 0;
                {
                    let mut buffer = prefetch_buffer.lock().unwrap();

                    if !buffer.is_empty()
                        && prefetch_track_index.load(Ordering::Relaxed) == current_index
                    {
                        skip_samples = buffer.len() as u64;

                        for sample in buffer.drain(..) {
                            loop {
                                if producer.try_push(sample).is_ok() {
                                    break;
                                }
                                thread::yield_now();
                            }
                        }
                    }
                }

                let mut first_sample = true;
                let mut skipped = 0;

                let _ = stream_decode(&path, device_rate, |sample| {
                    if track_id.load(Ordering::Relaxed) != my_id {
                        return;
                    }

                    if skipped < skip_samples {
                        skipped += 1;
                        return;
                    }

                    if first_sample {
                        decoder_ready.store(true, Ordering::Relaxed);
                        first_sample = false;
                    }

                    loop {
                        if producer.try_push(sample).is_ok() {
                            break;
                        }
                        thread::yield_now();
                    }
                });

                if track_id.load(Ordering::Relaxed) == my_id {
                    finished_flag.store(true, Ordering::Relaxed);
                }

                if finished_flag.load(Ordering::Relaxed) {
                    let mut pl = playlist.lock().unwrap();
                    pl.next();
                }
            }
        });
    }

    // =========================
    // 🔥 PREFETCH THREAD
    // =========================
    {
        let playlist = playlist.clone();
        let prefetch_buffer = prefetch_buffer.clone();
        let prefetch_track_index = prefetch_track_index.clone();

        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_millis(200));

                let (next_path, next_index) = {
                    let pl = playlist.lock().unwrap();
                    let idx = (pl.current + 1) % pl.tracks.len();
                    (pl.tracks[idx].clone(), idx as u64)
                };

                let mut buffer = Vec::with_capacity(48000 * 2);

                let _ = stream_decode(&next_path, 48000, |sample| {
                    if buffer.len() < 48000 * 2 {
                        buffer.push(sample);
                    }
                });

                let mut shared = prefetch_buffer.lock().unwrap();
                *shared = buffer;

                prefetch_track_index.store(next_index, Ordering::Relaxed);
            }
        });
    }

    // =========================
    // 🔥 INPUT THREAD
    // =========================
    {
        let running = running.clone();
        let track_id = track_id.clone();
        let finished_flag = finished_flag.clone();
        let control = control.clone();
        let playlist = playlist.clone();
        let selected = selected.clone();
        let follow = follow.clone();
        let decoder_ready = decoder_ready.clone();

        thread::spawn(move || {
            loop {
                if event::poll(Duration::from_millis(50)).unwrap() {
                    if let Event::Key(key) = event::read().unwrap() {
                        if key.kind != KeyEventKind::Press {
                            continue;
                        }

                        match key.code {
                            KeyCode::Char(' ') => {
                                if !control.is_started() {
                                    if let Some(path) = {
                                        let pl = playlist.lock().unwrap();
                                        pl.current()
                                    } {
                                        control.reset_for_new_track();
                                        apply_track_metadata(&control, &path, device_rate);
                                    }

                                    control.start();

                                    let mut wait = 0;
                                    while !decoder_ready.load(Ordering::Relaxed) && wait < 20 {
                                        thread::sleep(Duration::from_millis(10));
                                        wait += 1;
                                    }
                                } else {
                                    control.toggle_pause();
                                }
                            }

                            KeyCode::Right => {
                                let mut pl = playlist.lock().unwrap();
                                pl.next();

                                control.reset_for_new_track();

                                if let Some(path) = pl.current() {
                                    apply_track_metadata(&control, &path, device_rate);
                                }

                                finished_flag.store(false, Ordering::Relaxed);
                                track_id.fetch_add(1, Ordering::Relaxed);
                            }

                            KeyCode::Left => {
                                let mut pl = playlist.lock().unwrap();
                                pl.prev();

                                control.reset_for_new_track();

                                if let Some(path) = pl.current() {
                                    apply_track_metadata(&control, &path, device_rate);
                                }

                                finished_flag.store(false, Ordering::Relaxed);
                                track_id.fetch_add(1, Ordering::Relaxed);
                            }

                            KeyCode::Up => {
                                let mut sel = selected.lock().unwrap();
                                if *sel > 0 {
                                    *sel -= 1;
                                }
                                follow.store(false, Ordering::Relaxed);
                            }

                            KeyCode::Down => {
                                let mut sel = selected.lock().unwrap();
                                let pl = playlist.lock().unwrap();

                                if *sel + 1 < pl.tracks.len() {
                                    *sel += 1;
                                }

                                follow.store(false, Ordering::Relaxed);
                            }

                            KeyCode::Enter => {
                                let mut pl = playlist.lock().unwrap();
                                let sel = *selected.lock().unwrap();

                                if sel < pl.tracks.len() {
                                    pl.current = sel;
                                }

                                follow.store(true, Ordering::Relaxed);

                                control.reset_for_new_track();

                                if let Some(path) = pl.current() {
                                    apply_track_metadata(&control, &path, device_rate);
                                }

                                finished_flag.store(false, Ordering::Relaxed);
                                track_id.fetch_add(1, Ordering::Relaxed);

                                if !control.is_started() {
                                    control.start();
                                }
                            }

                            KeyCode::Char('+') | KeyCode::Char('=') => {
                                control.adjust_volume(0.1);
                            }

                            KeyCode::Char('-') => {
                                control.adjust_volume(-0.1);
                            }

                            KeyCode::Char('f') => {
                                let cur = follow.load(Ordering::Relaxed);
                                follow.store(!cur, Ordering::Relaxed);
                            }

                            KeyCode::Char('q') => {
                                running.store(false, Ordering::Relaxed);
                                break;
                            }

                            _ => {}
                        }
                    }
                }
            }
        });
    }

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    while running.load(Ordering::Relaxed) {
        if follow.load(Ordering::Relaxed) {
            let current = {
                let pl = playlist.lock().unwrap();
                pl.current
            };

            let mut sel = selected.lock().unwrap();
            *sel = current;
        }

        terminal.draw(|f| {
            ui::draw(f, &playlist, &control, &selected);
        })?;
    }

    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;

    Ok(())
}