use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tracing::debug;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::Terminal;

use deezer_core::api::models::{AudioQuality, TrackData};
use deezer_core::api::DeezerClient;
use deezer_core::player::engine::PlayerEngine;
use deezer_core::player::state::{PlaybackStatus, PlayerState, RepeatMode};
use deezer_core::Config;

use crate::event::{self, AppEvent};
use crate::ui;

const TICK_RATE: Duration = Duration::from_millis(250);

/// UI display mode: foreground (TUI visible) or background (music only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    Foreground,
    Background,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Search,
    Favorites,
    Radio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Login,
    Main,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Typing,
}

/// Async operation results passed back via channel.
pub enum AsyncResult {
    LoginSuccess(String),
    LoginError(String),
    MasterKeyReady([u8; 16]),
    MasterKeyError(String),
    SearchResults(Vec<TrackData>),
    SearchError(String),
    FavoritesLoaded(Vec<TrackData>),
    FavoritesError(String),
    TrackReady {
        audio_data: Vec<u8>,
        track: TrackData,
        quality: AudioQuality,
    },
    TrackFetchError(String),
}

pub struct App {
    pub running: bool,
    pub ui_mode: UiMode,
    pub screen: Screen,
    pub active_tab: ActiveTab,
    pub input_mode: InputMode,
    pub config: Config,
    pub status_msg: Option<String>,

    // Login screen state
    pub login_input: String,
    pub login_cursor: usize,
    pub login_error: Option<String>,
    pub login_loading: bool,

    // Search state
    pub search_input: String,
    pub search_results: Vec<TrackData>,
    pub search_selected: usize,
    pub search_loading: bool,

    // Favorites state
    pub favorites: Vec<TrackData>,
    pub favorites_selected: usize,
    pub favorites_loading: bool,

    // Player state (shared with engine)
    pub player_state: Arc<Mutex<PlayerState>>,

    // Core components
    client: Arc<tokio::sync::Mutex<DeezerClient>>,
    engine: Option<PlayerEngine>,
    master_key: Option<[u8; 16]>,

    // Async channel
    async_tx: tokio::sync::mpsc::UnboundedSender<AsyncResult>,
    async_rx: tokio::sync::mpsc::UnboundedReceiver<AsyncResult>,

    // Playback position tracking
    playback_started_at: Option<Instant>,
    playback_offset_secs: u64,

    // Cached audio data for current track (needed to resume after fork)
    current_audio_data: Option<Vec<u8>>,
}

impl App {
    pub fn new() -> Result<Self> {
        let config = Config::load();
        let screen = if config.arl.is_some() {
            Screen::Main
        } else {
            Screen::Login
        };

        let client = DeezerClient::new().map_err(|e| anyhow::anyhow!("{e}"))?;
        let (async_tx, async_rx) = tokio::sync::mpsc::unbounded_channel();

        Ok(Self {
            running: true,
            ui_mode: UiMode::Foreground,
            screen,
            active_tab: ActiveTab::Search,
            input_mode: if screen == Screen::Login {
                InputMode::Typing
            } else {
                InputMode::Normal
            },
            config,
            status_msg: None,

            login_input: String::new(),
            login_cursor: 0,
            login_error: None,
            login_loading: false,

            search_input: String::new(),
            search_results: Vec::new(),
            search_selected: 0,
            search_loading: false,

            favorites: Vec::new(),
            favorites_selected: 0,
            favorites_loading: false,

            player_state: Arc::new(Mutex::new(PlayerState::default())),

            client: Arc::new(tokio::sync::Mutex::new(client)),
            engine: None,
            master_key: None,

            async_tx,
            async_rx,

            playback_started_at: None,
            playback_offset_secs: 0,

            current_audio_data: None,
        })
    }

