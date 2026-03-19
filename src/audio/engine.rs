use std::sync::atomic::Ordering;
use anyhow::Result;
use cpal::traits::{DeviceTrait, StreamTrait};

use super::device::AudioDevice;

use ringbuf::consumer::Consumer;
use ringbuf::{traits::Split, HeapCons, HeapProd, HeapRb};
use crate::audio::control::AudioControl;

#[allow(dead_code)]
pub struct AudioEngine {
    _stream: cpal::Stream,
    pub producer: HeapProd<f32>,
    pub sample_rate: f32,
}

impl AudioEngine {
    pub fn new(audio_device: AudioDevice, sample_rate: u32, audio_control: AudioControl) -> Result<Self> {
        let device = audio_device.device;
        let mut config = audio_device.config;

        config.sample_rate = sample_rate;

        let channels = config.channels as usize;

        let rb = HeapRb::<f32>::new(sample_rate as usize * 2);
        let (producer, mut consumer): (HeapProd<f32>, HeapCons<f32>) = rb.split();

        let paused_flag = audio_control.paused.clone();
        let volume_bits = audio_control.volume_bits.clone();

        let stream = device.build_output_stream(
            &config,
            move |output: &mut [f32], _| {
                // 🔥 load sekali
                let is_paused = paused_flag.load(Ordering::Relaxed);
                let vol = f32::from_bits(volume_bits.load(Ordering::Relaxed));

                for frame in output.chunks_mut(channels) {
                    for ch in frame {
                        let mut s = if is_paused {
                            0.0
                        } else {
                            consumer.try_pop().unwrap_or(0.0)
                        };

                        s *= vol;

                        *ch = s;
                    }
                }
            },
            move |err| {
                eprintln!("Stream error: {:?}", err);
            },
            None,
        )?;

        stream.play()?;

        Ok(Self {
            _stream: stream,
            producer,
            sample_rate: sample_rate as f32,
        })
    }
}