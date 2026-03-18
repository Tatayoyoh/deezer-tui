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
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tracing::debug;

use deezer_core::api::models::{AlbumDetail, AudioQuality, DisplayItem, PlaylistData, TrackData};
use deezer_core::config::Config;
use deezer_core::player::state::{PlaybackStatus, RepeatMode};

use crate::protocol::{
    read_line, socket_path, ActiveTab, Command, DaemonSnapshot, FavoritesCategory, Screen,
    SearchCategory, ServerMessage,
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
    DislikeTrack,
    PlayNext,
    AddToQueue,
    MixFromTrack,
    Share,
    TrackInfo,
    ViewAlbum,
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
        let fav_label = if is_favorite {
            "Remove from favorites"
        } else {
            "Add to favorites"
        };
        let items = vec![
            PopupMenuItem {
                label: "── Manage ──".into(),
                action: PopupAction::Header,
                is_header: true,
            },
            PopupMenuItem {
                label: fav_label.into(),
                action: PopupAction::ToggleFavorite,
                is_header: false,
            },
            PopupMenuItem {
                label: "Add to playlist".into(),
                action: PopupAction::AddToPlaylist,
                is_header: false,
            },
            PopupMenuItem {
                label: "Don't recommend this track".into(),
                action: PopupAction::DislikeTrack,
                is_header: false,
            },
            PopupMenuItem {
                label: "── Playback ──".into(),
                action: PopupAction::Header,
                is_header: true,
            },
            PopupMenuItem {
                label: "Play next".into(),
                action: PopupAction::PlayNext,
                is_header: false,
            },
            PopupMenuItem {
                label: "Add to queue".into(),
                action: PopupAction::AddToQueue,
                is_header: false,
            },
            PopupMenuItem {
                label: "Mix inspired by this track".into(),
                action: PopupAction::MixFromTrack,
                is_header: false,
            },
            PopupMenuItem {
                label: "── Media ──".into(),
                action: PopupAction::Header,
                is_header: true,
            },
            PopupMenuItem {
                label: "Track album".into(),
                action: PopupAction::ViewAlbum,
                is_header: false,
            },
            PopupMenuItem {
                label: "Share".into(),
                action: PopupAction::Share,
                is_header: false,
            },
            PopupMenuItem {
                label: "Track info".into(),
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
        let fav_label = if is_favorite {
            "Remove from favorites"
        } else {
            "Add to favorites"
        };
        let title = format!("{} — {}", track.title, track.artist);
        let items = vec![
            PopupMenuItem {
                label: "── Manage ──".into(),
                action: PopupAction::Header,
                is_header: true,
            },
            PopupMenuItem {
                label: fav_label.into(),
                action: PopupAction::ToggleFavorite,
                is_header: false,
            },
            PopupMenuItem {
                label: "Add to playlist".into(),
                action: PopupAction::AddToPlaylist,
                is_header: false,
            },
            PopupMenuItem {
                label: "Don't recommend this track".into(),
                action: PopupAction::DislikeTrack,
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
    /// Album detail view.
    AlbumDetail,
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
    pub playlists: Vec<PlaylistData>,
    pub album_detail: Option<AlbumDetail>,
    pub album_detail_selected: usize,
    pub album_detail_loading: bool,
    pub status_msg: Option<String>,
    pub login_error: Option<String>,
    pub login_loading: bool,
    pub user_name: Option<String>,

    // Local client state
    pub input_mode: InputMode,
    pub search_input: String,
    pub login_mode: LoginMode,
    pub login_input: String,
    pub login_cursor: usize,
    pub popup: Option<PopupMenu>,
    pub overlay: Option<Overlay>,
    pub toast: Option<Toast>,

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
            playlists: snap.playlists.clone(),
            album_detail: snap.album_detail.clone(),
            album_detail_selected: snap.album_detail_selected,
            album_detail_loading: snap.album_detail_loading,
            status_msg: snap.status_msg.clone(),
            login_error: snap.login_error.clone(),
            login_loading: snap.login_loading,
            user_name: snap.user_name.clone(),

            input_mode: InputMode::Normal,
            search_input: String::new(),
            login_mode: LoginMode::Button,
            login_input: String::new(),
            login_cursor: 0,
            popup: None,
            overlay: None,
            toast: None,
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
        self.album_detail = snap.album_detail;
        // Don't overwrite album_detail_selected — it's managed client-side
        self.album_detail_loading = snap.album_detail_loading;
        self.status_msg = snap.status_msg;
        self.login_error = snap.login_error;
        self.login_loading = snap.login_loading;
        self.user_name = snap.user_name;

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
    }

    /// Check if a track is in the user's favorites.
    fn is_track_favorite(&self, track_id: &str) -> bool {
        self.favorites.iter().any(|t| t.track_id == track_id)
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
}

impl Client {
    pub async fn connect() -> Result<Self> {
        let path = socket_path();
        let stream = UnixStream::connect(&path).await?;
        let (read_half, write_half) = stream.into_split();

        Ok(Self {
            view: ViewState::from_snapshot(&DaemonSnapshot::default()),
            reader: BufReader::new(read_half),
            writer: write_half,
        })
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
                ui::draw(frame, &self.view);
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
                        self.send_cmd(&cmd).await?;
                    }
                    KeyAction::MultiCommand(cmds) => {
                        for cmd in &cmds {
                            self.send_cmd(cmd).await?;
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
                }
                Ok(Ok(Some(ServerMessage::Error(err)))) => {
                    self.view.status_msg = Some(format!("Error: {err}"));
                }
                Ok(Ok(None)) => {
                    // Daemon disconnected
                    self.view.status_msg = Some("Daemon disconnected".into());
                    running = false;
                }
                Ok(Err(_)) => {
                    // Read error
                    running = false;
                }
                Err(_) => {
                    // Timeout — no data available, continue
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
            eprintln!("deezer-tui: music continues in background. Run \"deezer-tui\" to restore the player.");
        }

        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> KeyAction {
        debug!(?key, "client key event");

        // Ctrl+C always detaches (daemon keeps playing)
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return KeyAction::Detach;
        }

        // ? : toggle help overlay
        if key.code == KeyCode::Char('?') && self.view.screen == Screen::Main {
            self.view.overlay = match self.view.overlay {
                Some(Overlay::Help) => None,
                _ => Some(Overlay::Help),
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

        // Ctrl+F: enter search mode (same as /)
        if key.code == KeyCode::Char('f') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if self.view.screen == Screen::Main && self.view.active_tab == ActiveTab::Search {
                self.view.input_mode = InputMode::Typing;
                self.view.search_input.clear();
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
        // Overlay mode — intercept all keys
        if self.view.overlay.is_some() {
            return self.handle_overlay_key(key);
        }

        // Popup mode — intercept all keys
        if self.view.popup.is_some() {
            return self.handle_popup_key(key);
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

        // Ctrl+P: open manage popup for currently playing track
        if key.code == KeyCode::Char('p') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if let Some(ref track) = self.view.current_track {
                let is_fav = self.view.is_track_favorite(&track.track_id);
                self.view.popup = Some(PopupMenu::manage_only(track.clone(), is_fav));
            }
            return KeyAction::Continue;
        }

        // Normal mode
        match key.code {
            KeyCode::Char('q') => KeyAction::Quit,

            // Tab navigation
            KeyCode::Tab => KeyAction::SendCommand(Command::NextTab),
            KeyCode::BackTab => KeyAction::SendCommand(Command::PrevTab),

            // Enter search typing mode
            KeyCode::Char('/') => {
                if self.view.active_tab == ActiveTab::Search {
                    self.view.input_mode = InputMode::Typing;
                    self.view.search_input.clear();
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

            // Category navigation (h/l or left/right)
            KeyCode::Char('h') | KeyCode::Left => KeyAction::SendCommand(Command::PrevCategory),
            KeyCode::Char('l') | KeyCode::Right => KeyAction::SendCommand(Command::NextCategory),

            // List navigation
            KeyCode::Up | KeyCode::Char('k') => KeyAction::SendCommand(Command::SelectUp),
            KeyCode::Down | KeyCode::Char('j') => KeyAction::SendCommand(Command::SelectDown),

            // Play selected track or open album detail
            KeyCode::Enter => {
                // Check if the selected item is an album (has album_id but no track)
                let item = match self.view.active_tab {
                    ActiveTab::Search => {
                        self.view.search_display.get(self.view.search_selected)
                    }
                    ActiveTab::Favorites => {
                        self.view.favorites_display.get(self.view.favorites_selected)
                    }
                    _ => None,
                };
                if let Some(item) = item {
                    if item.track.is_none() {
                        if let Some(album_id) = item.album_id.clone() {
                            self.view.overlay = Some(Overlay::AlbumDetail);
                            self.view.album_detail_selected = 0;
                            return KeyAction::SendCommand(Command::GetAlbumDetail {
                                album_id,
                            });
                        }
                    }
                }
                match self.view.active_tab {
                    ActiveTab::Search => KeyAction::SendCommand(Command::PlayFromSearch {
                        index: self.view.search_selected,
                    }),
                    ActiveTab::Favorites => {
                        KeyAction::SendCommand(Command::PlayFromFavorites {
                            index: self.view.favorites_selected,
                        })
                    }
                    _ => KeyAction::Continue,
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
            KeyCode::Char('p') | KeyCode::Char(' ') => KeyAction::SendCommand(Command::TogglePause),
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
                            _ => {
                                // Other entries — placeholder
                            }
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
            Overlay::AlbumDetail => {
                self.handle_album_detail_key(key)
            }
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
            _ => None,
        };

        if let Some(track) = track {
            let is_fav = self.view.is_track_favorite(&track.track_id);
            self.view.popup = Some(PopupMenu::full(track, is_fav));
        }
    }

    /// Handle key events in the album detail overlay.
    fn handle_album_detail_key(&mut self, key: KeyEvent) -> KeyAction {
        match key.code {
            KeyCode::Esc => {
                self.view.overlay = None;
                KeyAction::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.view.album_detail_selected =
                    self.view.album_detail_selected.saturating_sub(1);
                KeyAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(ref detail) = self.view.album_detail {
                    if !detail.tracks.is_empty() {
                        self.view.album_detail_selected = (self.view.album_detail_selected + 1)
                            .min(detail.tracks.len() - 1);
                    }
                }
                KeyAction::Continue
            }
            KeyCode::Enter => {
                let index = self.view.album_detail_selected;
                KeyAction::SendCommand(Command::PlayFromAlbum { index })
            }
            // Player controls still work in album detail
            KeyCode::Char('p') | KeyCode::Char(' ') => {
                KeyAction::SendCommand(Command::TogglePause)
            }
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
            self.view.overlay = Some(Overlay::AlbumDetail);
            self.view.album_detail_selected = 0;
            return KeyAction::SendCommand(Command::GetAlbumDetail { album_id });
        }

        // For tracks, get album_id from the embedded TrackData
        if let Some(album_id) = item
            .and_then(|i| i.track.as_ref())
            .and_then(|t| t.album_id.clone())
        {
            self.view.overlay = Some(Overlay::AlbumDetail);
            self.view.album_detail_selected = 0;
            return KeyAction::SendCommand(Command::GetAlbumDetail { album_id });
        }

        KeyAction::Continue
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
                        self.view.toast = Some(Toast::new(
                            "Link copied to clipboard!".into(),
                            Duration::from_secs(2),
                        ));
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
                    self.view.overlay = Some(Overlay::AlbumDetail);
                    self.view.album_detail_selected = 0;
                    KeyAction::SendCommand(Command::GetAlbumDetail { album_id })
                } else {
                    self.view.popup = None;
                    self.view.toast = Some(Toast::new(
                        "No album info available".into(),
                        Duration::from_secs(2),
                    ));
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
