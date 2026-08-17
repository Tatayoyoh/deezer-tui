use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::cursor::Show;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::widgets::TableState;
use ratatui::Terminal;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tracing::debug;

use deezer_core::api::models::{
    AlbumDetail, ArtistDetail, ArtistSubTab, AudioQuality, DisplayItem, PlaylistData,
    PlaylistDetail, TrackData,
};
use deezer_core::config::Config;
use deezer_core::player::state::{PlaybackStatus, RepeatMode};

use crate::i18n::{self, t, Locale};
use deezer_core::offline::OfflineTrack;

use crate::protocol::{
    pid_path, read_line, socket_path, ActiveTab, Command, DaemonSnapshot, ExploreCategory,
    FavoritesCategory, GenreDetailSubTab, GenreItem, MoodEntry, NavOverlay, OfflineCategory,
    RadioItem, Screen, SearchCategory, ServerMessage,
};
use crate::theme::{Theme, ThemeId};
use crate::ui;
use deezer_core::api::models::GenreDetail;

const TICK_RATE: Duration = Duration::from_millis(50);

/// Two left clicks on the same row within this delay count as a double click.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// Upper bound on the steps taken to walk a modal's cursor to a clicked option.
const MAX_MODAL_OPTIONS: usize = 64;

/// Input mode for the client (typing in search/login vs normal navigation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Typing,
}

/// Sub-mode for the login screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginMode {
    /// Default: shows a "Login" button (Enter = browser login, w = ARL input).
    Button,
    /// ARL text input (Enter = submit ARL, Esc = back to Button).
    ArlInput,
}

// --- Popup menu types ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopupAction {
    Header,
    ToggleFavorite,
    ToggleFavoriteArtist,
    ToggleFavoriteAlbum,
    AddToPlaylist,
    RemoveFromPlaylist,
    DownloadOffline,
    /// Download a whole album/playlist for offline mode.
    DownloadContainerOffline,
    /// Drop the targeted track/album/playlist from offline storage.
    RemoveOffline,
    DislikeTrack,
    PlayNext,
    AddToQueue,
    MixFromTrack,
    Share,
    TrackInfo,
    ViewAlbum,
    ViewArtist,
    RenamePlaylist,
    DeletePlaylist,
}

/// What the popup menu is operating on.
#[derive(Debug, Clone)]
pub enum PopupTarget {
    /// Boxed: `TrackData` dwarfs the other variants.
    Track(Box<TrackData>),
    Artist {
        artist_id: String,
        name: String,
    },
    Album {
        album_id: String,
        title: String,
        artist: String,
    },
    Playlist {
        playlist_id: String,
        title: String,
        nb_songs: u64,
    },
}

#[derive(Debug, Clone)]
pub struct PopupMenuItem {
    pub label: String,
    pub action: PopupAction,
    pub is_header: bool,
}

#[derive(Debug, Clone)]
pub enum SubMenu {
    PlaylistPicker {
        playlists: Vec<PlaylistData>,
        selected: usize,
        loading: bool,
        /// Fuzzy filter text typed with `/`; empty means unfiltered.
        filter_input: String,
        /// Whether the filter input box is currently capturing keystrokes.
        filter_typing: bool,
    },
    TrackInfo,
    /// Inline text input shown inside the playlist picker for creating a new
    /// playlist + adding the current track to it.
    CreatePlaylistInput {
        name: String,
        cursor: usize,
    },
    /// Modal text input shown for renaming a playlist (popup target = Playlist).
    RenamePlaylistInput {
        name: String,
        cursor: usize,
    },
    /// Yes/no confirmation for deleting a non-empty playlist.
    /// `confirm_yes` tracks which button is highlighted (true = Yes, false = No).
    ConfirmDeletePlaylist {
        confirm_yes: bool,
    },
}

#[derive(Debug, Clone)]
pub struct PopupMenu {
    pub title: Option<String>,
    pub items: Vec<PopupMenuItem>,
    pub selected: usize,
    pub target: PopupTarget,
    pub is_favorite: bool,
    pub sub_menu: Option<SubMenu>,
    /// When the popup is opened from inside a playlist detail view, this carries
    /// the playlist's id so actions like `RemoveFromPlaylist` know the context.
    pub playlist_context: Option<String>,
}

impl PopupMenu {
    /// Get the track if this popup targets a track.
    pub fn track(&self) -> Option<&TrackData> {
        match &self.target {
            PopupTarget::Track(t) => Some(t),
            _ => None,
        }
    }

    /// Build the full menu (for `x` key on a selected track in a list).
    fn full(track: TrackData, is_favorite: bool) -> Self {
        let s = t();
        let fav_label = if is_favorite {
            s.remove_from_favorites
        } else {
            s.add_to_favorites
        };
        let items = vec![
            PopupMenuItem {
                label: s.menu_manage.into(),
                action: PopupAction::Header,
                is_header: true,
            },
            PopupMenuItem {
                label: fav_label.into(),
                action: PopupAction::ToggleFavorite,
                is_header: false,
            },
            PopupMenuItem {
                label: s.add_to_playlist.into(),
                action: PopupAction::AddToPlaylist,
                is_header: false,
            },
            PopupMenuItem {
                label: s.download_for_offline.into(),
                action: PopupAction::DownloadOffline,
                is_header: false,
            },
            PopupMenuItem {
                label: s.dont_recommend.into(),
                action: PopupAction::DislikeTrack,
                is_header: false,
            },
            PopupMenuItem {
                label: s.menu_playback.into(),
                action: PopupAction::Header,
                is_header: true,
            },
            PopupMenuItem {
                label: s.play_next.into(),
                action: PopupAction::PlayNext,
                is_header: false,
            },
            PopupMenuItem {
                label: s.add_to_queue.into(),
                action: PopupAction::AddToQueue,
                is_header: false,
            },
            PopupMenuItem {
                label: s.mix_inspired.into(),
                action: PopupAction::MixFromTrack,
                is_header: false,
            },
            PopupMenuItem {
                label: s.menu_media.into(),
                action: PopupAction::Header,
                is_header: true,
            },
            PopupMenuItem {
                label: s.track_album.into(),
                action: PopupAction::ViewAlbum,
                is_header: false,
            },
            PopupMenuItem {
                label: s.track_artist.into(),
                action: PopupAction::ViewArtist,
                is_header: false,
            },
            PopupMenuItem {
                label: s.share.into(),
                action: PopupAction::Share,
                is_header: false,
            },
            PopupMenuItem {
                label: s.track_info.into(),
                action: PopupAction::TrackInfo,
                is_header: false,
            },
        ];
        Self {
            title: None,
            items,
            selected: 1, // First selectable item
            target: PopupTarget::Track(Box::new(track)),
            is_favorite,
            sub_menu: None,
            playlist_context: None,
        }
    }

    /// Like `full`, but for a track displayed inside a playlist detail view.
    /// "Add to playlist" is replaced by "Remove from this playlist".
    fn full_in_playlist(track: TrackData, is_favorite: bool, playlist_id: String) -> Self {
        let mut menu = Self::full(track, is_favorite);
        for item in menu.items.iter_mut() {
            if matches!(item.action, PopupAction::AddToPlaylist) {
                item.label = t().remove_from_playlist.into();
                item.action = PopupAction::RemoveFromPlaylist;
            }
        }
        menu.playlist_context = Some(playlist_id);
        menu
    }

    /// Build the manage-only menu (for `Ctrl+Space` on currently playing track).
    fn manage_only(track: TrackData, is_favorite: bool) -> Self {
        let s = t();
        let fav_label = if is_favorite {
            s.remove_from_favorites
        } else {
            s.add_to_favorites
        };
        let title = format!("{} — {}", track.title, track.artist);
        let items = vec![
            PopupMenuItem {
                label: s.menu_manage.into(),
                action: PopupAction::Header,
                is_header: true,
            },
            PopupMenuItem {
                label: fav_label.into(),
                action: PopupAction::ToggleFavorite,
                is_header: false,
            },
            PopupMenuItem {
                label: s.add_to_playlist.into(),
                action: PopupAction::AddToPlaylist,
                is_header: false,
            },
            PopupMenuItem {
                label: s.download_for_offline.into(),
                action: PopupAction::DownloadOffline,
                is_header: false,
            },
            PopupMenuItem {
                label: s.dont_recommend.into(),
                action: PopupAction::DislikeTrack,
                is_header: false,
            },
            PopupMenuItem {
                label: s.track_info.into(),
                action: PopupAction::TrackInfo,
                is_header: false,
            },
        ];
        Self {
            title: Some(title),
            items,
            selected: 1,
            target: PopupTarget::Track(Box::new(track)),
            is_favorite,
            sub_menu: None,
            playlist_context: None,
        }
    }

    /// Build a context menu for an artist item.
    fn for_artist(artist_id: String, name: String, is_favorite: bool) -> Self {
        let s = t();
        let fav_label = if is_favorite {
            s.remove_from_favorites
        } else {
            s.add_to_favorites
        };
        let items = vec![
            PopupMenuItem {
                label: s.menu_manage.into(),
                action: PopupAction::Header,
                is_header: true,
            },
            PopupMenuItem {
                label: fav_label.into(),
                action: PopupAction::ToggleFavoriteArtist,
                is_header: false,
            },
            PopupMenuItem {
                label: s.track_artist.into(),
                action: PopupAction::ViewArtist,
                is_header: false,
            },
        ];
        Self {
            title: Some(format!(" {} ", name)),
            items,
            selected: 1,
            target: PopupTarget::Artist { artist_id, name },
            is_favorite,
            sub_menu: None,
            playlist_context: None,
        }
    }

    /// Build a context menu for an album item.
    fn for_album(album_id: String, title: String, artist: String, is_favorite: bool) -> Self {
        let s = t();
        let fav_label = if is_favorite {
            s.remove_from_favorites
        } else {
            s.add_to_favorites
        };
        let items = vec![
            PopupMenuItem {
                label: s.menu_manage.into(),
                action: PopupAction::Header,
                is_header: true,
            },
            PopupMenuItem {
                label: fav_label.into(),
                action: PopupAction::ToggleFavoriteAlbum,
                is_header: false,
            },
            PopupMenuItem {
                label: s.album_page.into(),
                action: PopupAction::ViewAlbum,
                is_header: false,
            },
        ];
        Self {
            title: Some(format!(" {} — {} ", title, artist)),
            items,
            selected: 1,
            target: PopupTarget::Album {
                album_id,
                title,
                artist,
            },
            is_favorite,
            sub_menu: None,
            playlist_context: None,
        }
    }

    /// Build a context menu for a playlist item.
    fn for_playlist(playlist_id: String, title: String, nb_songs: u64) -> Self {
        let s = t();
        let items = vec![
            PopupMenuItem {
                label: s.menu_manage.into(),
                action: PopupAction::Header,
                is_header: true,
            },
            PopupMenuItem {
                label: s.rename_playlist.into(),
                action: PopupAction::RenamePlaylist,
                is_header: false,
            },
            PopupMenuItem {
                label: s.delete_playlist.into(),
                action: PopupAction::DeletePlaylist,
                is_header: false,
            },
            PopupMenuItem {
                label: s.download_for_offline.into(),
                action: PopupAction::DownloadContainerOffline,
                is_header: false,
            },
        ];
        Self {
            title: Some(format!(" {} ", title)),
            items,
            selected: 1,
            target: PopupTarget::Playlist {
                playlist_id,
                title,
                nb_songs,
            },
            is_favorite: false,
            sub_menu: None,
            playlist_context: None,
        }
    }

    /// Menu shown in offline mode: the only thing that can be done to a
    /// downloaded track, album or playlist is dropping it from storage.
    fn remove_offline_only(target: PopupTarget) -> Self {
        let s = t();
        let title = match &target {
            PopupTarget::Track(track) => format!(" {} — {} ", track.title, track.artist),
            PopupTarget::Album { title, artist, .. } => format!(" {title} — {artist} "),
            PopupTarget::Playlist { title, .. } => format!(" {title} "),
            PopupTarget::Artist { name, .. } => format!(" {name} "),
        };
        let items = vec![
            PopupMenuItem {
                label: s.menu_manage.into(),
                action: PopupAction::Header,
                is_header: true,
            },
            PopupMenuItem {
                label: s.remove_from_offline.into(),
                action: PopupAction::RemoveOffline,
                is_header: false,
            },
        ];
        Self {
            title: Some(title),
            items,
            selected: 1,
            target,
            is_favorite: false,
            sub_menu: None,
            playlist_context: None,
        }
    }

    /// Move selection to next selectable item.
    fn select_next(&mut self) {
        let len = self.items.len();
        let mut next = self.selected + 1;
        for _ in 0..len {
            if next >= len {
                next = 0;
            }
            if !self.items[next].is_header {
                self.selected = next;
                return;
            }
            next += 1;
        }
    }

    /// Move selection to previous selectable item.
    fn select_prev(&mut self) {
        let len = self.items.len();
        let mut prev = if self.selected == 0 {
            len - 1
        } else {
            self.selected - 1
        };
        for _ in 0..len {
            if !self.items[prev].is_header {
                self.selected = prev;
                return;
            }
            prev = if prev == 0 { len - 1 } else { prev - 1 };
        }
    }
}

/// Overlay menus independent from track popups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overlay {
    /// Keyboard shortcuts help screen.
    Help { scroll: usize },
    /// Settings menu with selectable entries.
    Settings { selected: usize },
    /// Theme picker.
    ThemePicker { selected: usize },
    /// Audio quality picker.
    QualityPicker { selected: usize },
    /// Language picker.
    LanguagePicker { selected: usize },
    /// Album detail view. `from_artist` is true when opened from the artist detail page.
    AlbumDetail { from_artist: bool },
    /// Artist detail view.
    ArtistDetail,
    /// Playlist detail modal.
    PlaylistDetail { selected: usize },
    /// Podcast show episodes modal. Reads the same `playlist_detail` state as
    /// `PlaylistDetail`, minus the playlist-only actions.
    ShowDetail { selected: usize },
    /// Genre/category detail page.
    GenreDetail {
        sub_tab: GenreDetailSubTab,
        selected: usize,
    },
    /// Waiting list (upcoming tracks in queue).
    WaitingList { selected: usize },
    /// Track list of a downloaded album or playlist (Offline tab), as a modal.
    OfflineDetail {
        /// True when the target is a playlist, false for an album.
        playlist: bool,
        /// Index into `offline_playlists` / `offline_albums`.
        index: usize,
        /// Cursor within the modal's (filtered) track list.
        selected: usize,
    },
    /// Application info modal.
    Info,
    /// Update available dialog with 3 options.
    UpdateAvailable {
        version: String,
        download_url: String,
        selected: usize,
    },
    /// Update in progress (downloading/installing).
    Updating {
        version: String,
        progress_msg: String,
    },
}

impl NavOverlay {
    /// Convert a daemon-side NavOverlay into a client-side Overlay.
    fn to_overlay(self) -> Overlay {
        match self {
            NavOverlay::ArtistDetail => Overlay::ArtistDetail,
            NavOverlay::AlbumDetail { from_artist } => Overlay::AlbumDetail { from_artist },
            NavOverlay::PlaylistDetail => Overlay::PlaylistDetail { selected: 0 },
            NavOverlay::GenreDetail => Overlay::GenreDetail {
                sub_tab: GenreDetailSubTab::default(),
                selected: 0,
            },
        }
    }
}

impl Overlay {
    /// Convert a client-side Overlay to a daemon-side NavOverlay (if applicable).
    fn to_nav(&self) -> Option<NavOverlay> {
        match self {
            Overlay::ArtistDetail => Some(NavOverlay::ArtistDetail),
            Overlay::AlbumDetail { from_artist } => Some(NavOverlay::AlbumDetail {
                from_artist: *from_artist,
            }),
            Overlay::PlaylistDetail { .. } => Some(NavOverlay::PlaylistDetail),
            Overlay::GenreDetail { .. } => Some(NavOverlay::GenreDetail),
            _ => None,
        }
    }
}

/// A clickable widget recorded by the draw pass.
#[derive(Debug, Clone, Copy)]
pub enum ClickTarget {
    /// Tab in the top tab bar.
    Tab(ActiveTab),
    /// Category chip of the active tab, by index in the category's `ALL` list.
    Category(usize),
    /// Sub-tab of the artist detail page, by index in `ArtistSubTab::ALL`.
    ArtistSubTab(usize),
    /// Sub-tab of the genre detail page, by index in `GenreDetailSubTab::ALL`.
    GenreSubTab(usize),
    /// Search / filter text input of the active tab.
    FilterInput,
    /// "f Flow" chip in the player bar.
    FlowChip,
    /// Heart icon / Like button in the player bar to toggle favorite for current track.
    ToggleLikeCurrentTrack,
    /// Playing track's name / progress bar in the player bar.
    CurrentTrack,
    /// "g Shuffle" button on the Favorites tab.
    ShuffleFavorites,
    /// "Esc Back to …" hint at the top of a detail page.
    Back,
    /// Cover + metadata column of the album / artist detail pages.
    DetailLeftPanel,
    /// Filter input of the "Add to playlist" picker.
    PlaylistFilterInput,
    /// Transparency switch of the theme picker.
    TransparencyToggle,
    /// Yes / no button of the delete-playlist confirmation.
    ConfirmChoice(bool),
}

/// Which list the rows recorded by the draw pass belong to — a click selects
/// the row in that list's own cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RowsKind {
    /// Main list of the active tab (and its category).
    Tab,
    AlbumDetail,
    ArtistDetail,
    GenreDetail,
    PlaylistDetail,
    WaitingList,
    /// Track list of the offline album/playlist detail modal.
    OfflineDetail,
    /// Option list of the open modal (menus, pickers, settings).
    Modal,
}

/// The visible rows of the topmost list, as laid out by the last draw pass.
#[derive(Debug, Clone, Copy)]
pub struct RowsArea {
    /// Data rows only — title, header and footer lines excluded.
    area: Rect,
    /// Index of the list's first visible row.
    offset: usize,
    /// Item count, so clicks past the last row are ignored.
    len: usize,
    kind: RowsKind,
}

impl RowsArea {
    /// Index of the list item drawn at this cell, if any.
    fn index_at(&self, col: u16, row: u16) -> Option<usize> {
        if !contains(self.area, col, row) {
            return None;
        }
        let index = self.offset + (row - self.area.y) as usize;
        (index < self.len).then_some(index)
    }
}

/// Screen regions recorded by the draw pass so mouse clicks can be routed to
/// the widget under the cursor. Rebuilt from scratch on every frame.
#[derive(Debug, Default)]
pub struct ClickAreas {
    targets: Vec<(Rect, ClickTarget)>,
    rows: Option<RowsArea>,
    /// Outline of the topmost modal, if one is open: clicking outside it closes
    /// the modal, and nothing drawn behind it is clickable.
    modal: Option<Rect>,
}

impl ClickAreas {
    /// Topmost target under this cell — later draws win, as they render on top.
    fn hit(&self, col: u16, row: u16) -> Option<ClickTarget> {
        self.targets
            .iter()
            .rev()
            .find(|(rect, _)| contains(*rect, col, row))
            .map(|(_, target)| *target)
    }
}

fn contains(rect: Rect, col: u16, row: u16) -> bool {
    col >= rect.x && col < rect.right() && row >= rect.y && row < rect.bottom()
}

/// View state used by UI rendering functions.
/// Combines daemon snapshot with local client-only state.
pub struct ViewState {
    // From daemon snapshot
    pub screen: Screen,
    pub active_tab: ActiveTab,
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
    pub search_results: Vec<TrackData>,
    pub search_selected: usize,
    pub search_loading: bool,
    pub search_category: SearchCategory,
    pub search_display: Vec<DisplayItem>,
    pub favorites: Vec<TrackData>,
    pub favorites_selected: usize,
    pub favorites_loading: bool,
    pub favorites_category: FavoritesCategory,
    pub favorites_display: Vec<DisplayItem>,
    pub favorites_filter_input: String,
    pub favorites_filter_typing: bool,
    pub favorites_filtered: Vec<(usize, DisplayItem)>,
    pub favorites_filter_selected: usize,
    pub favorite_track_ids: Vec<String>,
    pub favorite_artist_ids: Vec<String>,
    pub favorite_album_ids: Vec<String>,
    pub offline_category: OfflineCategory,
    pub offline_tracks: Vec<OfflineTrack>,
    pub offline_albums: Vec<AlbumDetail>,
    pub offline_playlists: Vec<PlaylistDetail>,
    pub offline_selected: usize,
    pub offline_loading: bool,
    pub offline_track_ids: Vec<String>,
    pub explore_category: ExploreCategory,
    pub radios: Vec<RadioItem>,
    pub radios_filtered: Vec<RadioItem>,
    pub radios_selected: usize,
    pub radios_loading: bool,
    pub radio_filter_input: String,
    pub radio_filter_typing: bool,
    pub moods: Vec<MoodEntry>,
    pub moods_selected: usize,
    pub moods_loading: bool,
    pub genres: Vec<GenreItem>,
    pub genres_filtered: Vec<GenreItem>,
    pub genres_selected: usize,
    pub genres_loading: bool,
    pub genres_filter_input: String,
    pub genres_filter_typing: bool,
    pub playlists: Vec<PlaylistData>,
    pub album_detail: Option<AlbumDetail>,
    pub album_detail_selected: usize,
    pub album_detail_loading: bool,
    pub album_detail_left_scroll: u16,
    pub album_detail_left_focused: bool,
    pub album_detail_left_scrollable: bool,
    pub artist_detail: Option<ArtistDetail>,
    pub artist_detail_selected: usize,
    pub artist_detail_loading: bool,
    pub artist_detail_sub_tab: ArtistSubTab,
    pub artist_detail_left_scroll: u16,
    pub artist_detail_left_focused: bool,
    pub artist_detail_left_scrollable: bool,
    pub playlist_detail: Option<PlaylistDetail>,
    pub playlist_detail_loading: bool,
    /// Fuzzy filter of the open playlist detail modal's track list (`/`).
    pub playlist_detail_filter_input: String,
    pub playlist_detail_filter_typing: bool,
    pub genre_detail: Option<GenreDetail>,
    pub genre_detail_loading: bool,
    pub status_msg: Option<String>,
    /// When `status_msg` last changed — the notification auto-hides after
    /// [`STATUS_MSG_TTL`], even though the daemon keeps the text in snapshots.
    pub status_msg_at: Option<Instant>,
    pub login_error: Option<String>,
    pub login_loading: bool,
    pub user_name: Option<String>,
    pub is_offline: bool,

