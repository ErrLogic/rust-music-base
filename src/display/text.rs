use crate::display::framebuffer::Framebuffer;
use crate::display::font::{get_char, FONT_HEIGHT, FONT_WIDTH, LETTER_SPACING};

pub fn draw_text(
    fb: &mut Framebuffer,
    x: i32,
    y: i32,
    text: &str,
    color: u16,
) {
    let mut x_pos = x;

    for c in text.chars() {
        let glyph = get_char(c);

        // Draw character pixels
        for (col, byte) in glyph.iter().enumerate() {
            for row in 0..FONT_HEIGHT {
                if (byte >> row) & 1 == 1 {
                    let px = x_pos + col as i32;
                    let py = y + row as i32;

                    if px >= 0 && py >= 0 && px < fb.width as i32 && py < fb.height as i32 {
                        fb.set_pixel(px as u16, py as u16, color);
                    }
                }
            }
        }

        // Add spacing for larger font
        x_pos += FONT_WIDTH as i32 + LETTER_SPACING;
    }
}