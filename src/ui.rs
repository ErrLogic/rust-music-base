use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::playlist::Playlist;
use crate::audio::control::AudioControl;

use std::sync::{Arc, Mutex};
use ratatui::widgets::ListState;

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
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(size);

    // 🔥 playlist snapshot (short lock)
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
                ListItem::new(format!("▶ {}", name)) // playing
            } else {
                ListItem::new(format!("  {}", name)) // cursor
            }
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().title("Songs").borders(Borders::ALL))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::REVERSED) // 🔥 block highlight
        )
        .highlight_symbol(""); // 🔥 no ">"

    let mut state = ListState::default();
    state.select(Some(selected));

    f.render_stateful_widget(list, chunks[0], &mut state);

    // 🔥 status
    let (title, status_text) = {
        let pl = playlist.lock().unwrap();

        let title = pl.current().unwrap_or_default();

        let name = std::path::Path::new(&title)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&title)
            .to_string();

        let paused = control.is_paused();
        let volume = control.volume();

        let status = format!(
            "{} | Vol {:.2}",
            if paused { "Paused" } else { "Playing" },
            volume
        );

        (name, status)
    };

    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(20),
        ])
        .split(chunks[1]);

    let title_widget = Paragraph::new(title)
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(title_widget, bottom[0]);

    let status_widget = Paragraph::new(status_text)
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(status_widget, bottom[1]);
}