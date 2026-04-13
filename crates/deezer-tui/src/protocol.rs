use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use deezer_core::api::models::{
    AlbumDetail, ArtistDetail, ArtistSubTab, AudioQuality, DisplayItem, PlaylistData,
    PlaylistDetail, TrackData,
};
use deezer_core::offline::OfflineTrack;
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
    /// Seek forward by a number of seconds in the current track.
    SeekForward { secs: u64 },
    /// Seek backward by a number of seconds in the current track.
    SeekBackward { secs: u64 },
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
    /// Switch to next category within current tab.
    NextCategory,
    /// Switch to previous category within current tab.
    PrevCategory,
    /// Play favorites in shuffle mode.
    ShuffleFavorites,
    /// Add a track to favorites.
    AddFavorite { track_id: String },
    /// Remove a track from favorites.
    RemoveFavorite { track_id: String },
    /// Add an artist to favorites.
    AddFavoriteArtist { artist_id: String },
    /// Remove an artist from favorites.
    RemoveFavoriteArtist { artist_id: String },
    /// Add an album to favorites.
    AddFavoriteAlbum { album_id: String },
    /// Remove an album from favorites.
    RemoveFavoriteAlbum { album_id: String },
    /// Request the user's playlists (for playlist picker).
    RequestPlaylists,
    /// Add a track to a playlist.
    AddToPlaylist {
        playlist_id: String,
        track_id: String,
    },
    /// Mark a track as disliked (don't recommend).
    DislikeTrack { track_id: String },
    /// Insert a track to play next in the queue.
    PlayNext { track: TrackData },
    /// Append a track to the end of the queue.
    AddToQueue { track: TrackData },
    /// Start a mix inspired by a track.
    StartMix { track_id: String },
    /// Start Deezer Flow (personalized radio).
    StartFlow,
    /// Jump to and play a specific track in the queue by index.
    PlayFromQueue { index: usize },
    /// Remove a track from the queue by index.
    RemoveFromQueue { index: usize },
    /// Load album detail (tracks, metadata).
    GetAlbumDetail { album_id: String },
    /// Play a track from the album detail view.
    PlayFromAlbum { index: usize },
    /// Load artist detail (top tracks, albums).
    GetArtistDetail { artist_id: String },
    /// Play a track from the artist detail top tracks.
    PlayFromArtist { index: usize },
    /// Open an album from the artist detail albums list.
    OpenArtistAlbum { index: usize },
    /// Load playlist detail (tracks, metadata).
    GetPlaylistDetail { playlist_id: String },
    /// Play a track from the playlist detail view.
    PlayFromPlaylist { index: usize },
    /// Logout — clear ARL, return to login screen.
    Logout,
    /// Load radio stations from Deezer.
    LoadRadios,
    /// Play the selected radio station's tracks.
    PlayFromRadio { index: usize },
    /// Download a track for offline mode.
    DownloadOffline { track: TrackData },
    /// Download an entire album for offline mode.
    DownloadAlbumOffline { album_id: String },
    /// Remove a track from offline storage.
    RemoveOfflineTrack { track_id: String },
    /// Remove an album from offline storage.
    RemoveOfflineAlbum { album_id: String },
    /// Play a track from offline storage.
    PlayFromOffline { index: usize },
    /// Play a track from an offline album (queue = album tracks).
    PlayOfflineAlbum {
        album_id: String,
        track_index: usize,
    },
    /// Push a navigation overlay onto the daemon-side stack.
    PushNavOverlay(NavOverlay),
    /// Pop the top navigation overlay from the daemon-side stack.
    PopNavOverlay,
    /// Clear the entire navigation overlay stack (e.g. when entering a new top-level detail).
    ClearNavOverlayStack,
    /// Graceful shutdown — daemon exits.
    Shutdown,
}

/// Navigation overlays that are persisted in the daemon across client reconnections.
/// Only content-detail views that correspond to loaded daemon data belong here.
/// Pure UI overlays (Help, Settings, WaitingList, etc.) stay client-side only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavOverlay {
    ArtistDetail,
    AlbumDetail { from_artist: bool },
    PlaylistDetail,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Screen {
    #[default]
    Login,
    Main,
}

/// Active tab in main view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ActiveTab {
    #[default]
    Search,
    Favorites,
    Radio,
    Downloads,
}

/// Search category filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SearchCategory {
    #[default]
    Track,
    Artist,
    Album,
    Playlist,
    Podcast,
    Episode,
    Profile,
}

