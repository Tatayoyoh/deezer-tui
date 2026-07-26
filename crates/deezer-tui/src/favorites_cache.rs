//! Persistent disk cache for favorites data.
//!
//! Avoids re-fetching from the Deezer API every time the user switches
//! between favorites sub-tabs. The cache is stored as a JSON file in the
//! XDG config directory (`~/.config/deezer-tui/favorites_cache.json`).
//!
//! # Invalidation rules
//! - Tracks cache   → invalidated on `FavoriteAdded` / `FavoriteRemoved`
//! - Artists cache  → invalidated on `FavoriteArtistAdded` / `FavoriteArtistRemoved`
//! - Albums cache   → invalidated on `FavoriteAlbumAdded` / `FavoriteAlbumRemoved`
//! - Playlists list → invalidated on `AddedToPlaylist` (playlist list may have changed)
//! - Playlist detail → invalidated for the specific playlist on `AddedToPlaylist`
//! - RecentlyPlayed → never cached (changes after every track play)
//! - Following      → invalidated explicitly (no in-app mutation currently)

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use deezer_core::api::models::{DisplayItem, PlaylistDetail, TrackData};
use deezer_core::Config;

use crate::protocol::MoodEntry;

/// Current cache format. Bump to discard every on-disk cache written by an
/// older build — used when a bug could have stored wrong data under a key.
const CACHE_VERSION: u32 = 2;

/// All cached favorites data, persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FavoritesCache {
    /// Format version of this cache file; mismatched files are discarded.
    #[serde(default)]
    pub version: u32,
    /// Favorite tracks (FavoritesCategory::Tracks).
    #[serde(default)]
    pub tracks: Option<Vec<TrackData>>,
    /// Favorite artists as display items (FavoritesCategory::Artists).
    #[serde(default)]
    pub artists: Option<Vec<DisplayItem>>,
    /// Favorite albums as display items (FavoritesCategory::Albums).
    #[serde(default)]
    pub albums: Option<Vec<DisplayItem>>,
    /// User playlists as display items (FavoritesCategory::Playlists).
    #[serde(default)]
    pub playlists: Option<Vec<DisplayItem>>,
    /// Following as display items (FavoritesCategory::Following).
    #[serde(default)]
    pub following: Option<Vec<DisplayItem>>,
    /// Individual playlist details, keyed by playlist_id.
    #[serde(default)]
    pub playlist_details: HashMap<String, PlaylistDetail>,
    /// Explore > Moods list. Rarely changes — cached forever, refreshed on login.
    #[serde(default)]
    pub moods: Option<Vec<MoodEntry>>,
}

impl FavoritesCache {
    /// File path: `<config_dir>/favorites_cache.json`.
    fn path() -> Option<std::path::PathBuf> {
        Config::dir().map(|d| d.join("favorites_cache.json"))
    }

    /// An empty cache stamped with the current format version.
    fn empty() -> Self {
        Self {
            version: CACHE_VERSION,
            ..Self::default()
        }
    }

    /// Load from disk. Returns an empty cache on any error, or when the file
    /// was written by an older, incompatible cache format.
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::empty();
        };
        let cache = match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str::<Self>(&content).unwrap_or_default(),
            Err(_) => return Self::empty(),
        };
        if cache.version == CACHE_VERSION {
            cache
        } else {
            Self::empty()
        }
    }

    /// Persist to disk (best-effort; errors are silently ignored).
    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut to_write = self.clone();
        to_write.version = CACHE_VERSION;
        if let Ok(content) = serde_json::to_string(&to_write) {
            let _ = std::fs::write(path, content);
        }
    }

    // ── Invalidation helpers ──────────────────────────────────────────────

    pub fn invalidate_tracks(&mut self) {
        self.tracks = None;
    }

    pub fn invalidate_artists(&mut self) {
        self.artists = None;
    }

    pub fn invalidate_albums(&mut self) {
        self.albums = None;
    }

    /// Invalidate the playlists list and, optionally, a specific playlist's detail.
    pub fn invalidate_playlists(&mut self, playlist_id: Option<&str>) {
        self.playlists = None;
        if let Some(id) = playlist_id {
            self.playlist_details.remove(id);
        }
    }
}
