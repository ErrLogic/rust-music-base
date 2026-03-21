use std::sync::{Arc, Mutex};
use crate::playlist::Playlist;
use crate::audio::control::AudioControl;

static mut LAST_SELECTED: usize = usize::MAX;

pub struct DisplayState {
    pub title: String,
    pub elapsed_sec: f32,
    pub total_sec: f32,
    pub progress: f32,
    pub marquee_offset: usize,
    pub playlist: Vec<String>,
    pub selected: usize,
    pub volume: u32,
}

pub fn build_display_state(
    playlist: &Arc<Mutex<Playlist>>,
    control: &AudioControl,
) -> DisplayState {
    let pl = playlist.lock().unwrap();

    let raw = pl.current().unwrap_or_default();
    let title = truncate(&*clean_title(&*raw), 20);

    let elapsed_sec = control.elapsed_time();

    let total_sec = if control.sample_rate() > 0 {
        control.total() as f32 / control.sample_rate() as f32
    } else {
        0.0
    };

    let progress = if total_sec > 0.0 {
        (elapsed_sec / total_sec).min(1.0)
    } else {
        0.0
    };
    
    let playlist = pl.tracks.clone();
    let selected = pl.current;
    let marquee_offset = next_marquee_offset(title.len(), selected);
    let volume = (control.volume().clamp(0.0, 1.0) * 100.0) as u32;

    DisplayState {
        title,
        elapsed_sec,
        total_sec,
        progress,
        marquee_offset,
        playlist,
        selected,
        volume,
    }
}

fn next_marquee_offset(title_len: usize, selected: usize) -> usize {
    use std::time::{Instant, Duration};

    static mut OFFSET: usize = 0;
    static mut LAST: Option<Instant> = None;
    static mut PAUSE: Option<Instant> = None;

    unsafe {
        let now = Instant::now();

        if LAST_SELECTED != selected {
            OFFSET = 0;
            LAST = Some(now);
            PAUSE = Some(now + Duration::from_secs(2));
            LAST_SELECTED = selected;
            return OFFSET;
        }

        // 🔥 PAUSE PHASE
        if let Some(until) = PAUSE {
            if now < until {
                return OFFSET;
            } else {
                PAUSE = None;
            }
        }

        // 🔥 UPDATE
        if let Some(last) = LAST {
            if now.duration_since(last) > Duration::from_millis(200) {
                OFFSET += 1;
                LAST = Some(now);

                // 🔥 LOOP + PAUSE
                if OFFSET > title_len {
                    OFFSET = 0;
                    PAUSE = Some(now + Duration::from_secs(2));
                }
            }
        } else {
            LAST = Some(now);
        }

        OFFSET
    }
}

pub fn format_time(sec: f32) -> String {
    let total = sec as u32;
    let m = total / 60;
    let s = total % 60;
    format!("{:02}:{:02}", m, s)
}

fn clean_title(path: &str) -> String {
    use std::path::Path;

    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .replace("_", " ")
}

pub(crate) fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_string()
    } else {
        format!("{}...", &text[..max])
    }
}