use std::path::Path;

use crate::display::text::draw_text;

use super::framebuffer::{Framebuffer, colors};
use super::state::{DisplayState, RenderState, format_time, truncate};

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
            let px = x + dx;
            let py = y + dy;
            if px < fb.width && py < fb.height {
                fb.set_pixel(px, py, color);
            }
        }
    }
}

pub fn draw_rounded_rect(
    fb: &mut Framebuffer,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    radius: u16,
    color: u16,
) {
    if w < radius * 2 || h < radius * 2 {
        draw_rect(fb, x, y, w, h, color);
        return;
    }

    for dy in 0..h {
        for dx in 0..w {
            let px = x + dx;
            let py = y + dy;

            if px >= fb.width || py >= fb.height {
                continue;
            }

            let in_corner = (dx < radius && dy < radius) ||
                (dx < radius && dy >= h - radius) ||
                (dx >= w - radius && dy < radius) ||
                (dx >= w - radius && dy >= h - radius);

            if in_corner {
                let corner_dist_x = if dx < radius { radius - dx } else { dx - (w - radius) };
                let corner_dist_y = if dy < radius { radius - dy } else { dy - (h - radius) };

                if (corner_dist_x as f32).hypot(corner_dist_y as f32) <= radius as f32 {
                    fb.set_pixel(px, py, color);
                }
            } else {
                fb.set_pixel(px, py, color);
            }
        }
    }
}

pub fn draw_hline(fb: &mut Framebuffer, y: u16, color: u16) {
    if y < fb.height {
        for x in 0..fb.width {
            fb.set_pixel(x, y, color);
        }
    }
}

fn draw_modern_border(fb: &mut Framebuffer) {
    let w = fb.width;
    let h = fb.height;

    for x in 0..w {
        fb.set_pixel(x, 0, colors::border());
        fb.set_pixel(x, h - 1, colors::border());
    }

    for y in 0..h {
        fb.set_pixel(0, y, colors::border());
        fb.set_pixel(w - 1, y, colors::border());
    }

    for x in 1..w-1 {
        if 1 < h && h - 2 < h {
            fb.set_pixel(x, 1, colors::text_muted());
            fb.set_pixel(x, h - 2, colors::text_muted());
        }
    }

    for y in 1..h-1 {
        if 1 < w && w - 2 < w {
            fb.set_pixel(1, y, colors::text_muted());
            fb.set_pixel(w - 2, y, colors::text_muted());
        }
    }
}

fn draw_volume_bar(fb: &mut Framebuffer, x: u16, y: u16, width: u16, volume: u32) {
    let bar_height = 4;
    let bar_width = (width as f32 * (volume as f32 / 100.0)) as u16;

    draw_rect(fb, x, y, width, bar_height, colors::volume_bg());

    if bar_width > 0 {
        draw_rect(fb, x, y, bar_width, bar_height, colors::volume_fill());
    }
}

/// Draw vertical scroll bar for playlist
fn draw_scroll_bar(
    fb: &mut Framebuffer,
    x: u16,
    top_y: u16,
    bottom_y: u16,
    current_index: usize,
    total_items: usize,
    visible_items: usize,
) {
    if total_items == 0 || visible_items == 0 || total_items <= visible_items {
        return;
    }

    let bar_width = 3; // Thin bar
    let scroll_area_height = bottom_y - top_y;

    // Calculate scroll bar position
    let scroll_ratio = if total_items > visible_items {
        current_index as f32 / (total_items - visible_items) as f32
    } else {
        0.0
    };

    let bar_height = (scroll_area_height as f32 * (visible_items as f32 / total_items as f32))
        .max(bar_width as f32) as u16;

    let bar_y = top_y + (scroll_area_height as f32 * scroll_ratio) as u16;
    let bar_y = bar_y.min(bottom_y - bar_height);

    // Draw scroll bar background (thin line)
    // Using draw_vline for the background would be inefficient for a thick line
    // So we use draw_rect for the background
    draw_rect(fb, x, top_y, bar_width, scroll_area_height, colors::volume_bg());

    // Draw scroll bar indicator
    draw_rect(fb, x, bar_y, bar_width, bar_height, colors::volume_fill());
}

