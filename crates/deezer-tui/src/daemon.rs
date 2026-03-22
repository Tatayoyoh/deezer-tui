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
use deezer_core::offline::OfflineIndex;
use deezer_core::player::engine::PlayerEngine;
use deezer_core::player::state::{PlaybackStatus, PlayerState, RepeatMode};
use deezer_core::Config;

use crate::i18n::t;
use crate::protocol::{
    read_line, socket_path, ActiveTab, Command, DaemonSnapshot, FavoritesCategory, OfflineCategory,
    RadioItem, Screen, SearchCategory, ServerMessage,
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
        generation: u64,
    },
    TrackFetchError {
        err: String,
        generation: u64,
    },
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
    RadiosReady(Vec<RadioItem>),
    RadiosError(String),
    RadioTracksReady(Vec<TrackData>),
    RadioTracksError(String),
    OfflineTrackSaved {
        track: TrackData,
        quality: AudioQuality,
    },
    OfflineTrackSaveError(String),
    OfflineAlbumSaved {
        album: AlbumDetail,
    },
    OfflineAlbumSaveError(String),
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

    // Radios
    radios: Vec<RadioItem>,
    radios_selected: usize,
    radios_loading: bool,

    // Offline
    offline_index: OfflineIndex,
    offline_category: OfflineCategory,
    offline_selected: usize,
    offline_loading: bool,

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

    // Network connectivity
    is_offline: bool,

    // Shared HTTP client for CDN downloads (connection reuse)
    cdn_http: deezer_core::CdnClient,

    // Async channel
    async_tx: tokio::sync::mpsc::UnboundedSender<AsyncResult>,
    async_rx: tokio::sync::mpsc::UnboundedReceiver<AsyncResult>,

    // Playback position tracking
    playback_started_at: Option<Instant>,
    playback_offset_secs: u64,

    // Generation counter to discard stale track fetch results
    track_generation: u64,
    // Count consecutive failed track fetches to avoid infinite skip loop
    consecutive_skip_count: u32,
}

