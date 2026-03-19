mod audio;

use anyhow::Result;
use ringbuf::producer::Producer;

use audio::device::{get_output_device, get_device_sample_rate};
use audio::engine::AudioEngine;
use audio::decoder::stream_decode;

fn main() -> Result<()> {
    // 1. ambil device dulu
    let audio_device = get_output_device()?;

    // 2. ambil sample rate dari device (pakai reference)
    let device_rate = get_device_sample_rate(&audio_device.device);

    // 3. buat engine SEKALI
    let engine = AudioEngine::new(audio_device, device_rate)?;

    let mut producer = engine.producer;

    // 4. spawn decoder
    std::thread::spawn(move || {
        let _ = stream_decode("test.mp3", device_rate, |sample| {
            loop {
                if producer.try_push(sample).is_ok() {
                    break;
                }
                std::thread::yield_now();
            }
        });
    });

    println!("Streaming...");

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}