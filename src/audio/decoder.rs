use symphonia::core::audio::Signal;
use crate::audio::resampler::LinearResampler;

pub struct DecodeInfo {
    pub sample_rate: u32,
    pub total_samples: u64,
}

pub fn stream_decode_with_seek<P: AsRef<std::path::Path>>(
    path: P,
    out_rate: u32,
    seek_samples: u64,
    mut push: impl FnMut(f32),
) -> anyhow::Result<()> {
    use symphonia::core::{
        audio::AudioBufferRef,
        codecs::DecoderOptions,
        formats::{FormatOptions, SeekMode, SeekTo},
        io::MediaSourceStream,
        meta::MetadataOptions,
        probe::Hint,
        units::Time,
    };

    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let probed = symphonia::default::get_probe()
        .format(&Hint::new(), mss, &FormatOptions::default(), &MetadataOptions::default())?;

    let mut format = probed.format;
    let track = format.default_track().unwrap();

    let sample_rate = track.codec_params.sample_rate.unwrap_or(48000);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())?;

    // 🔥 SEEK (REAL)
    if seek_samples > 0 && sample_rate > 0 {
        let seconds = seek_samples as f64 / sample_rate as f64;

        let _ = format.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time: Time::from(seconds),
                track_id: Some(track.id),
            },
        );
    }

    let mut resampler_l: Option<LinearResampler> = None;
    let mut resampler_r: Option<LinearResampler> = None;

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
                if resampler_r.is_none() {
                    resampler_r = Some(LinearResampler::new(in_rate, out_rate as f32));
                }

                let res_l = resampler_l.as_mut().unwrap();
                let res_r = resampler_r.as_mut().unwrap();

                if channels == 1 {
                    res_l.process(buf.chan(0), |s| {
                        push(s);
                        push(s);
                    });
                } else {
                    let mut l = Vec::new();
                    let mut r = Vec::new();

                    res_l.process(buf.chan(0), |s| l.push(s));
                    res_r.process(buf.chan(1), |s| r.push(s));

                    for i in 0..l.len().min(r.len()) {
                        push(l[i]);
                        push(r[i]);
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
                if resampler_r.is_none() {
                    resampler_r = Some(LinearResampler::new(in_rate, out_rate as f32));
                }

                let res_l = resampler_l.as_mut().unwrap();
                let res_r = resampler_r.as_mut().unwrap();

                if channels == 1 {
                    let data: Vec<f32> = buf
                        .chan(0)
                        .iter()
                        .map(|s| *s as f32 / i16::MAX as f32)
                        .collect();

                    res_l.process(&data, |s| {
                        push(s);
                        push(s);
                    });
                } else {
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
                        push(temp_l[i]);
                        push(temp_r[i]);
                    }
                }
            }

            _ => {}
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