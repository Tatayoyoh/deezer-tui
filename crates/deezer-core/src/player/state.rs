use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::api::models::{AudioQuality, TrackData};
use crate::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaybackStatus {
    Stopped,
    Playing,
    Paused,
    Loading,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepeatMode {
    Off,
    Track,
    Queue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    pub status: PlaybackStatus,
    pub current_track: Option<TrackData>,
    pub quality: AudioQuality,
    pub position_secs: u64,
    pub duration_secs: u64,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub queue: Vec<TrackData>,
    pub queue_index: usize,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            status: PlaybackStatus::Stopped,
            current_track: None,
            quality: AudioQuality::Mp3_128,
            position_secs: 0,
            duration_secs: 0,
            volume: 0.8,
            shuffle: false,
            repeat: RepeatMode::Off,
            queue: Vec::new(),
            queue_index: 0,
        }
    }
}

impl PlayerState {
    pub fn progress_percent(&self) -> f64 {
        if self.duration_secs == 0 {
            0.0
        } else {
            self.position_secs as f64 / self.duration_secs as f64
        }
    }

    pub fn format_position(&self) -> String {
        format!(
            "{}:{:02} / {}:{:02}",
            self.position_secs / 60,
            self.position_secs % 60,
            self.duration_secs / 60,
            self.duration_secs % 60,
        )
    }
}

/// Serializable snapshot of playback state, saved when going to background.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedState {
    pub current_track: Option<TrackData>,
    pub quality: AudioQuality,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub queue: Vec<TrackData>,
    pub queue_index: usize,
    pub was_playing: bool,
}

impl SavedState {
    fn file_path() -> Option<PathBuf> {
        Config::dir().map(|d| d.join("background_state.json"))
    }

    pub fn from_player_state(state: &PlayerState) -> Self {
        Self {
            current_track: state.current_track.clone(),
            quality: state.quality,
            volume: state.volume,
            shuffle: state.shuffle,
            repeat: state.repeat,
            queue: state.queue.clone(),
            queue_index: state.queue_index,
            was_playing: matches!(state.status, PlaybackStatus::Playing | PlaybackStatus::Paused),
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::file_path() else {
            return Ok(());
        };
        let json = serde_json::to_string(self).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, e)
        })?;
        std::fs::write(path, json)
    }

    pub fn load() -> Option<Self> {
        let path = Self::file_path()?;
        let json = std::fs::read_to_string(&path).ok()?;
        let _ = std::fs::remove_file(&path); // consume it
        serde_json::from_str(&json).ok()
    }
}
