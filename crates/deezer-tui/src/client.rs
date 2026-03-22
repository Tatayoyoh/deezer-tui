use std::cell::Cell;
use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
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
    read_line, socket_path, ActiveTab, Command, DaemonSnapshot, FavoritesCategory, OfflineCategory,
    RadioItem, Screen, SearchCategory, ServerMessage,
};
use crate::theme::{Theme, ThemeId};
use crate::ui;

const TICK_RATE: Duration = Duration::from_millis(50);

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
    AddToPlaylist,
    DownloadOffline,
    DislikeTrack,
    PlayNext,
    AddToQueue,
    MixFromTrack,
    Share,
    TrackInfo,
    ViewAlbum,
    ViewArtist,
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
    },
    TrackInfo,
}

#[derive(Debug, Clone)]
pub struct PopupMenu {
    pub title: Option<String>,
    pub items: Vec<PopupMenuItem>,
    pub selected: usize,
    pub track: TrackData,
    pub is_favorite: bool,
    pub sub_menu: Option<SubMenu>,
}

impl PopupMenu {
    /// Build the full menu (for `m` key on a selected track in a list).
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
            track,
            is_favorite,
            sub_menu: None,
        }
    }

    /// Build the manage-only menu (for `Ctrl+P` on currently playing track).
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
            track,
            is_favorite,
            sub_menu: None,
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
    Help,
    /// Settings menu with selectable entries.
    Settings { selected: usize },
    /// Theme picker.
    ThemePicker { selected: usize },
    /// Language picker.
    LanguagePicker { selected: usize },
    /// Album detail view. `from_artist` is true when opened from the artist detail page.
    AlbumDetail { from_artist: bool },
    /// Artist detail view.
    ArtistDetail,
    /// Playlist detail modal.
    PlaylistDetail { selected: usize },
    /// Waiting list (upcoming tracks in queue).
    WaitingList { selected: usize },
    /// Application info modal.
    Info,
}

/// An item in the offline albums tree view.
#[derive(Debug, Clone)]
pub enum OfflineTreeItem {
    /// Album header node (index into offline_albums).
    Album(usize),
    /// Track node (album index, track index within album).
    Track(usize, usize),
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
    pub offline_category: OfflineCategory,
    pub offline_tracks: Vec<OfflineTrack>,
    pub offline_albums: Vec<AlbumDetail>,
    pub offline_selected: usize,
    pub offline_loading: bool,
    pub offline_track_ids: Vec<String>,
    pub radios: Vec<RadioItem>,
    pub radios_filtered: Vec<RadioItem>,
    pub radios_selected: usize,
    pub radios_loading: bool,
    pub radio_filter_input: String,
    pub radio_filter_typing: bool,
    pub playlists: Vec<PlaylistData>,
    pub album_detail: Option<AlbumDetail>,
    pub album_detail_selected: usize,
    pub album_detail_loading: bool,
    pub artist_detail: Option<ArtistDetail>,
    pub artist_detail_selected: usize,
    pub artist_detail_loading: bool,
    pub artist_detail_sub_tab: ArtistSubTab,
    pub playlist_detail: Option<PlaylistDetail>,
    pub playlist_detail_loading: bool,
    pub status_msg: Option<String>,
    pub login_error: Option<String>,
    pub login_loading: bool,
    pub user_name: Option<String>,
    pub is_offline: bool,

    // Local client state — offline albums tree
    pub offline_tree_selected: usize,
    pub offline_expanded: Vec<String>,

    pub input_mode: InputMode,
    pub search_input: String,
    pub login_mode: LoginMode,
    pub login_input: String,
    pub login_cursor: usize,
    pub popup: Option<PopupMenu>,
    pub overlay: Option<Overlay>,
    pub toast: Option<Toast>,

    // Cover art image (client-side only)
    pub cover_image: Option<StatefulProtocol>,
    /// URL of the currently loaded cover image (to avoid re-fetching).
    pub cover_image_url: String,

    /// Button area set by the UI draw pass, used for mouse hit-testing.
    pub login_button_area: Cell<Option<Rect>>,
}

/// A temporary notification message that auto-dismisses.
#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub created_at: Instant,
    pub duration: Duration,
}

