use crate::api::models::{AudioQuality, TrackData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    Stopped,
    Playing,
    Paused,
    Loading,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    Off,
    Track,
    Queue,
}

#[derive(Debug, Clone)]
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