    /// Enter TUI mode: enable raw mode, alternate screen, create terminal.
    fn enter_tui() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
        // Reattach to the controlling terminal if we've been daemonized.
        // /dev/tty always refers to the calling user's terminal.
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            if let Ok(tty) = std::fs::OpenOptions::new().read(true).write(true).open("/dev/tty") {
                let tty_fd = tty.as_raw_fd();
                unsafe {
                    // Redirect stdout and stderr to the new tty
                    libc::dup2(tty_fd, 1); // stdout
                    libc::dup2(tty_fd, 2); // stderr
                    libc::dup2(tty_fd, 0); // stdin
                }
                // tty will be closed when dropped, but fds 0/1/2 remain valid
            }
        }

        enable_raw_mode()?;
        io::stdout().execute(EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;
        Ok(terminal)
    }

    /// Leave TUI mode: drop terminal, disable raw mode, leave alternate screen.
    fn leave_tui(terminal: Option<Terminal<CrosstermBackend<io::Stdout>>>) -> Result<()> {
        drop(terminal);
        disable_raw_mode()?;
        io::stdout().execute(LeaveAlternateScreen)?;
        Ok(())
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut terminal = Some(Self::enter_tui()?);

        // Register SIGUSR1 handler (unix only) for foreground recall
        #[cfg(unix)]
        let mut sigusr1 = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::user_defined1())?;

        // If we already have an ARL, auto-login
        if let Some(arl) = self.config.arl.clone() {
            self.status_msg = Some("Connecting...".into());
            self.start_login(arl);
        }

        // Main loop
        while self.running {
            match self.ui_mode {
                UiMode::Foreground => {
                    if let Some(ref mut term) = terminal {
                        term.draw(|frame| {
                            ui::draw(frame, self);
                        })?;
                    }

                    match event::poll(TICK_RATE)? {
                        AppEvent::Key(key) => self.handle_key(key),
                        AppEvent::Tick => self.on_tick(),
                    }

                    self.process_async_results();

                    // Transition to background if Ctrl+Z was pressed
                    if self.ui_mode == UiMode::Background {
                        Self::leave_tui(terminal.take())?;

                        // Fork: parent exits (returns shell prompt), child keeps playing
                        #[cfg(unix)]
                        {
                            eprintln!("deezer-tui: music continues in background. Run `deezer-tui` to restore.");
                            match unsafe { libc::fork() } {
                                -1 => {
                                    // fork failed — stay in foreground
                                    eprintln!("deezer-tui: fork failed, staying in foreground");
                                    terminal = Some(Self::enter_tui()?);
                                    self.ui_mode = UiMode::Foreground;
                                }
                                0 => {
                                    // Child: detach from terminal, continue running
                                    unsafe { libc::setsid() };
                                    // Re-register SIGUSR1 after fork
                                    sigusr1 = tokio::signal::unix::signal(
                                        tokio::signal::unix::SignalKind::user_defined1(),
                                    )?;
                                    // Update PID file with child's PID
                                    if let Some(path) = deezer_core::Config::dir().map(|d| d.join("deezer-tui.pid")) {
                                        let _ = std::fs::write(&path, std::process::id().to_string());
                                    }
                                    // Reinitialize audio — ALSA threads don't survive fork
                                    self.reinit_audio_after_fork();
                                }
                                _parent_pid => {
                                    // Parent: exit immediately to return shell prompt
                                    std::process::exit(0);
                                }
                            }
                        }
                    }
                }
                UiMode::Background => {
                    // Background mode: no TUI, just tick + wait for SIGUSR1
                    #[cfg(unix)]
                    {
                        tokio::select! {
                            _ = tokio::time::sleep(TICK_RATE) => {
                                self.on_tick();
                                self.process_async_results();
                            }
                            _ = sigusr1.recv() => {
                                // Another instance sent us SIGUSR1 — re-enter foreground
                                terminal = Some(Self::enter_tui()?);
                                self.ui_mode = UiMode::Foreground;
                            }
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        tokio::time::sleep(TICK_RATE).await;
                        self.on_tick();
                        self.process_async_results();
                    }
                }
            }
        }

        // Restore terminal if still in foreground
        if terminal.is_some() {
            Self::leave_tui(terminal)?;
        }

        Ok(())
    }

    /// Recreate the audio engine after fork().
    /// ALSA/cpal threads don't survive fork, so we must reinitialize.
    fn reinit_audio_after_fork(&mut self) {
        // Drop the old (broken) engine
        let old_state = self.player_state.lock().unwrap().clone();
        self.engine = None;

        if let Some(master_key) = self.master_key {
            match PlayerEngine::new(master_key) {
                Ok(mut engine) => {
                    // Restore volume
                    engine.set_volume(old_state.volume);

                    // Resume the current track if we have cached audio
                    if let Some(ref audio_data) = self.current_audio_data {
                        if old_state.status == PlaybackStatus::Playing
                            || old_state.status == PlaybackStatus::Paused
                        {
                            if let Some(ref track) = old_state.current_track {
                                if engine
                                    .play_decoded(audio_data.clone(), track, old_state.quality)
                                    .is_ok()
                                {
                                    // Seek approximation: skip ahead by consuming samples
                                    // This isn't perfect but keeps playback going
                                    debug!(
                                        position = old_state.position_secs,
                                        "Resumed track after fork"
                                    );
                                }
                            }
                        }
                    }

                    // Restore shared state (queue, shuffle, repeat, etc.)
                    {
                        let new_state_arc = engine.state();
                        let mut new_state = new_state_arc.lock().unwrap();
                        new_state.queue = old_state.queue;
                        new_state.queue_index = old_state.queue_index;
                        new_state.shuffle = old_state.shuffle;
                        new_state.repeat = old_state.repeat;
                        new_state.current_track = old_state.current_track;
                        new_state.duration_secs = old_state.duration_secs;
                        new_state.position_secs = old_state.position_secs;
                    }

                    self.player_state = engine.state();
                    self.engine = Some(engine);
                }
                Err(e) => {
                    debug!("Failed to reinit audio after fork: {e}");
                }
            }
        }
    }

    fn process_async_results(&mut self) {
        while let Ok(result) = self.async_rx.try_recv() {
            match result {
                AsyncResult::LoginSuccess(name) => {
                    self.login_loading = false;
                    self.screen = Screen::Main;
                    self.input_mode = InputMode::Normal;
                    self.status_msg = Some(format!("Connected as {name}"));
                    self.start_fetch_master_key();
                }
                AsyncResult::LoginError(err) => {
                    self.login_loading = false;
                    self.screen = Screen::Login;
                    self.input_mode = InputMode::Typing;
                    self.login_error = Some(err);
                }
                AsyncResult::MasterKeyReady(key) => {
                    self.master_key = Some(key);
                    self.status_msg = Some("Ready to play".into());
                    match PlayerEngine::new(key) {
                        Ok(engine) => {
                            self.player_state = engine.state();
                            self.engine = Some(engine);
                        }
                        Err(e) => {
                            self.status_msg = Some(format!("Audio init error: {e}"));
                        }
                    }
                    self.start_load_favorites();
                }
                AsyncResult::MasterKeyError(err) => {
                    self.status_msg = Some(format!("Key error: {err}"));
                }
                AsyncResult::SearchResults(tracks) => {
                    self.search_loading = false;
                    self.status_msg = Some(format!("{} results", tracks.len()));
                    self.search_results = tracks;
                    self.search_selected = 0;
                }
                AsyncResult::SearchError(err) => {
                    self.search_loading = false;
                    self.status_msg = Some(format!("Search error: {err}"));
                }
                AsyncResult::FavoritesLoaded(tracks) => {
                    self.favorites_loading = false;
                    self.status_msg = Some(format!("{} favorites loaded", tracks.len()));
                    self.favorites = tracks;
                    self.favorites_selected = 0;
                }
                AsyncResult::FavoritesError(err) => {
                    self.favorites_loading = false;
                    self.status_msg = Some(format!("Favorites error: {err}"));
                }
                AsyncResult::TrackReady { audio_data, track, quality } => {
                    if let Some(ref mut engine) = self.engine {
                        // Cache audio data for potential resume after fork
                        self.current_audio_data = Some(audio_data.clone());
                        match engine.play_decoded(audio_data, &track, quality) {
                            Ok(()) => {
                                self.playback_started_at = Some(Instant::now());
                                self.playback_offset_secs = 0;
                                self.status_msg = Some(format!(
                                    "{} - {} [{}]",
                                    track.title, track.artist, quality.as_api_format()
                                ));
                            }
                            Err(e) => {
                                self.status_msg = Some(format!("Playback error: {e}"));
                            }
                        }
                    }
                }
                AsyncResult::TrackFetchError(err) => {
                    self.status_msg = Some(format!("Track error: {err}"));
                    if let Ok(mut state) = self.player_state.lock() {
                        state.status = PlaybackStatus::Stopped;
                    }
                }
            }
        }
    }

    // --- Async actions ---

    fn start_login(&mut self, arl: String) {
        self.login_loading = true;
        self.login_error = None;
        let client = Arc::clone(&self.client);
        let tx = self.async_tx.clone();

        tokio::spawn(async move {
            let mut client = client.lock().await;
            match client.login_arl(&arl).await {
                Ok(session) => {
                    let _ = tx.send(AsyncResult::LoginSuccess(session.user_name));
                }
                Err(e) => {
                    let _ = tx.send(AsyncResult::LoginError(e.to_string()));
                }
            }
        });
    }

    fn start_fetch_master_key(&mut self) {
        self.status_msg = Some("Fetching decryption key...".into());
        let client = Arc::clone(&self.client);
        let tx = self.async_tx.clone();

        tokio::spawn(async move {
            let client = client.lock().await;
            match deezer_core::decrypt::fetch_master_key(client.http()).await {
                Ok(key) => {
                    let _ = tx.send(AsyncResult::MasterKeyReady(key));
                }
                Err(e) => {
                    let _ = tx.send(AsyncResult::MasterKeyError(e.to_string()));
                }
            }
        });
    }

    fn start_search(&mut self, query: String) {
        self.search_loading = true;
        let client = Arc::clone(&self.client);
        let tx = self.async_tx.clone();

        tokio::spawn(async move {
            let client = client.lock().await;
            match client.search(&query).await {
                Ok(results) => {
                    let _ = tx.send(AsyncResult::SearchResults(results.data));
                }
                Err(e) => {
                    let _ = tx.send(AsyncResult::SearchError(e.to_string()));
                }
            }
        });
    }

    fn start_load_favorites(&mut self) {
        self.favorites_loading = true;
        let client = Arc::clone(&self.client);
        let tx = self.async_tx.clone();

        tokio::spawn(async move {
            let client = client.lock().await;
            match client.get_favorites().await {
                Ok(tracks) => {
                    let _ = tx.send(AsyncResult::FavoritesLoaded(tracks));
                }
                Err(e) => {
                    let _ = tx.send(AsyncResult::FavoritesError(e.to_string()));
                }
            }
        });
    }

    fn start_play_track(&mut self, track: TrackData) {
        let Some(master_key) = self.master_key else {
            self.status_msg = Some("Player not ready yet".into());
            return;
        };

        // Update state to loading immediately
        if let Ok(mut state) = self.player_state.lock() {
            state.status = PlaybackStatus::Loading;
            state.current_track = Some(track.clone());
            state.duration_secs = track.duration_secs();
            state.position_secs = 0;
        }

        self.status_msg = Some(format!("Loading {} - {}...", track.title, track.artist));

        let client = Arc::clone(&self.client);
        let tx = self.async_tx.clone();
        let quality = self.config.quality;

        // Fetch + decrypt in background, send audio data back to main thread
        tokio::spawn(async move {
            let client = client.lock().await;

            // Ensure we have a TRACK_TOKEN (search results don't include it)
            let track = match client.ensure_track_token(&track).await {
                Ok(t) => t,
                Err(e) => {
                    let _ = tx.send(AsyncResult::TrackFetchError(e.to_string()));
                    return;
                }
            };

            match deezer_core::player::stream::fetch_track(&client, &track, quality, &master_key)
                .await
            {
                Ok((audio_data, actual_quality)) => {
                    let _ = tx.send(AsyncResult::TrackReady {
                        audio_data,
                        track,
                        quality: actual_quality,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AsyncResult::TrackFetchError(e.to_string()));
                }
            }
        });
    }

    fn play_selected_track(&mut self) {
        let track = match self.active_tab {
            ActiveTab::Search => self.search_results.get(self.search_selected).cloned(),
            ActiveTab::Favorites => self.favorites.get(self.favorites_selected).cloned(),
            _ => None,
        };

        if let Some(track) = track {
            // Set queue from current list
            let (queue, idx) = match self.active_tab {
                ActiveTab::Search => (self.search_results.clone(), self.search_selected),
                ActiveTab::Favorites => (self.favorites.clone(), self.favorites_selected),
                _ => (vec![track.clone()], 0),
            };

            if let Ok(mut state) = self.player_state.lock() {
                state.queue = queue;
                state.queue_index = idx;
            }

            self.start_play_track(track);
        }
    }

    fn play_next(&mut self) {
        let next_track = {
            let mut state = self.player_state.lock().unwrap();
            if state.queue.is_empty() {
                return;
            }

            let next_idx = if state.shuffle {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                Instant::now().hash(&mut hasher);
                hasher.finish() as usize % state.queue.len()
            } else {
                let next = state.queue_index + 1;
                if next >= state.queue.len() {
                    match state.repeat {
                        RepeatMode::Queue => 0,
                        _ => return,
                    }
                } else {
                    next
                }
            };

            state.queue_index = next_idx;
            state.queue.get(next_idx).cloned()
        };

        if let Some(track) = next_track {
            self.start_play_track(track);
        }
    }

    fn play_prev(&mut self) {
        let prev_track = {
            let mut state = self.player_state.lock().unwrap();
            if state.queue.is_empty() {
                return;
            }

            let prev_idx = if state.queue_index == 0 {
                match state.repeat {
                    RepeatMode::Queue => state.queue.len() - 1,
                    _ => 0,
                }
            } else {
                state.queue_index - 1
            };

            state.queue_index = prev_idx;
            state.queue.get(prev_idx).cloned()
        };

        if let Some(track) = prev_track {
            self.start_play_track(track);
        }
    }

    // --- Key handling ---

    fn handle_key(&mut self, key: KeyEvent) {
        debug!(?key, "key event received");

        // Ctrl+C always quits
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.running = false;
            return;
        }

        // Ctrl+Z: go to background mode (music continues)
        #[cfg(unix)]
        if matches!(key.code, KeyCode::Char('z') | KeyCode::Char('Z'))
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.ui_mode = UiMode::Background;
            return;
        }

        match self.screen {
            Screen::Login => self.handle_login_key(key),
            Screen::Main => self.handle_main_key(key),
        }
    }

    fn handle_login_key(&mut self, key: KeyEvent) {
        if self.login_loading {
            return;
        }

        match key.code {
            KeyCode::Esc => {
                self.running = false;
            }
            KeyCode::Char(c) => {
                self.login_input.insert(self.login_cursor, c);
                self.login_cursor += 1;
            }
            KeyCode::Backspace => {
                if self.login_cursor > 0 {
                    self.login_cursor -= 1;
                    self.login_input.remove(self.login_cursor);
                }
            }
            KeyCode::Left => {
                self.login_cursor = self.login_cursor.saturating_sub(1);
            }
            KeyCode::Right => {
                if self.login_cursor < self.login_input.len() {
                    self.login_cursor += 1;
                }
            }
            KeyCode::Enter => {
                if !self.login_input.is_empty() {
                    let arl = self.login_input.clone();
                    self.config.arl = Some(arl.clone());
                    let _ = self.config.save();
                    self.start_login(arl);
                }
            }
            _ => {}
        }
    }

    fn handle_main_key(&mut self, key: KeyEvent) {
        // Typing mode for search input
        if self.input_mode == InputMode::Typing {
            match key.code {
                KeyCode::Esc => {
                    self.input_mode = InputMode::Normal;
                }
                KeyCode::Enter => {
                    if !self.search_input.is_empty() {
                        let query = self.search_input.clone();
                        self.start_search(query);
                    }
                    self.input_mode = InputMode::Normal;
                }
                KeyCode::Char(c) => {
                    self.search_input.push(c);
                }
                KeyCode::Backspace => {
                    self.search_input.pop();
                }
                _ => {}
            }
            return;
        }

        // Normal mode
        match key.code {
            KeyCode::Char('q') => {
                self.running = false;
            }

            // Tab navigation
            KeyCode::Tab => {
                self.active_tab = match self.active_tab {
                    ActiveTab::Search => ActiveTab::Favorites,
                    ActiveTab::Favorites => ActiveTab::Radio,
                    ActiveTab::Radio => ActiveTab::Search,
                };
            }
            KeyCode::BackTab => {
                self.active_tab = match self.active_tab {
                    ActiveTab::Search => ActiveTab::Radio,
                    ActiveTab::Favorites => ActiveTab::Search,
                    ActiveTab::Radio => ActiveTab::Favorites,
                };
            }

            // Enter search typing mode
            KeyCode::Char('/') => {
                if self.active_tab == ActiveTab::Search {
                    self.input_mode = InputMode::Typing;
                    self.search_input.clear();
                }
            }

            // List navigation
            KeyCode::Up | KeyCode::Char('k') => match self.active_tab {
                ActiveTab::Search => {
                    self.search_selected = self.search_selected.saturating_sub(1);
                }
                ActiveTab::Favorites => {
                    self.favorites_selected = self.favorites_selected.saturating_sub(1);
                }
                _ => {}
            },
            KeyCode::Down | KeyCode::Char('j') => match self.active_tab {
                ActiveTab::Search => {
                    if !self.search_results.is_empty() {
                        self.search_selected =
                            (self.search_selected + 1).min(self.search_results.len() - 1);
                    }
                }
                ActiveTab::Favorites => {
                    if !self.favorites.is_empty() {
                        self.favorites_selected =
                            (self.favorites_selected + 1).min(self.favorites.len() - 1);
                    }
                }
                _ => {}
            },

            // Play selected track
            KeyCode::Enter => {
                self.play_selected_track();
            }

            // Player controls
            KeyCode::Char('p') | KeyCode::Char(' ') => {
                if let Some(ref engine) = self.engine {
                    engine.toggle_pause();
                    let status = self.player_state.lock().unwrap().status;
                    match status {
                        PlaybackStatus::Playing => {
                            self.playback_started_at = Some(Instant::now());
                        }
                        PlaybackStatus::Paused => {
                            if let Some(started) = self.playback_started_at.take() {
                                self.playback_offset_secs += started.elapsed().as_secs();
                            }
                        }
                        _ => {}
                    }
                }
            }
            KeyCode::Char('n') => self.play_next(),
            KeyCode::Char('b') => self.play_prev(),
            KeyCode::Char('s') => {
                let mut state = self.player_state.lock().unwrap();
                state.shuffle = !state.shuffle;
            }
            KeyCode::Char('r') => {
                let mut state = self.player_state.lock().unwrap();
                state.repeat = match state.repeat {
                    RepeatMode::Off => RepeatMode::Queue,
                    RepeatMode::Queue => RepeatMode::Track,
                    RepeatMode::Track => RepeatMode::Off,
                };
            }

            // Volume
            KeyCode::Char('+') | KeyCode::Char('=') => {
                if let Some(ref engine) = self.engine {
                    let new_vol = (engine.volume() + 0.05).min(1.0);
                    engine.set_volume(new_vol);
                }
                self.config.volume = (self.config.volume + 0.05).min(1.0);
            }
            KeyCode::Char('-') => {
                if let Some(ref engine) = self.engine {
                    let new_vol = (engine.volume() - 0.05).max(0.0);
                    engine.set_volume(new_vol);
                }
                self.config.volume = (self.config.volume - 0.05).max(0.0);
            }

            _ => {}
        }
    }

    fn on_tick(&mut self) {
        // Update playback position
        if let Ok(mut state) = self.player_state.lock() {
            if state.status == PlaybackStatus::Playing {
                if let Some(started) = self.playback_started_at {
                    state.position_secs =
                        self.playback_offset_secs + started.elapsed().as_secs();
                    if state.duration_secs > 0 && state.position_secs >= state.duration_secs {
                        state.position_secs = state.duration_secs;
                    }
                }
            }
        }

        // Auto-advance when track finishes
        if let Some(ref engine) = self.engine {
            if engine.is_finished() {
                let status = self.player_state.lock().unwrap().status;
                if status == PlaybackStatus::Playing {
                    let repeat = self.player_state.lock().unwrap().repeat;
                    if repeat == RepeatMode::Track {
                        let track = self.player_state.lock().unwrap().current_track.clone();
                        if let Some(track) = track {
                            self.start_play_track(track);
                        }
                    } else {
                        self.play_next();
                    }
                }
            }
        }
    }
}