impl Toast {
    pub fn new(message: String, duration: Duration) -> Self {
        Self {
            message,
            created_at: Instant::now(),
            duration,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.duration
    }
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
            offline_category: snap.offline_category,
            offline_tracks: snap.offline_tracks.clone(),
            offline_albums: snap.offline_albums.clone(),
            offline_selected: snap.offline_selected,
            offline_loading: snap.offline_loading,
            offline_track_ids: snap.offline_track_ids.clone(),
            radios: snap.radios.clone(),
            radios_filtered: snap.radios.clone(),
            radios_selected: snap.radios_selected,
            radios_loading: snap.radios_loading,
            radio_filter_input: String::new(),
            radio_filter_typing: false,
            playlists: snap.playlists.clone(),
            album_detail: snap.album_detail.clone(),
            album_detail_selected: snap.album_detail_selected,
            album_detail_loading: snap.album_detail_loading,
            artist_detail: snap.artist_detail.clone(),
            artist_detail_selected: snap.artist_detail_selected,
            artist_detail_loading: snap.artist_detail_loading,
            artist_detail_sub_tab: snap.artist_detail_sub_tab,
            playlist_detail: snap.playlist_detail.clone(),
            playlist_detail_loading: snap.playlist_detail_loading,
            status_msg: snap.status_msg.clone(),
            login_error: snap.login_error.clone(),
            login_loading: snap.login_loading,
            user_name: snap.user_name.clone(),
            is_offline: snap.is_offline,

            offline_tree_selected: 0,
            offline_expanded: Vec::new(),

            input_mode: InputMode::Normal,
            search_input: String::new(),
            login_mode: LoginMode::Button,
            login_input: String::new(),
            login_cursor: 0,
            popup: None,
            overlay: None,
            toast: None,
            cover_image: None,
            cover_image_url: String::new(),
            login_button_area: Cell::new(None),
        }
    }

    /// Update from a new daemon snapshot, preserving local-only state.
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
        self.favorites_category = snap.favorites_category;
        self.favorites_display = snap.favorites_display;
        self.offline_category = snap.offline_category;
        self.offline_tracks = snap.offline_tracks;
        self.offline_albums = snap.offline_albums;
        self.offline_selected = snap.offline_selected;
        self.offline_loading = snap.offline_loading;
        self.offline_track_ids = snap.offline_track_ids;
        // Update radios and re-apply filter
        if self.radios.len() != snap.radios.len() || self.radios_loading != snap.radios_loading {
            self.radios = snap.radios;
            self.radios_loading = snap.radios_loading;
            self.apply_radio_filter();
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
        self.status_msg = snap.status_msg;
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
            self.popup = None;
            self.radio_filter_input.clear();
            self.radio_filter_typing = false;
        }
    }

    /// Check if a track is in the user's favorites.
    fn is_track_favorite(&self, track_id: &str) -> bool {
        self.favorites.iter().any(|t| t.track_id == track_id)
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
                .filter(|r| r.title.to_lowercase().contains(&query))
                .cloned()
                .collect();
        }
        self.radios_selected = 0;
    }

    /// Build the flattened tree items for the offline albums view.
    pub fn offline_tree_items(&self) -> Vec<OfflineTreeItem> {
        let mut items = Vec::new();
        for (i, album) in self.offline_albums.iter().enumerate() {
            items.push(OfflineTreeItem::Album(i));
            if self.offline_expanded.contains(&album.album_id) {
                for j in 0..album.tracks.len() {
                    items.push(OfflineTreeItem::Track(i, j));
                }
            }
        }
        items
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
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
    picker: Picker,
    image_tx: mpsc::UnboundedSender<(String, image::DynamicImage)>,
    image_rx: mpsc::UnboundedReceiver<(String, image::DynamicImage)>,
}

impl Client {
    pub async fn connect() -> Result<Self> {
        let path = socket_path();
        let stream = UnixStream::connect(&path).await?;
        let (read_half, write_half) = stream.into_split();

        // Detect terminal graphics protocol (must happen before alternate screen)
        let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

        let (image_tx, image_rx) = mpsc::unbounded_channel();

        Ok(Self {
            view: ViewState::from_snapshot(&DaemonSnapshot::default()),
            reader: BufReader::new(read_half),
            writer: write_half,
            picker,
            image_tx,
            image_rx,
        })
    }

    /// Get the current cover art URL from album/artist detail overlay.
    fn current_cover_url(&self) -> Option<&str> {
        if matches!(self.view.overlay, Some(Overlay::AlbumDetail { .. })) {
            self.view
                .album_detail
                .as_ref()
                .map(|d| d.cover_url.as_str())
                .filter(|u| !u.is_empty())
        } else if self.view.overlay == Some(Overlay::ArtistDetail) {
            self.view
                .artist_detail
                .as_ref()
                .map(|d| d.picture_url.as_str())
                .filter(|u| !u.is_empty())
        } else {
            None
        }
    }