    // Local client state — Offline tab
    /// Fuzzy filter applied to the Offline tab (all three categories).
    pub offline_filter_input: String,
    pub offline_filter_typing: bool,
    /// Selection within the filtered list of the active offline category.
    pub offline_filter_selected: usize,
    /// Fuzzy filter of the offline album/playlist detail modal.
    pub offline_detail_filter_input: String,
    pub offline_detail_filter_typing: bool,

    pub input_mode: InputMode,
    pub search_input: String,
    pub login_mode: LoginMode,
    pub login_input: String,
    pub login_cursor: usize,
    pub popup: Option<PopupMenu>,
    pub overlay: Option<Overlay>,
    /// Navigation history stack for overlays (Esc pops back).
    pub overlay_stack: Vec<Overlay>,
    /// Pending navigation commands to send to daemon (queued by overlay changes).
    pub nav_commands: Vec<Command>,
    /// True once we've synced nav overlay state from the daemon on initial connect.
    /// After that, the client is authoritative for nav state — daemon snapshots
    /// must not overwrite it, or we get glitches when a snapshot arrives between
    /// a client-side push and the daemon's ack of the corresponding nav command.
    pub nav_synced_from_daemon: bool,
    pub toast: Option<Toast>,

    // Cover art image (client-side only)
    pub cover_image: Option<StatefulProtocol>,
    /// URL of the currently loaded cover image (to avoid re-fetching).
    pub cover_image_url: String,
    /// Screen rect where the real cover image was last drawn, so overlays can
    /// dim the background around it without touching the image cells (dimming
    /// sixel/kitty image cells corrupts the artwork). `None` when no image.
    pub cover_image_area: Option<Rect>,

    /// Button area set by the UI draw pass, used for mouse hit-testing.
    pub login_button_area: Cell<Option<Rect>>,
    /// Clickable regions of the last drawn frame.
    pub click: RefCell<ClickAreas>,
    /// Scroll offset each list was drawn with last frame, so the next draw can
    /// resume from it instead of recomputing the viewport from the top.
    pub scroll: RefCell<HashMap<RowsKind, usize>>,
}

/// How long a status notification stays visible before hiding itself.
pub const STATUS_MSG_TTL: Duration = Duration::from_secs(7);

/// A temporary notification message that auto-dismisses.
#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub created_at: Instant,
    pub duration: Duration,
    pub is_error: bool,
}

impl Toast {
    pub fn new(message: String, duration: Duration) -> Self {
        Self {
            message,
            created_at: Instant::now(),
            duration,
            is_error: false,
        }
    }

    pub fn error(message: String, duration: Duration) -> Self {
        Self {
            message,
            created_at: Instant::now(),
            duration,
            is_error: true,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.duration
    }
}

/// Fuzzy match: returns true if every character of `query` appears in `target`
/// as a subsequence (in order, not necessarily contiguous). Both inputs should
/// already be lowercased by the caller.
pub(crate) fn fuzzy_match(query: &str, target: &str) -> bool {
    let mut target_chars = target.chars();
    'outer: for qc in query.chars() {
        loop {
            match target_chars.next() {
                Some(tc) if tc == qc => continue 'outer,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}

impl ViewState {
    fn from_snapshot(snap: &DaemonSnapshot) -> Self {
        Self {
            screen: snap.screen,
            active_tab: snap.active_tab,
            status: snap.status,
            current_track: snap.current_track.clone(),
            quality: snap.quality,
            position_secs: snap.position_secs,
            duration_secs: snap.duration_secs,
            volume: snap.volume,
            shuffle: snap.shuffle,
            repeat: snap.repeat,
            queue: snap.queue.clone(),
            queue_index: snap.queue_index,
            search_results: snap.search_results.clone(),
            search_selected: snap.search_selected,
            search_loading: snap.search_loading,
            search_category: snap.search_category,
            search_display: snap.search_display.clone(),
            favorites: snap.favorites.clone(),
            favorites_selected: snap.favorites_selected,
            favorites_loading: snap.favorites_loading,
            favorites_category: snap.favorites_category,
            favorites_display: snap.favorites_display.clone(),
            favorites_filter_input: String::new(),
            favorites_filter_typing: false,
            favorites_filtered: snap
                .favorites_display
                .iter()
                .enumerate()
                .map(|(i, item)| (i, item.clone()))
                .collect(),
            favorites_filter_selected: 0,
            favorite_track_ids: snap.favorite_track_ids.clone(),
            favorite_artist_ids: snap.favorite_artist_ids.clone(),
            favorite_album_ids: snap.favorite_album_ids.clone(),
            offline_category: snap.offline_category,
            offline_tracks: snap.offline_tracks.clone(),
            offline_albums: snap.offline_albums.clone(),
            offline_playlists: snap.offline_playlists.clone(),
            offline_selected: snap.offline_selected,
            offline_loading: snap.offline_loading,
            offline_track_ids: snap.offline_track_ids.clone(),
            explore_category: snap.explore_category,
            radios: snap.radios.clone(),
            radios_filtered: snap.radios.clone(),
            radios_selected: snap.radios_selected,
            radios_loading: snap.radios_loading,
            radio_filter_input: String::new(),
            radio_filter_typing: false,
            moods: snap.moods.clone(),
            moods_selected: snap.moods_selected,
            moods_loading: snap.moods_loading,
            genres: snap.genres.clone(),
            genres_filtered: snap.genres.clone(),
            genres_selected: snap.genres_selected,
            genres_loading: snap.genres_loading,
            genres_filter_input: String::new(),
            genres_filter_typing: false,
            playlists: snap.playlists.clone(),
            album_detail: snap.album_detail.clone(),
            album_detail_selected: snap.album_detail_selected,
            album_detail_loading: snap.album_detail_loading,
            album_detail_left_scroll: 0,
            album_detail_left_focused: false,
            album_detail_left_scrollable: false,
            artist_detail: snap.artist_detail.clone(),
            artist_detail_selected: snap.artist_detail_selected,
            artist_detail_loading: snap.artist_detail_loading,
            artist_detail_sub_tab: snap.artist_detail_sub_tab,
            artist_detail_left_scroll: 0,
            artist_detail_left_focused: false,
            artist_detail_left_scrollable: false,
            playlist_detail: snap.playlist_detail.clone(),
            playlist_detail_loading: snap.playlist_detail_loading,
            playlist_detail_filter_input: String::new(),
            playlist_detail_filter_typing: false,
            genre_detail: snap.genre_detail.clone(),
            genre_detail_loading: snap.genre_detail_loading,
            status_msg: snap.status_msg.clone(),
            status_msg_at: snap.status_msg.as_ref().map(|_| Instant::now()),
            login_error: snap.login_error.clone(),
            login_loading: snap.login_loading,
            user_name: snap.user_name.clone(),
            is_offline: snap.is_offline,

            offline_filter_input: String::new(),
            offline_filter_typing: false,
            offline_filter_selected: 0,
            offline_detail_filter_input: String::new(),
            offline_detail_filter_typing: false,

            input_mode: InputMode::Normal,
            search_input: String::new(),
            login_mode: LoginMode::Button,
            login_input: String::new(),
            login_cursor: 0,
            popup: None,
            overlay: snap.nav_overlay.clone().map(|n| n.to_overlay()),
            overlay_stack: snap
                .nav_overlay_stack
                .iter()
                .cloned()
                .map(|n| n.to_overlay())
                .collect(),
            nav_commands: Vec::new(),
            nav_synced_from_daemon: true, // initial snapshot was consumed in the constructor
            toast: None,
            cover_image: None,
            cover_image_url: String::new(),
            cover_image_area: None,
            login_button_area: Cell::new(None),
            click: RefCell::default(),
            scroll: RefCell::default(),
        }
    }

    /// Set the transient status notification and restart its display timer.
    pub fn set_status_msg(&mut self, msg: Option<String>) {
        self.status_msg_at = msg.as_ref().map(|_| Instant::now());
        self.status_msg = msg;
    }

    /// The status notification, or `None` once it has been on screen for
    /// [`STATUS_MSG_TTL`].
    pub fn visible_status_msg(&self) -> Option<&str> {
        match self.status_msg_at {
            Some(at) if at.elapsed() < STATUS_MSG_TTL => self.status_msg.as_deref(),
            _ => None,
        }
    }

    /// Push current overlay onto the stack and navigate to a new one.
    /// Also queues the corresponding daemon nav command if it's a nav overlay.
    fn push_overlay(&mut self, new: Overlay) {
        if let Some(nav) = new.to_nav() {
            self.nav_commands.push(Command::PushNavOverlay(nav));
        }
        if let Some(current) = self.overlay.take() {
            self.overlay_stack.push(current);
        }
        self.overlay = Some(new);
    }

    /// Pop back to the previous overlay (or None if stack is empty).
    /// Only sends daemon nav command if the current overlay is a nav type.
    fn pop_overlay(&mut self) {
        if self.overlay.as_ref().and_then(|o| o.to_nav()).is_some() {
            self.nav_commands.push(Command::PopNavOverlay);
        }
        self.overlay = self.overlay_stack.pop();
    }

    /// Set a nav overlay as the top-level (clear stack first).
    /// Used when entering a detail view from the main content area.
    fn set_nav_overlay(&mut self, new: Overlay) {
        self.overlay_stack.clear();
        self.overlay = Some(new.clone());
        if let Some(nav) = new.to_nav() {
            self.nav_commands.push(Command::ClearNavOverlayStack);
            self.nav_commands.push(Command::PushNavOverlay(nav));
        }
    }

    /// Returns true if the current overlay is a navigation overlay (or None).
    /// Used to decide whether to sync from daemon nav state.
    fn is_nav_or_none_overlay(&self) -> bool {
        match &self.overlay {
            None => true,
            Some(Overlay::ArtistDetail)
            | Some(Overlay::AlbumDetail { .. })
            | Some(Overlay::PlaylistDetail { .. })
            | Some(Overlay::GenreDetail { .. }) => true,
            _ => false,
        }
    }

    /// Update from a new daemon snapshot, preserving local-only state.
    /// True while a returning modal (help, settings, pickers, dialogs) is open.
    /// Detail pages and the waiting list are not modals — they render as page
    /// content and register their own clickable rows.
    pub fn has_modal_overlay(&self) -> bool {
        matches!(
            self.overlay,
            Some(
                Overlay::Help { .. }
                    | Overlay::Settings { .. }
                    | Overlay::ThemePicker { .. }
                    | Overlay::QualityPicker { .. }
                    | Overlay::LanguagePicker { .. }
                    | Overlay::Info
                    | Overlay::UpdateAvailable { .. }
                    | Overlay::Updating { .. }
            )
        )
    }

    /// Drop the previous frame's clickable regions. Called once per draw pass,
    /// and again by each modal so the page behind it stops being clickable.
    pub fn clear_click_areas(&self) {
        let mut click = self.click.borrow_mut();
        click.targets.clear();
        click.rows = None;
        click.modal = None;
    }

    /// Record the outline of a modal being drawn: clicks outside it close it.
    /// Also clears what was recorded behind, which the modal covers.
    pub fn record_modal(&self, area: Rect) {
        self.clear_click_areas();
        self.click.borrow_mut().modal = Some(area);
    }

    /// Record a clickable widget drawn at `rect`.
    pub fn record_click(&self, rect: Rect, target: ClickTarget) {
        self.click.borrow_mut().targets.push((rect, target));
    }

    /// Record the list drawn in `area` as clickable. `top` is how many lines the
    /// table spends on its title and header before the first data row; `offset`
    /// is the scroll offset ratatui laid the table out with this frame.
    pub fn record_rows(&self, area: Rect, top: u16, offset: usize, len: usize, kind: RowsKind) {
        let rows = Rect {
            y: area.y + top,
            height: area.height.saturating_sub(top),
            ..area
        };
        self.click.borrow_mut().rows = Some(RowsArea {
            area: rows,
            offset,
            len,
            kind,
        });
        // Carry the offset into the next frame so `table_state` can resume from it.
        self.scroll.borrow_mut().insert(kind, offset);
    }

    /// `TableState` for `kind`, seeded with the offset the same list was drawn
    /// with last frame.
    ///
    /// Building a `TableState::default()` per frame instead pins the offset at
    /// 0, and ratatui then scrolls the minimum needed to reveal the cursor —
    /// which parks the cursor on the last visible row and scrolls the viewport
    /// under it on every move. Resuming from the previous offset keeps the
    /// cursor moving inside the viewport and only scrolls at the edges.
    ///
    /// A stale offset (list swapped, items removed) is self-correcting:
    /// ratatui clamps it to keep `selected` visible.
    pub fn table_state(&self, kind: RowsKind, selected: usize) -> TableState {
        let offset = self.scroll.borrow().get(&kind).copied().unwrap_or(0);
        TableState::default()
            .with_selected(Some(selected))
            .with_offset(offset)
    }

    /// Focus the active tab's search or filter input — the `/` and `Ctrl+F`
    /// shortcuts, and clicking the input bar.
    pub fn focus_filter_input(&mut self, clear: bool) {
        // An open playlist detail modal captures the filter shortcut for its
        // own track search instead of the tab underneath.
        if let Some(Overlay::PlaylistDetail { selected }) = self.overlay.as_mut() {
            *selected = 0;
            self.playlist_detail_filter_typing = true;
            if clear {
                self.playlist_detail_filter_input.clear();
            }
            return;
        }
        match self.active_tab {
            ActiveTab::Search => {
                self.input_mode = InputMode::Typing;
                if clear {
                    self.search_input.clear();
                }
            }
            ActiveTab::Favorites => {
                self.favorites_filter_typing = true;
                if clear {
                    self.favorites_filter_input.clear();
                }
                self.apply_favorites_filter();
            }
            ActiveTab::Explore => match self.explore_category {
                ExploreCategory::Moods => {}
                ExploreCategory::Categories => {
                    self.genres_filter_typing = true;
                    if clear {
                        self.genres_filter_input.clear();
                    }
                    self.apply_genres_filter();
                }
                ExploreCategory::Radios => {
                    self.radio_filter_typing = true;
                    if clear {
                        self.radio_filter_input.clear();
                    }
                    self.apply_radio_filter();
                }
            },
            ActiveTab::Downloads => {
                self.offline_filter_typing = true;
                if clear {
                    self.offline_filter_input.clear();
                    self.offline_filter_selected = 0;
                }
            }
        }
    }

    /// Clear the playlist detail modal's track filter (on open / re-open).
    pub fn reset_playlist_detail_filter(&mut self) {
        self.playlist_detail_filter_input.clear();
        self.playlist_detail_filter_typing = false;
    }

    /// Leave every text input: the mouse moved the focus somewhere else.
    pub fn blur_inputs(&mut self) {
        self.input_mode = InputMode::Normal;
        self.favorites_filter_typing = false;
        self.radio_filter_typing = false;
        self.genres_filter_typing = false;
        self.offline_filter_typing = false;
        self.playlist_detail_filter_typing = false;
        if let Some(SubMenu::PlaylistPicker { filter_typing, .. }) = self
            .popup
            .as_mut()
            .and_then(|popup| popup.sub_menu.as_mut())
        {
            *filter_typing = false;
        }
    }

    fn update_from_snapshot(&mut self, snap: DaemonSnapshot) {
        let prev_screen = self.screen;

        self.screen = snap.screen;
        self.active_tab = snap.active_tab;
        self.status = snap.status;
        self.current_track = snap.current_track;
        self.quality = snap.quality;
        self.position_secs = snap.position_secs;
        self.duration_secs = snap.duration_secs;
        self.volume = snap.volume;
        self.shuffle = snap.shuffle;
        self.repeat = snap.repeat;
        self.queue = snap.queue;
        self.queue_index = snap.queue_index;
        self.search_results = snap.search_results;
        self.search_selected = snap.search_selected;
        self.search_loading = snap.search_loading;
        self.search_category = snap.search_category;
        self.search_display = snap.search_display;
        self.favorites = snap.favorites;
        self.favorites_selected = snap.favorites_selected;
        self.favorites_loading = snap.favorites_loading;
        // Reset filter when category changes
        if self.favorites_category != snap.favorites_category {
            self.favorites_filter_input.clear();
            self.favorites_filter_typing = false;
        }
        self.favorites_category = snap.favorites_category;
        self.favorites_display = snap.favorites_display;
        self.apply_favorites_filter();
        self.favorite_track_ids = snap.favorite_track_ids;
        self.favorite_artist_ids = snap.favorite_artist_ids;
        self.favorite_album_ids = snap.favorite_album_ids;
        self.offline_category = snap.offline_category;
        self.offline_tracks = snap.offline_tracks;
        self.offline_albums = snap.offline_albums;
        self.offline_playlists = snap.offline_playlists;
        self.clamp_offline_selection();
        self.offline_selected = snap.offline_selected;
        self.offline_loading = snap.offline_loading;
        self.offline_track_ids = snap.offline_track_ids;
        self.explore_category = snap.explore_category;
        // Update radios and re-apply filter
        if self.radios.len() != snap.radios.len() || self.radios_loading != snap.radios_loading {
            self.radios = snap.radios;
            self.radios_loading = snap.radios_loading;
            self.apply_radio_filter();
        }
        // Update moods
        if self.moods.len() != snap.moods.len() || self.moods_loading != snap.moods_loading {
            self.moods = snap.moods;
            self.moods_loading = snap.moods_loading;
        }
        // Update genres and re-apply filter
        if self.genres.len() != snap.genres.len() || self.genres_loading != snap.genres_loading {
            self.genres = snap.genres;
            self.genres_loading = snap.genres_loading;
            self.apply_genres_filter();
        }

        self.album_detail = snap.album_detail;
        // Don't overwrite album_detail_selected — it's managed client-side
        self.album_detail_loading = snap.album_detail_loading;
        self.artist_detail = snap.artist_detail;
        // Don't overwrite artist_detail_selected or sub_tab — managed client-side
        self.artist_detail_loading = snap.artist_detail_loading;
        self.playlist_detail = snap.playlist_detail;
        // Don't overwrite playlist_detail selected — it's managed client-side via Overlay
        self.playlist_detail_loading = snap.playlist_detail_loading;
        self.genre_detail = snap.genre_detail;
        self.genre_detail_loading = snap.genre_detail_loading;

        // Restore navigation overlay from daemon state.
        // Only apply if the current overlay is a nav type or None (don't clobber UI overlays).
        // Skip entirely once we've synced once — the client is authoritative for nav
        // afterwards. Reapplying every tick races with pending client-side pushes
        // (which are async to the daemon) and causes flicker + loss of stack state.
        if !self.nav_synced_from_daemon && self.is_nav_or_none_overlay() {
            let new_overlay = snap.nav_overlay.map(|n| n.to_overlay());
            // Preserve client-side `selected` for PlaylistDetail: the daemon only
            // tracks that the overlay *exists*, not the cursor position within it.
            // Without this, every 250ms tick would reset selected to 0.
            self.overlay = match (new_overlay, &self.overlay) {
                (
                    Some(Overlay::PlaylistDetail { .. }),
                    Some(Overlay::PlaylistDetail { selected }),
                ) => Some(Overlay::PlaylistDetail {
                    selected: *selected,
                }),
                (
                    Some(Overlay::GenreDetail { .. }),
                    Some(Overlay::GenreDetail { sub_tab, selected }),
                ) => Some(Overlay::GenreDetail {
                    sub_tab: *sub_tab,
                    selected: *selected,
                }),
                (new, _) => new,
            };
            // Rebuild overlay_stack from daemon, but preserve UI-only fields
            // (sub_tab, selected) of matching client-side entries — the daemon
            // doesn't track those.
            let prev_stack = std::mem::take(&mut self.overlay_stack);
            self.overlay_stack = snap
                .nav_overlay_stack
                .into_iter()
                .enumerate()
                .map(|(i, n)| {
                    let new = n.to_overlay();
                    match (new, prev_stack.get(i)) {
                        (
                            Overlay::GenreDetail { .. },
                            Some(Overlay::GenreDetail { sub_tab, selected }),
                        ) => Overlay::GenreDetail {
                            sub_tab: *sub_tab,
                            selected: *selected,
                        },
                        (
                            Overlay::PlaylistDetail { .. },
                            Some(Overlay::PlaylistDetail { selected }),
                        ) => Overlay::PlaylistDetail {
                            selected: *selected,
                        },
                        (new, _) => new,
                    }
                })
                .collect();
            self.nav_synced_from_daemon = true;
        }
        if snap.status_msg != self.status_msg {
            self.set_status_msg(snap.status_msg);
        }
        self.login_error = snap.login_error;
        self.login_loading = snap.login_loading;
        self.user_name = snap.user_name;
        self.is_offline = snap.is_offline;

        // Update playlists and sync into popup if playlist picker is loading
        if !snap.playlists.is_empty() {
            self.playlists = snap.playlists;
            if let Some(ref mut popup) = self.popup {
                if let Some(SubMenu::PlaylistPicker {
                    ref mut playlists,
                    ref mut loading,
                    ..
                }) = popup.sub_menu
                {
                    if *loading {
                        *playlists = self.playlists.clone();
                        *loading = false;
                    }
                }
            }
        }

        // After login transition (Login → Main), reset typing mode
        if prev_screen == Screen::Login && self.screen == Screen::Main {
            self.input_mode = InputMode::Normal;
        }

        // After logout transition (Main → Login), reset local state
        if prev_screen == Screen::Main && self.screen == Screen::Login {
            self.input_mode = InputMode::Normal;
            self.login_mode = LoginMode::Button;
            self.login_input.clear();
            self.login_cursor = 0;
            self.overlay = None;
            self.overlay_stack.clear();
            self.popup = None;
            self.radio_filter_input.clear();
            self.radio_filter_typing = false;
            self.genres_filter_input.clear();
            self.genres_filter_typing = false;
            self.favorites_filter_input.clear();
            self.favorites_filter_typing = false;
        }
    }

    /// Check if a track is in the user's favorites.
    pub fn is_track_favorite(&self, track_id: &str) -> bool {
        self.favorite_track_ids.iter().any(|id| id == track_id)
            || self.favorites.iter().any(|t| t.track_id == track_id)
    }

    /// True when this track is the one loaded in the player, playing or paused.
    pub fn is_playing_track(&self, track_id: &str) -> bool {
        self.current_track
            .as_ref()
            .is_some_and(|t| t.track_id == track_id)
    }

    /// Check if an artist is in the user's favorites.
    fn is_artist_favorite(&self, artist_id: &str) -> bool {
        self.favorite_artist_ids.iter().any(|id| id == artist_id)
    }

    /// Check if an album is in the user's favorites.
    fn is_album_favorite(&self, album_id: &str) -> bool {
        self.favorite_album_ids.iter().any(|id| id == album_id)
    }

    /// Apply the current filter text to the full genres list.
    fn apply_genres_filter(&mut self) {
        if self.genres_filter_input.is_empty() {
            self.genres_filtered = self.genres.clone();
        } else {
            let query = self.genres_filter_input.to_lowercase();
            self.genres_filtered = self
                .genres
                .iter()
                .filter(|g| fuzzy_match(&query, &g.name.to_lowercase()))
                .cloned()
                .collect();
        }
        self.genres_selected = 0;
    }

    /// Apply the current filter text to the full radios list.
    fn apply_radio_filter(&mut self) {
        if self.radio_filter_input.is_empty() {
            self.radios_filtered = self.radios.clone();
        } else {
            let query = self.radio_filter_input.to_lowercase();
            self.radios_filtered = self
                .radios
                .iter()
                .filter(|r| fuzzy_match(&query, &r.title.to_lowercase()))
                .cloned()
                .collect();
        }
        self.radios_selected = 0;
    }

    /// Apply the current filter text to the full favorites list.
    fn apply_favorites_filter(&mut self) {
        let query = self.favorites_filter_input.to_lowercase();
        self.favorites_filtered = self
            .favorites_display
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                query.is_empty()
                    || fuzzy_match(&query, &item.col1.to_lowercase())
                    || fuzzy_match(&query, &item.col2.to_lowercase())
            })
            .map(|(i, item)| (i, item.clone()))
            .collect();
        self.favorites_filter_selected = self
            .favorites_filter_selected
            .min(self.favorites_filtered.len().saturating_sub(1));
    }

    /// Whether the favorites filter is active (has content or is being typed).
    pub fn favorites_filter_active(&self) -> bool {
        self.favorites_filter_typing || !self.favorites_filter_input.is_empty()
    }

    /// Get the currently selected DisplayItem in the favorites tab, respecting filter state.
    pub fn favorites_selected_item(&self) -> Option<&DisplayItem> {
        if self.favorites_filter_active() {
            self.favorites_filtered
                .get(self.favorites_filter_selected)
                .map(|(_, item)| item)
        } else {
            self.favorites_display.get(self.favorites_selected)
        }
    }

    /// Get the original (daemon-side) index of the currently selected favorites item.
    pub fn favorites_selected_original_index(&self) -> usize {
        if self.favorites_filter_active() {
            self.favorites_filtered
                .get(self.favorites_filter_selected)
                .map(|(i, _)| *i)
                .unwrap_or(0)
        } else {
            self.favorites_selected
        }
    }

    /// Offline tracks matching the filter, paired with their daemon-side index.
    pub fn offline_tracks_filtered(&self) -> Vec<(usize, &OfflineTrack)> {
        self.offline_tracks
            .iter()
            .enumerate()
            .filter(|(_, ot)| {
                self.offline_filter_matches(&[&ot.track.title, &ot.track.artist, &ot.track.album])
            })
            .collect()
    }

    /// Downloaded albums matching the filter, paired with their index.
    pub fn offline_albums_filtered(&self) -> Vec<(usize, &AlbumDetail)> {
        self.offline_albums
            .iter()
            .enumerate()
            .filter(|(_, a)| self.offline_filter_matches(&[&a.title, &a.artist]))
            .collect()
    }

    /// Downloaded playlists matching the filter, paired with their index.
    pub fn offline_playlists_filtered(&self) -> Vec<(usize, &PlaylistDetail)> {
        self.offline_playlists
            .iter()
            .enumerate()
            .filter(|(_, p)| self.offline_filter_matches(&[&p.title, &p.creator]))
            .collect()
    }

    /// Number of rows currently shown by the Offline tab.
    pub fn offline_list_len(&self) -> usize {
        match self.offline_category {
            OfflineCategory::Tracks => self.offline_tracks_filtered().len(),
            OfflineCategory::Albums => self.offline_albums_filtered().len(),
            OfflineCategory::Playlists => self.offline_playlists_filtered().len(),
        }
    }

    /// Cursor position within the rows the Offline tab currently shows.
    pub fn offline_list_selected(&self) -> usize {
        if self.offline_filter_active() {
            self.offline_filter_selected
        } else {
            self.offline_selected
        }
    }

    /// Whether the Offline tab filter is active (typed into, or has content).
    pub fn offline_filter_active(&self) -> bool {
        self.offline_filter_typing || !self.offline_filter_input.is_empty()
    }

    /// True when any of `fields` fuzzy-matches the Offline tab filter.
    fn offline_filter_matches(&self, fields: &[&str]) -> bool {
        if self.offline_filter_input.is_empty() {
            return true;
        }
        let query = self.offline_filter_input.to_lowercase();
        fields
            .iter()
            .any(|f| fuzzy_match(&query, &f.to_lowercase()))
    }

    /// Daemon-side index of the selected offline track, respecting the filter.
    pub fn offline_selected_track_index(&self) -> usize {
        if self.offline_filter_active() {
            self.offline_tracks_filtered()
                .get(self.offline_filter_selected)
                .map(|(i, _)| *i)
                .unwrap_or(0)
        } else {
            self.offline_selected
        }
    }

    /// The offline track under the cursor in the Tracks category.
    pub fn offline_selected_track(&self) -> Option<&OfflineTrack> {
        if self.offline_filter_active() {
            self.offline_tracks_filtered()
                .get(self.offline_filter_selected)
                .map(|(_, ot)| *ot)
        } else {
            self.offline_tracks.get(self.offline_selected)
        }
    }

    /// The album under the cursor in the Albums category, with its index.
    pub fn offline_selected_album(&self) -> Option<(usize, &AlbumDetail)> {
        self.offline_albums_filtered()
            .get(self.offline_list_selected())
            .copied()
    }

    /// The playlist under the cursor in the Playlists category, with its index.
    pub fn offline_selected_playlist(&self) -> Option<(usize, &PlaylistDetail)> {
        self.offline_playlists_filtered()
            .get(self.offline_list_selected())
            .copied()
    }

    /// Tracks of the album/playlist shown in the detail modal, matching its own
    /// filter, paired with their index inside that album/playlist.
    pub fn offline_detail_tracks(&self, playlist: bool, index: usize) -> Vec<(usize, &TrackData)> {
        let tracks: &[TrackData] = if playlist {
            match self.offline_playlists.get(index) {
                Some(p) => &p.tracks,
                None => return Vec::new(),
            }
        } else {
            match self.offline_albums.get(index) {
                Some(a) => &a.tracks,
                None => return Vec::new(),
            }
        };
        let query = self.offline_detail_filter_input.to_lowercase();
        tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                query.is_empty()
                    || fuzzy_match(&query, &t.title.to_lowercase())
                    || fuzzy_match(&query, &t.artist.to_lowercase())
            })
            .collect()
    }

