use serde::{Serialize, Deserialize};
use std::fs;

const STATE_FILE: &str = "state.json";

#[derive(Serialize, Deserialize)]
pub struct AppState {
    pub track_path: Option<String>,
    pub elapsed: u64,
    pub volume: f32,
}

pub fn load_state() -> Option<AppState> {
    let data = fs::read_to_string(STATE_FILE).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn save_state(state: &AppState) {
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = fs::write(STATE_FILE, json);
    }
}