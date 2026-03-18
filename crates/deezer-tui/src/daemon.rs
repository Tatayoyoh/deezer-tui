use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::io::BufReader;
use tokio::net::UnixListener;
use tracing::{debug, error, info, warn};

use deezer_core::api::models::{
    AlbumDetail, AudioQuality, DisplayItem, PlaylistData, PlaylistDetail, TrackData,
};
use deezer_core::api::DeezerClient;
use deezer_core::player::engine::PlayerEngine;
use deezer_core::player::state::{PlaybackStatus, PlayerState, RepeatMode};
use deezer_core::Config;

use crate::protocol::{
    read_line, socket_path, ActiveTab, Command, DaemonSnapshot, FavoritesCategory, Screen,
    SearchCategory, ServerMessage,
};

const TICK_RATE: Duration = Duration::from_millis(250);

/// Async results from background tasks.
enum AsyncResult {
    LoginSuccess(String),
    LoginError(String),
    MasterKeyReady([u8; 16]),
    MasterKeyError(String),
    SearchResults(Vec<TrackData>),
    SearchError(String),
    SearchDisplayResults(Vec<DisplayItem>),
    FavoritesLoaded(Vec<TrackData>),
    FavoritesError(String),
    FavoritesDisplayLoaded(Vec<DisplayItem>),
    TrackReady {
        audio_data: Vec<u8>,
        track: TrackData,
        quality: AudioQuality,
    },
    TrackFetchError(String),
    FavoriteAdded(String),
    FavoriteRemoved(String),
    FavoriteError(String),
    PlaylistsReady(Vec<PlaylistData>),
    PlaylistsError(String),
    AddedToPlaylist(String),
    AddToPlaylistError(String),
    DislikeOk,
    DislikeError(String),
    MixReady(Vec<TrackData>),
    MixError(String),
    AlbumDetailReady(AlbumDetail),
    AlbumDetailError(String),
    PlaylistDetailReady(PlaylistDetail),
    PlaylistDetailError(String),
}

pub struct Daemon {
    config: Config,
    screen: Screen,
    active_tab: ActiveTab,
    status_msg: Option<String>,

    // Login state
    login_error: Option<String>,
    login_loading: bool,
    user_name: Option<String>,

    // Search state
    search_results: Vec<TrackData>,
    search_selected: usize,
    search_loading: bool,
    search_category: SearchCategory,
    search_display: Vec<DisplayItem>,
    last_search_query: String,

    // Favorites state
    favorites: Vec<TrackData>,
    favorites_selected: usize,
    favorites_loading: bool,
    favorites_category: FavoritesCategory,
    favorites_display: Vec<DisplayItem>,

    // Playlists (for popup menu playlist picker)
    playlists: Vec<PlaylistData>,

    // Album detail
    album_detail: Option<AlbumDetail>,
    album_detail_selected: usize,
    album_detail_loading: bool,

    // Playlist detail
    playlist_detail: Option<PlaylistDetail>,
    playlist_detail_selected: usize,
    playlist_detail_loading: bool,

    // Player
    player_state: Arc<Mutex<PlayerState>>,
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

impl Daemon {
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
            config,
            screen,
            active_tab: ActiveTab::Search,
            status_msg: None,

            login_error: None,
            login_loading: false,
            user_name: None,

            search_results: Vec::new(),
            search_selected: 0,
            search_loading: false,
            search_category: SearchCategory::default(),
            search_display: Vec::new(),
            last_search_query: String::new(),

            favorites: Vec::new(),
            favorites_selected: 0,
            favorites_loading: false,
            favorites_category: FavoritesCategory::default(),
            favorites_display: Vec::new(),

            playlists: Vec::new(),

            album_detail: None,
            album_detail_selected: 0,
            album_detail_loading: false,

            playlist_detail: None,
            playlist_detail_selected: 0,
            playlist_detail_loading: false,

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

