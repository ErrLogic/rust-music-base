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

    #[allow(dead_code)]
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

// Modern color palette
pub mod colors {
    use super::rgb565;

    // Background colors
    #[allow(dead_code)]
    pub fn bg_dark() -> u16 { rgb565(10, 12, 18) }
    #[allow(dead_code)]
    pub fn bg_card() -> u16 { rgb565(18, 20, 28) }
    pub fn bg_highlight() -> u16 { rgb565(28, 32, 44) }

    // Accent colors
    pub fn accent_primary() -> u16 { rgb565(0, 210, 140) }
    pub fn accent_secondary() -> u16 { rgb565(100, 100, 220) }
    #[allow(dead_code)]
    pub fn accent_warning() -> u16 { rgb565(255, 100, 100) }

    // Text colors
    pub fn text_primary() -> u16 { rgb565(235, 240, 245) }
    pub fn text_secondary() -> u16 { rgb565(150, 160, 180) }
    pub fn text_muted() -> u16 { rgb565(80, 90, 110) }

    // UI elements
    pub fn border() -> u16 { rgb565(40, 45, 60) }
    pub fn progress_bg() -> u16 { rgb565(35, 40, 55) }
    pub fn progress_fill() -> u16 { rgb565(0, 210, 140) }
    pub fn volume_bg() -> u16 { rgb565(35, 40, 55) }
    pub fn volume_fill() -> u16 { rgb565(100, 100, 220) }

    // Status colors
    pub fn playing() -> u16 { rgb565(0, 210, 140) }
    pub fn selected() -> u16 { rgb565(200, 220, 255) }
    pub fn paused() -> u16 { rgb565(255, 180, 80) }
}

pub fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 & 0xF8) << 8)
        | ((g as u16 & 0xFC) << 3)
        | (b as u16 >> 3)
}