    /// Tracks of the open playlist detail modal matching its `/` filter, each
    /// paired with its original index in the playlist. Empty filter returns all.
    pub fn playlist_detail_tracks_filtered(&self) -> Vec<(usize, &TrackData)> {
        let Some(detail) = self.playlist_detail.as_ref() else {
            return Vec::new();
        };
        let query = self.playlist_detail_filter_input.to_lowercase();
        detail
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                query.is_empty()
                    || fuzzy_match(&query, &t.title.to_lowercase())
                    || fuzzy_match(&query, &t.artist.to_lowercase())
            })
            .collect()
    }

    /// Title shown by the offline detail modal.
    pub fn offline_detail_title(&self, playlist: bool, index: usize) -> String {
        if playlist {
            self.offline_playlists
                .get(index)
                .map(|p| format!(" {} — {} ", p.title, p.creator))
                .unwrap_or_default()
        } else {
            self.offline_albums
                .get(index)
                .map(|a| format!(" {} — {} ", a.title, a.artist))
                .unwrap_or_default()
        }
    }

    /// Keep the offline cursors inside their lists after a snapshot changed them.
    fn clamp_offline_selection(&mut self) {
        let len = self.offline_list_len();
        self.offline_filter_selected = self.offline_filter_selected.min(len.saturating_sub(1));
        let detail = match self.overlay {
            Some(Overlay::OfflineDetail {
                playlist, index, ..
            }) => Some((playlist, index)),
            _ => None,
        };
        if let Some((playlist, index)) = detail {
            let count = if playlist {
                self.offline_playlists.get(index).map(|p| p.tracks.len())
            } else {
                self.offline_albums.get(index).map(|a| a.tracks.len())
            }
            .unwrap_or(0);
            if let Some(Overlay::OfflineDetail { selected, .. }) = self.overlay.as_mut() {
                *selected = (*selected).min(count.saturating_sub(1));
            }
        }
    }

    /// Progress ratio for the progress bar.
    pub fn progress_percent(&self) -> f64 {
        if self.duration_secs == 0 {
            0.0
        } else {
            self.position_secs as f64 / self.duration_secs as f64
        }
    }

    /// Format position as "m:ss / m:ss".
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

pub struct Client {
    view: ViewState,
    /// Parsed messages from the daemon, produced by a dedicated reader task.
    /// `read_line` is not cancellation-safe, so it must never be raced against
    /// a timeout on a shared reader — that silently truncates/desyncs the
    /// stream. Reading to completion in its own task and forwarding results
    /// over this channel avoids that entirely.
    server_rx: mpsc::UnboundedReceiver<io::Result<ServerMessage>>,
    writer: tokio::net::unix::OwnedWriteHalf,
    picker: Picker,
    image_tx: mpsc::UnboundedSender<(String, image::DynamicImage)>,
    image_rx: mpsc::UnboundedReceiver<(String, image::DynamicImage)>,
    /// Cache of downloaded images by URL, cleared when leaving overlay pages.
    image_cache: HashMap<String, image::DynamicImage>,
    /// Cell and time of the last left click, for double-click detection.
    last_click: Option<(u16, u16, Instant)>,
}

/// Helper to restore standard terminal mode safely.
pub fn restore_terminal() {
    let mut stdout = io::stdout();
    let _ = stdout.execute(DisableMouseCapture);
    let _ = disable_raw_mode();
    let _ = stdout.execute(LeaveAlternateScreen);
    let _ = stdout.execute(Show);
    let _ = stdout.execute(SetTitle(""));
}

