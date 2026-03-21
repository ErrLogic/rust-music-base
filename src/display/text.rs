use crate::display::framebuffer::Framebuffer;
use crate::display::font::{get_char, FONT_HEIGHT};

pub fn draw_text(
    fb: &mut Framebuffer,
    mut x: i32,
    y: i32,
    text: &str,
    color: u16,
) {
    for c in text.chars() {
        let glyph = get_char(c);

        for (col, byte) in glyph.iter().enumerate() {
            for row in 0..FONT_HEIGHT {
                if (byte >> row) & 1 == 1 {
                    let px = x + col as i32;
                    let py = y + row as i32;

                    // 🔥 bounds check
                    if px >= 0 && py >= 0 {
                        let px = px as u16;
                        let py = py as u16;

                        if px < fb.width && py < fb.height {
                            fb.set_pixel(px, py, color);
                        }
                    }
                }
            }
        }

        x += 7; // spacing
    }
}