    /// Run the daemon: listen for client connections and process commands.
    pub async fn run(&mut self) -> Result<()> {
        let sock_path = socket_path();

        // Clean up stale socket
        if sock_path.exists() {
            let _ = std::fs::remove_file(&sock_path);
        }
        if let Some(parent) = sock_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&sock_path)?;
        info!(?sock_path, "Daemon listening");

        // If we have an ARL, auto-login
        if let Some(arl) = self.config.arl.clone() {
            self.status_msg = Some("Connecting...".into());
            self.start_login(arl);
        }

        // Main daemon loop
        let mut client_reader: Option<BufReader<tokio::net::unix::OwnedReadHalf>> = None;
        let mut client_writer: Option<tokio::net::unix::OwnedWriteHalf> = None;

        loop {
            // Build tick interval
            let tick = tokio::time::sleep(TICK_RATE);

            tokio::select! {
                // Accept new client connection
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _addr)) => {
                            info!("Client connected");
                            let (read_half, write_half) = stream.into_split();
                            client_reader = Some(BufReader::new(read_half));
                            client_writer = Some(write_half);
                            // Send initial snapshot
                            if let Some(ref mut writer) = client_writer {
                                let snap = self.snapshot();
                                let msg = ServerMessage::Snapshot(snap);
                                let mut tmp_stream = writer as &mut tokio::net::unix::OwnedWriteHalf;
                                if let Err(e) = send_line_writer(&mut tmp_stream, &msg).await {
                                    warn!("Failed to send initial snapshot: {e}");
                                    client_reader = None;
                                    client_writer = None;
                                }
                            }
                        }
                        Err(e) => {
                            error!("Accept error: {e}");
                        }
                    }
                }

                // Read command from connected client
                cmd = async {
                    if let Some(ref mut reader) = client_reader {
                        read_line::<Command, _>(reader).await
                    } else {
                        // No client connected — sleep forever (other branches will fire)
                        std::future::pending().await
                    }
                } => {
                    match cmd {
                        Ok(Some(command)) => {
                            debug!(?command, "Received command");
                            let should_shutdown = matches!(command, Command::Shutdown);
                            self.handle_command(command);

                            // Send snapshot back
                            if let Some(ref mut writer) = client_writer {
                                let snap = self.snapshot();
                                let msg = ServerMessage::Snapshot(snap);
                                if let Err(e) = send_line_writer(writer, &msg).await {
                                    warn!("Failed to send snapshot: {e}");
                                    client_reader = None;
                                    client_writer = None;
                                }
                            }

                            if should_shutdown {
                                info!("Shutdown requested, exiting daemon");
                                break;
                            }
                        }
                        Ok(None) => {
                            // Client disconnected
                            info!("Client disconnected");
                            client_reader = None;
                            client_writer = None;
                        }
                        Err(e) => {
                            warn!("Error reading from client: {e}");
                            client_reader = None;
                            client_writer = None;
                        }
                    }
                }

                // Tick: update position, auto-advance, process async results
                _ = tick => {
                    self.process_async_results();
                    self.on_tick();

                    // Send periodic snapshot to connected client
                    if let Some(ref mut writer) = client_writer {
                        let snap = self.snapshot();
                        let msg = ServerMessage::Snapshot(snap);
                        if let Err(e) = send_line_writer(writer, &msg).await {
                            warn!("Failed to send tick snapshot: {e}");
                            client_reader = None;
                            client_writer = None;
                        }
                    }
                }
            }
        }

        // Cleanup
        let _ = std::fs::remove_file(&sock_path);
        info!("Daemon stopped");
        Ok(())
    }

    fn handle_command(&mut self, cmd: Command) {
        match cmd {
            Command::GetSnapshot => {} // Snapshot is sent after every command anyway
            Command::Login { arl } => {
                self.config.arl = Some(arl.clone());
                let _ = self.config.save();
                self.start_login(arl);
            }
            Command::Search { query } => {
                self.last_search_query = query.clone();
                self.start_search(query);
            }
            Command::PlayFromSearch { index } => {
                // Try to get a playable track from display items
                if let Some(item) = self.search_display.get(index) {
                    if let Some(track) = &item.track {
                        // Build queue from all playable tracks in search results
                        let playable: Vec<TrackData> = self
                            .search_display
                            .iter()
                            .filter_map(|d| d.track.clone())
                            .collect();
                        let queue_idx = playable
                            .iter()
                            .position(|t| t.track_id == track.track_id)
                            .unwrap_or(0);
                        if let Ok(mut state) = self.player_state.lock() {
                            state.queue = playable;
                            state.queue_index = queue_idx;
                        }
                        self.start_play_track(track.clone());
                    }
                }
            }
            Command::PlayFromFavorites { index } => {
                if let Some(item) = self.favorites_display.get(index) {
                    if let Some(track) = &item.track {
                        let playable: Vec<TrackData> = self
                            .favorites_display
                            .iter()
                            .filter_map(|d| d.track.clone())
                            .collect();
                        let queue_idx = playable
                            .iter()
                            .position(|t| t.track_id == track.track_id)
                            .unwrap_or(0);
                        if let Ok(mut state) = self.player_state.lock() {
                            state.queue = playable;
                            state.queue_index = queue_idx;
                        }
                        self.start_play_track(track.clone());
                    }
                }
            }
            Command::TogglePause => {
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
            Command::NextTrack => self.play_next(),
            Command::PrevTrack => self.play_prev(),
            Command::SetVolume { volume } => {
                if let Some(ref engine) = self.engine {
                    engine.set_volume(volume);
                }
                self.config.volume = volume.clamp(0.0, 1.0);
            }
            Command::ToggleShuffle => {
                let mut state = self.player_state.lock().unwrap();
                state.shuffle = !state.shuffle;
            }
            Command::CycleRepeat => {
                let mut state = self.player_state.lock().unwrap();
                state.repeat = match state.repeat {
                    RepeatMode::Off => RepeatMode::Queue,
                    RepeatMode::Queue => RepeatMode::Track,
                    RepeatMode::Track => RepeatMode::Off,
                };
            }
            Command::LoadFavorites => {
                self.start_load_favorites();
            }
            Command::SelectUp => match self.active_tab {
                ActiveTab::Search => {
                    self.search_selected = self.search_selected.saturating_sub(1);
                }
                ActiveTab::Favorites => {
                    self.favorites_selected = self.favorites_selected.saturating_sub(1);
                }
                _ => {}
            },
            Command::SelectDown => match self.active_tab {
                ActiveTab::Search => {
                    if !self.search_display.is_empty() {
                        self.search_selected =
                            (self.search_selected + 1).min(self.search_display.len() - 1);
                    }
                }
                ActiveTab::Favorites => {
                    if !self.favorites_display.is_empty() {
                        self.favorites_selected =
                            (self.favorites_selected + 1).min(self.favorites_display.len() - 1);
                    }
                }
                _ => {}
            },
            Command::NextTab => {
                self.active_tab = match self.active_tab {
                    ActiveTab::Search => ActiveTab::Favorites,
                    ActiveTab::Favorites => ActiveTab::Radio,
                    ActiveTab::Radio => ActiveTab::Downloads,
                    ActiveTab::Downloads => ActiveTab::Search,
                };
            }
            Command::PrevTab => {
                self.active_tab = match self.active_tab {
                    ActiveTab::Search => ActiveTab::Downloads,
                    ActiveTab::Favorites => ActiveTab::Search,
                    ActiveTab::Radio => ActiveTab::Favorites,
                    ActiveTab::Downloads => ActiveTab::Radio,
                };
            }
            Command::NextCategory => match self.active_tab {
                ActiveTab::Search => {
                    self.search_category = self.search_category.next();
                    self.search_selected = 0;
                    if !self.last_search_query.is_empty() {
                        self.start_search(self.last_search_query.clone());
                    }
                }
                ActiveTab::Favorites => {
                    self.favorites_category = self.favorites_category.next();
                    self.favorites_selected = 0;
                    self.start_load_favorites_category();
                }
                _ => {}
            },
            Command::PrevCategory => match self.active_tab {
                ActiveTab::Search => {
                    self.search_category = self.search_category.prev();
                    self.search_selected = 0;
                    if !self.last_search_query.is_empty() {
                        self.start_search(self.last_search_query.clone());
                    }
                }
                ActiveTab::Favorites => {
                    self.favorites_category = self.favorites_category.prev();
                    self.favorites_selected = 0;
                    self.start_load_favorites_category();
                }
                _ => {}
            },
            Command::ShuffleFavorites => {
                if !self.favorites.is_empty() {
                    // Set queue from favorites with shuffle enabled
                    if let Ok(mut state) = self.player_state.lock() {
                        state.queue = self.favorites.clone();
                        state.shuffle = true;
                    }
                    // Pick a random track to start
                    use std::collections::hash_map::DefaultHasher;
                    use std::hash::{Hash, Hasher};
                    let mut hasher = DefaultHasher::new();
                    Instant::now().hash(&mut hasher);
                    let idx = hasher.finish() as usize % self.favorites.len();
                    if let Ok(mut state) = self.player_state.lock() {
                        state.queue_index = idx;
                    }
                    if let Some(track) = self.favorites.get(idx).cloned() {
                        self.start_play_track(track);
                    }
                }
            }
            Command::AddFavorite { track_id } => {
                self.start_add_favorite(track_id);
            }
            Command::RemoveFavorite { track_id } => {
                self.start_remove_favorite(track_id);
            }
            Command::RequestPlaylists => {
                self.start_load_playlists();
            }
            Command::AddToPlaylist {
                playlist_id,
                track_id,
            } => {
                self.start_add_to_playlist(playlist_id, track_id);
            }
            Command::DislikeTrack { track_id } => {
                self.start_dislike_track(track_id);
            }
            Command::PlayNext { track } => {
                if let Ok(mut state) = self.player_state.lock() {
                    let insert_idx = state.queue_index + 1;
                    if insert_idx <= state.queue.len() {
                        state.queue.insert(insert_idx, track.clone());
                    } else {
                        state.queue.push(track.clone());
                    }
                }
                self.status_msg = Some(format!("\"{}\" will play next", track.title));
            }
            Command::AddToQueue { track } => {
                if let Ok(mut state) = self.player_state.lock() {
                    state.queue.push(track.clone());
                }
                self.status_msg = Some(format!("\"{}\" added to queue", track.title));
            }
            Command::RemoveFromQueue { index } => {
                if let Ok(mut state) = self.player_state.lock() {
                    if index < state.queue.len() && index != state.queue_index {
                        state.queue.remove(index);
                        // Adjust queue_index if the removed track was before the current one
                        if index < state.queue_index {
                            state.queue_index = state.queue_index.saturating_sub(1);
                        }
                    }
                }
            }
            Command::StartMix { track_id } => {
                self.start_mix(track_id);
            }
            Command::GetAlbumDetail { album_id } => {
                self.start_load_album_detail(album_id);
            }
            Command::PlayFromAlbum { index } => {
                if let Some(ref detail) = self.album_detail {
                    if let Some(track) = detail.tracks.get(index).cloned() {
                        // Set queue from album tracks
                        if let Ok(mut state) = self.player_state.lock() {
                            state.queue = detail.tracks.clone();
                            state.queue_index = index;
                        }
                        self.start_play_track(track);
                    }
                }
            }
            Command::GetPlaylistDetail { playlist_id } => {
                self.start_load_playlist_detail(playlist_id);
            }
            Command::PlayFromPlaylist { index } => {
                if let Some(ref detail) = self.playlist_detail {
                    if let Some(track) = detail.tracks.get(index).cloned() {
                        if let Ok(mut state) = self.player_state.lock() {
                            state.queue = detail.tracks.clone();
                            state.queue_index = index;
                        }
                        self.start_play_track(track);
                    }
                }
            }
            Command::Shutdown => {
                // Handled in the main loop
            }
        }
    }

    fn snapshot(&self) -> DaemonSnapshot {
        let state = self.player_state.lock().unwrap();
        DaemonSnapshot {
            screen: self.screen,
            active_tab: self.active_tab,
            status: state.status,
            current_track: state.current_track.clone(),
            quality: state.quality,
            position_secs: state.position_secs,
            duration_secs: state.duration_secs,
            volume: state.volume,
            shuffle: state.shuffle,
            repeat: state.repeat,
            queue: state.queue.clone(),
            queue_index: state.queue_index,
            search_results: self.search_results.clone(),
            search_selected: self.search_selected,
            search_loading: self.search_loading,
            search_category: self.search_category,
            search_display: self.search_display.clone(),
            favorites: self.favorites.clone(),
            favorites_selected: self.favorites_selected,
            favorites_loading: self.favorites_loading,
            favorites_category: self.favorites_category,
            favorites_display: self.favorites_display.clone(),
            playlists: self.playlists.clone(),
            album_detail: self.album_detail.clone(),
            album_detail_selected: self.album_detail_selected,
            album_detail_loading: self.album_detail_loading,
            playlist_detail: self.playlist_detail.clone(),
            playlist_detail_selected: self.playlist_detail_selected,
            playlist_detail_loading: self.playlist_detail_loading,
            status_msg: self.status_msg.clone(),
            login_error: self.login_error.clone(),
            login_loading: self.login_loading,
            user_name: self.user_name.clone(),
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
        let category = self.search_category;
        let api_key = category.api_key().to_string();

        if category == SearchCategory::Track {
            // Track search: populate both search_results (for playback) and search_display
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
        } else {
            // Non-track search: only populate search_display
            tokio::spawn(async move {
                let client = client.lock().await;
                match client.search_category(&query, &api_key).await {
                    Ok(items) => {
                        let _ = tx.send(AsyncResult::SearchDisplayResults(items));
                    }
                    Err(e) => {
                        let _ = tx.send(AsyncResult::SearchError(e.to_string()));
                    }
                }
            });
        }
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

    fn start_load_favorites_category(&mut self) {
        self.favorites_loading = true;
        let client = Arc::clone(&self.client);
        let tx = self.async_tx.clone();
        let category = self.favorites_category;

        match category {
            FavoritesCategory::Tracks => {
                // Load favorite tracks (same as default)
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
            FavoritesCategory::RecentlyPlayed => {
                tokio::spawn(async move {
                    let client = client.lock().await;
                    match client.get_listening_history().await {
                        Ok(tracks) => {
                            let _ = tx.send(AsyncResult::FavoritesLoaded(tracks));
                        }
                        Err(e) => {
                            let _ = tx.send(AsyncResult::FavoritesError(e.to_string()));
                        }
                    }
                });
            }
            FavoritesCategory::Artists => {
                tokio::spawn(async move {
                    let client = client.lock().await;
                    debug!("Loading favorite artists...");
                    match client.get_favorite_artists().await {
                        Ok(items) => {
                            debug!("Favorite artists loaded: {} items", items.len());
                            let _ = tx.send(AsyncResult::FavoritesDisplayLoaded(items));
                        }
                        Err(e) => {
                            debug!("Favorite artists error: {e}");
                            let _ = tx.send(AsyncResult::FavoritesError(e.to_string()));
                        }
                    }
                });
            }
            FavoritesCategory::Albums => {
                tokio::spawn(async move {
                    let client = client.lock().await;
                    match client.get_favorite_albums().await {
                        Ok(items) => {
                            let _ = tx.send(AsyncResult::FavoritesDisplayLoaded(items));
                        }
                        Err(e) => {
                            let _ = tx.send(AsyncResult::FavoritesError(e.to_string()));
                        }
                    }
                });
            }
            FavoritesCategory::Playlists => {
                tokio::spawn(async move {
                    let client = client.lock().await;
                    match client.get_playlists().await {
                        Ok(items) => {
                            let _ = tx.send(AsyncResult::FavoritesDisplayLoaded(items));
                        }
                        Err(e) => {
                            let _ = tx.send(AsyncResult::FavoritesError(e.to_string()));
                        }
                    }
                });
            }
            FavoritesCategory::Following => {
                tokio::spawn(async move {
                    let client = client.lock().await;
                    match client.get_following().await {
                        Ok(items) => {
                            let _ = tx.send(AsyncResult::FavoritesDisplayLoaded(items));
                        }
                        Err(e) => {
                            let _ = tx.send(AsyncResult::FavoritesError(e.to_string()));
                        }
                    }
                });
            }
        }
    }

    fn start_add_favorite(&mut self, track_id: String) {
        let client = Arc::clone(&self.client);
        let tx = self.async_tx.clone();
        tokio::spawn(async move {
            let client = client.lock().await;
            match client.add_favorite(&track_id).await {
                Ok(()) => {
                    let _ = tx.send(AsyncResult::FavoriteAdded(track_id));
                }
                Err(e) => {
                    let _ = tx.send(AsyncResult::FavoriteError(e.to_string()));
                }
            }
        });
    }

    fn start_remove_favorite(&mut self, track_id: String) {
        let client = Arc::clone(&self.client);
        let tx = self.async_tx.clone();
        tokio::spawn(async move {
            let client = client.lock().await;
            match client.remove_favorite(&track_id).await {
                Ok(()) => {
                    let _ = tx.send(AsyncResult::FavoriteRemoved(track_id));
                }
                Err(e) => {
                    let _ = tx.send(AsyncResult::FavoriteError(e.to_string()));
                }
            }
        });
    }

    fn start_load_playlists(&mut self) {
        let client = Arc::clone(&self.client);
        let tx = self.async_tx.clone();
        tokio::spawn(async move {
            let client = client.lock().await;
            match client.get_user_playlists_raw().await {
                Ok(playlists) => {
                    let _ = tx.send(AsyncResult::PlaylistsReady(playlists));
                }
                Err(e) => {
                    let _ = tx.send(AsyncResult::PlaylistsError(e.to_string()));
                }
            }
        });
    }

    fn start_add_to_playlist(&mut self, playlist_id: String, track_id: String) {
        let client = Arc::clone(&self.client);
        let tx = self.async_tx.clone();
        tokio::spawn(async move {
            let client = client.lock().await;
            match client
                .add_to_playlist(&playlist_id, &[track_id.as_str()])
                .await
            {
                Ok(()) => {
                    let _ = tx.send(AsyncResult::AddedToPlaylist(playlist_id));
                }
                Err(e) => {
                    let _ = tx.send(AsyncResult::AddToPlaylistError(e.to_string()));
                }
            }
        });
    }

    fn start_dislike_track(&mut self, track_id: String) {
        let client = Arc::clone(&self.client);
        let tx = self.async_tx.clone();
        tokio::spawn(async move {
            let client = client.lock().await;
            match client.dislike_track(&track_id).await {
                Ok(()) => {
                    let _ = tx.send(AsyncResult::DislikeOk);
                }
                Err(e) => {
                    let _ = tx.send(AsyncResult::DislikeError(e.to_string()));
                }
            }
        });
    }

    fn start_mix(&mut self, track_id: String) {
        self.status_msg = Some("Loading mix...".into());
        let client = Arc::clone(&self.client);
        let tx = self.async_tx.clone();
        tokio::spawn(async move {
            let client = client.lock().await;
            match client.get_smart_radio(&track_id).await {
                Ok(tracks) => {
                    let _ = tx.send(AsyncResult::MixReady(tracks));
                }
                Err(e) => {
                    let _ = tx.send(AsyncResult::MixError(e.to_string()));
                }
            }
        });
    }

    fn start_load_album_detail(&mut self, album_id: String) {
        self.album_detail_loading = true;
        self.album_detail = None;
        self.album_detail_selected = 0;
        self.status_msg = Some("Loading album...".into());
        let client = Arc::clone(&self.client);
        let tx = self.async_tx.clone();
        tokio::spawn(async move {
            let client = client.lock().await;
            match client.get_album_detail(&album_id).await {
                Ok(detail) => {
                    let _ = tx.send(AsyncResult::AlbumDetailReady(detail));
                }
                Err(e) => {
                    let _ = tx.send(AsyncResult::AlbumDetailError(e.to_string()));
                }
            }
        });
    }

    fn start_load_playlist_detail(&mut self, playlist_id: String) {
        self.playlist_detail_loading = true;
        self.playlist_detail = None;
        self.playlist_detail_selected = 0;
        self.status_msg = Some("Loading playlist...".into());
        let client = Arc::clone(&self.client);
        let tx = self.async_tx.clone();
        tokio::spawn(async move {
            let client = client.lock().await;
            match client.get_playlist_detail(&playlist_id).await {
                Ok(detail) => {
                    let _ = tx.send(AsyncResult::PlaylistDetailReady(detail));
                }
                Err(e) => {
                    let _ = tx.send(AsyncResult::PlaylistDetailError(e.to_string()));
                }
            }
        });
    }

    fn start_play_track(&mut self, track: TrackData) {
        let Some(master_key) = self.master_key else {
            self.status_msg = Some("Player not ready yet".into());
            return;
        };

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

        tokio::spawn(async move {
            let client = client.lock().await;

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

    fn process_async_results(&mut self) {
        while let Ok(result) = self.async_rx.try_recv() {
            match result {
                AsyncResult::LoginSuccess(name) => {
                    self.login_loading = false;
                    self.screen = Screen::Main;
                    self.user_name = Some(name.clone());
                    self.status_msg = Some(format!("Connected as {name}"));
                    self.start_fetch_master_key();
                }
                AsyncResult::LoginError(err) => {
                    self.login_loading = false;
                    self.screen = Screen::Login;
                    self.login_error = Some(err);
                }
                AsyncResult::MasterKeyReady(key) => {
                    self.master_key = Some(key);
                    self.status_msg = Some("Ready to play".into());
                    match PlayerEngine::new(key) {
                        Ok(engine) => {
                            engine.set_volume(self.config.volume);
                            self.player_state = engine.state();
                            self.engine = Some(engine);
                        }
                        Err(e) => {
                            self.status_msg = Some(format!("Audio init error: {e}"));
                        }
                    }
                    self.start_load_favorites_category();
                }
                AsyncResult::MasterKeyError(err) => {
                    self.status_msg = Some(format!("Key error: {err}"));
                }
                AsyncResult::SearchResults(tracks) => {
                    self.search_loading = false;
                    self.status_msg = Some(format!("{} results", tracks.len()));
                    self.search_display = tracks.iter().map(DisplayItem::from_track).collect();
                    self.search_results = tracks;
                    self.search_selected = 0;
                }
                AsyncResult::SearchDisplayResults(items) => {
                    self.search_loading = false;
                    self.status_msg = Some(format!("{} results", items.len()));
                    self.search_results.clear();
                    self.search_display = items;
                    self.search_selected = 0;
                }
                AsyncResult::SearchError(err) => {
                    self.search_loading = false;
                    self.status_msg = Some(format!("Search error: {err}"));
                }
                AsyncResult::FavoritesLoaded(tracks) => {
                    self.favorites_loading = false;
                    self.status_msg = Some(format!("{} loaded", tracks.len()));
                    self.favorites_display = tracks.iter().map(DisplayItem::from_track).collect();
                    self.favorites = tracks;
                    self.favorites_selected = 0;
                }
                AsyncResult::FavoritesDisplayLoaded(items) => {
                    self.favorites_loading = false;
                    self.status_msg = Some(format!("{} loaded", items.len()));
                    self.favorites.clear();
                    self.favorites_display = items;
                    self.favorites_selected = 0;
                }
                AsyncResult::FavoritesError(err) => {
                    self.favorites_loading = false;
                    self.favorites_display.clear();
                    self.favorites.clear();
                    self.favorites_selected = 0;
                    self.status_msg = Some(format!("Favorites error: {err}"));
                }
                AsyncResult::FavoriteAdded(_track_id) => {
                    self.status_msg = Some("Added to favorites".into());
                    // Reload favorites to reflect the change
                    self.start_load_favorites();
                }
                AsyncResult::FavoriteRemoved(_track_id) => {
                    self.status_msg = Some("Removed from favorites".into());
                    self.start_load_favorites();
                }
                AsyncResult::FavoriteError(err) => {
                    self.status_msg = Some(format!("Favorite error: {err}"));
                }
                AsyncResult::PlaylistsReady(playlists) => {
                    self.playlists = playlists;
                    self.status_msg = Some(format!("{} playlists loaded", self.playlists.len()));
                }
                AsyncResult::PlaylistsError(err) => {
                    self.status_msg = Some(format!("Playlists error: {err}"));
                }
                AsyncResult::AddedToPlaylist(_playlist_id) => {
                    self.status_msg = Some("Added to playlist".into());
                }
                AsyncResult::AddToPlaylistError(err) => {
                    self.status_msg = Some(format!("Add to playlist error: {err}"));
                }
                AsyncResult::DislikeOk => {
                    self.status_msg = Some("Track marked as disliked".into());
                }
                AsyncResult::DislikeError(err) => {
                    self.status_msg = Some(format!("Dislike error: {err}"));
                }
                AsyncResult::MixReady(tracks) => {
                    if tracks.is_empty() {
                        self.status_msg = Some("No mix tracks found".into());
                    } else {
                        self.status_msg = Some(format!("Mix: {} tracks", tracks.len()));
                        let first = tracks[0].clone();
                        if let Ok(mut state) = self.player_state.lock() {
                            state.queue = tracks;
                            state.queue_index = 0;
                        }
                        self.start_play_track(first);
                    }
                }
                AsyncResult::MixError(err) => {
                    self.status_msg = Some(format!("Mix error: {err}"));
                }
                AsyncResult::AlbumDetailReady(detail) => {
                    self.album_detail_loading = false;
                    self.status_msg =
                        Some(format!("{} — {} tracks", detail.title, detail.tracks.len()));
                    self.album_detail = Some(detail);
                    self.album_detail_selected = 0;
                }
                AsyncResult::AlbumDetailError(err) => {
                    self.album_detail_loading = false;
                    self.status_msg = Some(format!("Album error: {err}"));
                }
                AsyncResult::PlaylistDetailReady(detail) => {
                    self.playlist_detail_loading = false;
                    self.status_msg =
                        Some(format!("{} — {} tracks", detail.title, detail.tracks.len()));
                    self.playlist_detail = Some(detail);
                    self.playlist_detail_selected = 0;
                }
                AsyncResult::PlaylistDetailError(err) => {
                    self.playlist_detail_loading = false;
                    self.status_msg = Some(format!("Playlist error: {err}"));
                }
                AsyncResult::TrackReady {
                    audio_data,
                    track,
                    quality,
                } => {
                    if let Some(ref mut engine) = self.engine {
                        match engine.play_decoded(audio_data, &track, quality) {
                            Ok(()) => {
                                self.playback_started_at = Some(Instant::now());
                                self.playback_offset_secs = 0;
                                self.status_msg = Some(format!(
                                    "{} - {} [{}]",
                                    track.title,
                                    track.artist,
                                    quality.as_api_format()
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

    fn on_tick(&mut self) {
        // Update playback position
        if let Ok(mut state) = self.player_state.lock() {
            if state.status == PlaybackStatus::Playing {
                if let Some(started) = self.playback_started_at {
                    state.position_secs = self.playback_offset_secs + started.elapsed().as_secs();
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

/// Send a line-delimited JSON message over a write half.
async fn send_line_writer<T: serde::Serialize>(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    msg: &T,
) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut json = serde_json::to_string(msg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    json.push('\n');
    writer.write_all(json.as_bytes()).await?;
    writer.flush().await
}
