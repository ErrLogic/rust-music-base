use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use crossterm::event;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use audio::device::{get_device_sample_rate, get_output_device};
use audio::engine::AudioEngine;

use crate::audio::control::AudioControl;
use crate::audio::decoder::{stream_decode};
use crate::playlist::{load_from_dir, Playlist};
use crate::state::{load_state, save_state, AppState};

use ringbuf::HeapProd;

mod audio;
mod playlist;
mod state;
mod display;

use display::state::build_display_state;
use display::framebuffer::Framebuffer;
use display::render::render;

use image::{ImageBuffer, Rgb};
use ringbuf::producer::Producer;

// =========================
// GLOBAL STATE
// =========================
struct AppContext {
    running: Arc<AtomicBool>,
    track_version: Arc<AtomicU64>,
    playlist: Arc<Mutex<Playlist>>,
    control: AudioControl,
    producer: Arc<Mutex<HeapProd<f32>>>,
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
        playlist: Arc::new(Mutex::new(Playlist::new(tracks))),
        control,
        producer,
        device_rate,
    });

    load_saved_state(&context);

    start_decoder_thread(context.clone());
    start_auto_save_thread(context.clone());

    // 🔥 DISPLAY THREAD (INI YANG BARU)
    start_display_thread(context.clone());
    start_input_thread(context.clone());

    // keep app alive
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

//
// =========================
// DISPLAY THREAD
// =========================
//
fn start_display_thread(ctx: Arc<AppContext>) {
    thread::spawn(move || {
        let mut fb = Framebuffer::new(240, 240);

        loop {
            let state = build_display_state(&ctx.playlist, &ctx.control);

            render(&mut fb, &state);

            save_framebuffer(&fb);

            println!(
                "elapsed: {:.2}, total: {:.2}, progress: {:.3}",
                state.elapsed_sec, state.total_sec, state.progress
            );

            thread::sleep(Duration::from_millis(200));
        }
    });
}

fn start_input_thread(ctx: Arc<AppContext>) {
    thread::spawn(move || {
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
                        KeyCode::Up => move_selection(&ctx, -1),
                        KeyCode::Down => move_selection(&ctx, 1),

                        KeyCode::Enter => select_track(&ctx),

                        KeyCode::Right => next_track(&ctx),
                        KeyCode::Left => prev_track(&ctx),

                        KeyCode::Char(' ') => toggle_playback(&ctx),

                        KeyCode::Char('+') | KeyCode::Char('=') => ctx.control.adjust_volume(0.05),
                        KeyCode::Char('-') => ctx.control.adjust_volume(-0.05),

                        KeyCode::Char('q') => {
                            ctx.running.store(false, Ordering::Relaxed);
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }
    });
}

fn move_selection(ctx: &Arc<AppContext>, delta: i32) {
    if let Ok(mut pl) = ctx.playlist.lock() {
        let new = (pl.current as i32 + delta)
            .max(0)
            .min(pl.tracks.len() as i32 - 1) as usize;

        pl.current = new;
    }
}

fn select_track(ctx: &Arc<AppContext>) {
    let path = {
        let pl = ctx.playlist.lock().unwrap();
        match pl.current() {
            Some(p) => p,
            None => return,
        }
    };

    // 🔥 RESET DULU
    ctx.control.reset_for_new_track();

    // 🔥 BARU SET METADATA
    update_track_metadata(ctx, &path);

    ctx.control.set_elapsed(0);

    ctx.track_version.fetch_add(1, Ordering::Relaxed);

    ctx.control.start();
}

fn toggle_playback(ctx: &Arc<AppContext>) {
    if !ctx.control.is_started() {
        select_track(ctx); // 🔥 penting: play dari state
    } else {
        ctx.control.toggle_pause();
    }
}

fn next_track(ctx: &Arc<AppContext>) {
    if let Ok(mut pl) = ctx.playlist.lock() {
        pl.next();
    }

    select_track(ctx);
}

fn prev_track(ctx: &Arc<AppContext>) {
    if let Ok(mut pl) = ctx.playlist.lock() {
        pl.prev();
    }

    select_track(ctx);
}

