use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::api::models::{AlbumDetail, AudioQuality, DeezerError, TrackData};
use crate::Config;

/// Metadata for a single offline track.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflineTrack {
    pub track: TrackData,
    pub quality: AudioQuality,
}

/// Persistent index of all offline content.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OfflineIndex {
    #[serde(default)]
    pub tracks: Vec<OfflineTrack>,
    #[serde(default)]
    pub albums: Vec<AlbumDetail>,
}

impl OfflineIndex {
    /// Directory where offline data is stored.
    pub fn dir() -> Option<PathBuf> {
        Config::data_dir().map(|d| d.join("offline"))
    }

    /// Path to the index JSON file.
    fn index_path() -> Option<PathBuf> {
        Self::dir().map(|d| d.join("index.json"))
    }

    /// Directory for audio files.
    fn tracks_dir() -> Option<PathBuf> {
        Self::dir().map(|d| d.join("tracks"))
    }

    /// Load the offline index from disk, or return empty if not found.
    pub fn load() -> Self {
        let Some(path) = Self::index_path() else {
            return Self::default();
        };
        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save the index to disk.
    pub fn save(&self) -> Result<(), DeezerError> {
        let dir = Self::dir()
            .ok_or_else(|| DeezerError::Api("Could not determine data directory".into()))?;
        fs::create_dir_all(&dir)
            .map_err(|e| DeezerError::Api(format!("Failed to create offline dir: {e}")))?;

        let path = dir.join("index.json");
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| DeezerError::Api(format!("Failed to serialize offline index: {e}")))?;
        fs::write(&path, content)
            .map_err(|e| DeezerError::Api(format!("Failed to write offline index: {e}")))?;
        Ok(())
    }

    /// Save audio data to disk for a track.
    pub fn save_track_audio(track_id: &str, audio_data: &[u8]) -> Result<(), DeezerError> {
        let dir = Self::tracks_dir()
            .ok_or_else(|| DeezerError::Api("Could not determine data directory".into()))?;
        fs::create_dir_all(&dir)
            .map_err(|e| DeezerError::Api(format!("Failed to create tracks dir: {e}")))?;

        let path = dir.join(format!("{track_id}.audio"));
        fs::write(&path, audio_data)
            .map_err(|e| DeezerError::Api(format!("Failed to write track audio: {e}")))?;
        Ok(())
    }

    /// Load audio data from disk for a track.
    pub fn load_track_audio(track_id: &str) -> Result<Vec<u8>, DeezerError> {
        let dir = Self::tracks_dir()
            .ok_or_else(|| DeezerError::Api("Could not determine data directory".into()))?;
        let path = dir.join(format!("{track_id}.audio"));
        fs::read(&path).map_err(|e| DeezerError::Api(format!("Failed to read track audio: {e}")))
    }

    /// Remove a track's audio file from disk.
    fn remove_track_file(track_id: &str) {
        if let Some(dir) = Self::tracks_dir() {
            let _ = fs::remove_file(dir.join(format!("{track_id}.audio")));
        }
    }

    /// Check if a track is in the offline index.
    pub fn has_track(&self, track_id: &str) -> bool {
        self.tracks.iter().any(|t| t.track.track_id == track_id)
    }

    /// Add a track to the index (no-op if already present).
    pub fn add_track(&mut self, track: TrackData, quality: AudioQuality) {
        if !self.has_track(&track.track_id) {
            self.tracks.push(OfflineTrack { track, quality });
        }
    }

    /// Remove a track from the index and delete its audio file.
    pub fn remove_track(&mut self, track_id: &str) {
        Self::remove_track_file(track_id);
        self.tracks.retain(|t| t.track.track_id != track_id);
    }

    /// Check if an album is in the offline index.
    pub fn has_album(&self, album_id: &str) -> bool {
        self.albums.iter().any(|a| a.album_id == album_id)
    }

    /// Add an album to the index (no-op if already present).
    pub fn add_album(&mut self, album: AlbumDetail) {
        if !self.has_album(&album.album_id) {
            self.albums.push(album);
        }
    }

    /// Remove an album and all its tracks from the index and disk.
    pub fn remove_album(&mut self, album_id: &str) {
        if let Some(album) = self.albums.iter().find(|a| a.album_id == album_id) {
            for track in &album.tracks {
                Self::remove_track_file(&track.track_id);
                self.tracks.retain(|t| t.track.track_id != track.track_id);
            }
        }
        self.albums.retain(|a| a.album_id != album_id);
    }

    /// Get all offline track IDs (for UI indicators).
    pub fn track_ids(&self) -> Vec<String> {
        self.tracks
            .iter()
            .map(|t| t.track.track_id.clone())
            .collect()
    }
}
