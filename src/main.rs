use anyhow::Result;
use std::io::stdout;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use audio::device::{get_device_sample_rate, get_output_device};
use audio::engine::AudioEngine;

use crate::audio::control::AudioControl;
use crate::audio::decoder::stream_decode_with_seek;
use crate::playlist::{load_from_dir, Playlist};
use crate::state::{load_state, save_state, AppState};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use ringbuf::HeapProd;
use ringbuf::producer::Producer;

mod audio;
mod playlist;
mod ui;
mod state;

// =========================
// 🔧 GLOBAL STATE
// =========================
struct AppContext {
    running: Arc<AtomicBool>,
    track_version: Arc<AtomicU64>,
    follow: Arc<AtomicBool>,
    playlist: Arc<Mutex<Playlist>>,
    selected: Arc<AtomicUsize>,
    control: AudioControl,
    producer: Arc<Mutex<HeapProd<f32>>>,  // Wrap in Mutex for thread safety
    device_rate: u32,
}

fn main() -> Result<()> {
    let audio_device = get_output_device()?;
    let device_rate = get_device_sample_rate(&audio_device.device);

    let control = AudioControl::new();
    control.set_sample_rate(device_rate);

    let engine = AudioEngine::new(audio_device, device_rate, control.clone())?;
    let producer = Arc::new(Mutex::new(engine.producer));

    let tracks = load_from_dir("songs");
    if tracks.is_empty() {
        return Ok(());
    }

    let context = Arc::new(AppContext {
        running: Arc::new(AtomicBool::new(true)),
        track_version: Arc::new(AtomicU64::new(0)),
        follow: Arc::new(AtomicBool::new(true)),
        playlist: Arc::new(Mutex::new(Playlist::new(tracks))),
        selected: Arc::new(AtomicUsize::new(0)),
        control,
        producer,
        device_rate,
    });

    // Load saved state
    load_saved_state(&context);

    // Start decoder thread
    start_decoder_thread(context.clone());

    // Start auto-save thread
    start_auto_save_thread(context.clone());

    // Start UI
    run_ui(context.clone())?;

    // Final save
    save_current_state(&context);

    Ok(())
}

fn load_saved_state(ctx: &Arc<AppContext>) {
    if let Some(state) = load_state() {
        ctx.control.set_volume(state.volume);

        if let Ok(mut pl) = ctx.playlist.lock() {
            if let Some(path) = state.track_path {
                if pl.set_by_path(&path) {
                    if let Some(current_path) = pl.current() {
                        update_track_metadata(ctx, &current_path);

                        let total = ctx.control.total();
                        let seek_pos = if total > 0 && state.elapsed < total {
                            state.elapsed
                        } else {
                            0
                        };

                        ctx.control.set_elapsed(seek_pos);
                        ctx.control.request_seek(seek_pos);
                    }
                } else {
                    pl.current = 0;
                    if let Some(path) = pl.current() {
                        update_track_metadata(ctx, &path);
                    }
                }
            }
        }
    } else {
        if let Ok(pl) = ctx.playlist.lock() {
            if let Some(path) = pl.current() {
                update_track_metadata(ctx, &path);
            }
        }
    }
}

fn update_track_metadata(ctx: &Arc<AppContext>, path: &str) {
    use crate::audio::decoder::probe_only;

    if let Ok(info) = probe_only(path) {
        ctx.control.set_sample_rate(info.sample_rate);

        let adjusted_total = if info.sample_rate > 0 {
            (info.total_samples as u128 * ctx.device_rate as u128 / info.sample_rate as u128) as u64
        } else {
            0
        };

        ctx.control.set_total_samples(adjusted_total);
    }
}

fn start_decoder_thread(ctx: Arc<AppContext>) {
    thread::spawn(move || {
        let mut current_track_version = 0u64;

        loop {
            // Check if we should stop
            if !ctx.running.load(Ordering::Relaxed) {
                break;
            }

            // Get current track path
            let path = {
                let pl = match ctx.playlist.lock() {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                match pl.current() {
                    Some(p) => p,
                    None => continue,
                }
            };

            // Check if track version changed
            let new_version = ctx.track_version.load(Ordering::Relaxed);
            if new_version != current_track_version {
                current_track_version = new_version;

                // Update metadata for UI
                update_track_metadata(&ctx, &path);

                // Reset elapsed
                let seek = ctx.control.take_seek().unwrap_or(0);
                ctx.control.set_elapsed(seek);

                // Decode and stream
                let _ = stream_decode_with_seek(
                    &path,
                    ctx.device_rate,
                    seek,
                    |sample| {
                        // Check if track changed during playback
                        if ctx.track_version.load(Ordering::Relaxed) != current_track_version {
                            return;
                        }

                        // Push to audio buffer
                        if let Ok(mut producer) = ctx.producer.lock() {
                            loop {
                                if producer.try_push(sample).is_ok() {
                                    break;
                                }
                                thread::yield_now();
                            }
                        }
                    },
                );

                // Move to next track only if this track completed naturally
                if ctx.track_version.load(Ordering::Relaxed) == current_track_version {
                    if let Ok(mut pl) = ctx.playlist.lock() {
                        pl.next();
                        ctx.track_version.fetch_add(1, Ordering::Relaxed);

                        // Update metadata for next track immediately
                        if let Some(next_path) = pl.current() {
                            update_track_metadata(&ctx, &next_path);
                        }
                    }
                }
            }

            thread::sleep(Duration::from_millis(10));
        }
    });
}

fn start_auto_save_thread(ctx: Arc<AppContext>) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(2));
            if !ctx.running.load(Ordering::Relaxed) {
                break;
            }
            save_current_state(&ctx);
        }
    });
}

