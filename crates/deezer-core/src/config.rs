use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::api::models::{AudioQuality, DeezerError};
use crate::player::eq::EqSettings;

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
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub skip_update_check: bool,
    /// Background transparency in percent (0–100, steps of 10). 0 = opaque, 100 = fully transparent.
    #[serde(default)]
    pub bg_transparency: u8,
    /// Enable vim-style navigation keys (h, j, k, l). Disabled by default.
    #[serde(default)]
    pub vim_keys: bool,
    /// Graphic equalizer settings (disabled by default).
    #[serde(default)]
    pub equalizer: EqSettings,
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
            language: None,
            skip_update_check: false,
            bg_transparency: 0,
            vim_keys: false,
            equalizer: EqSettings::default(),
        }
    }
}

impl Config {
    /// Get the config directory path (XDG on Linux, AppData on Windows, etc.)
    pub fn dir() -> Option<PathBuf> {
        ProjectDirs::from(APP_QUALIFIER, APP_ORGANIZATION, APP_NAME)
            .map(|p| p.config_dir().to_path_buf())
    }

    /// Get the local data directory (XDG data on Linux, AppData on Windows, etc.)
    /// Used for offline track storage.
    pub fn data_dir() -> Option<PathBuf> {
        ProjectDirs::from(APP_QUALIFIER, APP_ORGANIZATION, APP_NAME)
            .map(|p| p.data_local_dir().to_path_buf())
    }

    /// Full path to the config file.
    pub fn path() -> Option<PathBuf> {
        Self::dir().map(|d| d.join("config.json"))
    }

    /// Load config from disk, or return default if not found.
    pub fn load() -> Self {
        match Self::path() {
            Some(path) => Self::load_from(&path),
            None => Self::default(),
        }
    }

    /// Save config to disk.
    pub fn save(&self) -> Result<(), DeezerError> {
        self.save_to(&Self::config_path()?)
    }

    /// Change some settings on disk: reload the file, apply `edit`, save.
    ///
    /// The daemon and the client both write `config.json`, each its own
    /// fields (volume, quality… for the daemon; theme, language… for the
    /// client). Saving a copy loaded earlier would put back the other
    /// process's stale values, so every change goes through the file.
    pub fn update(edit: impl FnOnce(&mut Config)) -> Result<Config, DeezerError> {
        Self::update_at(&Self::config_path()?, edit)
    }

    fn config_path() -> Result<PathBuf, DeezerError> {
        Self::path().ok_or_else(|| DeezerError::Api("Could not determine config directory".into()))
    }

    fn load_from(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    fn update_at(path: &Path, edit: impl FnOnce(&mut Config)) -> Result<Config, DeezerError> {
        let mut config = Self::load_from(path);
        edit(&mut config);
        config.save_to(path)?;
        Ok(config)
    }

    /// Write through a temporary file renamed over the config: a reader in the
    /// other process never sees a truncated file (which would parse as the
    /// default config, and lose the ARL on its next save).
    fn save_to(&self, path: &Path) -> Result<(), DeezerError> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .map_err(|e| DeezerError::Api(format!("Failed to create config dir: {e}")))?;
        }

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| DeezerError::Api(format!("Failed to serialize config: {e}")))?;

        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        fs::write(&tmp, content)
            .and_then(|()| fs::rename(&tmp, path))
            .map_err(|e| {
                let _ = fs::remove_file(&tmp);
                DeezerError::Api(format!("Failed to write config: {e}"))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "deezer-tui-config-test-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir.join("config.json")
    }

    #[test]
    fn update_keeps_fields_written_by_another_process() {
        let path = temp_config("update");
        // The daemon loaded its copy at startup…
        let daemon_copy = Config::default();
        daemon_copy.save_to(&path).unwrap();

        // …then the client changed a setting of its own…
        Config::update_at(&path, |c| c.vim_keys = true).unwrap();

        // …and the daemon now saves the volume: the client's change survives.
        let saved = Config::update_at(&path, |c| c.volume = 0.3).unwrap();
        assert!(saved.vim_keys);
        assert_eq!(saved.volume, 0.3);

        let on_disk = Config::load_from(&path);
        assert!(on_disk.vim_keys);
        assert_eq!(on_disk.volume, 0.3);
        assert!(
            !daemon_copy.vim_keys,
            "the stale copy is not what got saved"
        );
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn save_leaves_no_temporary_file_behind() {
        let path = temp_config("atomic");
        Config::default().save_to(&path).unwrap();
        Config::update_at(&path, |c| c.arl = Some("arl".into())).unwrap();

        let files: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(files, ["config.json"]);
        assert_eq!(Config::load_from(&path).arl.as_deref(), Some("arl"));
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