impl Client {
    pub async fn connect() -> Result<Self> {
        let path = socket_path();
        let stream = UnixStream::connect(&path).await?;
        let (read_half, write_half) = stream.into_split();

        // Detect terminal graphics protocol (must happen before alternate screen)
        let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

        let (image_tx, image_rx) = mpsc::unbounded_channel();

        let (server_tx, server_rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut reader = BufReader::new(read_half);
            loop {
                match read_line::<ServerMessage, _>(&mut reader).await {
                    Ok(Some(msg)) => {
                        if server_tx.send(Ok(msg)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => break, // EOF — daemon disconnected
                    Err(e) => {
                        let _ = server_tx.send(Err(e));
                        break;
                    }
                }
            }
        });

        Ok(Self {
            view: ViewState::from_snapshot(&DaemonSnapshot::default()),
            server_rx,
            writer: write_half,
            picker,
            image_tx,
            image_rx,
            image_cache: HashMap::new(),
            last_click: None,
        })
    }

    /// Get the current cover art URL from album/artist detail overlay.
    fn current_cover_url(&self) -> Option<&str> {
        // Search overlay chain (current + stack) for an active album/artist detail.
        let active = std::iter::once(self.view.overlay.as_ref())
            .chain(self.view.overlay_stack.iter().rev().map(Some))
            .find(|o| {
                matches!(
                    o,
                    Some(Overlay::AlbumDetail { .. }) | Some(Overlay::ArtistDetail)
                )
            });
        match active {
            Some(Some(Overlay::AlbumDetail { .. })) => self
                .view
                .album_detail
                .as_ref()
                .map(|d| d.cover_url.as_str())
                .filter(|u| !u.is_empty()),
            Some(Some(Overlay::ArtistDetail)) => {
                // When browsing Albums/Lives/Other, show selected album cover if available
                let album_cover = match self.view.artist_detail_sub_tab {
                    ArtistSubTab::Albums | ArtistSubTab::Lives | ArtistSubTab::Other => {
                        self.view.artist_detail.as_ref().and_then(|d| {
                            let filtered = d.albums_for_tab(self.view.artist_detail_sub_tab);
                            filtered
                                .get(self.view.artist_detail_selected)
                                .map(|a| a.cover_url.as_str())
                                .filter(|u| !u.is_empty())
                        })
                    }
                    _ => None,
                };
                album_cover.or_else(|| {
                    self.view
                        .artist_detail
                        .as_ref()
                        .map(|d| d.picture_url.as_str())
                        .filter(|u| !u.is_empty())
                })
            }
            _ => None,
        }
    }

    /// Trigger async image fetch if the cover URL changed.
    /// Uses in-memory cache to avoid re-downloading images already seen on this page.
    fn maybe_fetch_cover_image(&mut self) {
        let url = match self.current_cover_url() {
            Some(u) => u.to_string(),
            None => {
                // No overlay or no URL — clear image and cache
                if self.view.cover_image.is_some() {
                    self.view.cover_image = None;
                    self.view.cover_image_url.clear();
                }
                self.image_cache.clear();
                return;
            }
        };

        if url == self.view.cover_image_url {
            return; // Already loaded or loading
        }

        // Mark as loading this URL (prevents re-triggering)
        self.view.cover_image_url = url.clone();
        self.view.cover_image = None;

        // Check cache first — instant display without HTTP fetch
        if let Some(img) = self.image_cache.get(&url) {
            let proto = self.picker.new_resize_protocol(img.clone());
            self.view.cover_image = Some(proto);
            return;
        }

        let tx = self.image_tx.clone();
        tokio::spawn(async move {
            match reqwest::get(&url).await {
                Ok(resp) => {
                    if let Ok(bytes) = resp.bytes().await {
                        if let Ok(img) = image::load_from_memory(&bytes) {
                            let _ = tx.send((url, img));
                        }
                    }
                }
                Err(e) => {
                    debug!("Failed to fetch cover image: {e}");
                }
            }
        });
    }

    /// Update the terminal emulator window/tab title with current playback info.
    fn update_terminal_title(&self) {
        let title = match (&self.view.status, &self.view.current_track) {
            (PlaybackStatus::Playing, Some(track)) => {
                format!("deezer-tui: ▶ {} — {}", track.title, track.artist)
            }
            (PlaybackStatus::Paused, Some(track)) => {
                format!("deezer-tui: ⏸ {} — {}", track.title, track.artist)
            }
            _ => "deezer-tui".to_string(),
        };
        let _ = io::stdout().execute(SetTitle(title));
    }

    async fn send_cmd(&mut self, cmd: &Command) -> std::io::Result<()> {
        use tokio::io::AsyncWriteExt;
        let mut json = serde_json::to_string(cmd)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        json.push('\n');
        self.writer.write_all(json.as_bytes()).await?;
        self.writer.flush().await
    }

    pub async fn run(&mut self, show_updated: bool) -> Result<()> {
        // Setup panic hook to ensure terminal is restored cleanly if a crash occurs
        let default_panic = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_terminal();
            default_panic(info);
        }));

        // Load saved theme and opacity from config
        let config = Config::load();
        if let Some(ref theme_str) = config.theme {
            if let Some(id) = ThemeId::from_str(theme_str) {
                Theme::set(id);
            }
        }
        Theme::set_transparency(config.bg_transparency);

        // Setup terminal
        enable_raw_mode()?;
        io::stdout().execute(EnterAlternateScreen)?;
        io::stdout().execute(EnableMouseCapture)?;
        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;
        self.update_terminal_title();

        // Spawn update check in background (non-blocking)
        let (update_tx, mut update_rx) = mpsc::unbounded_channel::<(String, String)>();
        if !config.skip_update_check {
            let tx = update_tx.clone();
            tokio::spawn(async move {
                if let Some(result) = check_for_update().await {
                    let _ = tx.send(result);
                }
            });
        }
        drop(update_tx); // Drop our copy so the channel closes when the task finishes

        // Wait for initial snapshot from daemon before drawing anything.
        // This avoids a brief flash of the Login screen when the user is already logged in.
        match tokio::time::timeout(Duration::from_secs(3), self.server_rx.recv()).await {
            Ok(Some(Ok(ServerMessage::Snapshot(snap)))) => {
                self.view.update_from_snapshot(snap);
                self.update_terminal_title();
            }
            _ => {
                // Timeout, disconnect, or error — proceed with default state
            }
        }

        // Reset login mode when starting on login screen
        if self.view.screen == Screen::Login {
            self.view.login_mode = LoginMode::Button;
        }

        // Show Info overlay after a successful update (--updated flag)
        if show_updated && self.view.screen == Screen::Main {
            self.view.overlay = Some(Overlay::Info);
        }

        // Main client loop
        let mut running = true;
        let mut send_shutdown = false;
        let mut update_check_done = config.skip_update_check;
        let mut prev_over_image = false;

        while running {
            // Clear expired toast
            if self.view.toast.as_ref().is_some_and(|t| t.is_expired()) {
                self.view.toast = None;
            }

            // On image-capable terminals (sixel/kitty), a modal drawn over the
            // cover image can leave remnants when it closes — the buffer diff
            // skips re-emitting the image. Force a full redraw on the frame right
            // after such a layer closes.
            // On detail pages `overlay` is the detail itself, so only count
            // layers drawn ON TOP of it: a popup, or a modal overlay that
            // replaces the detail (which then sits in `overlay_stack`).
            let over_image = self.view.cover_image.is_some()
                && (self.view.popup.is_some()
                    || !matches!(
                        self.view.overlay,
                        Some(Overlay::AlbumDetail { .. }) | Some(Overlay::ArtistDetail)
                    ));
            if prev_over_image && !over_image {
                terminal.clear()?;
            }
            prev_over_image = over_image;

            terminal.draw(|frame| {
                ui::draw(frame, &mut self.view);
            })?;

            // Poll for input events (non-blocking with short timeout)
            if event::poll(TICK_RATE)? {
                let action = match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key),
                    Event::Mouse(mouse) => self.handle_mouse(mouse),
                    _ => KeyAction::Continue,
                };

                match action {
                    KeyAction::Continue => {}
                    KeyAction::Quit => {
                        send_shutdown = true;
                        running = false;
                    }
                    KeyAction::Detach => {
                        running = false;
                    }
                    KeyAction::SendCommand(cmd) => {
                        if let Err(e) = self.send_cmd(&cmd).await {
                            debug!("Send command error: {e}");
                            self.view
                                .set_status_msg(Some(t().daemon_disconnected.into()));
                            running = false;
                        }
                    }
                    KeyAction::MultiCommand(cmds) => {
                        for cmd in &cmds {
                            if let Err(e) = self.send_cmd(cmd).await {
                                debug!("Send command error: {e}");
                                self.view
                                    .set_status_msg(Some(t().daemon_disconnected.into()));
                                running = false;
                                break;
                            }
                        }
                    }
                    KeyAction::WebLogin => {
                        // Suspend TUI
                        restore_terminal();
                        drop(terminal);

                        // Run browser login (blocking)
                        let result = crate::web_login::login_via_browser();

                        // Resume TUI
                        enable_raw_mode()?;
                        io::stdout().execute(EnterAlternateScreen)?;
                        io::stdout().execute(EnableMouseCapture)?;
                        terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
                        terminal.clear()?;
                        self.update_terminal_title();

                        if let Ok(Some(arl)) = result {
                            self.view.login_loading = true;
                            self.send_cmd(&Command::Login { arl }).await?;
                        }
                    }
                    KeyAction::PerformUpdate {
                        version,
                        download_url,
                    } => {
                        // Show "Downloading..." overlay and redraw
                        self.view.overlay = Some(Overlay::Updating {
                            version: version.clone(),
                            progress_msg: t().update_downloading.into(),
                        });
                        terminal.draw(|frame| {
                            ui::draw(frame, &mut self.view);
                        })?;

                        // Suspend TUI so sudo can prompt for password if needed
                        restore_terminal();
                        drop(terminal);

                        let update_result = perform_update(&download_url).await;

                        // Always re-create terminal (borrow checker requires it)
                        enable_raw_mode()?;
                        io::stdout().execute(EnterAlternateScreen)?;
                        io::stdout().execute(EnableMouseCapture)?;
                        terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
                        terminal.clear()?;
                        self.update_terminal_title();

                        match update_result {
                            Ok(binary_path) => {
                                // Restore terminal before exec
                                restore_terminal();

                                // Shut down the daemon and wait for it to actually exit
                                let _ = self.send_cmd(&Command::Shutdown).await;
                                wait_for_daemon_exit().await;

                                // Try to exec into the new binary (replaces this process)
                                #[cfg(unix)]
                                {
                                    use std::os::unix::process::CommandExt;
                                    let err = std::process::Command::new(&binary_path)
                                        .arg("--updated")
                                        .exec();
                                    // exec() only returns on error
                                    eprintln!("Failed to restart: {err}");
                                    eprintln!("{}", t().update_restart_manually);
                                }

                                #[cfg(not(unix))]
                                {
                                    eprintln!("{}", t().update_restart_manually);
                                    let _ = binary_path;
                                }

                                running = false;
                            }
                            Err(e) => {
                                self.view.overlay = None;
                                self.view.toast = Some(Toast::error(
                                    format!("{}: {e}", t().update_failed),
                                    Duration::from_secs(5),
                                ));
                            }
                        }
                    }
                }

                // Check if overlay changed (may need new cover image)
                self.maybe_fetch_cover_image();
            }

            // Send any queued navigation overlay commands to the daemon
            let nav_cmds: Vec<Command> = self.view.nav_commands.drain(..).collect();
            for cmd in &nav_cmds {
                if let Err(e) = self.send_cmd(cmd).await {
                    debug!("Send nav command error: {e}");
                    self.view
                        .set_status_msg(Some(t().daemon_disconnected.into()));
                    running = false;
                    break;
                }
            }

            if !running {
                break;
            }

            // Drain any messages from daemon (non-blocking; reading happens in
            // the dedicated task spawned in `connect`, never raced against a
            // timeout here).
            match self.server_rx.try_recv() {
                Ok(Ok(ServerMessage::Snapshot(snap))) => {
                    self.view.update_from_snapshot(snap);
                    self.update_terminal_title();
                    self.maybe_fetch_cover_image();
                }
                Ok(Ok(ServerMessage::Error(err))) => {
                    self.view.set_status_msg(Some(format!("Error: {err}")));
                }
                Ok(Err(e)) => {
                    debug!("Read error from daemon: {e}");
                    self.view
                        .set_status_msg(Some(format!("Communication error: {e}")));
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    // Daemon disconnected
                    self.view
                        .set_status_msg(Some(t().daemon_disconnected.into()));
                    running = false;
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // No data available, continue
                }
            }

            // Check for completed image fetches
            while let Ok((url, img)) = self.image_rx.try_recv() {
                self.image_cache.insert(url.clone(), img.clone());
                if url == self.view.cover_image_url {
                    let proto = self.picker.new_resize_protocol(img);
                    self.view.cover_image = Some(proto);
                }
            }

            // Check for update availability (background check result)
            if !update_check_done {
                if let Ok((version, download_url)) = update_rx.try_recv() {
                    if self.view.overlay.is_none() {
                        self.view.overlay = Some(Overlay::UpdateAvailable {
                            version,
                            download_url,
                            selected: 0,
                        });
                    }
                    update_check_done = true;
                }
            }
        }

        // Restore terminal
        restore_terminal();

        if send_shutdown {
            let _ = self.send_cmd(&Command::Shutdown).await;
        } else {
            eprintln!("{}", t().detach_message);
        }

        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> KeyAction {
        debug!(?key, "client key event");

        // Ctrl+C always detaches (daemon keeps playing)
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return KeyAction::Detach;
        }

        let popup_typing = self
            .view
            .popup
            .as_ref()
            .and_then(|p| p.sub_menu.as_ref())
            .is_some_and(|sm| {
                matches!(
                    sm,
                    SubMenu::CreatePlaylistInput { .. } | SubMenu::RenamePlaylistInput { .. }
                ) || matches!(
                    sm,
                    SubMenu::PlaylistPicker {
                        filter_typing: true,
                        ..
                    }
                )
            });

        // ? : toggle help overlay (not during text input)
        if key.code == KeyCode::Char('?')
            && self.view.screen == Screen::Main
            && self.view.input_mode != InputMode::Typing
            && !self.view.radio_filter_typing
            && !self.view.genres_filter_typing
            && !self.view.favorites_filter_typing
            && !self.view.offline_filter_typing
            && !self.view.playlist_detail_filter_typing
            && !popup_typing
        {
            if matches!(self.view.overlay, Some(Overlay::Help { .. })) {
                self.view.pop_overlay();
            } else {
                self.view.push_overlay(Overlay::Help { scroll: 0 });
            }
            return KeyAction::Continue;
        }

        // i : toggle info modal (not during text input)
        if key.code == KeyCode::Char('i')
            && self.view.screen == Screen::Main
            && self.view.input_mode != InputMode::Typing
            && !self.view.radio_filter_typing
            && !self.view.genres_filter_typing
            && !self.view.favorites_filter_typing
            && !self.view.offline_filter_typing
            && !self.view.playlist_detail_filter_typing
            && !popup_typing
        {
            if matches!(self.view.overlay, Some(Overlay::Info)) {
                self.view.pop_overlay();
            } else {
                self.view.push_overlay(Overlay::Info);
            }
            return KeyAction::Continue;
        }

        // Ctrl+O : toggle settings overlay
        if key.code == KeyCode::Char('o') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if matches!(self.view.overlay, Some(Overlay::Settings { .. })) {
                self.view.pop_overlay();
            } else {
                self.view.push_overlay(Overlay::Settings { selected: 0 });
            }
            return KeyAction::Continue;
        }

        // Ctrl+F: enter search/filter mode (same as /)
        if key.code == KeyCode::Char('f') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if self.view.screen == Screen::Main {
                self.view.focus_filter_input(true);
            }
            return KeyAction::Continue;
        }

        // Ctrl+L: toggle favorite / like for focused track or playing track
        if key.code == KeyCode::Char('l') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if self.view.screen == Screen::Main {
                return self.toggle_like_focused_or_playing();
            }
            return KeyAction::Continue;
        }

        // Ctrl+Right: seek forward 10s
        if key.code == KeyCode::Right && key.modifiers.contains(KeyModifiers::CONTROL) {
            return KeyAction::SendCommand(Command::SeekForward { secs: 10 });
        }

        // Ctrl+Left: seek backward 10s
        if key.code == KeyCode::Left && key.modifiers.contains(KeyModifiers::CONTROL) {
            return KeyAction::SendCommand(Command::SeekBackward { secs: 10 });
        }

        // Ctrl+Z: detach (daemon keeps playing)
        #[cfg(unix)]
        if matches!(key.code, KeyCode::Char('z') | KeyCode::Char('Z'))
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            return KeyAction::Detach;
        }

        match self.view.screen {
            Screen::Login => self.handle_login_key(key),
            Screen::Main => self.handle_main_key(key),
        }
    }

    fn handle_login_key(&mut self, key: KeyEvent) -> KeyAction {
        if self.view.login_loading {
            return KeyAction::Continue;
        }

        match self.view.login_mode {
            LoginMode::Button => match key.code {
                KeyCode::Enter => KeyAction::WebLogin,
                KeyCode::Char('w') => {
                    self.view.login_mode = LoginMode::ArlInput;
                    self.view.login_input.clear();
                    self.view.login_cursor = 0;
                    KeyAction::Continue
                }
                KeyCode::Esc => KeyAction::Quit,
                _ => KeyAction::Continue,
            },
            LoginMode::ArlInput => match key.code {
                KeyCode::Esc => {
                    self.view.login_mode = LoginMode::Button;
                    self.view.login_input.clear();
                    self.view.login_cursor = 0;
                    KeyAction::Continue
                }
                KeyCode::Enter => {
                    if !self.view.login_input.is_empty() {
                        let arl = self.view.login_input.clone();
                        self.view.login_loading = true;
                        KeyAction::SendCommand(Command::Login { arl })
                    } else {
                        KeyAction::Continue
                    }
                }
                KeyCode::Char(c) => {
                    self.view.login_input.insert(self.view.login_cursor, c);
                    self.view.login_cursor += 1;
                    KeyAction::Continue
                }
                KeyCode::Backspace => {
                    if self.view.login_cursor > 0 {
                        self.view.login_cursor -= 1;
                        self.view.login_input.remove(self.view.login_cursor);
                    }
                    KeyAction::Continue
                }
                KeyCode::Left => {
                    self.view.login_cursor = self.view.login_cursor.saturating_sub(1);
                    KeyAction::Continue
                }
                KeyCode::Right => {
                    if self.view.login_cursor < self.view.login_input.len() {
                        self.view.login_cursor += 1;
                    }
                    KeyAction::Continue
                }
                _ => KeyAction::Continue,
            },
        }
    }

    fn handle_main_key(&mut self, key: KeyEvent) -> KeyAction {
        // Ctrl+Q: quit from anywhere (send Shutdown to daemon)
        if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return KeyAction::Quit;
        }

        // Popup mode takes priority over overlays (popups can open on top of overlays)
        if self.view.popup.is_some() {
            return self.handle_popup_key(key);
        }

        // Ctrl+Space: open manage popup for currently playing track (global)
        if key.code == KeyCode::Char(' ') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if let Some(ref track) = self.view.current_track {
                let is_fav = self.view.is_track_favorite(&track.track_id);
                self.view.popup = Some(PopupMenu::manage_only(track.clone(), is_fav));
            }
            return KeyAction::Continue;
        }

        // Overlay mode — intercept all keys
        if self.view.overlay.is_some() {
            return self.handle_overlay_key(key);
        }

        // Typing mode (search input)
        if self.view.input_mode == InputMode::Typing {
            match key.code {
                KeyCode::Esc => {
                    self.view.input_mode = InputMode::Normal;
                    return KeyAction::Continue;
                }
                KeyCode::Enter => {
                    if !self.view.search_input.is_empty() {
                        let query = self.view.search_input.clone();
                        self.view.input_mode = InputMode::Normal;
                        return KeyAction::SendCommand(Command::Search { query });
                    }
                    self.view.input_mode = InputMode::Normal;
                    return KeyAction::Continue;
                }
                KeyCode::Char(c) => {
                    self.view.search_input.push(c);
                    return KeyAction::Continue;
                }
                KeyCode::Backspace => {
                    self.view.search_input.pop();
                    return KeyAction::Continue;
                }
                _ => return KeyAction::Continue,
            }
        }

        // Favorites filter typing mode
        if self.view.favorites_filter_typing {
            match key.code {
                KeyCode::Esc => {
                    self.view.favorites_filter_typing = false;
                    return KeyAction::Continue;
                }
                KeyCode::Enter => {
                    self.view.favorites_filter_typing = false;
                    return KeyAction::Continue;
                }
                KeyCode::Char(c) => {
                    self.view.favorites_filter_input.push(c);
                    self.view.apply_favorites_filter();
                    return KeyAction::Continue;
                }
                KeyCode::Backspace => {
                    self.view.favorites_filter_input.pop();
                    self.view.apply_favorites_filter();
                    return KeyAction::Continue;
                }
                _ => return KeyAction::Continue,
            }
        }

        // Downloads filter typing mode
        if self.view.offline_filter_typing {
            match key.code {
                KeyCode::Esc => {
                    self.view.offline_filter_typing = false;
                    self.view.offline_filter_input.clear();
                    self.view.offline_filter_selected = 0;
                    return KeyAction::Continue;
                }
                KeyCode::Enter => {
                    self.view.offline_filter_typing = false;
                    return KeyAction::Continue;
                }
                KeyCode::Char(c) => {
                    self.view.offline_filter_input.push(c);
                    self.view.offline_filter_selected = 0;
                    return KeyAction::Continue;
                }
                KeyCode::Backspace => {
                    self.view.offline_filter_input.pop();
                    self.view.offline_filter_selected = 0;
                    return KeyAction::Continue;
                }
                _ => return KeyAction::Continue,
            }
        }

        // Radio filter typing mode
        if self.view.radio_filter_typing {
            match key.code {
                KeyCode::Esc => {
                    self.view.radio_filter_typing = false;
                    return KeyAction::Continue;
                }
                KeyCode::Enter => {
                    self.view.radio_filter_typing = false;
                    return KeyAction::Continue;
                }
                KeyCode::Char(c) => {
                    self.view.radio_filter_input.push(c);
                    self.view.apply_radio_filter();
                    return KeyAction::Continue;
                }
                KeyCode::Backspace => {
                    self.view.radio_filter_input.pop();
                    self.view.apply_radio_filter();
                    return KeyAction::Continue;
                }
                _ => return KeyAction::Continue,
            }
        }

        // Genres filter typing mode
        if self.view.genres_filter_typing {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.view.genres_filter_typing = false;
                    return KeyAction::Continue;
                }
                KeyCode::Char(c) => {
                    self.view.genres_filter_input.push(c);
                    self.view.apply_genres_filter();
                    return KeyAction::Continue;
                }
                KeyCode::Backspace => {
                    self.view.genres_filter_input.pop();
                    self.view.apply_genres_filter();
                    return KeyAction::Continue;
                }
                _ => return KeyAction::Continue,
            }
        }

        // Normal mode
        match key.code {
            // Tab navigation (disabled in offline mode — only Offline tab available)
            KeyCode::Tab | KeyCode::BackTab if self.view.is_offline => KeyAction::Continue,
            KeyCode::Tab => KeyAction::SendCommand(Command::NextTab),
            KeyCode::BackTab => KeyAction::SendCommand(Command::PrevTab),

            // Enter search/filter typing mode
            KeyCode::Char('/') => {
                self.view.focus_filter_input(true);
                KeyAction::Continue
            }

            // Open track context menu
            KeyCode::Char('x') => {
                self.open_item_popup();
                KeyAction::Continue
            }

            // Open album detail page
            KeyCode::Char('a') => self.open_album_detail(),

            // Open artist detail page
            KeyCode::Char('t') => self.open_artist_detail(),

            // Open waiting list
            KeyCode::Char('w') => {
                self.view.push_overlay(Overlay::WaitingList { selected: 0 });
                KeyAction::Continue
            }

            // Category navigation (h/l or left/right)
            KeyCode::Char('h') | KeyCode::Left => KeyAction::SendCommand(Command::PrevCategory),
            KeyCode::Char('l') | KeyCode::Right => KeyAction::SendCommand(Command::NextCategory),

            // List navigation
            KeyCode::Up | KeyCode::Char('k') => {
                if self.view.active_tab == ActiveTab::Explore {
                    match self.view.explore_category {
                        ExploreCategory::Moods => {
                            self.view.moods_selected = self.view.moods_selected.saturating_sub(1);
                        }
                        ExploreCategory::Categories => {
                            self.view.genres_selected = self.view.genres_selected.saturating_sub(1);
                        }
                        ExploreCategory::Radios => {
                            self.view.radios_selected = self.view.radios_selected.saturating_sub(1);
                        }
                    }
                    return KeyAction::Continue;
                }
                if self.view.active_tab == ActiveTab::Downloads && self.view.offline_filter_active()
                {
                    self.view.offline_filter_selected =
                        self.view.offline_filter_selected.saturating_sub(1);
                    return KeyAction::Continue;
                }
                if self.view.active_tab == ActiveTab::Favorites
                    && self.view.favorites_filter_active()
                {
                    self.view.favorites_filter_selected =
                        self.view.favorites_filter_selected.saturating_sub(1);
                    return KeyAction::Continue;
                }
                KeyAction::SendCommand(Command::SelectUp)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.view.active_tab == ActiveTab::Explore {
                    match self.view.explore_category {
                        ExploreCategory::Moods => {
                            if !self.view.moods.is_empty() {
                                self.view.moods_selected =
                                    (self.view.moods_selected + 1).min(self.view.moods.len() - 1);
                            }
                        }
                        ExploreCategory::Categories => {
                            if !self.view.genres_filtered.is_empty() {
                                self.view.genres_selected = (self.view.genres_selected + 1)
                                    .min(self.view.genres_filtered.len() - 1);
                            }
                        }
                        ExploreCategory::Radios => {
                            if !self.view.radios_filtered.is_empty() {
                                self.view.radios_selected = (self.view.radios_selected + 1)
                                    .min(self.view.radios_filtered.len() - 1);
                            }
                        }
                    }
                    return KeyAction::Continue;
                }
                if self.view.active_tab == ActiveTab::Downloads && self.view.offline_filter_active()
                {
                    let len = self.view.offline_list_len();
                    if len > 0 {
                        self.view.offline_filter_selected =
                            (self.view.offline_filter_selected + 1).min(len - 1);
                    }
                    return KeyAction::Continue;
                }
                if self.view.active_tab == ActiveTab::Favorites
                    && self.view.favorites_filter_active()
                {
                    if !self.view.favorites_filtered.is_empty() {
                        self.view.favorites_filter_selected = (self.view.favorites_filter_selected
                            + 1)
                        .min(self.view.favorites_filtered.len() - 1);
                    }
                    return KeyAction::Continue;
                }
                KeyAction::SendCommand(Command::SelectDown)
            }

            // Play selected track or open album detail
            KeyCode::Enter => {
                // Check if the selected item is an album (has album_id but no track)
                let item = match self.view.active_tab {
                    ActiveTab::Search => self.view.search_display.get(self.view.search_selected),
                    ActiveTab::Favorites => self.view.favorites_selected_item(),
                    _ => None,
                };
                if let Some(item) = item {
                    if item.track.is_none() {
                        if let Some(artist_id) = item.artist_id.clone() {
                            self.view.set_nav_overlay(Overlay::ArtistDetail);
                            self.view.artist_detail_selected = 0;
                            self.view.artist_detail_left_scroll = 0;
                            self.view.artist_detail_left_focused = false;
                            return KeyAction::SendCommand(Command::GetArtistDetail { artist_id });
                        }
                        if let Some(album_id) = item.album_id.clone() {
                            self.view
                                .set_nav_overlay(Overlay::AlbumDetail { from_artist: false });
                            self.view.album_detail_selected = 0;
                            self.view.album_detail_left_scroll = 0;
                            self.view.album_detail_left_focused = false;
                            return KeyAction::SendCommand(Command::GetAlbumDetail { album_id });
                        }
                        if let Some(playlist_id) = item.playlist_id.clone() {
                            self.view.reset_playlist_detail_filter();
                            self.view
                                .set_nav_overlay(Overlay::PlaylistDetail { selected: 0 });
                            return KeyAction::SendCommand(Command::GetPlaylistDetail {
                                playlist_id,
                            });
                        }
                        if let Some(show_id) = item.show_id.clone() {
                            self.view
                                .set_nav_overlay(Overlay::ShowDetail { selected: 0 });
                            return KeyAction::SendCommand(Command::GetShowDetail { show_id });
                        }
                    }
                }
                match self.view.active_tab {
                    ActiveTab::Search => KeyAction::SendCommand(Command::PlayFromSearch {
                        index: self.view.search_selected,
                    }),
                    ActiveTab::Favorites => KeyAction::SendCommand(Command::PlayFromFavorites {
                        index: self.view.favorites_selected_original_index(),
                    }),
                    ActiveTab::Explore => match self.view.explore_category {
                        ExploreCategory::Moods => {
                            if self.view.moods.is_empty() {
                                KeyAction::Continue
                            } else {
                                KeyAction::SendCommand(Command::PlayFromMood {
                                    index: self.view.moods_selected,
                                })
                            }
                        }
                        ExploreCategory::Radios => {
                            // Find the original index of the filtered radio in the full list
                            if let Some(filtered_radio) =
                                self.view.radios_filtered.get(self.view.radios_selected)
                            {
                                let original_idx = self
                                    .view
                                    .radios
                                    .iter()
                                    .position(|r| r.id == filtered_radio.id)
                                    .unwrap_or(0);
                                KeyAction::SendCommand(Command::PlayFromRadio {
                                    index: original_idx,
                                })
                            } else {
                                KeyAction::Continue
                            }
                        }
                        ExploreCategory::Categories => {
                            if let Some(genre) =
                                self.view.genres_filtered.get(self.view.genres_selected)
                            {
                                let genre_id = genre.id;
                                let name = genre.name.clone();
                                self.view.set_nav_overlay(Overlay::GenreDetail {
                                    sub_tab: GenreDetailSubTab::default(),
                                    selected: 0,
                                });
                                KeyAction::SendCommand(Command::GetGenreDetail { genre_id, name })
                            } else {
                                KeyAction::Continue
                            }
                        }
                    },
                    ActiveTab::Downloads => match self.view.offline_category {
                        OfflineCategory::Tracks => {
                            KeyAction::SendCommand(Command::PlayFromOffline {
                                index: self.view.offline_selected_track_index(),
                            })
                        }
                        OfflineCategory::Albums | OfflineCategory::Playlists => {
                            self.open_offline_detail()
                        }
                    },
                }
            }

            // Shuffle favorites
            KeyCode::Char('g') => {
                if self.view.active_tab == ActiveTab::Favorites {
                    return KeyAction::SendCommand(Command::ShuffleFavorites);
                }
                KeyAction::Continue
            }

            // Deezer Flow
            KeyCode::Char('f') => KeyAction::SendCommand(Command::StartFlow),

            // Toggle favorite / like for focused track or playing track
            KeyCode::Char('L') => self.toggle_like_focused_or_playing(),

            // Player controls
            KeyCode::Char(' ') => KeyAction::SendCommand(Command::TogglePause),
            KeyCode::Char('n') => KeyAction::SendCommand(Command::NextTrack),
            KeyCode::Char('b') => KeyAction::SendCommand(Command::PrevTrack),
            KeyCode::Char('s') => KeyAction::SendCommand(Command::ToggleShuffle),
            KeyCode::Char('r') => KeyAction::SendCommand(Command::CycleRepeat),

            // Volume
            KeyCode::Char('+') | KeyCode::Char('=') => {
                let new_vol = (self.view.volume + 0.05).min(1.0);
                KeyAction::SendCommand(Command::SetVolume { volume: new_vol })
            }
            KeyCode::Char('-') => {
                let new_vol = (self.view.volume - 0.05).max(0.0);
                KeyAction::SendCommand(Command::SetVolume { volume: new_vol })
            }

            // Escape in normal mode: open settings
            KeyCode::Esc => {
                self.view.overlay = Some(Overlay::Settings { selected: 0 });
                KeyAction::Continue
            }

            _ => KeyAction::Continue,
        }
    }

    /// Handle key events when an overlay is open.
    fn handle_overlay_key(&mut self, key: KeyEvent) -> KeyAction {
        let overlay = self.view.overlay.as_mut().unwrap();
        match overlay {
            Overlay::Help { scroll } => {
                match key.code {
                    KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?') => {
                        self.view.pop_overlay();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        *scroll += 1;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        *scroll = scroll.saturating_sub(1);
                    }
                    _ => {}
                }
                KeyAction::Continue
            }
            Overlay::Info => {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter | KeyCode::Char('i') => {
                        self.view.pop_overlay();
                    }
                    _ => {}
                }
                KeyAction::Continue
            }
            Overlay::Settings { selected } => {
                const SETTINGS_COUNT: usize = 7;
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.view.pop_overlay();
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        *selected = selected.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        *selected = (*selected + 1).min(SETTINGS_COUNT - 1);
                    }
                    KeyCode::Enter => {
                        match *selected {
                            0 => {
                                // Keyboard shortcuts
                                self.view.push_overlay(Overlay::Help { scroll: 0 });
                                return KeyAction::Continue;
                            }
                            1 => {
                                // Themes
                                let current = Theme::current();
                                let idx =
                                    ThemeId::ALL.iter().position(|&t| t == current).unwrap_or(0);
                                self.view
                                    .push_overlay(Overlay::ThemePicker { selected: idx });
                                return KeyAction::Continue;
                            }
                            2 => {
                                // Audio quality
                                let current = self.view.quality;
                                let idx = AudioQuality::ALL
                                    .iter()
                                    .position(|&q| q == current)
                                    .unwrap_or(1);
                                self.view
                                    .push_overlay(Overlay::QualityPicker { selected: idx });
                                return KeyAction::Continue;
                            }
                            3 => {
                                // Language
                                let current = i18n::current_locale();
                                let idx =
                                    Locale::ALL.iter().position(|&l| l == current).unwrap_or(0);
                                self.view
                                    .push_overlay(Overlay::LanguagePicker { selected: idx });
                                return KeyAction::Continue;
                            }
                            4 => {
                                // Logout
                                self.view.pop_overlay();
                                return KeyAction::SendCommand(Command::Logout);
                            }
                            5 => {
                                // Send to background
                                return KeyAction::Detach;
                            }
                            6 => {
                                // Quit
                                return KeyAction::Quit;
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
                // Also close on Ctrl+O
                if key.code == KeyCode::Char('o') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.view.pop_overlay();
                }
                KeyAction::Continue
            }
            Overlay::LanguagePicker { selected } => {
                let count = Locale::ALL.len();
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.view.pop_overlay();
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        *selected = selected.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        *selected = (*selected + 1).min(count - 1);
                    }
                    KeyCode::Enter => {
                        let locale = Locale::ALL[*selected];
                        i18n::set(locale);
                        let mut config = Config::load();
                        config.language = Some(locale.as_str().to_string());
                        let _ = config.save();
                        self.view.pop_overlay();
                    }
                    _ => {}
                }
                KeyAction::Continue
            }
            Overlay::QualityPicker { selected } => {
                let count = AudioQuality::ALL.len();
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.view.pop_overlay();
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        *selected = selected.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        *selected = (*selected + 1).min(count - 1);
                    }
                    KeyCode::Enter => {
                        let quality = AudioQuality::ALL[*selected];
                        self.view.pop_overlay();
                        return KeyAction::SendCommand(Command::SetQuality { quality });
                    }
                    _ => {}
                }
                KeyAction::Continue
            }
            Overlay::AlbumDetail { .. } => self.handle_album_detail_key(key),
            Overlay::ArtistDetail => self.handle_artist_detail_key(key),
            Overlay::PlaylistDetail { .. } => self.handle_playlist_detail_key(key),
            Overlay::ShowDetail { .. } => self.handle_show_detail_key(key),
            Overlay::GenreDetail { .. } => self.handle_genre_detail_key(key),
            Overlay::WaitingList { .. } => self.handle_waiting_list_key(key),
            Overlay::OfflineDetail { .. } => self.handle_offline_detail_key(key),
            Overlay::ThemePicker { selected } => {
                let count = ThemeId::ALL.len();
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        // Save transparency then go back to settings
                        let mut config = Config::load();
                        config.bg_transparency = Theme::transparency();
                        let _ = config.save();
                        self.view.pop_overlay();
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        *selected = selected.saturating_sub(1);
                        Theme::set(ThemeId::ALL[*selected]);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        *selected = (*selected + 1).min(count - 1);
                        Theme::set(ThemeId::ALL[*selected]);
                    }
                    KeyCode::Left => {
                        // Transparency is a toggle: knob left = off.
                        Theme::set_transparency(0);
                    }
                    KeyCode::Right => {
                        // Transparency is a toggle: knob right = on.
                        Theme::set_transparency(100);
                    }
                    KeyCode::Enter => {
                        // Confirm selection, save theme + transparency to config, back to settings
                        let theme_id = ThemeId::ALL[*selected];
                        let mut config = Config::load();
                        config.theme = Some(theme_id.as_str().to_string());
                        config.bg_transparency = Theme::transparency();
                        let _ = config.save();
                        self.view.pop_overlay();
                    }
                    _ => {}
                }
                KeyAction::Continue
            }
            Overlay::UpdateAvailable {
                version,
                download_url,
                selected,
            } => {
                const UPDATE_OPTIONS: usize = 3;
                match key.code {
                    KeyCode::Esc => {
                        self.view.pop_overlay();
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        *selected = selected.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        *selected = (*selected + 1).min(UPDATE_OPTIONS - 1);
                    }
                    KeyCode::Enter => match *selected {
                        0 => {
                            let ver = version.clone();
                            let url = download_url.clone();
                            return KeyAction::PerformUpdate {
                                version: ver,
                                download_url: url,
                            };
                        }
                        1 => {
                            // "Later" — dismiss for this session
                            self.view.pop_overlay();
                        }
                        2 => {
                            // "Never ask again" — persist to config
                            let mut config = Config::load();
                            config.skip_update_check = true;
                            let _ = config.save();
                            self.view.pop_overlay();
                        }
                        _ => {}
                    },
                    _ => {}
                }
                KeyAction::Continue
            }
            Overlay::Updating { .. } => {
                // Ignore all keys while update is in progress
                KeyAction::Continue
            }
        }
    }

    /// Open the full track context menu for the selected track in the current list.
    /// Open the track list of the selected downloaded album/playlist as a modal.
    fn open_offline_detail(&mut self) -> KeyAction {
        let target = match self.view.offline_category {
            OfflineCategory::Playlists => self
                .view
                .offline_selected_playlist()
                .map(|(i, _)| (true, i)),
            _ => self.view.offline_selected_album().map(|(i, _)| (false, i)),
        };
        let Some((playlist, index)) = target else {
            return KeyAction::Continue;
        };
        self.view.offline_detail_filter_input.clear();
        self.view.offline_detail_filter_typing = false;
        self.view.push_overlay(Overlay::OfflineDetail {
            playlist,
            index,
            selected: 0,
        });
        KeyAction::Continue
    }

    /// Optimistically toggle a track's favorite status in the local view and send
    /// the corresponding AddFavorite/RemoveFavorite command to the daemon.
    fn toggle_track_favorite(&mut self, track_id: &str) -> KeyAction {
        let is_fav = self.view.is_track_favorite(track_id);
        if is_fav {
            self.view.favorite_track_ids.retain(|id| id != track_id);
            self.view.favorites.retain(|t| t.track_id != track_id);
            KeyAction::SendCommand(Command::RemoveFavorite {
                track_id: track_id.to_string(),
            })
        } else {
            if !self.view.favorite_track_ids.iter().any(|id| id == track_id) {
                self.view.favorite_track_ids.push(track_id.to_string());
            }
            KeyAction::SendCommand(Command::AddFavorite {
                track_id: track_id.to_string(),
            })
        }
    }

    /// Toggle favorite for the currently focused track in any list/overlay, or
    /// fall back to toggling the currently playing track.
    fn toggle_like_focused_or_playing(&mut self) -> KeyAction {
        // 1. If an overlay with tracks is open:
        if let Some(ref overlay) = self.view.overlay {
            match overlay {
                Overlay::AlbumDetail { .. } => {
                    if let Some(ref detail) = self.view.album_detail {
                        if let Some(track) = detail.tracks.get(self.view.album_detail_selected) {
                            let track_id = track.track_id.clone();
                            return self.toggle_track_favorite(&track_id);
                        }
                    }
                }
                Overlay::ArtistDetail => {
                    if self.view.artist_detail_sub_tab == ArtistSubTab::TopTracks {
                        if let Some(ref detail) = self.view.artist_detail {
                            if let Some(track) =
                                detail.top_tracks.get(self.view.artist_detail_selected)
                            {
                                let track_id = track.track_id.clone();
                                return self.toggle_track_favorite(&track_id);
                            }
                        }
                    }
                }
                Overlay::PlaylistDetail { selected } => {
                    let filtered = self.view.playlist_detail_tracks_filtered();
                    if let Some((_, track)) = filtered.get(*selected) {
                        let track_id = track.track_id.clone();
                        drop(filtered);
                        return self.toggle_track_favorite(&track_id);
                    }
                }
                Overlay::GenreDetail { sub_tab, selected } => {
                    if *sub_tab == GenreDetailSubTab::Tracks {
                        if let Some(ref detail) = self.view.genre_detail {
                            if let Some(track) = detail.tracks.get(*selected) {
                                let track_id = track.track_id.clone();
                                return self.toggle_track_favorite(&track_id);
                            }
                        }
                    }
                }
                Overlay::WaitingList { selected } => {
                    if let Some(track) = self.view.queue.get(*selected) {
                        let track_id = track.track_id.clone();
                        return self.toggle_track_favorite(&track_id);
                    }
                }
                _ => {}
            }
        }

        // 2. If on main screen without popup/modal:
        if self.view.screen == Screen::Main
            && self.view.overlay.is_none()
            && self.view.popup.is_none()
        {
            match self.view.active_tab {
                ActiveTab::Search => {
                    if self.view.search_category == SearchCategory::Track {
                        if let Some(item) = self.view.search_display.get(self.view.search_selected)
                        {
                            if let Some(ref track) = item.track {
                                let track_id = track.track_id.clone();
                                return self.toggle_track_favorite(&track_id);
                            }
                        }
                    }
                }
                ActiveTab::Favorites => match self.view.favorites_category {
                    FavoritesCategory::Tracks | FavoritesCategory::RecentlyPlayed => {
                        let selected = if self.view.favorites_filter_active() {
                            self.view.favorites_filter_selected
                        } else {
                            self.view.favorites_selected
                        };
                        let items: Vec<_> = if self.view.favorites_filter_active() {
                            self.view
                                .favorites_filtered
                                .iter()
                                .map(|(_, item)| item)
                                .collect()
                        } else {
                            self.view.favorites_display.iter().collect()
                        };
                        if let Some(item) = items.get(selected) {
                            if let Some(ref track) = item.track {
                                let track_id = track.track_id.clone();
                                return self.toggle_track_favorite(&track_id);
                            }
                        }
                    }
                    FavoritesCategory::Artists => {
                        if let Some(item) = self
                            .view
                            .favorites_display
                            .get(self.view.favorites_selected)
                        {
                            if let Some(ref artist_id) = item.artist_id {
                                let is_fav = self.view.is_artist_favorite(artist_id);
                                let cmd = if is_fav {
                                    Command::RemoveFavoriteArtist {
                                        artist_id: artist_id.clone(),
                                    }
                                } else {
                                    Command::AddFavoriteArtist {
                                        artist_id: artist_id.clone(),
                                    }
                                };
                                return KeyAction::SendCommand(cmd);
                            }
                        }
                    }
                    FavoritesCategory::Albums => {
                        if let Some(item) = self
                            .view
                            .favorites_display
                            .get(self.view.favorites_selected)
                        {
                            if let Some(ref album_id) = item.album_id {
                                let is_fav = self.view.is_album_favorite(album_id);
                                let cmd = if is_fav {
                                    Command::RemoveFavoriteAlbum {
                                        album_id: album_id.clone(),
                                    }
                                } else {
                                    Command::AddFavoriteAlbum {
                                        album_id: album_id.clone(),
                                    }
                                };
                                return KeyAction::SendCommand(cmd);
                            }
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        // 3. Fallback: toggle currently playing track
        if let Some(ref track) = self.view.current_track {
            let track_id = track.track_id.clone();
            return self.toggle_track_favorite(&track_id);
        }

        KeyAction::Continue
    }

    /// Player controls that stay available while a detail overlay is open.
    /// Returns `None` when the key is not a player control, so callers keep
    /// their own fallthrough behaviour.
    ///
    /// Shared by every detail overlay so the set can't drift between them —
    /// `s` and `r` used to be missing from all of them.
    fn player_control_action(&self, key: KeyCode) -> Option<KeyAction> {
        let cmd = match key {
            KeyCode::Char(' ') => Command::TogglePause,
            KeyCode::Char('n') => Command::NextTrack,
            KeyCode::Char('b') => Command::PrevTrack,
            KeyCode::Char('s') => Command::ToggleShuffle,
            KeyCode::Char('r') => Command::CycleRepeat,
            KeyCode::Char('L') => {
                if let Some(ref track) = self.view.current_track {
                    let is_fav = self.view.is_track_favorite(&track.track_id);
                    let cmd = if is_fav {
                        Command::RemoveFavorite {
                            track_id: track.track_id.clone(),
                        }
                    } else {
                        Command::AddFavorite {
                            track_id: track.track_id.clone(),
                        }
                    };
                    return Some(KeyAction::SendCommand(cmd));
                }
                return None;
            }
            KeyCode::Char('+') | KeyCode::Char('=') => Command::SetVolume {
                volume: (self.view.volume + 0.05).min(1.0),
            },
            KeyCode::Char('-') => Command::SetVolume {
                volume: (self.view.volume - 0.05).max(0.0),
            },
            _ => return None,
        };
        Some(KeyAction::SendCommand(cmd))
    }

    /// Handle key events in the offline album/playlist detail modal.
    fn handle_offline_detail_key(&mut self, key: KeyEvent) -> KeyAction {
        let (playlist, index, selected) = match self.view.overlay {
            Some(Overlay::OfflineDetail {
                playlist,
                index,
                selected,
            }) => (playlist, index, selected),
            _ => return KeyAction::Continue,
        };

        // Filter typing mode swallows most keys.
        if self.view.offline_detail_filter_typing {
            match key.code {
                KeyCode::Esc => {
                    self.view.offline_detail_filter_typing = false;
                    self.view.offline_detail_filter_input.clear();
                    self.set_offline_detail_selected(0);
                }
                KeyCode::Enter => self.view.offline_detail_filter_typing = false,
                KeyCode::Char(c) => {
                    self.view.offline_detail_filter_input.push(c);
                    self.set_offline_detail_selected(0);
                }
                KeyCode::Backspace => {
                    self.view.offline_detail_filter_input.pop();
                    self.set_offline_detail_selected(0);
                }
                _ => {}
            }
            return KeyAction::Continue;
        }

        let tracks: Vec<(usize, String)> = self
            .view
            .offline_detail_tracks(playlist, index)
            .iter()
            .map(|(i, t)| (*i, t.track_id.clone()))
            .collect();

        match key.code {
            KeyCode::Esc => {
                self.view.offline_detail_filter_input.clear();
                self.view.pop_overlay();
                KeyAction::Continue
            }
            KeyCode::Char('/') => {
                self.view.offline_detail_filter_typing = true;
                self.view.offline_detail_filter_input.clear();
                self.set_offline_detail_selected(0);
                KeyAction::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.set_offline_detail_selected(selected.saturating_sub(1));
                KeyAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = tracks.len().saturating_sub(1);
                self.set_offline_detail_selected((selected + 1).min(max));
                KeyAction::Continue
            }
            KeyCode::Enter => {
                let Some((track_index, _)) = tracks.get(selected) else {
                    return KeyAction::Continue;
                };
                if playlist {
                    let Some(p) = self.view.offline_playlists.get(index) else {
                        return KeyAction::Continue;
                    };
                    KeyAction::SendCommand(Command::PlayOfflinePlaylist {
                        playlist_id: p.playlist_id.clone(),
                        track_index: *track_index,
                    })
                } else {
                    let Some(a) = self.view.offline_albums.get(index) else {
                        return KeyAction::Continue;
                    };
                    KeyAction::SendCommand(Command::PlayOfflineAlbum {
                        album_id: a.album_id.clone(),
                        track_index: *track_index,
                    })
                }
            }
            KeyCode::Char('x') => {
                if let Some((track_index, _)) = tracks.get(selected) {
                    let track = if playlist {
                        self.view
                            .offline_playlists
                            .get(index)
                            .and_then(|p| p.tracks.get(*track_index))
                    } else {
                        self.view
                            .offline_albums
                            .get(index)
                            .and_then(|a| a.tracks.get(*track_index))
                    };
                    if let Some(track) = track.cloned() {
                        self.view.popup = Some(PopupMenu::remove_offline_only(PopupTarget::Track(
                            Box::new(track),
                        )));
                    }
                }
                KeyAction::Continue
            }
            // Player controls (play/pause, prev/next, shuffle, repeat,
            // volume) stay active while this overlay is open.
            other => self
                .player_control_action(other)
                .unwrap_or(KeyAction::Continue),
        }
    }

    /// Move the cursor of the offline detail modal.
    fn set_offline_detail_selected(&mut self, index: usize) {
        if let Some(Overlay::OfflineDetail { selected, .. }) = self.view.overlay.as_mut() {
            *selected = index;
        }
    }

    fn open_item_popup(&mut self) {
        // Downloads tab: everything listed is already stored locally, so the
        // only action offered is dropping it from offline storage.
        if self.view.active_tab == ActiveTab::Downloads {
            if let Some(target) = self.offline_popup_target() {
                self.view.popup = Some(PopupMenu::remove_offline_only(target));
            }
            return;
        }

        // Try to get a DisplayItem from the current tab
        let display_item = match self.view.active_tab {
            ActiveTab::Search => self
                .view
                .search_display
                .get(self.view.search_selected)
                .cloned(),
            ActiveTab::Favorites => self.view.favorites_selected_item().cloned(),
            _ => None,
        };

        // Check if the item is an artist or album (non-track item)
        if let Some(ref item) = display_item {
            if let Some(ref artist_id) = item.artist_id {
                if item.track.is_none() {
                    let is_fav = self.view.is_artist_favorite(artist_id);
                    self.view.popup = Some(PopupMenu::for_artist(
                        artist_id.clone(),
                        item.col1.clone(),
                        is_fav,
                    ));
                    return;
                }
            }
            if let Some(ref album_id) = item.album_id {
                if item.track.is_none() {
                    let is_fav = self.view.is_album_favorite(album_id);
                    self.view.popup = Some(PopupMenu::for_album(
                        album_id.clone(),
                        item.col1.clone(),
                        item.col2.clone(),
                        is_fav,
                    ));
                    return;
                }
            }
            if let Some(ref playlist_id) = item.playlist_id {
                if item.track.is_none() {
                    let nb_songs = item
                        .col3
                        .split_whitespace()
                        .next()
                        .and_then(|n| n.parse::<u64>().ok())
                        .unwrap_or(0);
                    self.view.popup = Some(PopupMenu::for_playlist(
                        playlist_id.clone(),
                        item.col1.clone(),
                        nb_songs,
                    ));
                    return;
                }
            }
        }

        // Fall back to track popup
        let track = match self.view.active_tab {
            ActiveTab::Search | ActiveTab::Favorites => display_item.and_then(|d| d.track),
            _ => None,
        };

        if let Some(track) = track {
            let is_fav = self.view.is_track_favorite(&track.track_id);
            self.view.popup = Some(PopupMenu::full(track, is_fav));
        }
    }

    /// What the Downloads tab cursor is pointing at: a stand-alone track, a
    /// downloaded album/playlist, or a track inside one of them.
    fn offline_popup_target(&self) -> Option<PopupTarget> {
        match self.view.offline_category {
            OfflineCategory::Tracks => self
                .view
                .offline_selected_track()
                .map(|ot| PopupTarget::Track(Box::new(ot.track.clone()))),
            OfflineCategory::Albums => {
                let (_, album) = self.view.offline_selected_album()?;
                Some(PopupTarget::Album {
                    album_id: album.album_id.clone(),
                    title: album.title.clone(),
                    artist: album.artist.clone(),
                })
            }
            OfflineCategory::Playlists => {
                let (_, playlist) = self.view.offline_selected_playlist()?;
                Some(PopupTarget::Playlist {
                    playlist_id: playlist.playlist_id.clone(),
                    title: playlist.title.clone(),
                    nb_songs: playlist.nb_tracks,
                })
            }
        }
    }

    /// Handle key events in the album detail overlay.
    fn handle_album_detail_key(&mut self, key: KeyEvent) -> KeyAction {
        // x: context menu for focused track
        if key.code == KeyCode::Char('x') {
            if let Some(ref detail) = self.view.album_detail {
                if let Some(track) = detail.tracks.get(self.view.album_detail_selected).cloned() {
                    let is_fav = self.view.is_track_favorite(&track.track_id);
                    self.view.popup = Some(PopupMenu::full(track, is_fav));
                }
            }
            return KeyAction::Continue;
        }
        // f / L: toggle favorite for focused track
        if key.code == KeyCode::Char('f') || key.code == KeyCode::Char('L') {
            if let Some(ref detail) = self.view.album_detail {
                if let Some(track) = detail.tracks.get(self.view.album_detail_selected) {
                    let track_id = track.track_id.clone();
                    return self.toggle_track_favorite(&track_id);
                }
            }
            return KeyAction::Continue;
        }
        match key.code {
            KeyCode::Esc => {
                self.view.pop_overlay();
                KeyAction::Continue
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if self.view.album_detail_left_scrollable {
                    self.view.album_detail_left_focused = true;
                }
                KeyAction::Continue
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.view.album_detail_left_focused = false;
                KeyAction::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.view.album_detail_left_focused {
                    self.view.album_detail_left_scroll =
                        self.view.album_detail_left_scroll.saturating_sub(1);
                } else {
                    self.view.album_detail_selected =
                        self.view.album_detail_selected.saturating_sub(1);
                }
                KeyAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.view.album_detail_left_focused {
                    self.view.album_detail_left_scroll =
                        self.view.album_detail_left_scroll.saturating_add(1);
                } else if let Some(ref detail) = self.view.album_detail {
                    if !detail.tracks.is_empty() {
                        self.view.album_detail_selected =
                            (self.view.album_detail_selected + 1).min(detail.tracks.len() - 1);
                    }
                }
                KeyAction::Continue
            }
            KeyCode::Enter => {
                if self.view.album_detail_left_focused {
                    KeyAction::Continue
                } else {
                    let index = self.view.album_detail_selected;
                    KeyAction::SendCommand(Command::PlayFromAlbum { index })
                }
            }
            KeyCode::Char('t') => {
                if let Some(ref detail) = self.view.album_detail {
                    if let Some(track) = detail.tracks.get(self.view.album_detail_selected) {
                        if let Some(ref artist_id) = track.artist_id {
                            let artist_id = artist_id.clone();
                            self.view.push_overlay(Overlay::ArtistDetail);
                            self.view.artist_detail_selected = 0;
                            return KeyAction::SendCommand(Command::GetArtistDetail { artist_id });
                        }
                    }
                }
                KeyAction::Continue
            }
            KeyCode::Char('d') => {
                if let Some(ref detail) = self.view.album_detail {
                    let album_id = detail.album_id.clone();
                    KeyAction::SendCommand(Command::DownloadAlbumOffline { album_id })
                } else {
                    KeyAction::Continue
                }
            }
            // Open waiting list on top of album detail
            KeyCode::Char('w') => {
                self.view.push_overlay(Overlay::WaitingList { selected: 0 });
                KeyAction::Continue
            }
            // Player controls (play/pause, prev/next, shuffle, repeat,
            // volume) stay active while this overlay is open.
            other => self
                .player_control_action(other)
                .unwrap_or(KeyAction::Continue),
        }
    }

    /// Handle key events in the artist detail overlay.
    fn handle_artist_detail_key(&mut self, key: KeyEvent) -> KeyAction {
        // x: context menu for focused track
        if key.code == KeyCode::Char('x') {
            if self.view.artist_detail_sub_tab == ArtistSubTab::TopTracks {
                if let Some(ref detail) = self.view.artist_detail {
                    if let Some(track) = detail
                        .top_tracks
                        .get(self.view.artist_detail_selected)
                        .cloned()
                    {
                        let is_fav = self.view.is_track_favorite(&track.track_id);
                        self.view.popup = Some(PopupMenu::full(track, is_fav));
                    }
                }
            }
            return KeyAction::Continue;
        }
        // f / L: toggle favorite for focused track or artist
        if key.code == KeyCode::Char('f') || key.code == KeyCode::Char('L') {
            if self.view.artist_detail_sub_tab == ArtistSubTab::TopTracks {
                if let Some(ref detail) = self.view.artist_detail {
                    if let Some(track) = detail.top_tracks.get(self.view.artist_detail_selected) {
                        let track_id = track.track_id.clone();
                        return self.toggle_track_favorite(&track_id);
                    }
                }
            } else if self.view.artist_detail_sub_tab == ArtistSubTab::Similar {
                if let Some(ref detail) = self.view.artist_detail {
                    if let Some(artist) =
                        detail.similar_artists.get(self.view.artist_detail_selected)
                    {
                        let is_fav = self.view.is_artist_favorite(&artist.artist_id);
                        let cmd = if is_fav {
                            Command::RemoveFavoriteArtist {
                                artist_id: artist.artist_id.clone(),
                            }
                        } else {
                            Command::AddFavoriteArtist {
                                artist_id: artist.artist_id.clone(),
                            }
                        };
                        return KeyAction::SendCommand(cmd);
                    }
                }
            }
            return KeyAction::Continue;
        }
        match key.code {
            KeyCode::Esc => {
                self.view.pop_overlay();
                KeyAction::Continue
            }
            // Switch sub-tab with h/l; left panel is a virtual tab before TopTracks
            KeyCode::Char('h') | KeyCode::Left => {
                if self.view.artist_detail_left_focused {
                    // Already on left panel, nothing to do
                } else if self.view.artist_detail_sub_tab == ArtistSubTab::TopTracks
                    && self.view.artist_detail_left_scrollable
                {
                    // Step into left panel only if it has scrollable content
                    self.view.artist_detail_left_focused = true;
                } else {
                    self.view.artist_detail_sub_tab = self.view.artist_detail_sub_tab.prev();
                    self.view.artist_detail_selected = 0;
                }
                KeyAction::Continue
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if self.view.artist_detail_left_focused {
                    // Step out of left panel into TopTracks
                    self.view.artist_detail_left_focused = false;
                    self.view.artist_detail_sub_tab = ArtistSubTab::TopTracks;
                    self.view.artist_detail_selected = 0;
                } else {
                    self.view.artist_detail_sub_tab = self.view.artist_detail_sub_tab.next();
                    self.view.artist_detail_selected = 0;
                }
                KeyAction::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.view.artist_detail_left_focused {
                    self.view.artist_detail_left_scroll =
                        self.view.artist_detail_left_scroll.saturating_sub(1);
                } else {
                    self.view.artist_detail_selected =
                        self.view.artist_detail_selected.saturating_sub(1);
                }
                KeyAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.view.artist_detail_left_focused {
                    self.view.artist_detail_left_scroll =
                        self.view.artist_detail_left_scroll.saturating_add(1);
                } else {
                    let count = self.artist_detail_list_len();
                    if count > 0 {
                        self.view.artist_detail_selected =
                            (self.view.artist_detail_selected + 1).min(count - 1);
                    }
                }
                KeyAction::Continue
            }
            KeyCode::Enter => {
                let sub_tab = self.view.artist_detail_sub_tab;
                let index = self.view.artist_detail_selected;
                if sub_tab == ArtistSubTab::TopTracks {
                    KeyAction::SendCommand(Command::PlayFromArtist { index })
                } else if sub_tab == ArtistSubTab::Similar {
                    // Open the artist detail for the selected similar artist
                    if let Some(ref detail) = self.view.artist_detail {
                        if let Some(artist) = detail.similar_artists.get(index) {
                            let artist_id = artist.artist_id.clone();
                            self.view.set_nav_overlay(Overlay::ArtistDetail);
                            self.view.artist_detail_selected = 0;
                            self.view.artist_detail_sub_tab = ArtistSubTab::TopTracks;
                            self.view.artist_detail_left_scroll = 0;
                            self.view.artist_detail_left_focused = false;
                            return KeyAction::SendCommand(Command::GetArtistDetail { artist_id });
                        }
                    }
                    KeyAction::Continue
                } else {
                    // Open the album detail for the selected album entry
                    if let Some(ref detail) = self.view.artist_detail {
                        let albums = detail.albums_for_tab(sub_tab);
                        if let Some(album) = albums.get(index) {
                            let album_id = album.album_id.clone();
                            self.view
                                .push_overlay(Overlay::AlbumDetail { from_artist: false });
                            self.view.album_detail_selected = 0;
                            return KeyAction::SendCommand(Command::GetAlbumDetail { album_id });
                        }
                    }
                    KeyAction::Continue
                }
            }
            // Open waiting list on top of artist detail
            KeyCode::Char('w') => {
                self.view.push_overlay(Overlay::WaitingList { selected: 0 });
                KeyAction::Continue
            }
            // Player controls (play/pause, prev/next, shuffle, repeat,
            // volume) stay active while this overlay is open.
            other => self
                .player_control_action(other)
                .unwrap_or(KeyAction::Continue),
        }
    }

    /// Get the number of items in the current artist detail sub-tab list.
    fn artist_detail_list_len(&self) -> usize {
        if let Some(ref detail) = self.view.artist_detail {
            match self.view.artist_detail_sub_tab {
                ArtistSubTab::TopTracks => detail.top_tracks.len(),
                ArtistSubTab::Similar => detail.similar_artists.len(),
                other => detail.albums_for_tab(other).len(),
            }
        } else {
            0
        }
    }

    /// Open the album detail overlay for the selected item.
    fn open_album_detail(&mut self) -> KeyAction {
        let item = match self.view.active_tab {
            ActiveTab::Search => self.view.search_display.get(self.view.search_selected),
            ActiveTab::Favorites => self.view.favorites_selected_item(),
            _ => None,
        };

        // Try to get album_id from the DisplayItem directly (album search/favorites)
        if let Some(album_id) = item.and_then(|i| i.album_id.clone()) {
            self.view
                .set_nav_overlay(Overlay::AlbumDetail { from_artist: false });
            self.view.album_detail_selected = 0;
            return KeyAction::SendCommand(Command::GetAlbumDetail { album_id });
        }

        // For tracks, get album_id from the embedded TrackData
        if let Some(album_id) = item
            .and_then(|i| i.track.as_ref())
            .and_then(|t| t.album_id.clone())
        {
            self.view
                .set_nav_overlay(Overlay::AlbumDetail { from_artist: false });
            self.view.album_detail_selected = 0;
            return KeyAction::SendCommand(Command::GetAlbumDetail { album_id });
        }

        KeyAction::Continue
    }

    /// Open artist detail page for the currently selected item.
    fn open_artist_detail(&mut self) -> KeyAction {
        let item = match self.view.active_tab {
            ActiveTab::Search => self.view.search_display.get(self.view.search_selected),
            ActiveTab::Favorites => self.view.favorites_selected_item(),
            _ => None,
        };

        // Try artist_id directly from the DisplayItem (artist search/favorites)
        if let Some(artist_id) = item.and_then(|i| i.artist_id.clone()) {
            self.view.set_nav_overlay(Overlay::ArtistDetail);
            self.view.artist_detail_selected = 0;
            return KeyAction::SendCommand(Command::GetArtistDetail { artist_id });
        }

        // For tracks, get artist_id from the embedded TrackData
        if let Some(artist_id) = item
            .and_then(|i| i.track.as_ref())
            .and_then(|t| t.artist_id.clone())
        {
            self.view.set_nav_overlay(Overlay::ArtistDetail);
            self.view.artist_detail_selected = 0;
            return KeyAction::SendCommand(Command::GetArtistDetail { artist_id });
        }

        KeyAction::Continue
    }

    /// Handle key events in the playlist detail overlay.
    fn handle_playlist_detail_key(&mut self, key: KeyEvent) -> KeyAction {
        let selected = match self.view.overlay {
            Some(Overlay::PlaylistDetail { selected }) => selected,
            _ => return KeyAction::Continue,
        };

        // Filter typing mode swallows most keys (title/artist search via `/`).
        if self.view.playlist_detail_filter_typing {
            match key.code {
                KeyCode::Esc => {
                    self.view.playlist_detail_filter_typing = false;
                    self.view.playlist_detail_filter_input.clear();
                    self.set_playlist_detail_selected(0);
                }
                KeyCode::Enter => self.view.playlist_detail_filter_typing = false,
                KeyCode::Char(c) => {
                    self.view.playlist_detail_filter_input.push(c);
                    self.set_playlist_detail_selected(0);
                }
                KeyCode::Backspace => {
                    self.view.playlist_detail_filter_input.pop();
                    self.set_playlist_detail_selected(0);
                }
                _ => {}
            }
            return KeyAction::Continue;
        }

        // The cursor addresses the filtered view; resolve it to the track's real
        // index in the playlist (and a clone of that one track) up front, so the
        // borrow is released before mutating the view below.
        let filtered = self.view.playlist_detail_tracks_filtered();
        let filtered_len = filtered.len();
        let focused: Option<(usize, TrackData)> =
            filtered.get(selected).map(|(i, t)| (*i, (*t).clone()));
        drop(filtered);

        // x: context menu for focused track
        if key.code == KeyCode::Char('x') {
            if let (Some((_, track)), Some(playlist_id)) = (
                focused.as_ref(),
                self.view
                    .playlist_detail
                    .as_ref()
                    .map(|d| d.playlist_id.clone()),
            ) {
                let is_fav = self.view.is_track_favorite(&track.track_id);
                self.view.popup = Some(PopupMenu::full_in_playlist(
                    track.clone(),
                    is_fav,
                    playlist_id,
                ));
            }
            return KeyAction::Continue;
        }

        // f / L: toggle favorite for focused track
        if key.code == KeyCode::Char('f') || key.code == KeyCode::Char('L') {
            if let Some((_, track)) = focused.as_ref() {
                let track_id = track.track_id.clone();
                return self.toggle_track_favorite(&track_id);
            }
            return KeyAction::Continue;
        }

        match key.code {
            KeyCode::Esc => {
                self.view.playlist_detail_filter_input.clear();
                self.view.pop_overlay();
                KeyAction::Continue
            }
            KeyCode::Char('/') => {
                self.view.playlist_detail_filter_typing = true;
                self.view.playlist_detail_filter_input.clear();
                self.set_playlist_detail_selected(0);
                KeyAction::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let new_sel = selected.saturating_sub(1);
                self.view.overlay = Some(Overlay::PlaylistDetail { selected: new_sel });
                KeyAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = filtered_len.saturating_sub(1);
                let new_sel = (selected + 1).min(max);
                self.view.overlay = Some(Overlay::PlaylistDetail { selected: new_sel });
                KeyAction::Continue
            }
            KeyCode::Enter => match focused {
                Some((track_index, _)) => {
                    KeyAction::SendCommand(Command::PlayFromPlaylist { index: track_index })
                }
                None => KeyAction::Continue,
            },
            // Download the whole playlist for offline mode (same as `d` on an album)
            KeyCode::Char('d') => {
                if let Some(ref detail) = self.view.playlist_detail {
                    KeyAction::SendCommand(Command::DownloadPlaylistOffline {
                        playlist_id: detail.playlist_id.clone(),
                    })
                } else {
                    KeyAction::Continue
                }
            }
            // Open waiting list on top of playlist detail
            KeyCode::Char('w') => {
                self.view.push_overlay(Overlay::WaitingList { selected: 0 });
                KeyAction::Continue
            }
            // Player controls (play/pause, prev/next, shuffle, repeat,
            // volume) stay active while this overlay is open.
            other => self
                .player_control_action(other)
                .unwrap_or(KeyAction::Continue),
        }
    }

    /// Move the cursor of the playlist detail modal.
    fn set_playlist_detail_selected(&mut self, index: usize) {
        if let Some(Overlay::PlaylistDetail { selected }) = self.view.overlay.as_mut() {
            *selected = index;
        }
    }

    /// Handle key events in the podcast show overlay. Same navigation as the
    /// playlist detail, without the playlist-only actions (`x`, `d`).
    fn handle_show_detail_key(&mut self, key: KeyEvent) -> KeyAction {
        let selected = match self.view.overlay {
            Some(Overlay::ShowDetail { selected }) => selected,
            _ => return KeyAction::Continue,
        };

        let episode_count = self
            .view
            .playlist_detail
            .as_ref()
            .map(|d| d.tracks.len())
            .unwrap_or(0);

        match key.code {
            KeyCode::Esc => {
                self.view.pop_overlay();
                KeyAction::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let new_sel = selected.saturating_sub(1);
                self.view.overlay = Some(Overlay::ShowDetail { selected: new_sel });
                KeyAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = episode_count.saturating_sub(1);
                let new_sel = (selected + 1).min(max);
                self.view.overlay = Some(Overlay::ShowDetail { selected: new_sel });
                KeyAction::Continue
            }
            KeyCode::Enter => KeyAction::SendCommand(Command::PlayFromPlaylist { index: selected }),
            // Open waiting list on top of the show
            KeyCode::Char('w') => {
                self.view.push_overlay(Overlay::WaitingList { selected: 0 });
                KeyAction::Continue
            }
            // Player controls (play/pause, prev/next, shuffle, repeat,
            // volume) stay active while this overlay is open.
            other => self
                .player_control_action(other)
                .unwrap_or(KeyAction::Continue),
        }
    }

    /// Handle key events in the genre/category detail overlay.
    fn handle_genre_detail_key(&mut self, key: KeyEvent) -> KeyAction {
        let (sub_tab, selected) = match self.view.overlay {
            Some(Overlay::GenreDetail { sub_tab, selected }) => (sub_tab, selected),
            _ => return KeyAction::Continue,
        };

        let count = self
            .view
            .genre_detail
            .as_ref()
            .map(|d| match sub_tab {
                GenreDetailSubTab::Tracks => d.tracks.len(),
                GenreDetailSubTab::Albums => d.albums.len(),
                GenreDetailSubTab::Artists => d.artists.len(),
                GenreDetailSubTab::Playlists => d.playlists.len(),
                GenreDetailSubTab::Radios => d.radios.len(),
            })
            .unwrap_or(0);

        // f / L: toggle favorite for focused track
        if (key.code == KeyCode::Char('f') || key.code == KeyCode::Char('L'))
            && sub_tab == GenreDetailSubTab::Tracks
        {
            if let Some(ref detail) = self.view.genre_detail {
                if let Some(track) = detail.tracks.get(selected) {
                    let track_id = track.track_id.clone();
                    return self.toggle_track_favorite(&track_id);
                }
            }
            return KeyAction::Continue;
        }

        match key.code {
            KeyCode::Esc => {
                self.view.pop_overlay();
                KeyAction::Continue
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.view.overlay = Some(Overlay::GenreDetail {
                    sub_tab: sub_tab.prev(),
                    selected: 0,
                });
                KeyAction::Continue
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.view.overlay = Some(Overlay::GenreDetail {
                    sub_tab: sub_tab.next(),
                    selected: 0,
                });
                KeyAction::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let new_sel = selected.saturating_sub(1);
                self.view.overlay = Some(Overlay::GenreDetail {
                    sub_tab,
                    selected: new_sel,
                });
                KeyAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = count.saturating_sub(1);
                let new_sel = (selected + 1).min(max);
                self.view.overlay = Some(Overlay::GenreDetail {
                    sub_tab,
                    selected: new_sel,
                });
                KeyAction::Continue
            }
            KeyCode::Enter => {
                let Some(detail) = self.view.genre_detail.as_ref() else {
                    return KeyAction::Continue;
                };
                match sub_tab {
                    GenreDetailSubTab::Tracks => {
                        if selected < detail.tracks.len() {
                            KeyAction::SendCommand(Command::PlayFromGenreTrack { index: selected })
                        } else {
                            KeyAction::Continue
                        }
                    }
                    GenreDetailSubTab::Albums => {
                        if let Some(album) = detail.albums.get(selected) {
                            let album_id = album.album_id.clone();
                            self.view
                                .push_overlay(Overlay::AlbumDetail { from_artist: false });
                            self.view.album_detail_selected = 0;
                            self.view.album_detail_left_scroll = 0;
                            self.view.album_detail_left_focused = false;
                            KeyAction::SendCommand(Command::GetAlbumDetail { album_id })
                        } else {
                            KeyAction::Continue
                        }
                    }
                    GenreDetailSubTab::Artists => {
                        if let Some(artist) = detail.artists.get(selected) {
                            let artist_id = artist.artist_id.clone();
                            self.view.push_overlay(Overlay::ArtistDetail);
                            self.view.artist_detail_selected = 0;
                            self.view.artist_detail_left_scroll = 0;
                            self.view.artist_detail_left_focused = false;
                            KeyAction::SendCommand(Command::GetArtistDetail { artist_id })
                        } else {
                            KeyAction::Continue
                        }
                    }
                    GenreDetailSubTab::Playlists => {
                        if let Some(playlist) = detail.playlists.get(selected) {
                            let playlist_id = playlist.playlist_id.clone();
                            self.view.reset_playlist_detail_filter();
                            self.view
                                .push_overlay(Overlay::PlaylistDetail { selected: 0 });
                            KeyAction::SendCommand(Command::GetPlaylistDetail { playlist_id })
                        } else {
                            KeyAction::Continue
                        }
                    }
                    GenreDetailSubTab::Radios => {
                        if selected < detail.radios.len() {
                            KeyAction::SendCommand(Command::PlayFromGenreRadio { index: selected })
                        } else {
                            KeyAction::Continue
                        }
                    }
                }
            }
            // Player controls (play/pause, prev/next, shuffle, repeat,
            // volume) stay active while this overlay is open.
            other => self
                .player_control_action(other)
                .unwrap_or(KeyAction::Continue),
        }
    }

    /// Handle key events in the waiting list overlay.
    fn handle_waiting_list_key(&mut self, key: KeyEvent) -> KeyAction {
        let selected = match self.view.overlay {
            Some(Overlay::WaitingList { selected }) => selected,
            _ => return KeyAction::Continue,
        };

        // x: context menu for focused track
        if key.code == KeyCode::Char('x') {
            if let Some(track) = self.view.queue.get(selected).cloned() {
                let is_fav = self.view.is_track_favorite(&track.track_id);
                self.view.popup = Some(PopupMenu::full(track, is_fav));
            }
            return KeyAction::Continue;
        }

        match key.code {
            KeyCode::Enter => {
                if selected < self.view.queue.len() {
                    KeyAction::SendCommand(Command::PlayFromQueue { index: selected })
                } else {
                    KeyAction::Continue
                }
            }
            KeyCode::Esc | KeyCode::Char('w') => {
                self.view.pop_overlay();
                KeyAction::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let new_sel = selected.saturating_sub(1);
                self.view.overlay = Some(Overlay::WaitingList { selected: new_sel });
                KeyAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.view.queue.len().saturating_sub(1);
                let new_sel = (selected + 1).min(max);
                self.view.overlay = Some(Overlay::WaitingList { selected: new_sel });
                KeyAction::Continue
            }
            // Remove from queue (delete/backspace)
            KeyCode::Delete | KeyCode::Char('d') => {
                // Can't remove the currently playing track
                if selected != self.view.queue_index && selected < self.view.queue.len() {
                    let action =
                        KeyAction::SendCommand(Command::RemoveFromQueue { index: selected });
                    // Adjust selection if needed
                    let new_sel = if selected >= self.view.queue.len().saturating_sub(1) {
                        selected.saturating_sub(1)
                    } else {
                        selected
                    };
                    self.view.overlay = Some(Overlay::WaitingList { selected: new_sel });
                    return action;
                }
                KeyAction::Continue
            }
            // Toggle favorite
            KeyCode::Char('f') | KeyCode::Char('L') => {
                if let Some(track) = self.view.queue.get(selected) {
                    let track_id = track.track_id.clone();
                    return self.toggle_track_favorite(&track_id);
                }
                KeyAction::Continue
            }
            // Player controls (play/pause, prev/next, shuffle, repeat,
            // volume) stay active while this overlay is open.
            other => self
                .player_control_action(other)
                .unwrap_or(KeyAction::Continue),
        }
    }

    /// Handle key events when a popup menu is open.
    fn handle_popup_key(&mut self, key: KeyEvent) -> KeyAction {
        let popup = self.view.popup.as_mut().unwrap();

        // Pre-extract track_id for playlist picker (avoids borrow conflict)
        let track_id_for_playlist = popup.track().map(|t| t.track_id.clone());

        // Handle playlist picker sub-menu
        if let Some(ref mut sub) = popup.sub_menu {
            match sub {
                SubMenu::PlaylistPicker {
                    playlists,
                    selected,
                    loading,
                    filter_input,
                    filter_typing,
                } => {
                    if *loading {
                        if key.code == KeyCode::Esc {
                            popup.sub_menu = None;
                        }
                        return KeyAction::Continue;
                    }

                    // Playlists matching the current fuzzy filter (indices into `playlists`).
                    let filtered: Vec<usize> = if filter_input.is_empty() {
                        (0..playlists.len()).collect()
                    } else {
                        let query = filter_input.to_lowercase();
                        playlists
                            .iter()
                            .enumerate()
                            .filter(|(_, pl)| fuzzy_match(&query, &pl.title.to_lowercase()))
                            .map(|(i, _)| i)
                            .collect()
                    };
                    // Index 0 = "+ Create new playlist" entry; 1..=N = filtered playlists.
                    let total = filtered.len() + 1;

                    if *filter_typing {
                        match key.code {
                            KeyCode::Esc | KeyCode::Enter => {
                                *filter_typing = false;
                                return KeyAction::Continue;
                            }
                            KeyCode::Char(c) => {
                                filter_input.push(c);
                                *selected = (*selected).min(total.saturating_sub(1));
                                return KeyAction::Continue;
                            }
                            KeyCode::Backspace => {
                                filter_input.pop();
                                *selected = (*selected).min(total.saturating_sub(1));
                                return KeyAction::Continue;
                            }
                            _ => return KeyAction::Continue,
                        }
                    }

                    match key.code {
                        KeyCode::Esc => {
                            popup.sub_menu = None;
                            return KeyAction::Continue;
                        }
                        KeyCode::Char('/') => {
                            *filter_typing = true;
                            return KeyAction::Continue;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            *selected = selected.saturating_sub(1);
                            return KeyAction::Continue;
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            *selected = (*selected + 1).min(total.saturating_sub(1));
                            return KeyAction::Continue;
                        }
                        KeyCode::Enter => {
                            if *selected == 0 {
                                popup.sub_menu = Some(SubMenu::CreatePlaylistInput {
                                    name: String::new(),
                                    cursor: 0,
                                });
                                return KeyAction::Continue;
                            }
                            if let Some(&idx) = filtered.get(*selected - 1) {
                                if let Some(pl) = playlists.get(idx) {
                                    if let Some(ref track_id) = track_id_for_playlist {
                                        let cmd = Command::AddToPlaylist {
                                            playlist_id: pl.playlist_id.clone(),
                                            track_id: track_id.clone(),
                                        };
                                        self.view.popup = None;
                                        return KeyAction::SendCommand(cmd);
                                    }
                                }
                            }
                            return KeyAction::Continue;
                        }
                        _ => return KeyAction::Continue,
                    }
                }
                SubMenu::TrackInfo => {
                    if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
                        self.view.popup = None;
                    }
                    return KeyAction::Continue;
                }
                SubMenu::CreatePlaylistInput { name, cursor } => match key.code {
                    KeyCode::Esc => {
                        // Back to picker
                        let playlists = self.view.playlists.clone();
                        if let Some(ref mut popup) = self.view.popup {
                            popup.sub_menu = Some(SubMenu::PlaylistPicker {
                                playlists,
                                selected: 0,
                                loading: false,
                                filter_input: String::new(),
                                filter_typing: false,
                            });
                        }
                        return KeyAction::Continue;
                    }
                    KeyCode::Enter => {
                        let title = name.trim().to_string();
                        if title.is_empty() {
                            return KeyAction::Continue;
                        }
                        let Some(track_id) = track_id_for_playlist else {
                            return KeyAction::Continue;
                        };
                        self.view.popup = None;
                        return KeyAction::SendCommand(Command::CreatePlaylistAndAdd {
                            title,
                            track_id,
                        });
                    }
                    KeyCode::Backspace => {
                        if *cursor > 0 {
                            let mut chars: Vec<char> = name.chars().collect();
                            chars.remove(*cursor - 1);
                            *name = chars.into_iter().collect();
                            *cursor -= 1;
                        }
                        return KeyAction::Continue;
                    }
                    KeyCode::Left => {
                        *cursor = cursor.saturating_sub(1);
                        return KeyAction::Continue;
                    }
                    KeyCode::Right => {
                        *cursor = (*cursor + 1).min(name.chars().count());
                        return KeyAction::Continue;
                    }
                    KeyCode::Char(c) => {
                        let mut chars: Vec<char> = name.chars().collect();
                        chars.insert(*cursor, c);
                        *name = chars.into_iter().collect();
                        *cursor += 1;
                        return KeyAction::Continue;
                    }
                    _ => return KeyAction::Continue,
                },
                SubMenu::RenamePlaylistInput { name, cursor } => match key.code {
                    KeyCode::Esc => {
                        popup.sub_menu = None;
                        return KeyAction::Continue;
                    }
                    KeyCode::Enter => {
                        let new_title = name.trim().to_string();
                        if new_title.is_empty() {
                            return KeyAction::Continue;
                        }
                        let playlist_id = match &popup.target {
                            PopupTarget::Playlist { playlist_id, .. } => playlist_id.clone(),
                            _ => return KeyAction::Continue,
                        };
                        self.view.popup = None;
                        return KeyAction::SendCommand(Command::RenamePlaylist {
                            playlist_id,
                            new_title,
                        });
                    }
                    KeyCode::Backspace => {
                        if *cursor > 0 {
                            let mut chars: Vec<char> = name.chars().collect();
                            chars.remove(*cursor - 1);
                            *name = chars.into_iter().collect();
                            *cursor -= 1;
                        }
                        return KeyAction::Continue;
                    }
                    KeyCode::Left => {
                        *cursor = cursor.saturating_sub(1);
                        return KeyAction::Continue;
                    }
                    KeyCode::Right => {
                        *cursor = (*cursor + 1).min(name.chars().count());
                        return KeyAction::Continue;
                    }
                    KeyCode::Char(c) => {
                        let mut chars: Vec<char> = name.chars().collect();
                        chars.insert(*cursor, c);
                        *name = chars.into_iter().collect();
                        *cursor += 1;
                        return KeyAction::Continue;
                    }
                    _ => return KeyAction::Continue,
                },
                SubMenu::ConfirmDeletePlaylist { confirm_yes } => match key.code {
                    KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                        popup.sub_menu = None;
                        return KeyAction::Continue;
                    }
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        let playlist_id = match &popup.target {
                            PopupTarget::Playlist { playlist_id, .. } => playlist_id.clone(),
                            _ => return KeyAction::Continue,
                        };
                        self.view.popup = None;
                        return KeyAction::SendCommand(Command::DeletePlaylist { playlist_id });
                    }
                    KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::Char('h')
                    | KeyCode::Char('l')
                    | KeyCode::Tab => {
                        *confirm_yes = !*confirm_yes;
                        return KeyAction::Continue;
                    }
                    KeyCode::Enter => {
                        if *confirm_yes {
                            let playlist_id = match &popup.target {
                                PopupTarget::Playlist { playlist_id, .. } => playlist_id.clone(),
                                _ => return KeyAction::Continue,
                            };
                            self.view.popup = None;
                            return KeyAction::SendCommand(Command::DeletePlaylist { playlist_id });
                        } else {
                            popup.sub_menu = None;
                            return KeyAction::Continue;
                        }
                    }
                    _ => return KeyAction::Continue,
                },
            }
        }

        // Main popup menu navigation
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.view.popup = None;
                KeyAction::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                popup.select_prev();
                KeyAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                popup.select_next();
                KeyAction::Continue
            }
            KeyCode::Enter => {
                let action = popup.items[popup.selected].action.clone();
                self.execute_popup_action(action)
            }
            _ => KeyAction::Continue,
        }
    }

    /// Execute a popup menu action.
    fn execute_popup_action(&mut self, action: PopupAction) -> KeyAction {
        let popup = self.view.popup.as_ref().unwrap();
        let target = popup.target.clone();
        let is_favorite = popup.is_favorite;

        match action {
            PopupAction::Header => KeyAction::Continue,
            PopupAction::ToggleFavorite => {
                if let PopupTarget::Track(track) = target {
                    let cmd = if is_favorite {
                        Command::RemoveFavorite {
                            track_id: track.track_id,
                        }
                    } else {
                        Command::AddFavorite {
                            track_id: track.track_id,
                        }
                    };
                    self.view.popup = None;
                    KeyAction::SendCommand(cmd)
                } else {
                    KeyAction::Continue
                }
            }
            PopupAction::ToggleFavoriteArtist => {
                if let PopupTarget::Artist { artist_id, .. } = target {
                    let cmd = if is_favorite {
                        Command::RemoveFavoriteArtist { artist_id }
                    } else {
                        Command::AddFavoriteArtist { artist_id }
                    };
                    self.view.popup = None;
                    KeyAction::SendCommand(cmd)
                } else {
                    KeyAction::Continue
                }
            }
            PopupAction::ToggleFavoriteAlbum => {
                if let PopupTarget::Album { album_id, .. } = target {
                    let cmd = if is_favorite {
                        Command::RemoveFavoriteAlbum { album_id }
                    } else {
                        Command::AddFavoriteAlbum { album_id }
                    };
                    self.view.popup = None;
                    KeyAction::SendCommand(cmd)
                } else {
                    KeyAction::Continue
                }
            }
            PopupAction::AddToPlaylist => {
                // Open playlist picker sub-menu
                if self.view.playlists.is_empty() {
                    // Request playlists from daemon and show loading
                    if let Some(ref mut popup) = self.view.popup {
                        popup.sub_menu = Some(SubMenu::PlaylistPicker {
                            playlists: Vec::new(),
                            selected: 0,
                            loading: true,
                            filter_input: String::new(),
                            filter_typing: false,
                        });
                    }
                    KeyAction::SendCommand(Command::RequestPlaylists)
                } else {
                    // Playlists already loaded
                    let playlists = self.view.playlists.clone();
                    if let Some(ref mut popup) = self.view.popup {
                        popup.sub_menu = Some(SubMenu::PlaylistPicker {
                            playlists,
                            selected: 0,
                            loading: false,
                            filter_input: String::new(),
                            filter_typing: false,
                        });
                    }
                    KeyAction::Continue
                }
            }
            PopupAction::DownloadOffline => {
                if let PopupTarget::Track(track) = target {
                    self.view.popup = None;
                    KeyAction::SendCommand(Command::DownloadOffline { track: *track })
                } else {
                    KeyAction::Continue
                }
            }
            PopupAction::DownloadContainerOffline => {
                self.view.popup = None;
                match target {
                    PopupTarget::Playlist { playlist_id, .. } => {
                        KeyAction::SendCommand(Command::DownloadPlaylistOffline { playlist_id })
                    }
                    PopupTarget::Album { album_id, .. } => {
                        KeyAction::SendCommand(Command::DownloadAlbumOffline { album_id })
                    }
                    _ => KeyAction::Continue,
                }
            }
            PopupAction::RemoveOffline => {
                self.view.popup = None;
                match target {
                    PopupTarget::Track(track) => {
                        KeyAction::SendCommand(Command::RemoveOfflineTrack {
                            track_id: track.track_id,
                        })
                    }
                    PopupTarget::Album { album_id, .. } => {
                        KeyAction::SendCommand(Command::RemoveOfflineAlbum { album_id })
                    }
                    PopupTarget::Playlist { playlist_id, .. } => {
                        KeyAction::SendCommand(Command::RemoveOfflinePlaylist { playlist_id })
                    }
                    PopupTarget::Artist { .. } => KeyAction::Continue,
                }
            }
            PopupAction::DislikeTrack => {
                if let PopupTarget::Track(track) = target {
                    self.view.popup = None;
                    KeyAction::SendCommand(Command::DislikeTrack {
                        track_id: track.track_id,
                    })
                } else {
                    KeyAction::Continue
                }
            }
            PopupAction::PlayNext => {
                if let PopupTarget::Track(track) = target {
                    self.view.popup = None;
                    KeyAction::SendCommand(Command::PlayNext { track: *track })
                } else {
                    KeyAction::Continue
                }
            }
            PopupAction::AddToQueue => {
                if let PopupTarget::Track(track) = target {
                    self.view.popup = None;
                    KeyAction::SendCommand(Command::AddToQueue { track: *track })
                } else {
                    KeyAction::Continue
                }
            }
            PopupAction::MixFromTrack => {
                if let PopupTarget::Track(track) = target {
                    self.view.popup = None;
                    KeyAction::SendCommand(Command::StartMix {
                        track_id: track.track_id,
                    })
                } else {
                    KeyAction::Continue
                }
            }
            PopupAction::Share => {
                let url = match &target {
                    PopupTarget::Track(track) => {
                        format!("https://www.deezer.com/track/{}", track.track_id)
                    }
                    PopupTarget::Artist { artist_id, .. } => {
                        format!("https://www.deezer.com/artist/{artist_id}")
                    }
                    PopupTarget::Album { album_id, .. } => {
                        format!("https://www.deezer.com/album/{album_id}")
                    }
                    PopupTarget::Playlist { playlist_id, .. } => {
                        format!("https://www.deezer.com/playlist/{playlist_id}")
                    }
                };
                match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(&url)) {
                    Ok(()) => {
                        self.view.toast =
                            Some(Toast::new(t().link_copied.into(), Duration::from_secs(2)));
                    }
                    Err(e) => {
                        self.view.toast = Some(Toast::new(
                            format!("Clipboard error: {e}"),
                            Duration::from_secs(3),
                        ));
                    }
                }
                self.view.popup = None;
                KeyAction::Continue
            }
            PopupAction::TrackInfo => {
                if let Some(ref mut popup) = self.view.popup {
                    popup.sub_menu = Some(SubMenu::TrackInfo);
                }
                KeyAction::Continue
            }
            PopupAction::ViewAlbum => {
                let album_id = match &target {
                    PopupTarget::Track(track) => track.album_id.clone(),
                    PopupTarget::Album { album_id, .. } => Some(album_id.clone()),
                    _ => None,
                };
                if let Some(album_id) = album_id {
                    self.view.popup = None;
                    self.view
                        .push_overlay(Overlay::AlbumDetail { from_artist: false });
                    self.view.album_detail_selected = 0;
                    KeyAction::SendCommand(Command::GetAlbumDetail { album_id })
                } else {
                    self.view.popup = None;
                    self.view.toast =
                        Some(Toast::new(t().no_album_info.into(), Duration::from_secs(2)));
                    KeyAction::Continue
                }
            }
            PopupAction::ViewArtist => {
                let artist_id = match &target {
                    PopupTarget::Track(track) => track.artist_id.clone(),
                    PopupTarget::Artist { artist_id, .. } => Some(artist_id.clone()),
                    _ => None,
                };
                if let Some(artist_id) = artist_id {
                    self.view.popup = None;
                    self.view.push_overlay(Overlay::ArtistDetail);
                    self.view.artist_detail_selected = 0;
                    KeyAction::SendCommand(Command::GetArtistDetail { artist_id })
                } else {
                    self.view.popup = None;
                    KeyAction::Continue
                }
            }
            PopupAction::RemoveFromPlaylist => {
                let popup = self.view.popup.as_ref().unwrap();
                let playlist_id = popup.playlist_context.clone();
                if let (PopupTarget::Track(track), Some(playlist_id)) = (target, playlist_id) {
                    self.view.popup = None;
                    KeyAction::SendCommand(Command::RemoveFromPlaylist {
                        playlist_id,
                        track_id: track.track_id,
                    })
                } else {
                    KeyAction::Continue
                }
            }
            PopupAction::RenamePlaylist => {
                if let PopupTarget::Playlist { ref title, .. } = target {
                    let prefill = title.clone();
                    let cursor = prefill.chars().count();
                    if let Some(ref mut popup) = self.view.popup {
                        popup.sub_menu = Some(SubMenu::RenamePlaylistInput {
                            name: prefill,
                            cursor,
                        });
                    }
                }
                KeyAction::Continue
            }
            PopupAction::DeletePlaylist => {
                if let PopupTarget::Playlist {
                    playlist_id,
                    nb_songs,
                    ..
                } = target
                {
                    if nb_songs == 0 {
                        self.view.popup = None;
                        KeyAction::SendCommand(Command::DeletePlaylist { playlist_id })
                    } else {
                        if let Some(ref mut popup) = self.view.popup {
                            popup.sub_menu =
                                Some(SubMenu::ConfirmDeletePlaylist { confirm_yes: false });
                        }
                        KeyAction::Continue
                    }
                } else {
                    KeyAction::Continue
                }
            }
        }
    }

    /// Route a mouse event. The wheel reuses the arrow-key handlers, so it
    /// scrolls whatever list or panel has focus — except over a detail page's
    /// left column, which scrolls under the cursor whether focused or not.
    fn handle_mouse(&mut self, mouse: MouseEvent) -> KeyAction {
        let (col, row) = (mouse.column, mouse.row);
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => self.handle_left_click(col, row),
            MouseEventKind::Down(MouseButton::Right) => self.handle_right_click(col, row),
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let up = mouse.kind == MouseEventKind::ScrollUp;
                if self.scroll_detail_left_panel(col, row, up) {
                    return KeyAction::Continue;
                }
                // A text input swallows the arrow keys, so let the wheel take
                // the focus out of it and move the list instead.
                self.view.blur_inputs();
                self.handle_key(KeyEvent::from(if up { KeyCode::Up } else { KeyCode::Down }))
            }
            _ => KeyAction::Continue,
        }
    }

    /// Scroll the album / artist detail left column when the wheel turns over
    /// it. Returns false when the cursor is elsewhere.
    fn scroll_detail_left_panel(&mut self, col: u16, row: u16, up: bool) -> bool {
        let over_panel = matches!(
            self.view.click.borrow().hit(col, row),
            Some(ClickTarget::DetailLeftPanel)
        );
        if !over_panel {
            return false;
        }
        let step = |scroll: u16| {
            if up {
                scroll.saturating_sub(1)
            } else {
                scroll.saturating_add(1)
            }
        };
        // The draw pass clamps the scroll to the panel's content height.
        match self.view.overlay {
            Some(Overlay::AlbumDetail { .. }) => {
                self.view.album_detail_left_scroll = step(self.view.album_detail_left_scroll);
            }
            Some(Overlay::ArtistDetail) => {
                self.view.artist_detail_left_scroll = step(self.view.artist_detail_left_scroll);
            }
            _ => return false,
        }
        true
    }

    /// Hit-test a left click against the widgets the last draw pass recorded.
    /// A second click on the same row within [`DOUBLE_CLICK`] acts as `Enter`.
    fn handle_left_click(&mut self, col: u16, row: u16) -> KeyAction {
        if self.view.screen == Screen::Login {
            let on_button = self.view.login_button_area.get().is_some_and(|rect| {
                col >= rect.x && col < rect.right() && row >= rect.y && row < rect.bottom()
            });
            if on_button && self.view.login_mode == LoginMode::Button && !self.view.login_loading {
                return KeyAction::WebLogin;
            }
            return KeyAction::Continue;
        }

        let double = self.last_click.is_some_and(|(c, r, at)| {
            r == row && c.abs_diff(col) <= 2 && at.elapsed() < DOUBLE_CLICK
        });
        self.last_click = Some((col, row, Instant::now()));

        let (target, rows, modal) = {
            let click = self.view.click.borrow();
            (click.hit(col, row), click.rows, click.modal)
        };

        match modal {
            // Clicking the dimmed backdrop closes the modal on top.
            Some(modal) if !contains(modal, col, row) => {
                return self.handle_key(KeyEvent::from(KeyCode::Esc))
            }
            // A modal that records no outline (an update in progress) is inert.
            None if self.view.popup.is_some() || self.view.has_modal_overlay() => {
                return KeyAction::Continue
            }
            _ => {}
        }

        // Clicking anywhere but a text input takes the focus out of it, so the
        // click (and the Enter a double click sends) reaches the list, not the
        // input that was being typed in.
        if !matches!(
            target,
            Some(ClickTarget::FilterInput | ClickTarget::PlaylistFilterInput)
        ) {
            self.view.blur_inputs();
        }

        if let Some(target) = target {
            return self.activate_click_target(target);
        }

        let Some((rows, index)) = rows.and_then(|rows| Some((rows, rows.index_at(col, row)?)))
        else {
            return KeyAction::Continue;
        };
        // A modal's options act on a single click, like any menu.
        if rows.kind == RowsKind::Modal {
            return self.click_modal_row(index);
        }
        let select = self.select_row(rows.kind, index);
        if !double {
            return select.map_or(KeyAction::Continue, KeyAction::SendCommand);
        }
        // Double click: select, then act on the row as Enter would.
        let action = self.handle_key(KeyEvent::from(KeyCode::Enter));
        with_selection(select, action)
    }

    /// Activate the clicked option of the open modal. The cursor is walked there
    /// with the same keys the keyboard uses, so per-modal side effects (the
    /// theme picker's live preview, for one) still run.
    fn click_modal_row(&mut self, index: usize) -> KeyAction {
        // Section titles inside a context menu are not selectable.
        if let Some(popup) = &self.view.popup {
            if popup.sub_menu.is_none() && popup.items.get(index).is_some_and(|i| i.is_header) {
                return KeyAction::Continue;
            }
        }
        let mut guard = 0;
        while let Some(current) = self.modal_selection() {
            if current == index || guard > MAX_MODAL_OPTIONS {
                break;
            }
            guard += 1;
            let key = if current < index {
                KeyCode::Down
            } else {
                KeyCode::Up
            };
            self.handle_key(KeyEvent::from(key));
            // The cursor hit an end of the list: stop rather than spin.
            if self.modal_selection() == Some(current) {
                break;
            }
        }
        self.handle_key(KeyEvent::from(KeyCode::Enter))
    }

    /// Cursor position within the open modal's option list.
    fn modal_selection(&self) -> Option<usize> {
        if let Some(popup) = &self.view.popup {
            return match &popup.sub_menu {
                Some(SubMenu::PlaylistPicker { selected, .. }) => Some(*selected),
                Some(_) => None,
                None => Some(popup.selected),
            };
        }
        match self.view.overlay {
            Some(Overlay::Settings { selected })
            | Some(Overlay::ThemePicker { selected })
            | Some(Overlay::LanguagePicker { selected })
            | Some(Overlay::QualityPicker { selected })
            | Some(Overlay::UpdateAvailable { selected, .. }) => Some(selected),
            _ => None,
        }
    }

    /// Right click opens the context menu of whatever sits under the cursor: the
    /// playing track when clicked in the player bar, otherwise the clicked row.
    fn handle_right_click(&mut self, col: u16, row: u16) -> KeyAction {
        if self.view.screen != Screen::Main
            || self.view.popup.is_some()
            || self.view.has_modal_overlay()
        {
            return KeyAction::Continue;
        }

        let (target, rows) = {
            let click = self.view.click.borrow();
            (click.hit(col, row), click.rows)
        };

        if let Some(target) = target {
            // Same as Ctrl+Space: manage menu for the playing track.
            if matches!(target, ClickTarget::CurrentTrack) {
                self.view.blur_inputs();
                return self.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::CONTROL));
            }
            return KeyAction::Continue;
        }

        let Some((rows, index)) = rows.and_then(|rows| Some((rows, rows.index_at(col, row)?)))
        else {
            return KeyAction::Continue;
        };
        // Select the row, then open its menu as `x` would.
        let select = self.select_row(rows.kind, index);
        let action = self.handle_key(KeyEvent::from(KeyCode::Char('x')));
        with_selection(select, action)
    }

    /// Perform the action of a clicked widget (tab, category chip, button…).
    fn activate_click_target(&mut self, target: ClickTarget) -> KeyAction {
        match target {
            // Offline mode shows a single tab — there is nothing to switch to.
            ClickTarget::Tab(_) if self.view.is_offline => KeyAction::Continue,
            ClickTarget::Tab(tab) => KeyAction::SendCommand(Command::SetTab { tab }),
            ClickTarget::Category(index) => KeyAction::SendCommand(Command::SetCategory { index }),
            ClickTarget::ArtistSubTab(index) => {
                if let Some(&sub_tab) = ArtistSubTab::ALL.get(index) {
                    self.view.artist_detail_sub_tab = sub_tab;
                    self.view.artist_detail_selected = 0;
                    self.view.artist_detail_left_focused = false;
                }
                KeyAction::Continue
            }
            ClickTarget::GenreSubTab(index) => {
                if let (Some(&new_tab), Some(Overlay::GenreDetail { sub_tab, selected })) = (
                    GenreDetailSubTab::ALL.get(index),
                    self.view.overlay.as_mut(),
                ) {
                    *sub_tab = new_tab;
                    *selected = 0;
                }
                KeyAction::Continue
            }
            ClickTarget::FilterInput => {
                self.view.focus_filter_input(false);
                KeyAction::Continue
            }
            ClickTarget::FlowChip => KeyAction::SendCommand(Command::StartFlow),
            ClickTarget::ToggleLikeCurrentTrack => {
                if let Some(ref track) = self.view.current_track {
                    let track_id = track.track_id.clone();
                    self.toggle_track_favorite(&track_id)
                } else {
                    KeyAction::Continue
                }
            }
            ClickTarget::ShuffleFavorites => KeyAction::SendCommand(Command::ShuffleFavorites),
            // Player bar: right click only.
            ClickTarget::CurrentTrack => KeyAction::Continue,
            ClickTarget::Back => self.handle_key(KeyEvent::from(KeyCode::Esc)),
            // Focus the left column, as `h` does — only when it has something to
            // scroll, otherwise focus there means nothing.
            ClickTarget::DetailLeftPanel => {
                match self.view.overlay {
                    Some(Overlay::AlbumDetail { .. }) => {
                        self.view.album_detail_left_focused =
                            self.view.album_detail_left_scrollable;
                    }
                    Some(Overlay::ArtistDetail) => {
                        self.view.artist_detail_left_focused =
                            self.view.artist_detail_left_scrollable;
                    }
                    _ => {}
                }
                KeyAction::Continue
            }
            ClickTarget::PlaylistFilterInput => {
                if let Some(SubMenu::PlaylistPicker { filter_typing, .. }) = self
                    .view
                    .popup
                    .as_mut()
                    .and_then(|popup| popup.sub_menu.as_mut())
                {
                    *filter_typing = true;
                }
                KeyAction::Continue
            }
            // Left/Right flip the switch; aim at the side that changes it.
            ClickTarget::TransparencyToggle => {
                let key = if Theme::is_transparent() {
                    KeyCode::Left
                } else {
                    KeyCode::Right
                };
                self.handle_key(KeyEvent::from(key))
            }
            ClickTarget::ConfirmChoice(yes) => {
                self.handle_key(KeyEvent::from(KeyCode::Char(if yes { 'y' } else { 'n' })))
            }
        }
    }

    /// Move a list's cursor to a clicked row. Returns the command that syncs the
    /// daemon's cursor, for the lists the daemon owns.
    fn select_row(&mut self, kind: RowsKind, index: usize) -> Option<Command> {
        match kind {
            RowsKind::Tab => match self.view.active_tab {
                ActiveTab::Search => {
                    self.view.search_selected = index;
                    Some(Command::SelectIndex { index })
                }
                ActiveTab::Favorites => {
                    if self.view.favorites_filter_active() {
                        self.view.favorites_filter_selected = index;
                        return None;
                    }
                    self.view.favorites_selected = index;
                    Some(Command::SelectIndex { index })
                }
                ActiveTab::Explore => {
                    match self.view.explore_category {
                        ExploreCategory::Moods => self.view.moods_selected = index,
                        ExploreCategory::Categories => self.view.genres_selected = index,
                        ExploreCategory::Radios => self.view.radios_selected = index,
                    }
                    None
                }
                ActiveTab::Downloads => {
                    if self.view.offline_filter_active() {
                        self.view.offline_filter_selected = index;
                        return None;
                    }
                    self.view.offline_selected = index;
                    Some(Command::SelectIndex { index })
                }
            },
            RowsKind::AlbumDetail => {
                self.view.album_detail_left_focused = false;
                self.view.album_detail_selected = index;
                None
            }
            RowsKind::ArtistDetail => {
                self.view.artist_detail_left_focused = false;
                self.view.artist_detail_selected = index;
                None
            }
            RowsKind::GenreDetail => {
                if let Some(Overlay::GenreDetail { selected, .. }) = self.view.overlay.as_mut() {
                    *selected = index;
                }
                None
            }
            RowsKind::PlaylistDetail => {
                match self.view.overlay.as_mut() {
                    Some(Overlay::PlaylistDetail { selected })
                    | Some(Overlay::ShowDetail { selected }) => *selected = index,
                    _ => {}
                }
                None
            }
            RowsKind::WaitingList => {
                if let Some(Overlay::WaitingList { selected }) = self.view.overlay.as_mut() {
                    *selected = index;
                }
                None
            }
            RowsKind::OfflineDetail => {
                if let Some(Overlay::OfflineDetail { selected, .. }) = self.view.overlay.as_mut() {
                    *selected = index;
                }
                None
            }
            // Handled by `click_modal_row`, which walks the modal's own cursor.
            RowsKind::Modal => None,
        }
    }
}

