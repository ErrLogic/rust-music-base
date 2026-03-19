use std::fs;
use std::path::Path;

pub struct Playlist {
    pub tracks: Vec<String>,
    pub current: usize,
}

impl Playlist {
    pub fn new(tracks: Vec<String>) -> Self {
        Self { tracks, current: 0 }
    }

    pub fn current(&self) -> Option<String> {
        self.tracks.get(self.current).cloned()
    }

    pub fn next(&mut self) {
        if self.tracks.is_empty() { return; }
        self.current = (self.current + 1) % self.tracks.len();
    }

    pub fn prev(&mut self) {
        if self.tracks.is_empty() { return; }
        if self.current == 0 {
            self.current = self.tracks.len() - 1;
        } else {
            self.current -= 1;
        }
    }
}

pub fn load_from_dir<P: AsRef<Path>>(dir: P) -> Vec<String> {
    let mut tracks = Vec::new();

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return tracks,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        // filter extension
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext = ext.to_lowercase();

            if matches!(ext.as_str(), "mp3" | "flac" | "wav" | "ogg" | "aac") {
                if let Some(p) = path.to_str() {
                    tracks.push(p.to_string());
                }
            }
        }
    }

    // 🔥 sort biar deterministic
    tracks.sort();

    tracks
}