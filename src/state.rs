use serde::{Serialize, Deserialize};
use std::fs;
use std::path::Path;

const STATE_FILE: &str = "state.json";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppState {
    pub track_path: Option<String>,
    pub elapsed: u64,
    pub volume: f32,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            track_path: None,
            elapsed: 0,
            volume: 1.0,
        }
    }
}

pub fn load_state() -> Option<AppState> {
    // Check if file exists first
    if !Path::new(STATE_FILE).exists() {
        return None;
    }

    match fs::read_to_string(STATE_FILE) {
        Ok(data) => {
            // Specify the type explicitly
            match serde_json::from_str::<AppState>(&data) {
                Ok(state) => Some(state),
                Err(_) => None,
            }
        }
        Err(_) => None,
    }
}

pub fn save_state(state: &AppState) -> Result<(), String> {
    match serde_json::to_string_pretty(state) {
        Ok(json) => {
            match fs::write(STATE_FILE, json) {
                Ok(_) => Ok(()),
                Err(e) => Err(format!("Failed to write state file: {}", e))
            }
        }
        Err(e) => Err(format!("Failed to serialize state: {}", e))
    }
}