pub fn render(
    fb: &mut Framebuffer,
    state: &DisplayState,
    rs: &mut RenderState,
) {
    // Simple dark background
    for y in 0..fb.height {
        let color = super::framebuffer::rgb565(10, 12, 18);
        for x in 0..fb.width {
            fb.set_pixel(x, y, color);
        }
    }

    let padding: u16 = 8;
    let width = fb.width - padding * 2;

    // =========================
    // LAYOUT
    // =========================
    let header_top = 8;
    let now_playing_y = 24;
    let title_y = 40;
    let progress_y = 66;
    let time_y = 80;
    let volume_y = 98;
    let volume_percent_x = fb.width - padding - 35;
    let separator_y = 116;
    let playlist_header_y = 130;
    let playlist_top = 144;
    let playlist_bottom = fb.height - 18;

    // Scroll bar position (right side, with padding)
    let scroll_bar_x = fb.width - padding - 4; // 3px bar + 1px margin

    // =========================
    // ANIMATION UPDATE
    // =========================
    let item_height = 16.0;
    let viewport_height = (playlist_bottom - playlist_top) as f32;
    let visible_items = (viewport_height / item_height) as usize;
    let center_offset = viewport_height / 2.0 - item_height / 2.0;

    let target_scroll = (state.selected as f32 * item_height) - center_offset;
    let max_scroll = (state.playlist.len() as f32 * item_height - viewport_height).max(0.0);
    let target_scroll = target_scroll.clamp(0.0, max_scroll);

    rs.update(state.progress, target_scroll);

    // =========================
    // HEADER SECTION
    // =========================
    draw_text(fb, padding as i32, header_top, "MUSIC PLAYER", colors::accent_primary());
    draw_text(fb, padding as i32, now_playing_y, "NOW PLAYING", colors::text_secondary());

    // =========================
    // NOW PLAYING TRACK - FULL WIDTH
    // =========================
    let max_chars = ((width as i32 - 16) / 7) as usize;

    let title = if state.title.len() > max_chars {
        let padded = format!("{}   ", state.title);
        let extended = padded.repeat(2);

        extended
            .chars()
            .skip(state.marquee_offset % padded.len())
            .take(max_chars)
            .collect::<String>()
    } else {
        state.title.clone()
    };

    draw_text(fb, padding as i32, title_y, &title, colors::text_primary());

    // =========================
    // PROGRESS BAR
    // =========================
    draw_rounded_rect(fb, padding, progress_y, width, 4, 2, colors::progress_bg());

    let filled = (width as f32 * rs.smooth_progress) as u16;
    if filled > 0 && filled <= width {
        draw_rounded_rect(fb, padding, progress_y, filled, 4, 2, colors::progress_fill());
    }

    // =========================
    // TIME DISPLAY
    // =========================
    let elapsed_str = format_time(state.elapsed_sec);
    let total_str = format_time(state.total_sec);

    draw_text(fb, padding as i32, time_y, &elapsed_str, colors::text_secondary());
    let total_width = (total_str.len() * 7) as i32;
    draw_text(fb, fb.width as i32 - padding as i32 - total_width, time_y, &total_str, colors::text_secondary());

    // =========================
    // VOLUME SECTION
    // =========================
    draw_text(fb, padding as i32, volume_y, "VOLUME", colors::text_secondary());
    let volume_bar_width = 70;
    draw_volume_bar(fb, padding + 55, (volume_y + 2) as u16, volume_bar_width, state.volume);
    draw_text(fb, volume_percent_x as i32, volume_y, &format!("{}%", state.volume), colors::accent_secondary());

    // =========================
    // SEPARATOR
    // =========================
    draw_hline(fb, separator_y, colors::border());
    draw_hline(fb, separator_y + 1, colors::bg_highlight());

    // =========================
    // PLAYLIST HEADER
    // =========================
    draw_text(fb, padding as i32, playlist_header_y, "PLAYLIST", colors::accent_secondary());

    // =========================
    // PLAYLIST ITEMS (with reduced width to make room for scroll bar)
    // =========================
    let playlist_width = width - 8; // Make room for scroll bar
    let max_playlist_chars = ((playlist_width as i32 - 32) / 7) as usize;

    for (i, track) in state.playlist.iter().enumerate() {
        let y = playlist_top as f32 + (i as f32 * item_height) - rs.scroll;

        if y < playlist_top as f32 || y > playlist_bottom as f32 {
            continue;
        }

        let name = Path::new(track)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(track);

        let is_selected = i == state.selected;
        let is_playing = i == state.playing_index;

        // Background for selected item
        if is_selected {
            let y_u16 = y as u16;
            if y_u16 + 14 < fb.height {
                draw_rect(fb, padding, y_u16, playlist_width, 14, colors::bg_highlight());
            }
        }

        let prefix = if is_playing {
            "▶ "
        } else if is_selected {
            "• "
        } else {
            "  "
        };

        let color = if is_playing {
            colors::playing()
        } else if is_selected {
            colors::selected()
        } else {
            colors::text_secondary()
        };

        let display_name = if is_selected && name.len() > max_playlist_chars {
            let padded = format!("{}   ", name);
            let extended = padded.repeat(2);

            extended
                .chars()
                .skip(state.marquee_offset % padded.len())
                .take(max_playlist_chars)
                .collect::<String>()
        } else {
            truncate(name, max_playlist_chars)
        };

        let final_text = format!("{}{}", prefix, display_name);
        draw_text(fb, padding as i32, y as i32, &final_text, color);
    }

    // =========================
    // SCROLL BAR INDICATOR
    // =========================
    draw_scroll_bar(
        fb,
        scroll_bar_x,
        playlist_top,
        playlist_bottom,
        state.selected,
        state.playlist.len(),
        visible_items,
    );

    // =========================
    // BORDER
    // =========================
    draw_modern_border(fb);

    // =========================
    // STATUS INDICATOR (bottom right)
    // =========================
    // Use symbols directly instead of text
    let status_text = if state.is_playing { "▶" } else { "⏸" };
    let status_full = if state.is_playing { " PLAYING" } else { " PAUSED" };
    let status_color = if state.is_playing { colors::playing() } else { colors::paused() };

    // Draw symbol and text separately to ensure symbols render
    let symbol_width = 7; // Width of symbol character
    let text_width = status_full.len() as i32 * 7;
    let total_width = symbol_width + text_width;

    let start_x = fb.width as i32 - padding as i32 - total_width;
    let status_y = (fb.height - 10) as i32;

    // Draw the symbol
    draw_text(fb, start_x, status_y, status_text, status_color);
    // Draw the text after the symbol
    draw_text(fb, start_x + symbol_width, status_y, status_full, status_color);
}