impl SearchCategory {
    pub const ALL: [Self; 7] = [
        Self::Track,
        Self::Artist,
        Self::Album,
        Self::Playlist,
        Self::Podcast,
        Self::Episode,
        Self::Profile,
    ];

    /// API section key used in deezer.pageSearch response.
    pub fn api_key(&self) -> &'static str {
        match self {
            Self::Track => "TRACK",
            Self::Artist => "ARTIST",
            Self::Album => "ALBUM",
            Self::Playlist => "PLAYLIST",
            Self::Podcast => "SHOW",
            Self::Episode => "EPISODE",
            Self::Profile => "USER",
        }
    }

    /// Column width constraints for this category's table.
    pub fn column_widths(&self) -> [ratatui::prelude::Constraint; 5] {
        use ratatui::prelude::Constraint;
        match self {
            Self::Album => [
                Constraint::Length(4),
                Constraint::Percentage(40),
                Constraint::Percentage(30),
                Constraint::Length(0),
                Constraint::Length(10),
            ],
            Self::Artist => [
                Constraint::Length(4),
                Constraint::Percentage(45),
                Constraint::Percentage(25),
                Constraint::Length(0),
                Constraint::Length(0),
            ],
            _ => [
                Constraint::Length(4),
                Constraint::Percentage(35),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Length(6),
            ],
        }
    }

    pub fn next(&self) -> Self {
        let all = Self::ALL;
        let idx = all.iter().position(|c| c == self).unwrap_or(0);
        all[(idx + 1) % all.len()]
    }

    pub fn prev(&self) -> Self {
        let all = Self::ALL;
        let idx = all.iter().position(|c| c == self).unwrap_or(0);
        all[(idx + all.len() - 1) % all.len()]
    }
}

/// Favorites category filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FavoritesCategory {
    #[default]
    RecentlyPlayed,
    Tracks,
    Artists,
    Albums,
    Playlists,
    Following,
}

impl FavoritesCategory {
    pub const ALL: [Self; 6] = [
        Self::RecentlyPlayed,
        Self::Tracks,
        Self::Artists,
        Self::Albums,
        Self::Playlists,
        Self::Following,
    ];

    pub fn next(&self) -> Self {
        let all = Self::ALL;
        let idx = all.iter().position(|c| c == self).unwrap_or(0);
        all[(idx + 1) % all.len()]
    }

    pub fn prev(&self) -> Self {
        let all = Self::ALL;
        let idx = all.iter().position(|c| c == self).unwrap_or(0);
        all[(idx + all.len() - 1) % all.len()]
    }
}

/// Offline content category filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OfflineCategory {
    #[default]
    Tracks,
    Albums,
}

impl OfflineCategory {
    pub const ALL: [Self; 2] = [Self::Tracks, Self::Albums];

    pub fn next(&self) -> Self {
        match self {
            Self::Tracks => Self::Albums,
            Self::Albums => Self::Tracks,
        }
    }

    pub fn prev(&self) -> Self {
        self.next() // Only 2 variants, prev == next
    }
}

/// A radio station item for display.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RadioItem {
    pub id: u64,
    pub title: String,
}

