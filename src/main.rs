// ⚠️ SAFE PATCH — NO FEATURE LOSS

use std::io::stdout;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use anyhow::Result;

use audio::device::{get_output_device, get_device_sample_rate};
use audio::engine::AudioEngine;

use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use ringbuf::producer::Producer;
use crate::audio::control::AudioControl;
use crate::audio::decoder::{probe_only, stream_decode_with_seek};
use crate::playlist::{load_from_dir, Playlist};
use crate::state::{load_state, save_state, AppState};

mod audio;
mod playlist;
mod ui;
mod state;

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
    control.set_sample_rate(device_rate);

    let engine = AudioEngine::new(audio_device, device_rate, control.clone())?;
    let mut producer = engine.producer;

    let running = Arc::new(AtomicBool::new(true));
    let track_id = Arc::new(AtomicU64::new(0));
    let finished_flag = Arc::new(AtomicBool::new(false));
    let follow = Arc::new(AtomicBool::new(true));

    let tracks = load_from_dir("songs");
    if tracks.is_empty() {
        eprintln!("No audio files found");
        return Ok(());
    }

    let playlist = Arc::new(Mutex::new(Playlist::new(tracks)));
    let selected = Arc::new(Mutex::new(0usize));

    // =========================
    // 🔥 LOAD STATE (PATH BASED)
    // =========================
    if let Some(state) = load_state() {
        let mut pl = playlist.lock().unwrap();

        if let Some(path) = state.track_path {
            if pl.set_by_path(&path) {
                control.set_elapsed(state.elapsed);
                control.set_volume(state.volume);
            }
        }
    }

    // =========================
    // 🔥 AUTO START (UNCHANGED)
    // =========================
    {
        let pl = playlist.lock().unwrap();
        if let Some(path) = pl.current() {
            apply_track_metadata(&control, &path, device_rate);
        }
    }
    control.start();

    // =========================
    // 🔥 DECODER THREAD
    // =========================
    {
        let playlist = playlist.clone();
        let track_id = track_id.clone();
        let finished_flag = finished_flag.clone();
        let control = control.clone();

        thread::spawn(move || {
            loop {
                let path = {
                    let pl = playlist.lock().unwrap();
                    match pl.current() {
                        Some(p) => p,
                        None => break,
                    }
                };

                let my_id = track_id.fetch_add(1, Ordering::Relaxed) + 1;

                let seek = control.take_seek().unwrap_or(control.elapsed());

                let _ = stream_decode_with_seek(
                    &path,
                    device_rate,
                    seek,
                    |sample| {
                        if track_id.load(Ordering::Relaxed) != my_id {
                            return;
                        }

                        loop {
                            if producer.try_push(sample).is_ok() {
                                break;
                            }
                            thread::yield_now();
                        }
                    },
                );

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
    // 🔥 AUTO SAVE
    // =========================
    {
        let playlist = playlist.clone();
        let control = control.clone();

        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(1));

                let pl = playlist.lock().unwrap();

                let state = AppState {
                    track_path: pl.current(),
                    elapsed: control.elapsed(),
                    volume: control.volume(),
                };

                save_state(&state);
            }
        });
    }

    // =========================
    // 🔥 INPUT THREAD (UNCHANGED)
    // =========================
    {
        let running = running.clone();
        let track_id = track_id.clone();
        let control = control.clone();
        let playlist = playlist.clone();
        let selected = selected.clone();
        let follow = follow.clone();

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
                                } else {
                                    control.toggle_pause();
                                }
                            }

                            KeyCode::Right => {
                                let mut pl = playlist.lock().unwrap();
                                pl.next();

                                control.reset_for_new_track();
                                track_id.fetch_add(1, Ordering::Relaxed);
                            }

                            KeyCode::Left => {
                                let mut pl = playlist.lock().unwrap();
                                pl.prev();

                                control.reset_for_new_track();
                                track_id.fetch_add(1, Ordering::Relaxed);
                            }

                            KeyCode::Up => {
                                let mut sel = selected.lock().unwrap();
                                if *sel > 0 { *sel -= 1; }
                                follow.store(false, Ordering::Relaxed);
                            }

                            KeyCode::Down => {
                                let mut sel = selected.lock().unwrap();
                                let pl = playlist.lock().unwrap();
                                if *sel + 1 < pl.tracks.len() { *sel += 1; }
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
                                track_id.fetch_add(1, Ordering::Relaxed);
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

                            KeyCode::Char('l') => {
                                let sr = control.sample_rate() as u64;
                                let new_pos = control.elapsed() + sr * 5;

                                control.request_seek(new_pos);
                                track_id.fetch_add(1, Ordering::Relaxed);
                            }

                            KeyCode::Char('h') => {
                                let sr = control.sample_rate() as u64;
                                let cur = control.elapsed();

                                let new_pos = cur.saturating_sub(sr * 5);

                                control.request_seek(new_pos);
                                track_id.fetch_add(1, Ordering::Relaxed);
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