    /// Trigger async image fetch if the cover URL changed.
    fn maybe_fetch_cover_image(&mut self) {
        let url = match self.current_cover_url() {
            Some(u) => u.to_string(),
            None => {
                // No overlay or no URL — clear image
                if self.view.cover_image.is_some() {
                    self.view.cover_image = None;
                    self.view.cover_image_url.clear();
                }
                return;
            }
        };

        if url == self.view.cover_image_url {
            return; // Already loaded or loading
        }

        // Mark as loading this URL (prevents re-triggering)
        self.view.cover_image_url = url.clone();
        self.view.cover_image = None;

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

    async fn send_cmd(&mut self, cmd: &Command) -> std::io::Result<()> {
        use tokio::io::AsyncWriteExt;
        let mut json = serde_json::to_string(cmd)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        json.push('\n');
        self.writer.write_all(json.as_bytes()).await?;
        self.writer.flush().await
    }

    pub async fn run(&mut self) -> Result<()> {
        // Load saved theme from config
        let config = Config::load();
        if let Some(ref theme_str) = config.theme {
            if let Some(id) = ThemeId::from_str(theme_str) {
                Theme::set(id);
            }
        }

        // Setup terminal
        enable_raw_mode()?;
        io::stdout().execute(EnterAlternateScreen)?;
        io::stdout().execute(EnableMouseCapture)?;
        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        // Reset login mode when starting on login screen
        if self.view.screen == Screen::Login {
            self.view.login_mode = LoginMode::Button;
        }

        // Main client loop
        let mut running = true;
        let mut send_shutdown = false;

        while running {
            // Clear expired toast
            if self.view.toast.as_ref().is_some_and(|t| t.is_expired()) {
                self.view.toast = None;
            }

            terminal.draw(|frame| {
                ui::draw(frame, &mut self.view);
            })?;

            // Poll for input events (non-blocking with short timeout)
            if event::poll(TICK_RATE)? {
                let action = match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key),
                    Event::Mouse(mouse)
                        if mouse.kind == MouseEventKind::Down(MouseButton::Left) =>
                    {
                        self.handle_mouse_click(mouse.column, mouse.row)
                            .unwrap_or(KeyAction::Continue)
                    }
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
                            self.view.status_msg = Some(t().daemon_disconnected.into());
                            running = false;
                        }
                    }
                    KeyAction::MultiCommand(cmds) => {
                        for cmd in &cmds {
                            if let Err(e) = self.send_cmd(cmd).await {
                                debug!("Send command error: {e}");
                                self.view.status_msg = Some(t().daemon_disconnected.into());
                                running = false;
                                break;
                            }
                        }
                    }
                    KeyAction::WebLogin => {
                        // Suspend TUI
                        io::stdout().execute(DisableMouseCapture)?;
                        disable_raw_mode()?;
                        io::stdout().execute(LeaveAlternateScreen)?;
                        drop(terminal);

                        // Run browser login (blocking)
                        let result = crate::web_login::login_via_browser();

                        // Resume TUI
                        enable_raw_mode()?;
                        io::stdout().execute(EnterAlternateScreen)?;
                        io::stdout().execute(EnableMouseCapture)?;
                        terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
                        terminal.clear()?;

                        if let Ok(Some(arl)) = result {
                            self.view.login_loading = true;
                            self.send_cmd(&Command::Login { arl }).await?;
                        }
                    }
                }

