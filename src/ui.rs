use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph, Gauge, ListState},
    style::{Style, Modifier},
};

use crate::playlist::Playlist;
use crate::audio::control::AudioControl;

use std::sync::{Arc, Mutex};

pub fn draw(
    f: &mut Frame,
    playlist: &Arc<Mutex<Playlist>>,
    control: &AudioControl,
    selected: &Arc<Mutex<usize>>,
) {
    let size = f.area();

    // 🔥 layout: player bar (top) + list (bottom)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // player bar
            Constraint::Min(1),    // list
        ])
        .split(size);

    // =========================
    // 🔥 PLAYER BAR (TOP)
    // =========================

    let (title, status_text, progress, elapsed_str, total_str) = {
        let pl = playlist.lock().unwrap();

        let title = pl.current().unwrap_or_default();

        let name = std::path::Path::new(&title)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&title)
            .to_string();

        let volume = control.volume();

        let elapsed_samples = control.elapsed();
        let total_samples = control.total();
        let sample_rate = control.sample_rate() as f32;

        let elapsed_sec = control.elapsed_seconds();
        let total_sec = if total_samples > 0 {
            total_samples as f32 / sample_rate
        } else {
            0.0
        };

        let progress = if total_samples > 0 {
            elapsed_samples as f64 / total_samples as f64
        } else {
            0.0
        };

        let elapsed_str = format!(
            "{:02}:{:02}",
            (elapsed_sec / 60.0) as u32,
            (elapsed_sec % 60.0) as u32
        );

        let total_str = if total_sec > 0.0 {
            format!(
                "{:02}:{:02}",
                (total_sec / 60.0) as u32,
                (total_sec % 60.0) as u32
            )
        } else {
            "--:--".to_string()
        };

        let started = control.is_started();
        let paused = control.is_paused();

        let status = if !started {
            "Idle"
        } else if paused {
            "Paused"
        } else {
            "Playing"
        };

        let status = format!("{} | Vol {:.1}", status, volume);

        (name, status, progress, elapsed_str, total_str)
    };

    let player_block = Block::default()
        .borders(Borders::ALL)
        .title("Now Playing");

    f.render_widget(player_block.clone(), chunks[0]);

    let inner = player_block.inner(chunks[0]);

    let top = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    // 🔥 title (left) + status (right)
    let header = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(20),
        ])
        .split(top[0]);

    f.render_widget(Paragraph::new(title), header[0]);
    f.render_widget(Paragraph::new(status_text), header[1]);

    // 🔥 progress bar
    let gauge = Gauge::default()
        .ratio(progress.clamp(0.0, 1.0))
        .label(format!("{} / {}", elapsed_str, total_str))
        .gauge_style(
            Style::default()
        );

    f.render_widget(gauge, top[1]);

    // =========================
    // 🔥 PLAYLIST (BOTTOM)
    // =========================

    let (tracks, current) = {
        let pl = playlist.lock().unwrap();
        (pl.tracks.clone(), pl.current)
    };

    let selected = *selected.lock().unwrap();

    let items: Vec<ListItem> = tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let name = std::path::Path::new(t)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(t);

            if i == current {
                ListItem::new(format!("▶ {}", name))
            } else {
                ListItem::new(format!("  {}", name))
            }
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().title("Songs").borders(Borders::ALL))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::REVERSED)
        )
        .highlight_symbol("");

    let mut state = ListState::default();
    state.select(Some(selected));

    f.render_stateful_widget(list, chunks[1], &mut state);
}