/// Combine the command that syncs a clicked row's selection with the action the
/// synthesized key produced, so both reach the daemon in order.
fn with_selection(select: Option<Command>, action: KeyAction) -> KeyAction {
    match (select, action) {
        (Some(cmd), KeyAction::SendCommand(next)) => KeyAction::MultiCommand(vec![cmd, next]),
        (Some(cmd), KeyAction::MultiCommand(mut cmds)) => {
            cmds.insert(0, cmd);
            KeyAction::MultiCommand(cmds)
        }
        (Some(cmd), KeyAction::Continue) => KeyAction::SendCommand(cmd),
        (_, action) => action,
    }
}

/// Result of handling a key press.
enum KeyAction {
    /// Do nothing special.
    Continue,
    /// Send a command to the daemon.
    SendCommand(Command),
    /// Send multiple commands to the daemon.
    #[allow(dead_code)]
    MultiCommand(Vec<Command>),
    /// Quit: send shutdown to daemon and exit.
    Quit,
    /// Detach: exit client but keep daemon running (Ctrl+Z / Ctrl+C).
    Detach,
    /// Open browser for Deezer web login.
    WebLogin,
    /// Download and install an update, then restart.
    PerformUpdate {
        version: String,
        download_url: String,
    },
}

// ── Update check & install ──────────────────────────────────────────

