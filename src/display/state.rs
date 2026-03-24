use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::playlist::Playlist;
use crate::audio::control::AudioControl;

pub struct DisplayState {
    pub title: String,
    pub progress: f32,
    pub elapsed_sec: f32,
    pub total_sec: f32,
    pub playlist: Vec<String>,
    pub selected: usize,      // Currently selected/highlighted track in playlist (for navigation)
    pub playing_index: usize, // Actually playing track
    pub volume: u32,
    pub marquee_offset: usize,
    pub is_playing: bool,
}

// =========================
// VISUAL STATE
// =========================
pub struct RenderState {
    pub smooth_progress: f32,
    pub scroll: f32,
    last_frame: Instant,
    pub fps: f32,
    frame_count: u32,
    fps_timer: Instant,
}

impl RenderState {
    pub fn new() -> Self {
        Self {
            smooth_progress: 0.0,
            scroll: 0.0,
            last_frame: Instant::now(),
            fps: 0.0,
            frame_count: 0,
            fps_timer: Instant::now(),
        }
    }

    pub fn update(&mut self, target_progress: f32, target_scroll: f32) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        self.frame_count += 1;
        if self.fps_timer.elapsed() >= Duration::from_secs(1) {
            self.fps = self.frame_count as f32;
            self.frame_count = 0;
            self.fps_timer = Instant::now();
        }

        let alpha = (dt * 12.0).min(1.0);
        self.smooth_progress += (target_progress - self.smooth_progress) * alpha;

        let scroll_speed = 12.0;
        let s_alpha = (dt * scroll_speed).min(1.0);
        self.scroll += (target_scroll - self.scroll) * s_alpha;
    }
}

// =========================
// BUILD STATE
// =========================
pub fn build_display_state(
    playlist: &Arc<Mutex<Playlist>>,
    control: &AudioControl,
    is_playing: bool,
    selected_index: usize,  // Pass selected index separately
) -> DisplayState {
    let pl = playlist.lock().unwrap();

    // Get currently playing track
    let playing_path = pl.current().unwrap_or_default();
    let title = clean_title(&playing_path);

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

    DisplayState {
        title,
        progress,
        elapsed_sec,
        total_sec,
        playlist: pl.tracks.clone(),
        selected: selected_index,           // Use the passed selected index
        playing_index: pl.current,          // Playing index is the actual playing track
        volume: (control.volume().clamp(0.0, 1.0) * 100.0) as u32,
        marquee_offset: next_marquee_offset(),
        is_playing,
    }
}

// =========================
// MARQUEE
// =========================
fn next_marquee_offset() -> usize {
    static mut OFFSET: usize = 0;
    static mut LAST: Option<Instant> = None;

    unsafe {
        let now = Instant::now();

        if let Some(last) = LAST {
            if now.duration_since(last) > Duration::from_millis(120) {
                OFFSET += 1;
                LAST = Some(now);
            }
        } else {
            LAST = Some(now);
        }

        OFFSET
    }
}

// =========================
// UTILS
// =========================
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
        .replace("-", " - ")
}

pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max - 3).collect();
        format!("{}...", truncated)
    }
}