fn save_framebuffer(fb: &Framebuffer) {
    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(fb.width as u32, fb.height as u32);

    for y in 0..fb.height {
        for x in 0..fb.width {
            let idx = y as usize * fb.width as usize + x as usize;
            let color = fb.buffer[idx];

            let r = ((color >> 11) & 0x1F) << 3;
            let g = ((color >> 5) & 0x3F) << 2;
            let b = (color & 0x1F) << 3;

            img.put_pixel(x as u32, y as u32, Rgb([r as u8, g as u8, b as u8]));
        }
    }

    let _ = img.save("frame.png");
}

//
// =========================
// STATE LOAD / SAVE
// =========================
//
fn load_saved_state(ctx: &Arc<AppContext>) {
    if let Some(state) = load_state() {
        ctx.control.set_volume(state.volume);

        if let Ok(mut pl) = ctx.playlist.lock() {
            if let Some(path) = state.track_path {
                if pl.set_by_path(&path) {
                    // 🔥 penting: sync metadata
                    if let Some(current) = pl.current() {
                        update_track_metadata(ctx, &current);
                    }
                }
            }
        }
    }
}

fn update_track_metadata(ctx: &Arc<AppContext>, path: &str) {
    use crate::audio::decoder::probe_only;

    if let Ok(info) = probe_only(path) {
        // 🔥 FIX: gunakan device rate, bukan file rate
        ctx.control.set_sample_rate(ctx.device_rate);

        let adjusted_total = if info.sample_rate > 0 {
            (info.total_samples as u128 * ctx.device_rate as u128
                / info.sample_rate as u128) as u64
        } else {
            0
        };

        ctx.control.set_total_samples(adjusted_total);
    }
}

fn save_current_state(ctx: &Arc<AppContext>) {
    if let Ok(pl) = ctx.playlist.lock() {
        let state = AppState {
            track_path: pl.current(),
            elapsed: ctx.control.elapsed(),
            volume: ctx.control.volume(),
        };

        let _ = save_state(&state);
    }
}

//
// =========================
// DECODER THREAD (UNCHANGED CORE)
// =========================
//
fn start_decoder_thread(ctx: Arc<AppContext>) {
    thread::spawn(move || {
        let mut current_track_version = 0u64;

        loop {
            if !ctx.running.load(Ordering::Relaxed) {
                break;
            }

            let path = {
                let pl = ctx.playlist.lock().unwrap();
                match pl.current() {
                    Some(p) => p,
                    None => continue,
                }
            };

            let new_version = ctx.track_version.load(Ordering::Relaxed);

            if new_version != current_track_version {
                current_track_version = new_version;

                let version_snapshot = current_track_version;

                let _ = stream_decode(
                    &path,
                    ctx.device_rate,
                    |sample| {
                        // 🔥 STOP kalau track berubah
                        if ctx.track_version.load(Ordering::Relaxed) != version_snapshot {
                            return false;
                        }

                        if let Ok(mut producer) = ctx.producer.lock() {
                            let mut spin = 0;

                            loop {
                                if producer.try_push(sample).is_ok() {
                                    break;
                                }

                                spin += 1;

                                if spin < 10 {
                                    std::hint::spin_loop();
                                } else if spin < 20 {
                                    thread::yield_now();
                                } else {
                                    thread::sleep(Duration::from_micros(200));
                                    spin = 0;
                                }
                            }
                        }

                        true
                    },
                );

                // 🔥 kalau selesai & track gak berubah → next
                if ctx.track_version.load(Ordering::Relaxed) == version_snapshot {
                    if let Ok(mut pl) = ctx.playlist.lock() {
                        pl.next();
                    }

                    ctx.control.reset_for_new_track();

                    let path = {
                        let pl = ctx.playlist.lock().unwrap();
                        match pl.current() {
                            Some(p) => p,
                            None => continue,
                        }
                    };

                    update_track_metadata(&ctx, &path);

                    ctx.track_version.fetch_add(1, Ordering::Relaxed);
                }
            }

            thread::sleep(Duration::from_millis(2));
        }
    });
}

//
// =========================
// AUTO SAVE THREAD
// =========================
//
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