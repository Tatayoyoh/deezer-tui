use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::Terminal;
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tracing::debug;

use deezer_core::api::models::{AudioQuality, TrackData};
use deezer_core::player::state::{PlaybackStatus, RepeatMode};

use crate::protocol::{
    ActiveTab, Command, DaemonSnapshot, Screen, ServerMessage, read_line, socket_path,
};
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
    pub favorites: Vec<TrackData>,
    pub favorites_selected: usize,
    pub favorites_loading: bool,
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
            favorites: snap.favorites.clone(),
            favorites_selected: snap.favorites_selected,
            favorites_loading: snap.favorites_loading,
            status_msg: snap.status_msg.clone(),
            login_error: snap.login_error.clone(),
            login_loading: snap.login_loading,
            user_name: snap.user_name.clone(),

            input_mode: InputMode::Normal,
            search_input: String::new(),
            login_mode: LoginMode::Button,
            login_input: String::new(),
            login_cursor: 0,
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
        self.favorites = snap.favorites;
        self.favorites_selected = snap.favorites_selected;
        self.favorites_loading = snap.favorites_loading;
        self.status_msg = snap.status_msg;
        self.login_error = snap.login_error;
        self.login_loading = snap.login_loading;
        self.user_name = snap.user_name;

        // After login transition (Login → Main), reset typing mode
        if prev_screen == Screen::Login && self.screen == Screen::Main {
            self.input_mode = InputMode::Normal;
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
        let mut json = serde_json::to_string(cmd).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;
        json.push('\n');
        self.writer.write_all(json.as_bytes()).await?;
        self.writer.flush().await
    }

    pub async fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        io::stdout().execute(EnterAlternateScreen)?;
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
            terminal.draw(|frame| {
                ui::draw(frame, &self.view);
            })?;

            // Poll for keyboard events (non-blocking with short timeout)
            if event::poll(TICK_RATE)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        match self.handle_key(key) {
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
                            KeyAction::WebLogin => {
                                // Suspend TUI
                                disable_raw_mode()?;
                                io::stdout().execute(LeaveAlternateScreen)?;
                                drop(terminal);

                                // Run browser login (blocking)
                                let result = crate::web_login::login_via_browser();

                                // Resume TUI
                                enable_raw_mode()?;
                                io::stdout().execute(EnterAlternateScreen)?;
                                terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
                                terminal.clear()?;

                                if let Ok(Some(arl)) = result {
                                    self.view.login_loading = true;
                                    self.send_cmd(&Command::Login { arl }).await?;
                                }
                            }
                        }
                    }
                }
            }

            if !running {
                break;
            }

            // Try to read messages from daemon (non-blocking)
            match tokio::time::timeout(Duration::from_millis(1), read_line::<ServerMessage, _>(&mut self.reader)).await {
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

            // List navigation
            KeyCode::Up | KeyCode::Char('k') => {
                KeyAction::SendCommand(Command::SelectUp)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                KeyAction::SendCommand(Command::SelectDown)
            }

            // Play selected track
            KeyCode::Enter => match self.view.active_tab {
                ActiveTab::Search => {
                    KeyAction::SendCommand(Command::PlayFromSearch {
                        index: self.view.search_selected,
                    })
                }
                ActiveTab::Favorites => {
                    KeyAction::SendCommand(Command::PlayFromFavorites {
                        index: self.view.favorites_selected,
                    })
                }
                _ => KeyAction::Continue,
            },

            // Player controls
            KeyCode::Char('p') | KeyCode::Char(' ') => {
                KeyAction::SendCommand(Command::TogglePause)
            }
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

            _ => KeyAction::Continue,
        }
    }
}

/// Result of handling a key press.
enum KeyAction {
    /// Do nothing special.
    Continue,
    /// Send a command to the daemon.
    SendCommand(Command),
    /// Quit: send shutdown to daemon and exit.
    Quit,
    /// Detach: exit client but keep daemon running (Ctrl+Z / Ctrl+C).
    Detach,
    /// Open browser for Deezer web login.
    WebLogin,
}
