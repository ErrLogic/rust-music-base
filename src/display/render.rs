use std::path::Path;
use crate::display::text::draw_text;
use super::framebuffer::{Framebuffer, rgb565};
use super::state::{format_time, DisplayState, truncate};

pub fn draw_rect(
    fb: &mut Framebuffer,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    color: u16,
) {
    for dy in 0..h {
        for dx in 0..w {
            fb.set_pixel(x + dx, y + dy, color);
        }
    }
}

pub fn render(fb: &mut Framebuffer, state: &DisplayState) {
    // clear screen
    fb.clear(rgb565(0, 0, 0));

    // setup
    let title_y = 10;
    let bar_y   = 40;
    let time_y  = 60;

    // Title
    let display_title = {
        let padded = format!("{}   ", state.title);
        let extended = padded.repeat(2);

        let width = 20; // jumlah karakter terlihat

        extended
            .chars()
            .skip(state.marquee_offset % padded.len())
            .take(width)
            .collect::<String>()
    };

    let title_x = 240 / 2 - (display_title.len() as i32 * 6 / 2);
    draw_text(fb, title_x, title_y, &display_title, 0xFFFF);

    // volume
    let volume_text = format!("VOL {}%", state.volume);
    draw_text(fb, 20, 30, &volume_text, rgb565(180, 180, 180));

    // progress bar background
    draw_rect(fb, 20, bar_y - 1, 200, 12, rgb565(50, 50, 50));

    // progress bar fill
    let filled = (200.0 * state.progress) as u16;
    draw_rect(fb, 20, bar_y, filled, 10, rgb565(0, 255, 0));

    // Time
    let time_str = format!(
        "{} / {}",
        format_time(state.elapsed_sec),
        format_time(state.total_sec)
    );

    let time_x = 240 / 2 - (time_str.len() as i32 * 6 / 2);
    draw_text(fb, time_x, time_y, &time_str, 0xFFFF);

    let start_y = 100;
    let line_height = 12;

    let window_size = 5;
    let half = window_size / 2;

    let start = state.selected.saturating_sub(half);
    let end = (start + window_size).min(state.playlist.len());

    for (i, track) in state.playlist[start..end].iter().enumerate() {
        let idx = start + i;

        let name = Path::new(track)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(track);

        let y = start_y + (i as i32 * line_height);

        let is_selected = idx == state.selected;

        let color = if is_selected {
            rgb565(0, 255, 0)
        } else {
            rgb565(200, 200, 200)
        };

        let display_text = if is_selected {
            // 🔥 MARQUEE
            let padded = format!("{}   ", name);
            let extended = padded.repeat(2);

            let width = 20;

            extended
                .chars()
                .skip(state.marquee_offset % padded.len())
                .take(width)
                .collect::<String>()
        } else {
            // 🔥 TRUNCATE
            truncate(name, 20)
        };

        let display_text = if is_selected {
            format!("▶ {}", display_text)
        } else {
            format!("  {}", display_text)
        };

        draw_text(fb, 20, y, &display_text, color);
    }
}