/// Complete state snapshot sent from daemon to client.
/// All fields use `#[serde(default)]` for forward/backward compatibility
/// when daemon and client are running different binary versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonSnapshot {
    #[serde(default)]
    pub screen: Screen,
    #[serde(default)]
    pub active_tab: ActiveTab,

    // Player state
    #[serde(default)]
    pub status: PlaybackStatus,
    #[serde(default)]
    pub current_track: Option<TrackData>,
    #[serde(default)]
    pub quality: AudioQuality,
    #[serde(default)]
    pub position_secs: u64,
    #[serde(default)]
    pub duration_secs: u64,
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default)]
    pub shuffle: bool,
    #[serde(default)]
    pub repeat: RepeatMode,

    // Queue
    #[serde(default)]
    pub queue: Vec<TrackData>,
    #[serde(default)]
    pub queue_index: usize,

    // Search
    #[serde(default)]
    pub search_results: Vec<TrackData>,
    #[serde(default)]
    pub search_selected: usize,
    #[serde(default)]
    pub search_loading: bool,
    #[serde(default)]
    pub search_category: SearchCategory,
    #[serde(default)]
    pub search_display: Vec<DisplayItem>,

    // Favorites
    #[serde(default)]
    pub favorites: Vec<TrackData>,
    #[serde(default)]
    pub favorites_selected: usize,
    #[serde(default)]
    pub favorites_loading: bool,
    #[serde(default)]
    pub favorites_category: FavoritesCategory,
    #[serde(default)]
    pub favorites_display: Vec<DisplayItem>,
    /// IDs of artists in the user's favorites.
    #[serde(default)]
    pub favorite_artist_ids: Vec<String>,
    /// IDs of albums in the user's favorites.
    #[serde(default)]
    pub favorite_album_ids: Vec<String>,

    // Offline
    #[serde(default)]
    pub offline_category: OfflineCategory,
    #[serde(default)]
    pub offline_tracks: Vec<OfflineTrack>,
    #[serde(default)]
    pub offline_albums: Vec<AlbumDetail>,
    #[serde(default)]
    pub offline_selected: usize,
    #[serde(default)]
    pub offline_loading: bool,
    /// IDs of tracks available offline (for UI indicators in other tabs).
    #[serde(default)]
    pub offline_track_ids: Vec<String>,

    // Radios
    #[serde(default)]
    pub radios: Vec<RadioItem>,
    #[serde(default)]
    pub radios_selected: usize,
    #[serde(default)]
    pub radios_loading: bool,

    // Playlists (for playlist picker in popup menu)
    #[serde(default)]
    pub playlists: Vec<PlaylistData>,

    // Album detail
    #[serde(default)]
    pub album_detail: Option<AlbumDetail>,
    #[serde(default)]
    pub album_detail_selected: usize,
    #[serde(default)]
    pub album_detail_loading: bool,

    // Artist detail
    #[serde(default)]
    pub artist_detail: Option<ArtistDetail>,
    #[serde(default)]
    pub artist_detail_selected: usize,
    #[serde(default)]
    pub artist_detail_loading: bool,
    #[serde(default)]
    pub artist_detail_sub_tab: ArtistSubTab,

    // Playlist detail
    #[serde(default)]
    pub playlist_detail: Option<PlaylistDetail>,
    #[serde(default)]
    pub playlist_detail_selected: usize,
    #[serde(default)]
    pub playlist_detail_loading: bool,

    // Navigation overlay stack (persisted across reconnections)
    #[serde(default)]
    pub nav_overlay: Option<NavOverlay>,
    #[serde(default)]
    pub nav_overlay_stack: Vec<NavOverlay>,

    // UI hints
    #[serde(default)]
    pub status_msg: Option<String>,
    #[serde(default)]
    pub login_error: Option<String>,
    #[serde(default)]
    pub login_loading: bool,
    #[serde(default)]
    pub user_name: Option<String>,
    #[serde(default)]
    pub is_offline: bool,
}

fn default_volume() -> f32 {
    0.8
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
            search_category: SearchCategory::default(),
            search_display: Vec::new(),
            favorites: Vec::new(),
            favorites_selected: 0,
            favorites_loading: false,
            favorites_category: FavoritesCategory::default(),
            favorites_display: Vec::new(),
            favorite_artist_ids: Vec::new(),
            favorite_album_ids: Vec::new(),
            offline_category: OfflineCategory::default(),
            offline_tracks: Vec::new(),
            offline_albums: Vec::new(),
            offline_selected: 0,
            offline_loading: false,
            offline_track_ids: Vec::new(),
            radios: Vec::new(),
            radios_selected: 0,
            radios_loading: false,
            playlists: Vec::new(),
            album_detail: None,
            album_detail_selected: 0,
            album_detail_loading: false,
            artist_detail: None,
            artist_detail_selected: 0,
            artist_detail_loading: false,
            artist_detail_sub_tab: ArtistSubTab::default(),
            playlist_detail: None,
            playlist_detail_selected: 0,
            playlist_detail_loading: false,
            nav_overlay: None,
            nav_overlay_stack: Vec::new(),
            status_msg: None,
            login_error: None,
            login_loading: false,
            user_name: None,
            is_offline: false,
        }
    }
}

/// Get the Unix socket path for daemon IPC.
pub fn socket_path() -> PathBuf {
    deezer_core::Config::dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("daemon.sock")
}

/// Get the PID file path for the daemon process.
pub fn pid_path() -> PathBuf {
    deezer_core::Config::dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("daemon.pid")
}

/// Send a line-delimited JSON message over a Unix stream.
pub async fn send_line<T: Serialize>(stream: &mut UnixStream, msg: &T) -> std::io::Result<()> {
    let mut json = serde_json::to_string(msg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
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
    let msg = serde_json::from_str(&line)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(Some(msg))
}
