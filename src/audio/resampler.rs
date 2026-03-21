pub struct LinearResampler {
    ratio: f32,
    pos: f32,
    input_buffer: Vec<f32>,
}

impl LinearResampler {
    pub fn new(in_rate: f32, out_rate: f32) -> Self {
        Self {
            ratio: in_rate / out_rate, // 🔥 IMPORTANT
            pos: 0.0,
            input_buffer: Vec::new(),
        }
    }

    pub fn process(&mut self, input: &[f32], mut push: impl FnMut(f32)) {
        if input.is_empty() {
            return;
        }

        // append input
        self.input_buffer.extend_from_slice(input);

        // output-driven sampling
        while self.pos < self.input_buffer.len() as f32 - 1.0 {
            let i = self.pos as usize;
            let frac = self.pos - i as f32;

            let s0 = self.input_buffer[i];
            let s1 = self.input_buffer[i + 1];

            // linear interpolation
            let sample = s0 + (s1 - s0) * frac;

            push(sample);

            self.pos += self.ratio;
        }

        // cleanup consumed samples
        let consumed = self.pos.floor() as usize;
        if consumed > 0 {
            self.input_buffer.drain(0..consumed);
            self.pos -= consumed as f32;
        }
    }
}