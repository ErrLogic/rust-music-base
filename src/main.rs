mod audio;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use anyhow::Result;
use ringbuf::producer::Producer;

use audio::device::{get_output_device, get_device_sample_rate};
use audio::engine::AudioEngine;
use audio::decoder::stream_decode;

use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crate::audio::control::AudioControl;

fn main() -> Result<()> {
    // 1. device
    let audio_device = get_output_device()?;

    // 2. sample rate
    let device_rate = get_device_sample_rate(&audio_device.device);

    let control = AudioControl::new();

    // 3. engine (baseline)
    let engine = AudioEngine::new(audio_device, device_rate, control.clone())?;

    let mut producer = engine.producer;

    let running = Arc::new(AtomicBool::new(true));

    // 4. decoder thread (UNCHANGED)
    thread::spawn(move || {
        let _ = stream_decode("test.mp3", device_rate, |sample| {
            loop {
                if producer.try_push(sample).is_ok() {
                    break;
                }
                thread::yield_now();
            }
        });
    });

    let running_clone = running.clone();

    // 🔥 5. INPUT THREAD (ISOLATED)
    thread::spawn(move || {
        loop {
            if event::poll(Duration::from_millis(50)).unwrap() {
                if let Event::Key(key) = event::read().unwrap() {
                    // 🔥 FILTER: hanya ambil press pertama
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    match key.code {
                        KeyCode::Char(' ') => control.toggle_pause(),
                        KeyCode::Char('+') | KeyCode::Char('=') => control.adjust_volume(0.1),
                        KeyCode::Char('-') => control.adjust_volume(-0.1),
                        KeyCode::Char('q') => {
                            running_clone.store(false, Ordering::Relaxed);
                            break;
                        },
                        _ => {}
                    }
                }
            }
        }
    });

    // 6. keep alive (UNCHANGED)
    while running.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(200));
    }
    
    Ok(())
}