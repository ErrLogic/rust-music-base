pub struct Framebuffer {
    pub width: u16,
    pub height: u16,
    pub buffer: Vec<u16>,
}

impl Framebuffer {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            buffer: vec![0; width as usize * height as usize],
        }
    }

    pub fn clear(&mut self, color: u16) {
        self.buffer.fill(color);
    }

    pub fn set_pixel(&mut self, x: u16, y: u16, color: u16) {
        if x >= self.width || y >= self.height {
            return;
        }

        let idx = y as usize * self.width as usize + x as usize;
        self.buffer[idx] = color;
    }
}

pub fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 & 0xF8) << 8)
        | ((g as u16 & 0xFC) << 3)
        | (b as u16 >> 3)
}