/// Simple semver comparison: returns true if `remote` > `current`.
fn version_is_newer(remote: &str, current: &str) -> bool {
    let parse = |s: &str| -> (u32, u32, u32) {
        let parts: Vec<u32> = s.split('.').filter_map(|p| p.parse().ok()).collect();
        (
            parts.first().copied().unwrap_or(0),
            parts.get(1).copied().unwrap_or(0),
            parts.get(2).copied().unwrap_or(0),
        )
    };
    parse(remote) > parse(current)
}

/// Wait for the daemon to fully exit before launching the new binary.
/// Sends SIGTERM if the daemon is still alive after a grace period.
async fn wait_for_daemon_exit() {
    let sock = socket_path();
    let pid_file = pid_path();

    // Read daemon PID for fallback kill
    let daemon_pid: Option<u32> = std::fs::read_to_string(&pid_file)
        .ok()
        .and_then(|s| s.trim().parse().ok());

    // Poll until socket disappears (daemon cleaned up) — up to 3 seconds
    for i in 0..30 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if !sock.exists() {
            return; // daemon exited cleanly
        }
        // After 1s with no clean exit, send SIGTERM to be sure
        #[cfg(unix)]
        if i == 10 {
            if let Some(pid) = daemon_pid {
                unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
            }
        }
    }

    // Last resort: SIGKILL and remove stale files manually
    #[cfg(unix)]
    if let Some(pid) = daemon_pid {
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
    }
    let _ = std::fs::remove_file(&sock);
    let _ = std::fs::remove_file(&pid_file);
}

