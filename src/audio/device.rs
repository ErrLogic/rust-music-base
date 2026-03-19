use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};

pub struct AudioDevice {
    pub device: cpal::Device,
    pub config: cpal::StreamConfig,
}

pub fn get_output_device() -> Result<AudioDevice> {
    let host = cpal::default_host();

    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("No output device found"))?;

    let config = device.default_output_config()?;

    Ok(AudioDevice {
        device,
        config: config.into(),
    })
}

pub fn get_device_sample_rate(device: &cpal::Device) -> u32 {
    device
        .default_output_config()
        .unwrap()
        .sample_rate()
}