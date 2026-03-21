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
        let started_flag = audio_control.started.clone();

        let started_output_flag = audio_control.started_at_output.clone();
        let start_time = audio_control.start_time.clone();

        let elapsed_samples = audio_control.elapsed_samples.clone();

        let stream = device.build_output_stream(
            &config,
            move |output: &mut [f32], _| {
                let is_paused = paused_flag.load(Ordering::Relaxed);
                let is_started = started_flag.load(Ordering::Relaxed);
                let vol = f32::from_bits(volume_bits.load(Ordering::Relaxed));

                let mut frames_played = 0;

                for frame in output.chunks_mut(channels) {
                    let (mut l, mut r) = (0.0, 0.0);

                    if is_started && !is_paused {
                        if !started_output_flag.load(Ordering::Relaxed) {
                            if let Some(_) = consumer.try_peek() {
                                let mut st = start_time.lock().unwrap();
                                *st = Some(std::time::Instant::now());
                                started_output_flag.store(true, Ordering::Relaxed);
                            }
                        }

                        if let Some(sample_l) = consumer.try_pop() {
                            l = sample_l;
                            frames_played += 1;

                            if channels > 1 {
                                r = consumer.try_pop().unwrap_or(l);
                            } else {
                                r = l;
                            }
                        }
                    }

                    l *= vol;
                    r *= vol;

                    frame[0] = l;
                    if channels > 1 {
                        frame[1] = r;
                    }
                }

                if frames_played > 0 {
                    elapsed_samples.fetch_add(frames_played as u64, Ordering::Relaxed);
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