                // Check if overlay changed (may need new cover image)
                self.maybe_fetch_cover_image();
            }

            if !running {
                break;
            }

            // Try to read messages from daemon (non-blocking)
            match tokio::time::timeout(
                Duration::from_millis(1),
                read_line::<ServerMessage, _>(&mut self.reader),
            )
            .await
            {
                Ok(Ok(Some(ServerMessage::Snapshot(snap)))) => {
                    self.view.update_from_snapshot(snap);
                    self.maybe_fetch_cover_image();
                }
                Ok(Ok(Some(ServerMessage::Error(err)))) => {
                    self.view.status_msg = Some(format!("Error: {err}"));
                }
                Ok(Ok(None)) => {
                    // Daemon disconnected
                    self.view.status_msg = Some(t().daemon_disconnected.into());
                    running = false;
                }
                Ok(Err(e)) => {
                    // Read/parse error — log but don't crash immediately
                    debug!("Read error from daemon: {e}");
                    self.view.status_msg = Some(format!("Communication error: {e}"));
                }
                Err(_) => {
                    // Timeout — no data available, continue
                }
            }

            // Check for completed image fetches
            while let Ok((url, img)) = self.image_rx.try_recv() {
                if url == self.view.cover_image_url {
                    let proto = self.picker.new_resize_protocol(img);
                    self.view.cover_image = Some(proto);
                }
            }
        }

        // Restore terminal
        io::stdout().execute(DisableMouseCapture)?;
        disable_raw_mode()?;
        io::stdout().execute(LeaveAlternateScreen)?;

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

        // ? : toggle help overlay (not during text input)
        if key.code == KeyCode::Char('?')
            && self.view.screen == Screen::Main
            && self.view.input_mode != InputMode::Typing
            && !self.view.radio_filter_typing
        {
            self.view.overlay = match self.view.overlay {
                Some(Overlay::Help) => None,
                _ => Some(Overlay::Help),
            };
            return KeyAction::Continue;
        }

        // i : toggle info modal (not during text input)
        if key.code == KeyCode::Char('i')
            && self.view.screen == Screen::Main
            && self.view.input_mode != InputMode::Typing
            && !self.view.radio_filter_typing
        {
            self.view.overlay = match self.view.overlay {
                Some(Overlay::Info) => None,
                _ => Some(Overlay::Info),
            };
            return KeyAction::Continue;
        }

        // Ctrl+O : toggle settings overlay
        if key.code == KeyCode::Char('o') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.view.overlay = match self.view.overlay {
                Some(Overlay::Settings { .. }) => None,
                _ => Some(Overlay::Settings { selected: 0 }),
            };
            return KeyAction::Continue;
        }

        // Ctrl+F: enter search/filter mode (same as /)
        if key.code == KeyCode::Char('f') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if self.view.screen == Screen::Main {
                match self.view.active_tab {
                    ActiveTab::Search => {
                        self.view.input_mode = InputMode::Typing;
                        self.view.search_input.clear();
                    }
                    ActiveTab::Radio => {
                        self.view.radio_filter_typing = true;
                        self.view.radio_filter_input.clear();
                        self.view.apply_radio_filter();
                    }
                    _ => {}
                }
            }
            return KeyAction::Continue;
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
        // Popup mode takes priority over overlays (popups can open on top of overlays)
        if self.view.popup.is_some() {
            return self.handle_popup_key(key);
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

        // Ctrl+P: open manage popup for currently playing track
        if key.code == KeyCode::Char('p') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if let Some(ref track) = self.view.current_track {
                let is_fav = self.view.is_track_favorite(&track.track_id);
                self.view.popup = Some(PopupMenu::manage_only(track.clone(), is_fav));
            }
            return KeyAction::Continue;
        }

        // Ctrl+Q: quit (send Shutdown to daemon)
        if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return KeyAction::Quit;
        }

        // Normal mode
        match key.code {
            // Tab navigation (disabled in offline mode — only Offline tab available)
            KeyCode::Tab | KeyCode::BackTab if self.view.is_offline => KeyAction::Continue,
            KeyCode::Tab => KeyAction::SendCommand(Command::NextTab),
            KeyCode::BackTab => KeyAction::SendCommand(Command::PrevTab),

            // Enter search/filter typing mode
            KeyCode::Char('/') => {
                match self.view.active_tab {
                    ActiveTab::Search => {
                        self.view.input_mode = InputMode::Typing;
                        self.view.search_input.clear();
                    }
                    ActiveTab::Radio => {
                        self.view.radio_filter_typing = true;
                        self.view.radio_filter_input.clear();
                        self.view.apply_radio_filter();
                    }
                    _ => {}
                }
                KeyAction::Continue
            }

            // Open track context menu
            KeyCode::Char('m') => {
                self.open_track_popup();
                KeyAction::Continue
            }

            // Open album detail page
            KeyCode::Char('a') => self.open_album_detail(),

            // Open artist detail page
            KeyCode::Char('t') => self.open_artist_detail(),

            // Open waiting list
            KeyCode::Char('w') => {
                if !self.view.queue.is_empty() {
                    self.view.overlay = Some(Overlay::WaitingList { selected: 0 });
                }
                KeyAction::Continue
            }

            // Category navigation (h/l or left/right)
            KeyCode::Char('h') | KeyCode::Left => KeyAction::SendCommand(Command::PrevCategory),
            KeyCode::Char('l') | KeyCode::Right => {
                if self.view.active_tab == ActiveTab::Downloads
                    && self.view.offline_category == OfflineCategory::Albums
                {
                    return self.handle_offline_tree_toggle();
                }
                KeyAction::SendCommand(Command::NextCategory)
            }

            // List navigation
            KeyCode::Up | KeyCode::Char('k') => {
                if self.view.active_tab == ActiveTab::Radio {
                    self.view.radios_selected = self.view.radios_selected.saturating_sub(1);
                    return KeyAction::Continue;
                }
                if self.view.active_tab == ActiveTab::Downloads
                    && self.view.offline_category == OfflineCategory::Albums
                {
                    self.view.offline_tree_selected =
                        self.view.offline_tree_selected.saturating_sub(1);
                    return KeyAction::Continue;
                }
                KeyAction::SendCommand(Command::SelectUp)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.view.active_tab == ActiveTab::Radio {
                    if !self.view.radios_filtered.is_empty() {
                        self.view.radios_selected = (self.view.radios_selected + 1)
                            .min(self.view.radios_filtered.len() - 1);
                    }
                    return KeyAction::Continue;
                }
                if self.view.active_tab == ActiveTab::Downloads
                    && self.view.offline_category == OfflineCategory::Albums
                {
                    let tree_len = self.view.offline_tree_items().len();
                    if tree_len > 0 {
                        self.view.offline_tree_selected =
                            (self.view.offline_tree_selected + 1).min(tree_len - 1);
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
                    ActiveTab::Favorites => self
                        .view
                        .favorites_display
                        .get(self.view.favorites_selected),
                    _ => None,
                };
                if let Some(item) = item {
                    if item.track.is_none() {
                        if let Some(artist_id) = item.artist_id.clone() {
                            self.view.overlay = Some(Overlay::ArtistDetail);
                            self.view.artist_detail_selected = 0;
                            return KeyAction::SendCommand(Command::GetArtistDetail { artist_id });
                        }
                        if let Some(album_id) = item.album_id.clone() {
                            self.view.overlay = Some(Overlay::AlbumDetail { from_artist: false });
                            self.view.album_detail_selected = 0;
                            return KeyAction::SendCommand(Command::GetAlbumDetail { album_id });
                        }
                        if let Some(playlist_id) = item.playlist_id.clone() {
                            self.view.overlay = Some(Overlay::PlaylistDetail { selected: 0 });
                            return KeyAction::SendCommand(Command::GetPlaylistDetail {
                                playlist_id,
                            });
                        }
                    }
                }
                match self.view.active_tab {
                    ActiveTab::Search => KeyAction::SendCommand(Command::PlayFromSearch {
                        index: self.view.search_selected,
                    }),
                    ActiveTab::Favorites => KeyAction::SendCommand(Command::PlayFromFavorites {
                        index: self.view.favorites_selected,
                    }),
                    ActiveTab::Radio => {
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
                    ActiveTab::Downloads => match self.view.offline_category {
                        OfflineCategory::Tracks => {
                            KeyAction::SendCommand(Command::PlayFromOffline {
                                index: self.view.offline_selected,
                            })
                        }
                        OfflineCategory::Albums => self.handle_offline_tree_enter(),
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
            Overlay::Help => {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter | KeyCode::Char('?') => {
                        self.view.overlay = None;
                    }
                    _ => {}
                }
                KeyAction::Continue
            }
            Overlay::Info => {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter | KeyCode::Char('i') => {
                        self.view.overlay = None;
                    }
                    _ => {}
                }
                KeyAction::Continue
            }
            Overlay::Settings { selected } => {
                const SETTINGS_COUNT: usize = 4;
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.view.overlay = None;
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
                                self.view.overlay = Some(Overlay::Help);
                                return KeyAction::Continue;
                            }
                            1 => {
                                // Themes
                                let current = Theme::current();
                                let idx =
                                    ThemeId::ALL.iter().position(|&t| t == current).unwrap_or(0);
                                self.view.overlay = Some(Overlay::ThemePicker { selected: idx });
                                return KeyAction::Continue;
                            }
                            2 => {
                                // Language
                                let current = i18n::current_locale();
                                let idx =
                                    Locale::ALL.iter().position(|&l| l == current).unwrap_or(0);
                                self.view.overlay = Some(Overlay::LanguagePicker { selected: idx });
                                return KeyAction::Continue;
                            }
                            3 => {
                                // Logout
                                self.view.overlay = None;
                                return KeyAction::SendCommand(Command::Logout);
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
                // Also close on Ctrl+O
                if key.code == KeyCode::Char('o') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.view.overlay = None;
                }
                KeyAction::Continue
            }
            Overlay::LanguagePicker { selected } => {
                let count = Locale::ALL.len();
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.view.overlay = Some(Overlay::Settings { selected: 2 });
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
                        self.view.overlay = Some(Overlay::Settings { selected: 2 });
                    }
                    _ => {}
                }
                KeyAction::Continue
            }
            Overlay::AlbumDetail { .. } => self.handle_album_detail_key(key),
            Overlay::ArtistDetail => self.handle_artist_detail_key(key),
            Overlay::PlaylistDetail { .. } => self.handle_playlist_detail_key(key),
            Overlay::WaitingList { .. } => self.handle_waiting_list_key(key),
            Overlay::ThemePicker { selected } => {
                let count = ThemeId::ALL.len();
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        // Back to settings
                        self.view.overlay = Some(Overlay::Settings { selected: 1 });
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        *selected = selected.saturating_sub(1);
                        Theme::set(ThemeId::ALL[*selected]);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        *selected = (*selected + 1).min(count - 1);
                        Theme::set(ThemeId::ALL[*selected]);
                    }
                    KeyCode::Enter => {
                        // Confirm selection, save to config, back to settings
                        let theme_id = ThemeId::ALL[*selected];
                        let mut config = Config::load();
                        config.theme = Some(theme_id.as_str().to_string());
                        let _ = config.save();
                        self.view.overlay = Some(Overlay::Settings { selected: 1 });
                    }
                    _ => {}
                }
                KeyAction::Continue
            }
        }
    }

    /// Open the full track context menu for the selected track in the current list.
    /// Handle Enter key in the offline albums tree view.
    fn handle_offline_tree_enter(&mut self) -> KeyAction {
        let items = self.view.offline_tree_items();
        let Some(item) = items.get(self.view.offline_tree_selected) else {
            return KeyAction::Continue;
        };
        match item {
            OfflineTreeItem::Album(album_idx) => {
                let Some(album) = self.view.offline_albums.get(*album_idx) else {
                    return KeyAction::Continue;
                };
                KeyAction::SendCommand(Command::PlayOfflineAlbum {
                    album_id: album.album_id.clone(),
                    track_index: 0,
                })
            }
            OfflineTreeItem::Track(album_idx, track_idx) => {
                let Some(album) = self.view.offline_albums.get(*album_idx) else {
                    return KeyAction::Continue;
                };
                KeyAction::SendCommand(Command::PlayOfflineAlbum {
                    album_id: album.album_id.clone(),
                    track_index: *track_idx,
                })
            }
        }
    }

    fn handle_offline_tree_toggle(&mut self) -> KeyAction {
        let items = self.view.offline_tree_items();
        let Some(item) = items.get(self.view.offline_tree_selected) else {
            return KeyAction::Continue;
        };
        match item {
            OfflineTreeItem::Album(album_idx) => {
                let Some(album) = self.view.offline_albums.get(*album_idx) else {
                    return KeyAction::Continue;
                };
                let album_id = album.album_id.clone();
                if self.view.offline_expanded.contains(&album_id) {
                    self.view.offline_expanded.retain(|id| id != &album_id);
                } else {
                    self.view.offline_expanded.push(album_id);
                }
            }
            OfflineTreeItem::Track(..) => {}
        }
        KeyAction::Continue
    }

    fn open_track_popup(&mut self) {
        let track = match self.view.active_tab {
            ActiveTab::Search => self
                .view
                .search_display
                .get(self.view.search_selected)
                .and_then(|d| d.track.clone()),
            ActiveTab::Favorites => self
                .view
                .favorites_display
                .get(self.view.favorites_selected)
                .and_then(|d| d.track.clone()),
            ActiveTab::Downloads => match self.view.offline_category {
                OfflineCategory::Tracks => self
                    .view
                    .offline_tracks
                    .get(self.view.offline_selected)
                    .map(|ot| ot.track.clone()),
                OfflineCategory::Albums => {
                    let items = self.view.offline_tree_items();
                    match items.get(self.view.offline_tree_selected) {
                        Some(OfflineTreeItem::Track(ai, ti)) => self
                            .view
                            .offline_albums
                            .get(*ai)
                            .and_then(|a| a.tracks.get(*ti).cloned()),
                        _ => None,
                    }
                }
            },
            _ => None,
        };

        if let Some(track) = track {
            let is_fav = self.view.is_track_favorite(&track.track_id);
            self.view.popup = Some(PopupMenu::full(track, is_fav));
        }
    }

    /// Handle key events in the album detail overlay.
    fn handle_album_detail_key(&mut self, key: KeyEvent) -> KeyAction {
        let from_artist = matches!(
            self.view.overlay,
            Some(Overlay::AlbumDetail { from_artist: true })
        );
        match key.code {
            KeyCode::Esc => {
                if from_artist {
                    self.view.overlay = Some(Overlay::ArtistDetail);
                } else {
                    self.view.overlay = None;
                }
                KeyAction::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.view.album_detail_selected = self.view.album_detail_selected.saturating_sub(1);
                KeyAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(ref detail) = self.view.album_detail {
                    if !detail.tracks.is_empty() {
                        self.view.album_detail_selected =
                            (self.view.album_detail_selected + 1).min(detail.tracks.len() - 1);
                    }
                }
                KeyAction::Continue
            }
            KeyCode::Enter => {
                let index = self.view.album_detail_selected;
                KeyAction::SendCommand(Command::PlayFromAlbum { index })
            }
            KeyCode::Char('m') => {
                if let Some(ref detail) = self.view.album_detail {
                    if let Some(track) = detail.tracks.get(self.view.album_detail_selected).cloned()
                    {
                        let is_fav = self.view.is_track_favorite(&track.track_id);
                        self.view.popup = Some(PopupMenu::full(track, is_fav));
                    }
                }
                KeyAction::Continue
            }
            KeyCode::Char('t') => {
                if let Some(ref detail) = self.view.album_detail {
                    if let Some(track) = detail.tracks.get(self.view.album_detail_selected) {
                        if let Some(ref artist_id) = track.artist_id {
                            let artist_id = artist_id.clone();
                            self.view.overlay = Some(Overlay::ArtistDetail);
                            self.view.artist_detail_selected = 0;
                            return KeyAction::SendCommand(Command::GetArtistDetail { artist_id });
                        }
                    }
                }
                KeyAction::Continue
            }
            KeyCode::Char('o') => {
                if let Some(ref detail) = self.view.album_detail {
                    let album_id = detail.album_id.clone();
                    KeyAction::SendCommand(Command::DownloadAlbumOffline { album_id })
                } else {
                    KeyAction::Continue
                }
            }
            // Player controls still work in album detail
            KeyCode::Char(' ') => KeyAction::SendCommand(Command::TogglePause),
            KeyCode::Char('n') => KeyAction::SendCommand(Command::NextTrack),
            KeyCode::Char('b') => KeyAction::SendCommand(Command::PrevTrack),
            KeyCode::Char('+') | KeyCode::Char('=') => {
                let new_vol = (self.view.volume + 0.05).min(1.0);
                KeyAction::SendCommand(Command::SetVolume { volume: new_vol })
            }
            KeyCode::Char('-') => {
                let new_vol = (self.view.volume - 0.05).max(0.0);
                KeyAction::SendCommand(Command::SetVolume { volume: new_vol })
            }
            _ => KeyAction::Continue,
        }
    }

    /// Handle key events in the artist detail overlay.
    fn handle_artist_detail_key(&mut self, key: KeyEvent) -> KeyAction {
        match key.code {
            KeyCode::Esc => {
                self.view.overlay = None;
                KeyAction::Continue
            }
            // Switch sub-tab with h/l
            KeyCode::Char('h') | KeyCode::Left => {
                self.view.artist_detail_sub_tab = self.view.artist_detail_sub_tab.prev();
                self.view.artist_detail_selected = 0;
                KeyAction::Continue
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.view.artist_detail_sub_tab = self.view.artist_detail_sub_tab.next();
                self.view.artist_detail_selected = 0;
                KeyAction::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.view.artist_detail_selected =
                    self.view.artist_detail_selected.saturating_sub(1);
                KeyAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let count = self.artist_detail_list_len();
                if count > 0 {
                    self.view.artist_detail_selected =
                        (self.view.artist_detail_selected + 1).min(count - 1);
                }
                KeyAction::Continue
            }
            KeyCode::Enter => {
                let sub_tab = self.view.artist_detail_sub_tab;
                let index = self.view.artist_detail_selected;
                if sub_tab == ArtistSubTab::TopTracks {
                    KeyAction::SendCommand(Command::PlayFromArtist { index })
                } else {
                    // Open the album detail for the selected album entry
                    if let Some(ref detail) = self.view.artist_detail {
                        let albums = detail.albums_for_tab(sub_tab);
                        if let Some(album) = albums.get(index) {
                            let album_id = album.album_id.clone();
                            self.view.overlay = Some(Overlay::AlbumDetail { from_artist: true });
                            self.view.album_detail_selected = 0;
                            return KeyAction::SendCommand(Command::GetAlbumDetail { album_id });
                        }
                    }
                    KeyAction::Continue
                }
            }
            KeyCode::Char('m') => {
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
                KeyAction::Continue
            }
            // Player controls still work
            KeyCode::Char(' ') => KeyAction::SendCommand(Command::TogglePause),
            KeyCode::Char('n') => KeyAction::SendCommand(Command::NextTrack),
            KeyCode::Char('b') => KeyAction::SendCommand(Command::PrevTrack),
            KeyCode::Char('+') | KeyCode::Char('=') => {
                let new_vol = (self.view.volume + 0.05).min(1.0);
                KeyAction::SendCommand(Command::SetVolume { volume: new_vol })
            }
            KeyCode::Char('-') => {
                let new_vol = (self.view.volume - 0.05).max(0.0);
                KeyAction::SendCommand(Command::SetVolume { volume: new_vol })
            }
            _ => KeyAction::Continue,
        }
    }

    /// Get the number of items in the current artist detail sub-tab list.
    fn artist_detail_list_len(&self) -> usize {
        if let Some(ref detail) = self.view.artist_detail {
            match self.view.artist_detail_sub_tab {
                ArtistSubTab::TopTracks => detail.top_tracks.len(),
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
            ActiveTab::Favorites => self
                .view
                .favorites_display
                .get(self.view.favorites_selected),
            _ => None,
        };

        // Try to get album_id from the DisplayItem directly (album search/favorites)
        if let Some(album_id) = item.and_then(|i| i.album_id.clone()) {
            self.view.overlay = Some(Overlay::AlbumDetail { from_artist: false });
            self.view.album_detail_selected = 0;
            return KeyAction::SendCommand(Command::GetAlbumDetail { album_id });
        }

        // For tracks, get album_id from the embedded TrackData
        if let Some(album_id) = item
            .and_then(|i| i.track.as_ref())
            .and_then(|t| t.album_id.clone())
        {
            self.view.overlay = Some(Overlay::AlbumDetail { from_artist: false });
            self.view.album_detail_selected = 0;
            return KeyAction::SendCommand(Command::GetAlbumDetail { album_id });
        }

        KeyAction::Continue
    }

    /// Open artist detail page for the currently selected item.
    fn open_artist_detail(&mut self) -> KeyAction {
        let item = match self.view.active_tab {
            ActiveTab::Search => self.view.search_display.get(self.view.search_selected),
            ActiveTab::Favorites => self
                .view
                .favorites_display
                .get(self.view.favorites_selected),
            _ => None,
        };

        // Try artist_id directly from the DisplayItem (artist search/favorites)
        if let Some(artist_id) = item.and_then(|i| i.artist_id.clone()) {
            self.view.overlay = Some(Overlay::ArtistDetail);
            self.view.artist_detail_selected = 0;
            return KeyAction::SendCommand(Command::GetArtistDetail { artist_id });
        }

        // For tracks, get artist_id from the embedded TrackData
        if let Some(artist_id) = item
            .and_then(|i| i.track.as_ref())
            .and_then(|t| t.artist_id.clone())
        {
            self.view.overlay = Some(Overlay::ArtistDetail);
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

        let track_count = self
            .view
            .playlist_detail
            .as_ref()
            .map(|d| d.tracks.len())
            .unwrap_or(0);

        match key.code {
            KeyCode::Esc => {
                self.view.overlay = None;
                KeyAction::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let new_sel = selected.saturating_sub(1);
                self.view.overlay = Some(Overlay::PlaylistDetail { selected: new_sel });
                KeyAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = track_count.saturating_sub(1);
                let new_sel = (selected + 1).min(max);
                self.view.overlay = Some(Overlay::PlaylistDetail { selected: new_sel });
                KeyAction::Continue
            }
            KeyCode::Enter => KeyAction::SendCommand(Command::PlayFromPlaylist { index: selected }),
            // Context menu for selected track
            KeyCode::Char('m') => {
                if let Some(track) = self
                    .view
                    .playlist_detail
                    .as_ref()
                    .and_then(|d| d.tracks.get(selected))
                    .cloned()
                {
                    let is_fav = self.view.is_track_favorite(&track.track_id);
                    self.view.popup = Some(PopupMenu::full(track, is_fav));
                }
                KeyAction::Continue
            }
            // Player controls
            KeyCode::Char(' ') => KeyAction::SendCommand(Command::TogglePause),
            KeyCode::Char('n') => KeyAction::SendCommand(Command::NextTrack),
            KeyCode::Char('b') => KeyAction::SendCommand(Command::PrevTrack),
            KeyCode::Char('+') | KeyCode::Char('=') => {
                let new_vol = (self.view.volume + 0.05).min(1.0);
                KeyAction::SendCommand(Command::SetVolume { volume: new_vol })
            }
            KeyCode::Char('-') => {
                let new_vol = (self.view.volume - 0.05).max(0.0);
                KeyAction::SendCommand(Command::SetVolume { volume: new_vol })
            }
            _ => KeyAction::Continue,
        }
    }

    /// Handle key events in the waiting list overlay.
    fn handle_waiting_list_key(&mut self, key: KeyEvent) -> KeyAction {
        let selected = match self.view.overlay {
            Some(Overlay::WaitingList { selected }) => selected,
            _ => return KeyAction::Continue,
        };

        match key.code {
            KeyCode::Esc | KeyCode::Char('w') => {
                self.view.overlay = None;
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
            KeyCode::Char('f') => {
                if let Some(track) = self.view.queue.get(selected) {
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
                    return KeyAction::SendCommand(cmd);
                }
                KeyAction::Continue
            }
            // Open full context menu for selected track
            KeyCode::Char('m') => {
                if let Some(track) = self.view.queue.get(selected).cloned() {
                    let is_fav = self.view.is_track_favorite(&track.track_id);
                    self.view.popup = Some(PopupMenu::full(track, is_fav));
                }
                KeyAction::Continue
            }
            // Player controls still work
            KeyCode::Char(' ') => KeyAction::SendCommand(Command::TogglePause),
            KeyCode::Char('n') => KeyAction::SendCommand(Command::NextTrack),
            KeyCode::Char('b') => KeyAction::SendCommand(Command::PrevTrack),
            KeyCode::Char('+') | KeyCode::Char('=') => {
                let new_vol = (self.view.volume + 0.05).min(1.0);
                KeyAction::SendCommand(Command::SetVolume { volume: new_vol })
            }
            KeyCode::Char('-') => {
                let new_vol = (self.view.volume - 0.05).max(0.0);
                KeyAction::SendCommand(Command::SetVolume { volume: new_vol })
            }
            _ => KeyAction::Continue,
        }
    }

    /// Handle key events when a popup menu is open.
    fn handle_popup_key(&mut self, key: KeyEvent) -> KeyAction {
        let popup = self.view.popup.as_mut().unwrap();

        // Handle playlist picker sub-menu
        if let Some(ref mut sub) = popup.sub_menu {
            match sub {
                SubMenu::PlaylistPicker {
                    playlists,
                    selected,
                    loading,
                } => {
                    if *loading {
                        if key.code == KeyCode::Esc {
                            popup.sub_menu = None;
                        }
                        return KeyAction::Continue;
                    }
                    match key.code {
                        KeyCode::Esc => {
                            popup.sub_menu = None;
                            return KeyAction::Continue;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            *selected = selected.saturating_sub(1);
                            return KeyAction::Continue;
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if !playlists.is_empty() {
                                *selected = (*selected + 1).min(playlists.len() - 1);
                            }
                            return KeyAction::Continue;
                        }
                        KeyCode::Enter => {
                            if let Some(pl) = playlists.get(*selected) {
                                let cmd = Command::AddToPlaylist {
                                    playlist_id: pl.playlist_id.clone(),
                                    track_id: popup.track.track_id.clone(),
                                };
                                self.view.popup = None;
                                return KeyAction::SendCommand(cmd);
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
        let track = popup.track.clone();
        let is_favorite = popup.is_favorite;

        match action {
            PopupAction::Header => KeyAction::Continue,
            PopupAction::ToggleFavorite => {
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
                        });
                    }
                    KeyAction::Continue
                }
            }
            PopupAction::DownloadOffline => {
                self.view.popup = None;
                KeyAction::SendCommand(Command::DownloadOffline { track })
            }
            PopupAction::DislikeTrack => {
                self.view.popup = None;
                KeyAction::SendCommand(Command::DislikeTrack {
                    track_id: track.track_id,
                })
            }
            PopupAction::PlayNext => {
                self.view.popup = None;
                KeyAction::SendCommand(Command::PlayNext { track })
            }
            PopupAction::AddToQueue => {
                self.view.popup = None;
                KeyAction::SendCommand(Command::AddToQueue { track })
            }
            PopupAction::MixFromTrack => {
                self.view.popup = None;
                KeyAction::SendCommand(Command::StartMix {
                    track_id: track.track_id,
                })
            }
            PopupAction::Share => {
                let url = format!("https://www.deezer.com/track/{}", track.track_id);
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
                if let Some(ref album_id) = track.album_id {
                    let album_id = album_id.clone();
                    self.view.popup = None;
                    self.view.overlay = Some(Overlay::AlbumDetail { from_artist: false });
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
                if let Some(ref artist_id) = track.artist_id {
                    let artist_id = artist_id.clone();
                    self.view.popup = None;
                    self.view.overlay = Some(Overlay::ArtistDetail);
                    self.view.artist_detail_selected = 0;
                    KeyAction::SendCommand(Command::GetArtistDetail { artist_id })
                } else {
                    self.view.popup = None;
                    KeyAction::Continue
                }
            }
        }
    }

    fn handle_mouse_click(&mut self, col: u16, row: u16) -> Option<KeyAction> {
        // Only handle clicks on the login button
        if self.view.screen == Screen::Login
            && self.view.login_mode == LoginMode::Button
            && !self.view.login_loading
        {
            if let Some(rect) = self.view.login_button_area.get() {
                if col >= rect.x
                    && col < rect.x + rect.width
                    && row >= rect.y
                    && row < rect.y + rect.height
                {
                    return Some(KeyAction::WebLogin);
                }
            }
        }
        None
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
}