impl Daemon {
    pub fn new() -> Result<Self> {
        let config = Config::load();

        // Check network connectivity
        let is_offline = std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([1, 1, 1, 1], 53)),
            Duration::from_secs(2),
        )
        .is_err();

        let screen = if is_offline {
            Screen::Main
        } else if config.arl.is_some() {
            Screen::Main
        } else {
            Screen::Login
        };

        let client = DeezerClient::new().map_err(|e| anyhow::anyhow!("{e}"))?;
        let cdn_http =
            deezer_core::player::stream::new_cdn_client().map_err(|e| anyhow::anyhow!("{e}"))?;
        let (async_tx, async_rx) = tokio::sync::mpsc::unbounded_channel();

        Ok(Self {
            config,
            screen,
            active_tab: if is_offline {
                ActiveTab::Downloads
            } else {
                ActiveTab::Search
            },
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

            offline_index: OfflineIndex::load(),
            offline_category: OfflineCategory::default(),
            offline_selected: 0,
            offline_loading: false,

            radios: Vec::new(),
            radios_selected: 0,
            radios_loading: false,

            playlists: Vec::new(),

            album_detail: None,
            album_detail_selected: 0,
            album_detail_loading: false,

            playlist_detail: None,
            playlist_detail_selected: 0,
            playlist_detail_loading: false,

            player_state: Arc::new(Mutex::new(PlayerState::default())),

            client: Arc::new(tokio::sync::Mutex::new(client)),
            cdn_http,
            engine: None,
            master_key: None,

            is_offline,

            async_tx,
            async_rx,

            playback_started_at: None,
            playback_offset_secs: 0,
            track_generation: 0,
            consecutive_skip_count: 0,
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

        if self.is_offline {
            // In offline mode, create the audio engine immediately (no master key needed for local playback)
            match PlayerEngine::new([0u8; 16]) {
                Ok(engine) => {
                    engine.set_volume(self.config.volume);
                    self.player_state = engine.state();
                    self.engine = Some(engine);
                }
                Err(e) => {
                    warn!("Failed to init audio engine in offline mode: {e}");
                }
            }
        } else if let Some(arl) = self.config.arl.clone() {
            self.status_msg = Some(t().login_connecting.into());
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
                info!(
                    index,
                    favorites_display_len = self.favorites_display.len(),
                    "PlayFromFavorites"
                );
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
                        info!(track_id = %track.track_id, title = %track.title, queue_len = playable.len(), queue_idx, "PlayFromFavorites: setting queue and playing");
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
                ActiveTab::Radio => {
                    self.radios_selected = self.radios_selected.saturating_sub(1);
                }
                ActiveTab::Downloads => {
                    self.offline_selected = self.offline_selected.saturating_sub(1);
                }
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
                ActiveTab::Radio => {
                    if !self.radios.is_empty() {
                        self.radios_selected =
                            (self.radios_selected + 1).min(self.radios.len() - 1);
                    }
                }
                ActiveTab::Downloads => {
                    let len = match self.offline_category {
                        OfflineCategory::Tracks => self.offline_index.tracks.len(),
                        OfflineCategory::Albums => self.offline_index.albums.len(),
                    };
                    if len > 0 {
                        self.offline_selected = (self.offline_selected + 1).min(len - 1);
                    }
                }
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
                ActiveTab::Downloads => {
                    self.offline_category = self.offline_category.next();
                    self.offline_selected = 0;
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
                ActiveTab::Downloads => {
                    self.offline_category = self.offline_category.prev();
                    self.offline_selected = 0;
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
                self.status_msg = Some(t().status_play_next.into());
            }
            Command::AddToQueue { track } => {
                if let Ok(mut state) = self.player_state.lock() {
                    state.queue.push(track.clone());
                }
                self.status_msg = Some(t().status_added_to_queue.into());
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
            Command::Logout => {
                // Clear ARL, stop playback, return to login screen
                self.config.arl = None;
                let _ = self.config.save();
                self.screen = Screen::Login;
                self.user_name = None;
                self.master_key = None;
                self.engine = None;
                if let Ok(mut state) = self.player_state.lock() {
                    state.status = PlaybackStatus::Stopped;
                    state.current_track = None;
                    state.queue.clear();
                    state.queue_index = 0;
                }
                self.search_results.clear();
                self.search_display.clear();
                self.favorites.clear();
                self.favorites_display.clear();
                self.radios.clear();
                self.playlists.clear();
                self.status_msg = None;
                self.login_error = None;
                self.login_loading = false;
            }
            Command::LoadRadios => {
                self.start_load_radios();
            }
            Command::PlayFromRadio { index } => {
                if let Some(radio) = self.radios.get(index) {
                    let radio_id = radio.id;
                    self.start_play_radio(radio_id);
                }
            }
            Command::DownloadOffline { track } => {
                self.start_download_offline(track);
            }
            Command::DownloadAlbumOffline { album_id } => {
                self.start_download_album_offline(album_id);
            }
            Command::RemoveOfflineTrack { track_id } => {
                self.offline_index.remove_track(&track_id);
                let _ = self.offline_index.save();
                self.status_msg = Some(t().status_removed_offline.into());
            }
            Command::RemoveOfflineAlbum { album_id } => {
                self.offline_index.remove_album(&album_id);
                let _ = self.offline_index.save();
                self.status_msg = Some(t().status_removed_offline.into());
            }
            Command::PlayFromOffline { index } => {
                let tracks: Vec<TrackData> = self
                    .offline_index
                    .tracks
                    .iter()
                    .map(|ot| ot.track.clone())
                    .collect();
                if let Some(track) = tracks.get(index).cloned() {
                    if let Ok(mut state) = self.player_state.lock() {
                        state.queue = tracks;
                        state.queue_index = index;
                    }
                    self.start_play_offline_track(track);
                }
            }
            Command::PlayOfflineAlbum {
                album_id,
                track_index,
            } => {
                if let Some(album) = self
                    .offline_index
                    .albums
                    .iter()
                    .find(|a| a.album_id == album_id)
                {
                    let tracks = album.tracks.clone();
                    if let Some(track) = tracks.get(track_index).cloned() {
                        if let Ok(mut state) = self.player_state.lock() {
                            state.queue = tracks;
                            state.queue_index = track_index;
                        }
                        self.start_play_offline_track(track);
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
            offline_category: self.offline_category,
            offline_tracks: {
                let album_track_ids: std::collections::HashSet<&str> = self
                    .offline_index
                    .albums
                    .iter()
                    .flat_map(|a| a.tracks.iter().map(|t| t.track_id.as_str()))
                    .collect();
                self.offline_index
                    .tracks
                    .iter()
                    .filter(|t| !album_track_ids.contains(t.track.track_id.as_str()))
                    .cloned()
                    .collect()
            },
            offline_albums: self.offline_index.albums.clone(),
            offline_selected: self.offline_selected,
            offline_loading: self.offline_loading,
            offline_track_ids: self.offline_index.track_ids(),
            radios: self.radios.clone(),
            radios_selected: self.radios_selected,
            radios_loading: self.radios_loading,
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
            is_offline: self.is_offline,
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
        self.status_msg = Some(t().status_fetching_key.into());
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
        if self.is_offline {
            self.status_msg = Some(t().no_internet.into());
            return;
        }
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
        if self.is_offline {
            self.status_msg = Some(t().no_internet.into());
            return;
        }
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
        if self.is_offline {
            self.status_msg = Some(t().no_internet.into());
            return;
        }
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
        if self.is_offline {
            return;
        }
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
        self.status_msg = Some(t().status_loading_mix.into());
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
        if self.is_offline {
            self.status_msg = Some(t().no_internet.into());
            return;
        }
        self.album_detail_loading = true;
        self.album_detail = None;
        self.album_detail_selected = 0;
        self.status_msg = Some(t().status_loading_album.into());
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
        if self.is_offline {
            self.status_msg = Some(t().no_internet.into());
            return;
        }
        self.playlist_detail_loading = true;
        self.playlist_detail = None;
        self.playlist_detail_selected = 0;
        self.status_msg = Some(t().status_loading_playlist.into());
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

    fn start_load_radios(&mut self) {
        if self.is_offline {
            return;
        }
        self.radios_loading = true;
        let client = Arc::clone(&self.client);
        let tx = self.async_tx.clone();

        tokio::spawn(async move {
            let client = client.lock().await;
            match client.get_radios().await {
                Ok(radios) => {
                    let items: Vec<RadioItem> = radios
                        .iter()
                        .map(|r| RadioItem {
                            id: r.id,
                            title: r.title.clone(),
                        })
                        .collect();
                    let _ = tx.send(AsyncResult::RadiosReady(items));
                }
                Err(e) => {
                    let _ = tx.send(AsyncResult::RadiosError(e.to_string()));
                }
            }
        });
    }

    fn start_play_radio(&mut self, radio_id: u64) {
        if self.is_offline {
            self.status_msg = Some(t().no_internet.into());
            return;
        }
        self.status_msg = Some(t().status_loading_radio_tracks.into());
        let client = Arc::clone(&self.client);
        let tx = self.async_tx.clone();

        tokio::spawn(async move {
            let client = client.lock().await;
            match client.get_radio_tracks(radio_id).await {
                Ok(tracks) => {
                    let _ = tx.send(AsyncResult::RadioTracksReady(tracks));
                }
                Err(e) => {
                    let _ = tx.send(AsyncResult::RadioTracksError(e.to_string()));
                }
            }
        });
    }

    fn start_play_track(&mut self, track: TrackData) {
        if self.is_offline {
            self.status_msg = Some(t().no_internet.into());
            return;
        }
        let Some(master_key) = self.master_key else {
            self.status_msg = Some(t().status_player_not_ready.into());
            return;
        };

        // Increment generation so any in-flight fetch for a previous track is ignored
        self.track_generation += 1;
        let generation = self.track_generation;

        info!(
            gen = generation,
            track_id = %track.track_id,
            title = %track.title,
            has_token = track.has_track_token(),
            "start_play_track: begin"
        );

        if let Ok(mut state) = self.player_state.lock() {
            state.status = PlaybackStatus::Loading;
            state.current_track = Some(track.clone());
            state.duration_secs = track.duration_secs();
            state.position_secs = 0;
        }

        self.status_msg = Some(t().loading.into());

        let client = Arc::clone(&self.client);
        let cdn_http = self.cdn_http.clone();
        let tx = self.async_tx.clone();
        let quality = self.config.quality;

        tokio::spawn(async move {
            let start = Instant::now();

            // Lock the client only for the short API calls (token + stream URL),
            // then release it before the potentially long CDN download.
            info!(gen = generation, track_id = %track.track_id, "fetch_task: waiting for client lock");
            let (track, url, actual_quality) = {
                let lock_wait = Instant::now();
                let client = client.lock().await;
                info!(gen = generation, track_id = %track.track_id, lock_ms = lock_wait.elapsed().as_millis(), "fetch_task: got client lock");

                info!(gen = generation, track_id = %track.track_id, "fetch_task: ensure_track_token");
                let token_start = Instant::now();
                let track = match client.ensure_track_token(&track).await {
                    Ok(t) => t,
                    Err(e) => {
                        warn!(gen = generation, track_id = %track.track_id, err = %e, elapsed_ms = token_start.elapsed().as_millis(), "fetch_task: ensure_track_token FAILED");
                        let _ = tx.send(AsyncResult::TrackFetchError {
                            err: e.to_string(),
                            generation,
                        });
                        return;
                    }
                };
                info!(gen = generation, track_id = %track.track_id, elapsed_ms = token_start.elapsed().as_millis(), "fetch_task: ensure_track_token OK");

                info!(gen = generation, track_id = %track.track_id, quality = quality.as_api_format(), "fetch_task: get_stream_url");
                let url_start = Instant::now();
                match client.get_stream_url(&track, quality).await {
                    Ok((url, actual_quality)) => {
                        info!(gen = generation, track_id = %track.track_id, actual_quality = actual_quality.as_api_format(), elapsed_ms = url_start.elapsed().as_millis(), "fetch_task: get_stream_url OK");
                        (track, url, actual_quality)
                    }
                    Err(first_err) => {
                        // Token may be expired — re-fetch track data with a fresh token and retry
                        warn!(gen = generation, track_id = %track.track_id, err = %first_err, elapsed_ms = url_start.elapsed().as_millis(), "fetch_task: get_stream_url failed, refreshing token");
                        let refresh_start = Instant::now();
                        let refreshed = match client.get_track(&track.track_id).await {
                            Ok(t) => t,
                            Err(e) => {
                                warn!(gen = generation, track_id = %track.track_id, err = %e, "fetch_task: token refresh (get_track) FAILED");
                                let _ = tx.send(AsyncResult::TrackFetchError {
                                    err: e.to_string(),
                                    generation,
                                });
                                return;
                            }
                        };
                        info!(gen = generation, track_id = %track.track_id, elapsed_ms = refresh_start.elapsed().as_millis(), "fetch_task: token refreshed, retrying get_stream_url");
                        match client.get_stream_url(&refreshed, quality).await {
                            Ok((url, actual_quality)) => {
                                info!(gen = generation, track_id = %track.track_id, actual_quality = actual_quality.as_api_format(), "fetch_task: get_stream_url OK after refresh");
                                (refreshed, url, actual_quality)
                            }
                            Err(_) => {
                                // Try FALLBACK track if available
                                if let Some(ref fb) = refreshed.fallback {
                                    info!(gen = generation, track_id = %track.track_id, fallback_id = %fb.track_id, "fetch_task: trying FALLBACK track");
                                    match client.get_track(&fb.track_id).await {
                                        Ok(fb_track) => {
                                            match client.get_stream_url(&fb_track, quality).await {
                                                Ok((url, actual_quality)) => {
                                                    info!(gen = generation, fallback_id = %fb_track.track_id, actual_quality = actual_quality.as_api_format(), "fetch_task: FALLBACK get_stream_url OK");
                                                    (fb_track, url, actual_quality)
                                                }
                                                Err(e) => {
                                                    warn!(gen = generation, track_id = %track.track_id, fallback_id = %fb_track.track_id, err = %e, "fetch_task: FALLBACK also failed");
                                                    let _ = tx.send(AsyncResult::TrackFetchError {
                                                        err: e.to_string(),
                                                        generation,
                                                    });
                                                    return;
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            warn!(gen = generation, fallback_id = %fb.track_id, err = %e, "fetch_task: FALLBACK get_track failed");
                                            let _ = tx.send(AsyncResult::TrackFetchError {
                                                err: e.to_string(),
                                                generation,
                                            });
                                            return;
                                        }
                                    }
                                } else {
                                    warn!(gen = generation, track_id = %track.track_id, "fetch_task: get_stream_url FAILED, no FALLBACK available");
                                    let _ = tx.send(AsyncResult::TrackFetchError {
                                        err: first_err.to_string(),
                                        generation,
                                    });
                                    return;
                                }
                            }
                        }
                    }
                }
                // client lock is dropped here
            };
            info!(gen = generation, track_id = %track.track_id, api_ms = start.elapsed().as_millis(), "fetch_task: client lock released, starting download");

            // Download + decrypt without holding the client lock
            let dl_start = Instant::now();
            match deezer_core::player::stream::download_and_decrypt(
                &url,
                &track.track_id,
                &master_key,
                &cdn_http,
            )
            .await
            {
                Ok(audio_data) => {
                    info!(
                        gen = generation,
                        track_id = %track.track_id,
                        bytes = audio_data.len(),
                        dl_ms = dl_start.elapsed().as_millis(),
                        total_ms = start.elapsed().as_millis(),
                        "fetch_task: download+decrypt OK"
                    );
                    let _ = tx.send(AsyncResult::TrackReady {
                        audio_data,
                        track,
                        quality: actual_quality,
                        generation,
                    });
                }
                Err(e) => {
                    warn!(
                        gen = generation,
                        track_id = %track.track_id,
                        err = %e,
                        dl_ms = dl_start.elapsed().as_millis(),
                        total_ms = start.elapsed().as_millis(),
                        "fetch_task: download+decrypt FAILED"
                    );
                    let _ = tx.send(AsyncResult::TrackFetchError {
                        err: e.to_string(),
                        generation,
                    });
                }
            }
        });
    }

    fn play_next(&mut self) {
        let status = self.player_state.lock().unwrap().status;
        let was_paused = status == PlaybackStatus::Paused;
        let queue_info = {
            let state = self.player_state.lock().unwrap();
            (
                state.queue.len(),
                state.queue_index,
                state.shuffle,
                state.repeat,
            )
        };
        info!(
            status = ?status,
            queue_len = queue_info.0,
            queue_index = queue_info.1,
            shuffle = queue_info.2,
            repeat = ?queue_info.3,
            "play_next called"
        );

        let next_track = {
            let mut state = self.player_state.lock().unwrap();
            if state.queue.is_empty() {
                info!("play_next: queue is empty, returning");
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
                        _ => {
                            // End of queue, no repeat — resume current track if paused
                            if was_paused {
                                drop(state);
                                self.resume_playback();
                            }
                            return;
                        }
                    }
                } else {
                    next
                }
            };

            state.queue_index = next_idx;
            state.queue.get(next_idx).cloned()
        };

        if let Some(ref track) = next_track {
            info!(track_id = %track.track_id, title = %track.title, "play_next: advancing to track");
            self.start_play_track(track.clone());
        } else {
            warn!("play_next: next_track is None (queue_index out of bounds?)");
        }
    }

    fn play_prev(&mut self) {
        let was_paused = self.player_state.lock().unwrap().status == PlaybackStatus::Paused;

        let prev_track = {
            let mut state = self.player_state.lock().unwrap();
            if state.queue.is_empty() {
                return;
            }

            let prev_idx = if state.queue_index == 0 {
                match state.repeat {
                    RepeatMode::Queue => state.queue.len() - 1,
                    _ => {
                        // Already at start — resume current track if paused
                        if was_paused {
                            drop(state);
                            self.resume_playback();
                        }
                        return;
                    }
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

    /// Resume playback from pause, updating position tracking.
    fn resume_playback(&mut self) {
        if let Some(ref engine) = self.engine {
            engine.resume();
            self.playback_started_at = Some(Instant::now());
        }
    }

    fn process_async_results(&mut self) {
        while let Ok(result) = self.async_rx.try_recv() {
            match result {
                AsyncResult::LoginSuccess(name) => {
                    self.login_loading = false;
                    self.screen = Screen::Main;
                    self.user_name = Some(name.clone());
                    self.status_msg = Some(t().fmt_connected_as(&name));
                    self.start_fetch_master_key();
                }
                AsyncResult::LoginError(err) => {
                    self.login_loading = false;
                    self.screen = Screen::Login;
                    self.login_error = Some(err);
                }
                AsyncResult::MasterKeyReady(key) => {
                    self.master_key = Some(key);
                    self.status_msg = Some(t().status_ready.into());
                    match PlayerEngine::new(key) {
                        Ok(engine) => {
                            engine.set_volume(self.config.volume);
                            self.player_state = engine.state();
                            self.engine = Some(engine);
                        }
                        Err(e) => {
                            self.status_msg =
                                Some(t().fmt_error(t().status_audio_init_error, &e.to_string()));
                        }
                    }
                    self.start_load_favorites_category();
                    self.start_load_radios();
                }
                AsyncResult::MasterKeyError(err) => {
                    self.status_msg = Some(t().fmt_error(t().status_key_error, &err));
                }
                AsyncResult::SearchResults(tracks) => {
                    self.search_loading = false;
                    self.status_msg = Some(t().fmt_results(tracks.len()));
                    self.search_display = tracks.iter().map(DisplayItem::from_track).collect();
                    self.search_results = tracks;
                    self.search_selected = 0;
                }
                AsyncResult::SearchDisplayResults(items) => {
                    self.search_loading = false;
                    self.status_msg = Some(t().fmt_results(items.len()));
                    self.search_results.clear();
                    self.search_display = items;
                    self.search_selected = 0;
                }
                AsyncResult::SearchError(err) => {
                    self.search_loading = false;
                    self.status_msg = Some(t().fmt_error(t().status_search_error, &err));
                }
                AsyncResult::FavoritesLoaded(tracks) => {
                    self.favorites_loading = false;
                    self.status_msg = Some(t().fmt_loaded(tracks.len()));
                    self.favorites_display = tracks.iter().map(DisplayItem::from_track).collect();
                    self.favorites = tracks;
                    self.favorites_selected = 0;
                }
                AsyncResult::FavoritesDisplayLoaded(items) => {
                    self.favorites_loading = false;
                    self.status_msg = Some(t().fmt_loaded(items.len()));
                    self.favorites.clear();
                    self.favorites_display = items;
                    self.favorites_selected = 0;
                }
                AsyncResult::FavoritesError(err) => {
                    self.favorites_loading = false;
                    self.favorites_display.clear();
                    self.favorites.clear();
                    self.favorites_selected = 0;
                    self.status_msg = Some(t().fmt_error(t().status_favorites_error, &err));
                }
                AsyncResult::FavoriteAdded(_track_id) => {
                    self.status_msg = Some(t().status_added_to_favorites.into());
                    // Reload favorites to reflect the change
                    self.start_load_favorites();
                }
                AsyncResult::FavoriteRemoved(_track_id) => {
                    self.status_msg = Some(t().status_removed_from_favorites.into());
                    self.start_load_favorites();
                }
                AsyncResult::FavoriteError(err) => {
                    self.status_msg = Some(t().fmt_error(t().status_favorite_error, &err));
                }
                AsyncResult::PlaylistsReady(playlists) => {
                    self.playlists = playlists;
                    self.status_msg = Some(t().fmt_playlists_loaded(self.playlists.len()));
                }
                AsyncResult::PlaylistsError(err) => {
                    self.status_msg = Some(t().fmt_error(t().status_playlists_error, &err));
                }
                AsyncResult::AddedToPlaylist(_playlist_id) => {
                    self.status_msg = Some(t().status_added_to_playlist.into());
                }
                AsyncResult::AddToPlaylistError(err) => {
                    self.status_msg = Some(t().fmt_error(t().status_add_to_playlist_error, &err));
                }
                AsyncResult::DislikeOk => {
                    self.status_msg = Some(t().status_track_disliked.into());
                }
                AsyncResult::DislikeError(err) => {
                    self.status_msg = Some(t().fmt_error(t().status_dislike_error, &err));
                }
                AsyncResult::MixReady(tracks) => {
                    if tracks.is_empty() {
                        self.status_msg = Some(t().status_no_mix_tracks.into());
                    } else {
                        self.status_msg = Some(t().fmt_mix_tracks(tracks.len()));
                        let first = tracks[0].clone();
                        if let Ok(mut state) = self.player_state.lock() {
                            state.queue = tracks;
                            state.queue_index = 0;
                        }
                        self.start_play_track(first);
                    }
                }
                AsyncResult::MixError(err) => {
                    self.status_msg = Some(t().fmt_error(t().status_mix_error, &err));
                }
                AsyncResult::AlbumDetailReady(detail) => {
                    self.album_detail_loading = false;
                    self.status_msg =
                        Some(t().fmt_album_tracks_status(&detail.title, detail.tracks.len()));
                    self.album_detail = Some(detail);
                    self.album_detail_selected = 0;
                }
                AsyncResult::AlbumDetailError(err) => {
                    self.album_detail_loading = false;
                    self.status_msg = Some(t().fmt_error(t().status_album_error, &err));
                }
                AsyncResult::PlaylistDetailReady(detail) => {
                    self.playlist_detail_loading = false;
                    self.status_msg =
                        Some(t().fmt_playlist_tracks_status(&detail.title, detail.tracks.len()));
                    self.playlist_detail = Some(detail);
                    self.playlist_detail_selected = 0;
                }
                AsyncResult::PlaylistDetailError(err) => {
                    self.playlist_detail_loading = false;
                    self.status_msg = Some(t().fmt_error(t().status_playlist_error, &err));
                }
                AsyncResult::RadiosReady(items) => {
                    self.radios_loading = false;
                    self.status_msg = Some(t().fmt_radios_loaded(items.len()));
                    self.radios = items;
                    self.radios_selected = 0;
                }
                AsyncResult::RadiosError(err) => {
                    self.radios_loading = false;
                    self.status_msg = Some(t().fmt_error(t().status_radios_error, &err));
                }
                AsyncResult::RadioTracksReady(tracks) => {
                    if tracks.is_empty() {
                        self.status_msg = Some(t().status_no_radio_tracks.into());
                    } else {
                        self.status_msg = Some(t().fmt_radio_tracks(tracks.len()));
                        let first = tracks[0].clone();
                        if let Ok(mut state) = self.player_state.lock() {
                            state.queue = tracks;
                            state.queue_index = 0;
                        }
                        self.start_play_track(first);
                    }
                }
                AsyncResult::RadioTracksError(err) => {
                    self.status_msg = Some(t().fmt_error(t().status_radio_tracks_error, &err));
                }
                AsyncResult::OfflineTrackSaved { track, quality } => {
                    self.offline_loading = false;
                    self.offline_index.add_track(track, quality);
                    let _ = self.offline_index.save();
                    self.status_msg = Some(t().status_track_saved_offline.into());
                }
                AsyncResult::OfflineTrackSaveError(err) => {
                    self.offline_loading = false;
                    self.status_msg = Some(t().fmt_error(t().status_offline_download_error, &err));
                }
                AsyncResult::OfflineAlbumSaved { album } => {
                    self.offline_loading = false;
                    // Add individual tracks to the index
                    for track in &album.tracks {
                        if !self.offline_index.has_track(&track.track_id) {
                            self.offline_index
                                .add_track(track.clone(), self.config.quality);
                        }
                    }
                    self.offline_index.add_album(album);
                    let _ = self.offline_index.save();
                    self.status_msg = Some(t().status_album_saved_offline.into());
                }
                AsyncResult::OfflineAlbumSaveError(err) => {
                    self.offline_loading = false;
                    self.status_msg = Some(t().fmt_error(t().status_offline_download_error, &err));
                }
                AsyncResult::TrackReady {
                    audio_data,
                    track,
                    quality,
                    generation,
                } => {
                    // Ignore stale results from a previous track request
                    if generation != self.track_generation {
                        info!(
                            gen = generation,
                            current_gen = self.track_generation,
                            track_id = %track.track_id,
                            title = %track.title,
                            "process_async: TrackReady IGNORED (stale generation)"
                        );
                        continue;
                    }
                    info!(
                        gen = generation,
                        track_id = %track.track_id,
                        title = %track.title,
                        bytes = audio_data.len(),
                        "process_async: TrackReady, calling play_decoded"
                    );
                    if let Some(ref mut engine) = self.engine {
                        match engine.play_decoded(audio_data, &track, quality) {
                            Ok(()) => {
                                info!(gen = generation, track_id = %track.track_id, "process_async: play_decoded OK");
                                self.consecutive_skip_count = 0;
                                self.playback_started_at = Some(Instant::now());
                                self.playback_offset_secs = 0;
                                self.status_msg = None;
                            }
                            Err(e) => {
                                warn!(gen = generation, track_id = %track.track_id, err = %e, "process_async: play_decoded FAILED");
                                self.status_msg =
                                    Some(t().fmt_error(t().status_playback_error, &e.to_string()));
                            }
                        }
                    } else {
                        warn!(gen = generation, track_id = %track.track_id, "process_async: TrackReady but engine is None!");
                    }
                }
                AsyncResult::TrackFetchError { err, generation } => {
                    // Ignore stale errors from a previous track request
                    if generation != self.track_generation {
                        info!(
                            gen = generation,
                            current_gen = self.track_generation,
                            err = %err,
                            "process_async: TrackFetchError IGNORED (stale generation)"
                        );
                        continue;
                    }
                    warn!(gen = generation, err = %err, consecutive_skips = self.consecutive_skip_count, "process_async: TrackFetchError, auto-skipping");
                    self.status_msg = Some(t().fmt_error(t().status_track_error, &err));
                    // Auto-skip to next track instead of stopping,
                    // but limit consecutive skips to avoid infinite loop
                    self.consecutive_skip_count += 1;
                    if self.consecutive_skip_count <= 5 {
                        self.play_next();
                    } else {
                        warn!("Too many consecutive track failures, stopping");
                        self.consecutive_skip_count = 0;
                        if let Ok(mut state) = self.player_state.lock() {
                            state.status = PlaybackStatus::Stopped;
                        }
                    }
                }
            }
        }
    }

    fn start_download_offline(&mut self, track: TrackData) {
        if self.is_offline {
            self.status_msg = Some(t().no_internet.into());
            return;
        }
        if self.offline_index.has_track(&track.track_id) {
            self.status_msg = Some(t().status_track_saved_offline.into());
            return;
        }

        let Some(master_key) = self.master_key else {
            self.status_msg = Some(t().status_player_not_ready.into());
            return;
        };

        self.offline_loading = true;
        self.status_msg = Some(t().status_downloading_track.into());

        let client = Arc::clone(&self.client);
        let cdn_http = self.cdn_http.clone();
        let tx = self.async_tx.clone();
        let quality = self.config.quality;

        tokio::spawn(async move {
            // Ensure we have a track token
            let track = {
                let client = client.lock().await;
                match client.ensure_track_token(&track).await {
                    Ok(t) => t,
                    Err(e) => {
                        let _ = tx.send(AsyncResult::OfflineTrackSaveError(e.to_string()));
                        return;
                    }
                }
            };

            // Get stream URL
            let (url, actual_quality) = {
                let client = client.lock().await;
                match client.get_stream_url(&track, quality).await {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = tx.send(AsyncResult::OfflineTrackSaveError(e.to_string()));
                        return;
                    }
                }
            };

            // Download and decrypt
            match deezer_core::player::stream::download_and_decrypt(
                &url,
                &track.track_id,
                &master_key,
                &cdn_http,
            )
            .await
            {
                Ok(audio_data) => {
                    // Save to disk
                    if let Err(e) = OfflineIndex::save_track_audio(&track.track_id, &audio_data) {
                        let _ = tx.send(AsyncResult::OfflineTrackSaveError(e.to_string()));
                        return;
                    }
                    let _ = tx.send(AsyncResult::OfflineTrackSaved {
                        track,
                        quality: actual_quality,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AsyncResult::OfflineTrackSaveError(e.to_string()));
                }
            }
        });
    }

    fn start_download_album_offline(&mut self, album_id: String) {
        if self.is_offline {
            self.status_msg = Some(t().no_internet.into());
            return;
        }
        if self.offline_index.has_album(&album_id) {
            self.status_msg = Some(t().status_album_saved_offline.into());
            return;
        }

        let Some(master_key) = self.master_key else {
            self.status_msg = Some(t().status_player_not_ready.into());
            return;
        };

        self.offline_loading = true;
        self.status_msg = Some(t().status_downloading_track.into());

        let client = Arc::clone(&self.client);
        let cdn_http = self.cdn_http.clone();
        let tx = self.async_tx.clone();
        let quality = self.config.quality;

        tokio::spawn(async move {
            // First fetch album detail
            let detail = {
                let client = client.lock().await;
                match client.get_album_detail(&album_id).await {
                    Ok(d) => d,
                    Err(e) => {
                        let _ = tx.send(AsyncResult::OfflineAlbumSaveError(e.to_string()));
                        return;
                    }
                }
            };

            // Download each track
            for track in &detail.tracks {
                let track = {
                    let client = client.lock().await;
                    match client.ensure_track_token(track).await {
                        Ok(t) => t,
                        Err(_) => continue,
                    }
                };

                let (url, _actual_quality) = {
                    let client = client.lock().await;
                    match client.get_stream_url(&track, quality).await {
                        Ok(r) => r,
                        Err(_) => continue,
                    }
                };

                match deezer_core::player::stream::download_and_decrypt(
                    &url,
                    &track.track_id,
                    &master_key,
                    &cdn_http,
                )
                .await
                {
                    Ok(audio_data) => {
                        let _ = OfflineIndex::save_track_audio(&track.track_id, &audio_data);
                    }
                    Err(_) => continue,
                }
            }

            let _ = tx.send(AsyncResult::OfflineAlbumSaved { album: detail });
        });
    }

    fn start_play_offline_track(&mut self, track: TrackData) {
        self.track_generation += 1;
        let generation = self.track_generation;

        if let Ok(mut state) = self.player_state.lock() {
            state.status = PlaybackStatus::Loading;
            state.current_track = Some(track.clone());
            state.duration_secs = track.duration_secs();
            state.position_secs = 0;
        }

        self.status_msg = Some(t().loading.into());

        let tx = self.async_tx.clone();
        let track_id = track.track_id.clone();

        // Load audio from disk in a blocking task
        tokio::spawn(async move {
            match OfflineIndex::load_track_audio(&track_id) {
                Ok(audio_data) => {
                    let _ = tx.send(AsyncResult::TrackReady {
                        audio_data,
                        track,
                        quality: AudioQuality::Mp3_128, // actual quality stored in index
                        generation,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AsyncResult::TrackFetchError {
                        err: e.to_string(),
                        generation,
                    });
                }
            }
        });
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