fn save_current_state(ctx: &Arc<AppContext>) {
    if let Ok(pl) = ctx.playlist.lock() {
        let elapsed = ctx.control.elapsed();
        let total = ctx.control.total();

        let valid_elapsed = if total > 0 && elapsed >= total {
            total.saturating_sub(1)
        } else {
            elapsed
        };

        let state = AppState {
            track_path: pl.current(),
            elapsed: valid_elapsed,
            volume: ctx.control.volume(),
        };

        let _ = save_state(&state);
    }
}

fn run_ui(ctx: Arc<AppContext>) -> Result<()> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // Input handling thread
    let ctx_input = ctx.clone();
    let input_handle = thread::spawn(move || {
        handle_input(ctx_input);
    });

    // Main UI loop
    while ctx.running.load(Ordering::Relaxed) {
        // Update selected index if follow mode is on
        if ctx.follow.load(Ordering::Relaxed) {
            if let Ok(pl) = ctx.playlist.lock() {
                ctx.selected.store(pl.current, Ordering::Relaxed);
            }
        }

        terminal.draw(|f| {
            ui::draw(f, &ctx.playlist, &ctx.control, &ctx.selected);
        })?;

        thread::sleep(Duration::from_millis(16));
    }

    input_handle.join().unwrap();

    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;

    Ok(())
}

fn handle_input(ctx: Arc<AppContext>) {
    loop {
        if !ctx.running.load(Ordering::Relaxed) {
            break;
        }

        if let Ok(true) = event::poll(Duration::from_millis(50)) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                match key.code {
                    KeyCode::Char(' ') => toggle_playback(&ctx),
                    KeyCode::Right => next_track(&ctx),
                    KeyCode::Left => prev_track(&ctx),
                    KeyCode::Up => move_selection(&ctx, -1),
                    KeyCode::Down => move_selection(&ctx, 1),
                    KeyCode::Enter => select_current_track(&ctx),
                    KeyCode::Char('+') | KeyCode::Char('=') => ctx.control.adjust_volume(0.05),
                    KeyCode::Char('-') => ctx.control.adjust_volume(-0.05),
                    KeyCode::Char('f') => toggle_follow(&ctx),
                    KeyCode::Char('l') => seek_forward(&ctx),
                    KeyCode::Char('h') => seek_backward(&ctx),
                    KeyCode::Char('q') => {
                        ctx.running.store(false, Ordering::Relaxed);
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
}

fn toggle_playback(ctx: &Arc<AppContext>) {
    if !ctx.control.is_started() {
        // Start playback
        if let Ok(pl) = ctx.playlist.lock() {
            if let Some(path) = pl.current() {
                update_track_metadata(ctx, &path);
                ctx.control.reset_for_new_track();
                ctx.control.start();
                ctx.track_version.fetch_add(1, Ordering::Relaxed);
            }
        }
    } else {
        ctx.control.toggle_pause();
    }
    save_current_state(ctx);
}

fn next_track(ctx: &Arc<AppContext>) {
    if let Ok(mut pl) = ctx.playlist.lock() {
        pl.next();
        if let Some(path) = pl.current() {
            update_track_metadata(ctx, &path);
        }
    }
    ctx.control.reset_for_new_track();
    ctx.control.set_elapsed(0);
    ctx.track_version.fetch_add(1, Ordering::Relaxed);
    save_current_state(ctx);
}

fn prev_track(ctx: &Arc<AppContext>) {
    if let Ok(mut pl) = ctx.playlist.lock() {
        pl.prev();
        if let Some(path) = pl.current() {
            update_track_metadata(ctx, &path);
        }
    }
    ctx.control.reset_for_new_track();
    ctx.control.set_elapsed(0);
    ctx.track_version.fetch_add(1, Ordering::Relaxed);
    save_current_state(ctx);
}

fn move_selection(ctx: &Arc<AppContext>, delta: i32) {
    let current = ctx.selected.load(Ordering::Relaxed);
    let new_sel = (current as i32 + delta).max(0);

    if let Ok(pl) = ctx.playlist.lock() {
        if (new_sel as usize) < pl.tracks.len() {
            ctx.selected.store(new_sel as usize, Ordering::Relaxed);
        }
    }
    ctx.follow.store(false, Ordering::Relaxed);
}

fn select_current_track(ctx: &Arc<AppContext>) {
    let selected_idx = ctx.selected.load(Ordering::Relaxed);

    if let Ok(mut pl) = ctx.playlist.lock() {
        if selected_idx < pl.tracks.len() {
            pl.current = selected_idx;
            if let Some(path) = pl.current() {
                update_track_metadata(ctx, &path);
            }
        }
    }

    ctx.follow.store(true, Ordering::Relaxed);
    ctx.control.reset_for_new_track();
    ctx.control.set_elapsed(0);
    ctx.track_version.fetch_add(1, Ordering::Relaxed);
    save_current_state(ctx);
}

fn toggle_follow(ctx: &Arc<AppContext>) {
    let current = ctx.follow.load(Ordering::Relaxed);
    ctx.follow.store(!current, Ordering::Relaxed);
}

fn seek_forward(ctx: &Arc<AppContext>) {
    let sr = ctx.control.sample_rate() as u64;
    let new_pos = ctx.control.elapsed() + sr * 5;
    ctx.control.request_seek(new_pos);
    ctx.track_version.fetch_add(1, Ordering::Relaxed);
    save_current_state(ctx);
}

fn seek_backward(ctx: &Arc<AppContext>) {
    let sr = ctx.control.sample_rate() as u64;
    let cur = ctx.control.elapsed();
    let new_pos = cur.saturating_sub(sr * 5);
    ctx.control.request_seek(new_pos);
    ctx.track_version.fetch_add(1, Ordering::Relaxed);
    save_current_state(ctx);
}