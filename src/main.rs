use anyhow::Result;
use std::io::stdout;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use audio::device::{get_device_sample_rate, get_output_device};
use audio::engine::AudioEngine;

use std::thread;
use std::time::Duration;

use crate::audio::control::AudioControl;
use crate::audio::decoder::{probe_only, stream_decode_with_seek};
use crate::playlist::{load_from_dir, Playlist};
use crate::state::{load_state, save_state, AppState};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use ringbuf::producer::Producer;

mod audio;
mod playlist;
mod ui;
mod state;

// =========================
// 🔧 SAFE LOCK HELPERS
// =========================
fn lock_playlist(playlist: &'_ Arc<Mutex<Playlist>>) -> Option<MutexGuard<'_, Playlist>> {
    match playlist.lock() {
        Ok(guard) => Some(guard),
        Err(poisoned) => {
            Some(poisoned.into_inner())
        }
    }
}

fn lock_selected(selected: &'_ Arc<Mutex<usize>>) -> Option<MutexGuard<'_, usize>> {
    match selected.lock() {
        Ok(guard) => Some(guard),
        Err(poisoned) => {
            Some(poisoned.into_inner())
        }
    }
}

// =========================
// 🔧 METADATA
// =========================
fn apply_track_metadata(control: &AudioControl, path: &str, device_rate: u32) {
    if let Ok(info) = probe_only(path) {
        control.set_sample_rate(info.sample_rate);

        let adjusted_total = if info.sample_rate > 0 {
            (info.total_samples as u128 * device_rate as u128
                / info.sample_rate as u128) as u64
        } else {
            0
        };

        control.set_total_samples(adjusted_total);
    }
}

// =========================
// 🔧 SAVE STATE HELPER
// =========================
fn save_current_state(playlist: &Arc<Mutex<Playlist>>, control: &AudioControl) {
    if let Some(pl) = lock_playlist(playlist) {
        let current_elapsed = control.elapsed();
        let total_samples = control.total();

        let valid_elapsed = if total_samples > 0 && current_elapsed >= total_samples {
            total_samples.saturating_sub(1)
        } else {
            current_elapsed
        };

        let state = AppState {
            track_path: pl.current(),
            elapsed: valid_elapsed,
            volume: control.volume(),
        };

        let _ = save_state(&state);
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
        return Ok(());
    }

    let playlist = Arc::new(Mutex::new(Playlist::new(tracks)));
    let selected = Arc::new(Mutex::new(0usize));

    // =========================
    // 🔥 LOAD STATE
    // =========================
    if let Some(state) = load_state() {
        // Set volume first
        control.set_volume(state.volume);

        if let Some(mut pl) = lock_playlist(&playlist) {
            if let Some(path) = state.track_path {
                if pl.set_by_path(&path) {
                    // Track found - load metadata first
                    if let Some(current_path) = pl.current() {
                        apply_track_metadata(&control, &current_path, device_rate);

                        // Validate seek position
                        let total_samples = control.total();
                        let seek_pos = if total_samples > 0 && state.elapsed < total_samples {
                            state.elapsed
                        } else if total_samples > 0 {
                            0
                        } else {
                            0
                        };

                        // Request seek and update UI
                        control.request_seek(seek_pos);
                        control.set_elapsed(seek_pos);
                    }
                } else {
                    // Track not found - start from first track
                    pl.current = 0;
                    if let Some(current_path) = pl.current() {
                        apply_track_metadata(&control, &current_path, device_rate);
                    }
                    control.request_seek(0);
                }
            } else {
                // No track path in state - just use current track
                if let Some(current_path) = pl.current() {
                    apply_track_metadata(&control, &current_path, device_rate);
                }
            }
        }
    } else {
        // No saved state - just load metadata for current track
        if let Some(pl) = lock_playlist(&playlist) {
            if let Some(path) = pl.current() {
                apply_track_metadata(&control, &path, device_rate);
            }
        }
    }

    // Start playback
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
                    match lock_playlist(&playlist) {
                        Some(pl) => match pl.current() {
                            Some(p) => p,
                            None => break,
                        },
                        None => break,
                    }
                };

                // Update metadata for new track
                apply_track_metadata(&control, &path, device_rate);

                let my_id = track_id.fetch_add(1, Ordering::Relaxed) + 1;

                // Get seek position (or start from 0)
                let seek = control.take_seek().unwrap_or(0);

                // Update elapsed to match seek position
                control.set_elapsed(seek);

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
                    if let Some(mut pl) = lock_playlist(&playlist) {
                        pl.next();
                    }
                    finished_flag.store(false, Ordering::Relaxed);
                }
            }
        });
    }

    // =========================
    // 🔥 AUTO SAVE (IMPROVED - SAVE EVERY 2 SECONDS)
    // =========================
    {
        let playlist = playlist.clone();
        let control = control.clone();

        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(2));
                save_current_state(&playlist, &control);
            }
        });
    }

    // =========================
    // 🔥 INPUT THREAD
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
                if let Ok(true) = event::poll(Duration::from_millis(50)) {
                    if let Ok(Event::Key(key)) = event::read() {
                        if key.kind != KeyEventKind::Press {
                            continue;
                        }

                        match key.code {
                            KeyCode::Char(' ') => {
                                if !control.is_started() {
                                    if let Some(pl) = lock_playlist(&playlist) {
                                        if let Some(path) = pl.current() {
                                            control.reset_for_new_track();
                                            apply_track_metadata(&control, &path, device_rate);
                                        }
                                    }
                                    control.start();
                                } else {
                                    control.toggle_pause();
                                }
                                // Save state after pause/unpause
                                save_current_state(&playlist, &control);
                            }

                            KeyCode::Right => {
                                if let Some(mut pl) = lock_playlist(&playlist) {
                                    pl.next();
                                }
                                // Update metadata for new track
                                if let Some(pl) = lock_playlist(&playlist) {
                                    if let Some(path) = pl.current() {
                                        apply_track_metadata(&control, &path, device_rate);
                                    }
                                }
                                control.reset_for_new_track();
                                track_id.fetch_add(1, Ordering::Relaxed);
                                // Save state after track change
                                save_current_state(&playlist, &control);
                            }

                            KeyCode::Left => {
                                if let Some(mut pl) = lock_playlist(&playlist) {
                                    pl.prev();
                                }
                                // Update metadata for new track
                                if let Some(pl) = lock_playlist(&playlist) {
                                    if let Some(path) = pl.current() {
                                        apply_track_metadata(&control, &path, device_rate);
                                    }
                                }
                                control.reset_for_new_track();
                                track_id.fetch_add(1, Ordering::Relaxed);
                                // Save state after track change
                                save_current_state(&playlist, &control);
                            }

                            KeyCode::Up => {
                                if let Some(mut sel) = lock_selected(&selected) {
                                    if *sel > 0 {
                                        *sel -= 1;
                                    }
                                }
                                follow.store(false, Ordering::Relaxed);
                            }

                            KeyCode::Down => {
                                if let Some(mut sel) = lock_selected(&selected) {
                                    if let Some(pl) = lock_playlist(&playlist) {
                                        if *sel + 1 < pl.tracks.len() {
                                            *sel += 1;
                                        }
                                    }
                                }
                                follow.store(false, Ordering::Relaxed);
                            }

                            KeyCode::Enter => {
                                if let Some(mut pl) = lock_playlist(&playlist) {
                                    if let Some(sel) = lock_selected(&selected) {
                                        if *sel < pl.tracks.len() {
                                            pl.current = *sel;
                                        }
                                    }
                                }
                                // Update metadata for selected track
                                if let Some(pl) = lock_playlist(&playlist) {
                                    if let Some(path) = pl.current() {
                                        apply_track_metadata(&control, &path, device_rate);
                                    }
                                }
                                follow.store(true, Ordering::Relaxed);
                                control.reset_for_new_track();
                                track_id.fetch_add(1, Ordering::Relaxed);
                                // Save state after track selection
                                save_current_state(&playlist, &control);
                            }

                            KeyCode::Char('+') | KeyCode::Char('=') => {
                                control.adjust_volume(0.1);
                                // Save state after volume change
                                save_current_state(&playlist, &control);
                            }

                            KeyCode::Char('-') => {
                                control.adjust_volume(-0.1);
                                // Save state after volume change
                                save_current_state(&playlist, &control);
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
                                // Save state after seek
                                save_current_state(&playlist, &control);
                            }

                            KeyCode::Char('h') => {
                                let sr = control.sample_rate() as u64;
                                let cur = control.elapsed();

                                let new_pos = cur.saturating_sub(sr * 5);

                                control.request_seek(new_pos);
                                track_id.fetch_add(1, Ordering::Relaxed);
                                // Save state after seek
                                save_current_state(&playlist, &control);
                            }

                            KeyCode::Char('q') => {
                                // Save final state before exit
                                save_current_state(&playlist, &control);
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
            if let Some(pl) = lock_playlist(&playlist) {
                let current = pl.current;

                if let Some(mut sel) = lock_selected(&selected) {
                    *sel = current;
                }
            }
        }

        terminal.draw(|f| {
            ui::draw(f, &playlist, &control, &selected);
        })?;
    }

    // Final save before exit (redundant but safe)
    save_current_state(&playlist, &control);

    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;

    Ok(())
}