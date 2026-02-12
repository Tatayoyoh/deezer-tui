use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        io::stdout().execute(EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        // If we already have an ARL, auto-login
        if let Some(arl) = self.config.arl.clone() {
            self.status_msg = Some("Connecting...".into());
            self.start_login(arl);
        }

        // Main loop
        while self.running {
            terminal.draw(|frame| {
                ui::draw(frame, self);
            })?;

            match event::poll(TICK_RATE)? {
                AppEvent::Key(key) => self.handle_key(key),
                AppEvent::Tick => self.on_tick(),
            }

            // Process async results
            self.process_async_results();
        }

        // Restore terminal
        disable_raw_mode()?;
        io::stdout().execute(LeaveAlternateScreen)?;

        Ok(())
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
        // Ctrl+C always quits
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.running = false;
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
