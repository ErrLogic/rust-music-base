use crate::audio::resampler::LinearResampler;
use symphonia::core::audio::Signal;

pub struct DecodeInfo {
    pub sample_rate: u32,
    pub total_samples: u64,
}

pub fn stream_decode<P: AsRef<std::path::Path>>(
    path: P,
    out_rate: u32,
    mut push: impl FnMut(f32) -> bool, // 🔥 return bool
) -> anyhow::Result<()> {
    use symphonia::core::{
        audio::AudioBufferRef,
        codecs::DecoderOptions,
        formats::FormatOptions,
        io::MediaSourceStream,
        meta::MetadataOptions,
        probe::Hint
        ,
    };

    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let probed = symphonia::default::get_probe()
        .format(&Hint::new(), mss, &FormatOptions::default(), &MetadataOptions::default())?;

    let mut format = probed.format;
    let track = format.default_track().unwrap();

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())?;

    let mut resampler_l: Option<LinearResampler> = None;
    let mut resampler_r: Option<LinearResampler> = None;

    const CHUNK_SIZE: usize = 1024;
    let mut chunk_buffer = Vec::with_capacity(CHUNK_SIZE);

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(_) => break,
        };

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(_) => continue,
        };

        match decoded {
            AudioBufferRef::F32(buf) => {
                let spec = buf.spec();
                let in_rate = spec.rate as f32;
                let channels = spec.channels.count();

                if resampler_l.is_none() {
                    resampler_l = Some(LinearResampler::new(in_rate, out_rate as f32));
                }
                if resampler_r.is_none() && channels > 1 {
                    resampler_r = Some(LinearResampler::new(in_rate, out_rate as f32));
                }

                let res_l = resampler_l.as_mut().unwrap();

                if channels == 1 {
                    res_l.process(buf.chan(0), |s| {
                        chunk_buffer.push(s);
                        chunk_buffer.push(s);

                        if chunk_buffer.len() >= CHUNK_SIZE {
                            for sample in chunk_buffer.drain(..) {
                                if !push(sample) {
                                    return;
                                }
                            }
                        }
                    });
                } else if let Some(res_r) = resampler_r.as_mut() {
                    let mut l = Vec::new();
                    let mut r = Vec::new();

                    res_l.process(buf.chan(0), |s| l.push(s));
                    res_r.process(buf.chan(1), |s| r.push(s));

                    for i in 0..l.len().min(r.len()) {
                        chunk_buffer.push(l[i]);
                        chunk_buffer.push(r[i]);

                        if chunk_buffer.len() >= CHUNK_SIZE {
                            for sample in chunk_buffer.drain(..) {
                                if !push(sample) {
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
            }

            AudioBufferRef::S16(buf) => {
                let spec = buf.spec();
                let in_rate = spec.rate as f32;
                let channels = spec.channels.count();

                if resampler_l.is_none() {
                    resampler_l = Some(LinearResampler::new(in_rate, out_rate as f32));
                }
                if resampler_r.is_none() && channels > 1 {
                    resampler_r = Some(LinearResampler::new(in_rate, out_rate as f32));
                }

                let res_l = resampler_l.as_mut().unwrap();

                if channels == 1 {
                    let data: Vec<f32> = buf
                        .chan(0)
                        .iter()
                        .map(|s| *s as f32 / i16::MAX as f32)
                        .collect();

                    res_l.process(&data, |s| {
                        chunk_buffer.push(s);
                        chunk_buffer.push(s);

                        if chunk_buffer.len() >= CHUNK_SIZE {
                            for sample in chunk_buffer.drain(..) {
                                if !push(sample) {
                                    return;
                                }
                            }
                        }
                    });
                } else if let Some(res_r) = resampler_r.as_mut() {
                    let left: Vec<f32> = buf
                        .chan(0)
                        .iter()
                        .map(|s| *s as f32 / i16::MAX as f32)
                        .collect();

                    let right: Vec<f32> = buf
                        .chan(1)
                        .iter()
                        .map(|s| *s as f32 / i16::MAX as f32)
                        .collect();

                    let mut temp_l = Vec::new();
                    let mut temp_r = Vec::new();

                    res_l.process(&left, |s| temp_l.push(s));
                    res_r.process(&right, |s| temp_r.push(s));

                    for i in 0..temp_l.len().min(temp_r.len()) {
                        chunk_buffer.push(temp_l[i]);
                        chunk_buffer.push(temp_r[i]);

                        if chunk_buffer.len() >= CHUNK_SIZE {
                            for sample in chunk_buffer.drain(..) {
                                if !push(sample) {
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
            }

            _ => {}
        }
    }

    for sample in chunk_buffer {
        if !push(sample) {
            break;
        }
    }

    Ok(())
}

pub fn probe_only<P: AsRef<std::path::Path>>(
    path: P,
) -> anyhow::Result<DecodeInfo> {
    use symphonia::core::{
        formats::FormatOptions,
        io::MediaSourceStream,
        meta::MetadataOptions,
        probe::Hint,
    };

    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let probed = symphonia::default::get_probe()
        .format(&Hint::new(), mss, &FormatOptions::default(), &MetadataOptions::default())?;

    let format = probed.format;
    let track = format.default_track().unwrap();

    Ok(DecodeInfo {
        sample_rate: track.codec_params.sample_rate.unwrap_or(48000),
        total_samples: track.codec_params.n_frames.unwrap_or(0),
    })
}