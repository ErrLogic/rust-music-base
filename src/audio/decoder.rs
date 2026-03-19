use symphonia::core::audio::Signal;
use crate::audio::resampler::LinearResampler;

pub fn stream_decode<P: AsRef<std::path::Path>>(
    path: P,
    out_rate: u32,
    mut push: impl FnMut(f32),
) -> anyhow::Result<()> {
    use symphonia::core::{
        audio::AudioBufferRef,
        codecs::DecoderOptions,
        formats::FormatOptions,
        io::MediaSourceStream,
        meta::MetadataOptions,
        probe::Hint,
    };

    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let hint = Hint::new();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())?;

    let mut format = probed.format;

    let track = format.default_track().unwrap();

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())?;

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
                    let data = buf.chan(0);

                    res_l.process(data, |s| {
                        push(s);
                        push(s);
                    });
                } else {
                    let left = buf.chan(0);
                    let right = buf.chan(1);

                    let mut temp_l = Vec::new();
                    let mut temp_r = Vec::new();

                    res_l.process(left, |s| temp_l.push(s));
                    res_r.process(right, |s| temp_r.push(s));

                    let len = temp_l.len().min(temp_r.len());

                    for i in 0..len {
                        push(temp_l[i]);
                        push(temp_r[i]);
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

                    let len = temp_l.len().min(temp_r.len());

                    for i in 0..len {
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