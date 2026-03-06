use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::api::models::{AudioQuality, DeezerError};

const APP_QUALIFIER: &str = "com";
const APP_ORGANIZATION: &str = "deezer-tui";
const APP_NAME: &str = "deezer-tui";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub arl: Option<String>,
    #[serde(default = "default_quality")]
    pub quality: AudioQuality,
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default)]
    pub theme: Option<String>,
}

fn default_quality() -> AudioQuality {
    AudioQuality::Mp3_128
}

fn default_volume() -> f32 {
    0.8
}

impl Default for Config {
    fn default() -> Self {
        Self {
            arl: None,
            quality: default_quality(),
            volume: default_volume(),
            theme: None,
        }
    }
}

impl Config {
    /// Get the config directory path (XDG on Linux, AppData on Windows, etc.)
    pub fn dir() -> Option<PathBuf> {
        ProjectDirs::from(APP_QUALIFIER, APP_ORGANIZATION, APP_NAME).map(|p| p.config_dir().to_path_buf())
    }

    /// Full path to the config file.
    pub fn path() -> Option<PathBuf> {
        Self::dir().map(|d| d.join("config.json"))
    }

    /// Load config from disk, or return default if not found.
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };

        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save config to disk.
    pub fn save(&self) -> Result<(), DeezerError> {
        let dir = Self::dir().ok_or_else(|| {
            DeezerError::Api("Could not determine config directory".into())
        })?;

        fs::create_dir_all(&dir).map_err(|e| {
            DeezerError::Api(format!("Failed to create config dir: {e}"))
        })?;

        let path = dir.join("config.json");
        let content = serde_json::to_string_pretty(self).map_err(|e| {
            DeezerError::Api(format!("Failed to serialize config: {e}"))
        })?;

        fs::write(&path, content).map_err(|e| {
            DeezerError::Api(format!("Failed to write config: {e}"))
        })?;

        Ok(())
    }
}
