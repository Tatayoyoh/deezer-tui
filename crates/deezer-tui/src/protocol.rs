use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use deezer_core::api::models::{AudioQuality, TrackData};
use deezer_core::player::state::{PlaybackStatus, RepeatMode};

/// Commands sent from the TUI client to the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    /// Request a full state snapshot.
    GetSnapshot,
    /// Login with ARL token.
    Login { arl: String },
    /// Search for tracks.
    Search { query: String },
    /// Play track at index from search results.
    PlayFromSearch { index: usize },
    /// Play track at index from favorites.
    PlayFromFavorites { index: usize },
    /// Toggle play/pause.
    TogglePause,
    /// Next track in queue.
    NextTrack,
    /// Previous track in queue.
    PrevTrack,
    /// Set volume (0.0 - 1.0).
    SetVolume { volume: f32 },
    /// Toggle shuffle mode.
    ToggleShuffle,
    /// Cycle repeat mode (Off -> Queue -> Track -> Off).
    CycleRepeat,
    /// Load favorites from Deezer.
    LoadFavorites,
    /// Navigate list selection up.
    SelectUp,
    /// Navigate list selection down.
    SelectDown,
    /// Switch to next tab.
    NextTab,
    /// Switch to previous tab.
    PrevTab,
    /// Graceful shutdown — daemon exits.
    Shutdown,
}

/// Messages sent from the daemon to the TUI client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Full state snapshot.
    Snapshot(DaemonSnapshot),
    /// Error message.
    Error(String),
}

/// Screen the daemon is on (login vs main).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Screen {
    Login,
    Main,
}

/// Active tab in main view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActiveTab {
    Search,
    Favorites,
    Radio,
}

/// Complete state snapshot sent from daemon to client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonSnapshot {
    pub screen: Screen,
    pub active_tab: ActiveTab,

    // Player state
    pub status: PlaybackStatus,
    pub current_track: Option<TrackData>,
    pub quality: AudioQuality,
    pub position_secs: u64,
    pub duration_secs: u64,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: RepeatMode,

    // Queue
    pub queue: Vec<TrackData>,
    pub queue_index: usize,

    // Search
    pub search_results: Vec<TrackData>,
    pub search_selected: usize,
    pub search_loading: bool,

    // Favorites
    pub favorites: Vec<TrackData>,
    pub favorites_selected: usize,
    pub favorites_loading: bool,

    // UI hints
    pub status_msg: Option<String>,
    pub login_error: Option<String>,
    pub login_loading: bool,
    pub user_name: Option<String>,
}

impl Default for DaemonSnapshot {
    fn default() -> Self {
        Self {
            screen: Screen::Login,
            active_tab: ActiveTab::Search,
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
            search_results: Vec::new(),
            search_selected: 0,
            search_loading: false,
            favorites: Vec::new(),
            favorites_selected: 0,
            favorites_loading: false,
            status_msg: None,
            login_error: None,
            login_loading: false,
            user_name: None,
        }
    }
}

/// Get the Unix socket path for daemon IPC.
pub fn socket_path() -> PathBuf {
    deezer_core::Config::dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("daemon.sock")
}

/// Send a line-delimited JSON message over a Unix stream.
pub async fn send_line<T: Serialize>(stream: &mut UnixStream, msg: &T) -> std::io::Result<()> {
    let mut json = serde_json::to_string(msg).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, e)
    })?;
    json.push('\n');
    stream.write_all(json.as_bytes()).await?;
    stream.flush().await
}

/// Read a line-delimited JSON message from a buffered reader.
pub async fn read_line<T: for<'de> Deserialize<'de>, R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> std::io::Result<Option<T>> {
    let mut line = String::new();
    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        return Ok(None); // EOF — peer disconnected
    }
    let msg = serde_json::from_str(&line).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, e)
    })?;
    Ok(Some(msg))
}