/// Check GitHub for a newer release. Returns (version, download_url) if newer.
async fn check_for_update() -> Option<(String, String)> {
    let url = "https://api.github.com/repos/Tatayoyoh/deezer-tui/releases/latest";

    let client = reqwest::Client::builder()
        .user_agent("deezer-tui")
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;

    let resp = client.get(url).send().await.ok()?;
    let json: serde_json::Value = resp.json().await.ok()?;

    let tag = json["tag_name"].as_str()?;
    let remote_version = tag.strip_prefix('v').unwrap_or(tag);
    let current_version = env!("CARGO_PKG_VERSION");

    if !version_is_newer(remote_version, current_version) {
        return None;
    }

    // Select correct asset for this platform
    let asset_name = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "deezer-tui-linux-x86_64",
        ("linux", "aarch64") => "deezer-tui-linux-aarch64",
        ("macos", _) => "deezer-tui-macos-universal",
        _ => return None,
    };

    let assets = json["assets"].as_array()?;
    let download_url = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(asset_name))
        .and_then(|a| a["browser_download_url"].as_str())
        .map(String::from)?;

    Some((remote_version.to_string(), download_url))
}

/// Download the new binary and replace the current executable.
/// Returns the path of the installed binary.
async fn perform_update(download_url: &str) -> Result<std::path::PathBuf> {
    use std::io::Write;

    let current_exe = std::env::current_exe()
        .map_err(|e| anyhow::anyhow!("Cannot determine current binary path: {e}"))?;

    // Download the new binary
    let client = reqwest::Client::builder()
        .user_agent("deezer-tui")
        .timeout(Duration::from_secs(120))
        .build()?;

    let resp = client.get(download_url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("Download failed: HTTP {}", resp.status());
    }

    let bytes = resp.bytes().await?;
    let tmp_path = std::path::PathBuf::from("/tmp/deezer-tui-update.tmp");

    // Write to temp file
    {
        let mut f = std::fs::File::create(&tmp_path)?;
        f.write_all(&bytes)?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755))?;
    }

    // Try direct rename first (works if same filesystem and writable)
    if std::fs::rename(&tmp_path, &current_exe).is_ok() {
        return Ok(current_exe);
    }

    // Rename failed — try sudo install (needed for e.g. /usr/local/bin)
    // Re-open /dev/tty so sudo can prompt for a password even if
    // stdin/stdout were redirected by the TUI or by tokio.
    eprintln!("{}", t().update_sudo_prompt);

    let tty_in = std::fs::File::open("/dev/tty")
        .map_err(|e| anyhow::anyhow!("Cannot open /dev/tty for sudo prompt: {e}"))?;
    let tty_out = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/tty")
        .map_err(|e| anyhow::anyhow!("Cannot open /dev/tty for sudo output: {e}"))?;

    let status = std::process::Command::new("sudo")
        .args([
            "install",
            "-m",
            "755",
            tmp_path.to_str().unwrap_or("/tmp/deezer-tui-update.tmp"),
            current_exe
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid binary path"))?,
        ])
        .stdin(std::process::Stdio::from(tty_in))
        .stdout(std::process::Stdio::from(tty_out.try_clone()?))
        .stderr(std::process::Stdio::from(tty_out))
        .status()?;

    let _ = std::fs::remove_file(&tmp_path);

    if !status.success() {
        anyhow::bail!("sudo install failed (exit code: {:?})", status.code());
    }

    Ok(current_exe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::common::tab_rects;

    fn rows(y: u16, height: u16, offset: usize, len: usize) -> RowsArea {
        RowsArea {
            area: Rect {
                x: 0,
                y,
                width: 40,
                height,
            },
            offset,
            len,
            kind: RowsKind::Tab,
        }
    }

    #[test]
    fn row_index_accounts_for_scroll_offset() {
        let rows = rows(5, 4, 10, 20);
        assert_eq!(rows.index_at(0, 4), None, "above the first row");
        assert_eq!(rows.index_at(0, 5), Some(10));
        assert_eq!(rows.index_at(0, 8), Some(13));
        assert_eq!(rows.index_at(0, 9), None, "below the last visible row");
    }

    #[test]
    fn row_index_ignores_clicks_past_the_last_item() {
        // Three items drawn in a four-row area: the empty row is not clickable.
        let rows = rows(0, 4, 0, 3);
        assert_eq!(rows.index_at(0, 2), Some(2));
        assert_eq!(rows.index_at(0, 3), None);
    }

    #[test]
    fn tab_rects_match_ratatui_padding_and_dividers() {
        // Ratatui draws " Aa | Bbb | C ": one space each side, one-cell divider.
        let area = Rect {
            x: 0,
            y: 0,
            width: 40,
            height: 1,
        };
        let rects = tab_rects(area, &["Aa", "Bbb", "C"]);
        assert_eq!(rects.len(), 3);
        assert_eq!((rects[0].x, rects[0].width), (0, 4));
        assert_eq!((rects[1].x, rects[1].width), (5, 5));
        assert_eq!((rects[2].x, rects[2].width), (11, 3));
    }

    #[test]
    fn tab_rects_clip_the_last_visible_tab() {
        // " Aa " then "|", leaving 1 cell of " Bbb " on screen — still clickable.
        let area = Rect {
            x: 0,
            y: 0,
            width: 6,
            height: 1,
        };
        let rects = tab_rects(area, &["Aa", "Bbb", "C"]);
        assert_eq!(rects.len(), 2);
        assert_eq!((rects[1].x, rects[1].width), (5, 1));
    }

    #[test]
    fn playlist_detail_filter_maps_to_original_indices() {
        fn track(id: &str, title: &str, artist: &str) -> TrackData {
            serde_json::from_value(serde_json::json!({
                "SNG_ID": id,
                "SNG_TITLE": title,
                "ART_NAME": artist,
            }))
            .unwrap()
        }

        let mut view = ViewState::from_snapshot(&DaemonSnapshot::default());
        view.playlist_detail = Some(PlaylistDetail {
            playlist_id: "1".into(),
            title: "Mix".into(),
            creator: "Me".into(),
            nb_tracks: 3,
            tracks: vec![
                track("10", "Alpha", "A"),
                track("20", "Beta", "B"),
                track("30", "Alfetta", "C"),
            ],
        });

        // No filter → every track, identity indices.
        let all = view.playlist_detail_tracks_filtered();
        assert_eq!(all.iter().map(|(i, _)| *i).collect::<Vec<_>>(), [0, 1, 2]);

        // A filter that matches a single title keeps that track's real index,
        // so playback targets the right position in the full playlist.
        view.playlist_detail_filter_input = "beta".into();
        let filtered = view.playlist_detail_tracks_filtered();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0, 1);
        assert_eq!(filtered[0].1.title, "Beta");
    }

    #[test]
    fn is_track_favorite_checks_favorite_ids_and_favorites_list() {
        let mut view = ViewState::from_snapshot(&DaemonSnapshot::default());
        assert!(!view.is_track_favorite("123"));

        view.favorite_track_ids.push("123".into());
        assert!(view.is_track_favorite("123"));
        assert!(!view.is_track_favorite("456"));

        let snap = DaemonSnapshot {
            favorite_track_ids: vec!["456".into()],
            ..DaemonSnapshot::default()
        };
        view.update_from_snapshot(snap);
        assert!(!view.is_track_favorite("123"));
        assert!(view.is_track_favorite("456"));
    }
}
