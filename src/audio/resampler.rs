pub struct LinearResampler {
    ratio: f32,
    pos: f32,
    prev: f32,
}

impl LinearResampler {
    pub fn new(in_rate: f32, out_rate: f32) -> Self {
        Self {
            ratio: in_rate / out_rate,
            pos: 0.0,
            prev: 0.0,
        }
    }

    pub fn process(&mut self, input: &[f32], mut push: impl FnMut(f32)) {
        let mut i = 0;

        while i < input.len() {
            let current = input[i];

            while self.pos <= 1.0 {
                let sample = self.prev + (current - self.prev) * self.pos;
                push(sample);
                self.pos += self.ratio;
            }

            self.pos -= 1.0;
            self.prev = current;
            i += 1;
        }
    }
}