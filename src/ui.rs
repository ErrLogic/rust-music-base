use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph, Gauge, ListState},
    style::{Style, Modifier, Color},
};

use crate::playlist::Playlist;
use crate::audio::control::AudioControl;

use std::sync::{Arc, Mutex};
use std::time::Instant;

// =========================
// 🔥 GLOBAL STATE
// =========================
static mut SMOOTH_PROGRESS: f64 = 0.0;
static mut LAST_FRAME: Option<Instant> = None;

static mut MARQUEE_OFFSET: usize = 0;
static mut LAST_MARQUEE: Option<Instant> = None;

// =========================
// 🔥 THEME
// =========================
const GREEN: Color = Color::Rgb(30, 215, 96);
const GREEN_LIGHT: Color = Color::Rgb(120, 255, 180);
const BG_DIM: Color = Color::Rgb(40, 40, 40);
const FG_SOFT: Color = Color::Gray;

// =========================
// 🔥 HELPERS
// =========================
fn display_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}

fn truncate(text: &str, width: usize) -> String {
    if text.len() <= width {
        return text.to_string();
    }
    if width <= 3 {
        return "...".to_string();
    }
    format!("{}...", &text[..width - 3])
}

fn marquee(text: &str, width: usize) -> String {
    if text.len() <= width {
        return text.to_string();
    }

    unsafe {
        let now = Instant::now();

        if let Some(last) = LAST_MARQUEE {
            if now.duration_since(last).as_millis() > 120 {
                MARQUEE_OFFSET = (MARQUEE_OFFSET + 1) % text.len();
                LAST_MARQUEE = Some(now);
            }
        } else {
            LAST_MARQUEE = Some(now);
        }

        let padded = format!("{}   ", text);
        let extended = padded.repeat(2);

        extended
            .chars()
            .skip(MARQUEE_OFFSET)
            .take(width)
            .collect()
    }
}

fn volume_icon(vol: u32) -> &'static str {
    match vol {
        0 => "🔇",
        1..=30 => "🔈",
        31..=70 => "🔉",
        _ => "🔊",
    }
}

// =========================
// 🔥 DRAW
// =========================
pub fn draw(
    f: &mut Frame,
    playlist: &Arc<Mutex<Playlist>>,
    control: &AudioControl,
    selected: &Arc<Mutex<usize>>,
) {
    let size = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(1),
        ])
        .split(size);

    // =========================
    // 🔥 DATA
    // =========================
    let (title_raw, progress, elapsed_str, total_str, header_right) = {
        let pl = playlist.lock().unwrap();

        let title = pl.current().unwrap_or_default();
        let name = display_name(&title);

        let volume = control.volume();
        let vol = (volume.clamp(0.0, 1.0) * 100.0).round() as u32;

        let elapsed = control.elapsed();
        let total = control.total();
        let sr = control.sample_rate() as f32;

        let elapsed_sec = if sr > 0.0 { elapsed as f32 / sr } else { 0.0 };
        let total_sec = if total > 0 { total as f32 / sr } else { 0.0 };

        let raw_progress = if total > 0 {
            elapsed as f64 / total as f64
        } else {
            0.0
        };

        let progress = unsafe {
            let now = Instant::now();

            if let Some(last) = LAST_FRAME {
                let dt = now.duration_since(last).as_secs_f64();
                let alpha = (dt * 8.0).clamp(0.0, 1.0);
                SMOOTH_PROGRESS += (raw_progress - SMOOTH_PROGRESS) * alpha;
            } else {
                SMOOTH_PROGRESS = raw_progress;
            }

            LAST_FRAME = Some(now);
            SMOOTH_PROGRESS
        };

        let elapsed_str = format!("{:02}:{:02}", (elapsed_sec / 60.0) as u32, (elapsed_sec % 60.0) as u32);

        let total_str = if total_sec > 0.0 {
            format!("{:02}:{:02}", (total_sec / 60.0) as u32, (total_sec % 60.0) as u32)
        } else {
            "--:--".to_string()
        };

        let state = if !control.is_started() {
            "■"
        } else if control.is_paused() {
            "⏸"
        } else {
            "▶"
        };

        let icon = volume_icon(vol);

        let header_right = format!("{}  {} {}%", state, icon, vol);

        (name, progress, elapsed_str, total_str, header_right)
    };

    // =========================
    // 🔥 PLAYER BLOCK
    // =========================
    let player_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(GREEN))
        .title(" Now Playing ");

    f.render_widget(player_block.clone(), chunks[0]);

    let inner = player_block.inner(chunks[0]);

    let top = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    // =========================
    // 🔥 HEADER
    // =========================
    let header = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(20),
        ])
        .split(top[0]);

    let title_width = header[0].width as usize;
    let title = marquee(&title_raw, title_width);

    f.render_widget(
        Paragraph::new(title)
            .style(Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
        header[0],
    );

    f.render_widget(
        Paragraph::new(header_right)
            .style(Style::default().fg(FG_SOFT))
            .alignment(Alignment::Right),
        header[1],
    );

    // =========================
    // 🔥 PROGRESS
    // =========================
    let progress_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(18),
        ])
        .split(top[1]);

    let gauge = Gauge::default()
        .ratio(progress.clamp(0.0, 1.0))
        .use_unicode(false)
        .gauge_style(
            Style::default()
                .fg(GREEN)
                .bg(BG_DIM),
        )
        .label("");

    f.render_widget(gauge, progress_layout[0]);

    f.render_widget(
        Paragraph::new(format!("{} / {}", elapsed_str, total_str))
            .style(Style::default().fg(FG_SOFT))
            .alignment(Alignment::Right),
        progress_layout[1],
    );

    // =========================
    // 🔥 PLAYLIST
    // =========================
    let (tracks, current) = {
        let pl = playlist.lock().unwrap();
        (pl.tracks.clone(), pl.current)
    };

    let selected = *selected.lock().unwrap();
    let width = chunks[1].width as usize - 4;

    let items: Vec<ListItem> = tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let raw = display_name(t);

            let text = if i == selected {
                marquee(&raw, width)
            } else {
                truncate(&raw, width)
            };

            if i == current {
                ListItem::new(format!("▶ {}", text))
                    .style(Style::default().fg(GREEN).add_modifier(Modifier::BOLD))
            } else {
                ListItem::new(format!("  {}", text))
                    .style(Style::default().fg(FG_SOFT))
            }
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Songs ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(GREEN)),
        )
        .highlight_style(
            Style::default()
                .fg(GREEN_LIGHT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(" ");

    let mut state = ListState::default();
    state.select(Some(selected));

    f.render_stateful_widget(list, chunks[1], &mut state);
}