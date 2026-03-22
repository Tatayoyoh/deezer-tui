use std::cell::Cell;

use crate::protocol::{FavoritesCategory, OfflineCategory, SearchCategory};

/// Supported locales.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    English,
    French,
    Spanish,
    Portuguese,
    German,
}

impl Locale {
    pub const ALL: &[Locale] = &[
        Locale::English,
        Locale::French,
        Locale::Spanish,
        Locale::Portuguese,
        Locale::German,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Locale::English => "English",
            Locale::French => "Français",
            Locale::Spanish => "Español (México)",
            Locale::Portuguese => "Português (Brasil)",
            Locale::German => "Deutsch",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Locale::English => "en",
            Locale::French => "fr",
            Locale::Spanish => "es",
            Locale::Portuguese => "pt",
            Locale::German => "de",
        }
    }

    pub fn from_str(s: &str) -> Option<Locale> {
        match s {
            "en" => Some(Locale::English),
            "fr" => Some(Locale::French),
            "es" => Some(Locale::Spanish),
            "pt" => Some(Locale::Portuguese),
            "de" => Some(Locale::German),
            _ => None,
        }
    }
}

/// All translatable UI strings.
pub struct Strings {
    // --- Tabs ---
    pub tab_search: &'static str,
    pub tab_favorites: &'static str,
    pub tab_radios: &'static str,
    pub tab_offline: &'static str,

    // --- Login ---
    pub login_connecting: &'static str,
    pub login_button: &'static str,
    pub login_hint: &'static str,
    pub login_arl_title: &'static str,
    pub login_arl_placeholder: &'static str,
    pub login_arl_hint: &'static str,

    // --- Search ---
    pub search_title_typing: &'static str,
    pub search_title_normal: &'static str,
    pub search_placeholder: &'static str,
    pub searching: &'static str,
    pub no_results: &'static str,
    pub results: &'static str,

    // --- Favorites ---
    pub shuffle_favorites: &'static str,
    pub loading: &'static str,
    pub no_favorites: &'static str,
    pub favorites: &'static str,

    // --- Player ---
    pub no_track_loaded: &'static str,
    pub play_pause: &'static str,
    pub next: &'static str,
    pub prev: &'static str,
    pub shuffle: &'static str,
    pub repeat: &'static str,
    pub repeat_all: &'static str,
    pub repeat_one: &'static str,
    pub vol: &'static str,
    pub help: &'static str,

    // --- Popup menu ---
    pub menu_manage: &'static str,
    pub menu_playback: &'static str,
    pub menu_media: &'static str,
    pub remove_from_favorites: &'static str,
    pub add_to_favorites: &'static str,
    pub add_to_playlist: &'static str,
    pub dont_recommend: &'static str,
    pub play_next: &'static str,
    pub add_to_queue: &'static str,
    pub mix_inspired: &'static str,
    pub track_album: &'static str,
    pub share: &'static str,
    pub track_info: &'static str,

    // --- Track info popup ---
    pub info_title: &'static str,
    pub info_artist: &'static str,
    pub info_album: &'static str,
    pub info_duration: &'static str,
    pub info_track_id: &'static str,
    pub press_esc_close: &'static str,

    // --- Playlist picker ---
    pub add_to_playlist_title: &'static str, // "Add \"{}\" to playlist"
    pub loading_playlists: &'static str,
    pub no_playlists: &'static str,

    // --- Help overlay ---
    pub keyboard_shortcuts: &'static str,
    pub help_switch_tabs: &'static str,
    pub help_search: &'static str,
    pub help_play_submit: &'static str,
    pub help_settings_back: &'static str,
    pub help_navigate_list: &'static str,
    pub help_navigate_categories: &'static str,
    pub help_play_pause: &'static str,
    pub help_next_track: &'static str,
    pub help_prev_track: &'static str,
    pub help_toggle_shuffle: &'static str,
    pub help_cycle_repeat: &'static str,
    pub help_volume: &'static str,
    pub help_album_detail: &'static str,
    pub help_waiting_list: &'static str,
    pub help_context_menu: &'static str,
    pub help_playing_menu: &'static str,
    pub help_shuffle_favorites: &'static str,
    pub help_this_help: &'static str,
    pub help_settings: &'static str,
    pub help_quit: &'static str,
    pub help_detach: &'static str,
    pub help_info: &'static str,

    // --- Settings ---
    pub settings: &'static str,
    pub settings_shortcuts: &'static str,
    pub settings_themes: &'static str,
    pub settings_language: &'static str,
    pub settings_logout: &'static str,

    // --- About modal ---
    pub about_title: &'static str,
    pub about_version: &'static str,
    pub about_architecture: &'static str,
    pub about_author: &'static str,
    pub about_github: &'static str,
    pub about_license: &'static str,

    // --- Themes ---
    pub themes: &'static str,
    pub official_deezer_themes: &'static str,

    // --- Album detail ---
    pub loading_album: &'static str,
    pub no_album_loaded: &'static str,
    pub date_label: &'static str,
    pub tracks_label: &'static str,
    pub label_label: &'static str,
    pub esc_back: &'static str,
    pub enter_play_track: &'static str,
    pub no_tracks: &'static str,

    // --- Playlist detail ---
    pub playlist: &'static str,

    // --- Waiting list ---
    pub waiting_list: &'static str,
    pub queue_empty: &'static str,

    // --- Footer hints ---
    pub hint_play: &'static str,
    pub hint_menu: &'static str,
    pub hint_close: &'static str,
    pub hint_remove: &'static str,
    pub hint_favorite: &'static str,

    // --- Radio ---
    pub radios_loading: &'static str,
    pub radios_no_results: &'static str,
    pub radios_filter_typing: &'static str,
    pub radios_filter_normal: &'static str,
    pub radios_filter_placeholder: &'static str,
    pub radios_title: &'static str,
    pub header_radio: &'static str,

    // --- Offline ---
    pub offline_empty: &'static str,
    pub download_for_offline: &'static str,
    pub remove_offline: &'static str,
    pub status_downloading_track: &'static str,
    pub status_track_saved_offline: &'static str,
    pub status_album_saved_offline: &'static str,
    pub status_offline_download_error: &'static str,
    pub status_removed_offline: &'static str,
    pub hint_download_album: &'static str,
    pub hint_expand_collapse: &'static str,

    // --- Toasts / status ---
    pub link_copied: &'static str,
    pub no_album_info: &'static str,
    pub daemon_disconnected: &'static str,
    pub detach_message: &'static str,

    // --- Daemon status messages ---
    pub status_fetching_key: &'static str,
    pub status_loading_mix: &'static str,
    pub status_loading_album: &'static str,
    pub status_loading_playlist: &'static str,
    pub status_loading_radio_tracks: &'static str,
    pub status_player_not_ready: &'static str,
    pub status_connected_as: &'static str,
    pub status_ready: &'static str,
    pub status_audio_init_error: &'static str,
    pub status_key_error: &'static str,
    pub status_results: &'static str,
    pub status_loaded: &'static str,
    pub status_search_error: &'static str,
    pub status_favorites_error: &'static str,
    pub status_added_to_favorites: &'static str,
    pub status_removed_from_favorites: &'static str,
    pub status_favorite_error: &'static str,
    pub status_playlists_loaded: &'static str,
    pub status_playlists_error: &'static str,
    pub status_added_to_playlist: &'static str,
    pub status_add_to_playlist_error: &'static str,
    pub status_track_disliked: &'static str,
    pub status_dislike_error: &'static str,
    pub status_no_mix_tracks: &'static str,
    pub status_mix_tracks: &'static str,
    pub status_mix_error: &'static str,
    pub status_album_error: &'static str,
    pub status_playlist_error: &'static str,
    pub status_radios_loaded: &'static str,
    pub status_radios_error: &'static str,
    pub status_no_radio_tracks: &'static str,
    pub status_radio_tracks: &'static str,
    pub status_radio_tracks_error: &'static str,
    pub status_playback_error: &'static str,
    pub status_track_error: &'static str,
    pub status_play_next: &'static str,
    pub status_added_to_queue: &'static str,

    // --- Table headers ---
    pub header_title: &'static str,
    pub header_artist: &'static str,
    pub header_album: &'static str,
    pub header_duration: &'static str,
    pub header_fans: &'static str,
    pub header_tracks: &'static str,
    pub header_author: &'static str,
    pub header_description: &'static str,
    pub header_profile: &'static str,
    pub header_playlist: &'static str,
    pub header_podcast: &'static str,
    pub header_episode: &'static str,

    // --- Search categories ---
    pub cat_tracks: &'static str,
    pub cat_artists: &'static str,
    pub cat_albums: &'static str,
    pub cat_playlists: &'static str,
    pub cat_podcasts: &'static str,
    pub cat_episodes: &'static str,
    pub cat_profiles: &'static str,

    // --- Favorites categories ---
    pub cat_recently_played: &'static str,
    pub cat_following: &'static str,
}

impl Strings {
    pub fn search_category_label(&self, cat: SearchCategory) -> &'static str {
        match cat {
            SearchCategory::Track => self.cat_tracks,
            SearchCategory::Artist => self.cat_artists,
            SearchCategory::Album => self.cat_albums,
            SearchCategory::Playlist => self.cat_playlists,
            SearchCategory::Podcast => self.cat_podcasts,
            SearchCategory::Episode => self.cat_episodes,
            SearchCategory::Profile => self.cat_profiles,
        }
    }

    pub fn search_category_headers(&self, cat: SearchCategory) -> [&'static str; 4] {
        match cat {
            SearchCategory::Track => [
                self.header_title,
                self.header_artist,
                self.header_album,
                self.header_duration,
            ],
            SearchCategory::Artist => [self.header_artist, self.header_fans, "", ""],
            SearchCategory::Album => [
                self.header_album,
                self.header_artist,
                "",
                self.header_tracks,
            ],
            SearchCategory::Playlist => [
                self.header_playlist,
                self.header_author,
                self.header_tracks,
                "",
            ],
            SearchCategory::Podcast => [self.header_podcast, self.header_description, "", ""],
            SearchCategory::Episode => [
                self.header_episode,
                self.header_podcast,
                "",
                self.header_duration,
            ],
            SearchCategory::Profile => [self.header_profile, "", "", ""],
        }
    }

    pub fn favorites_category_label(&self, cat: FavoritesCategory) -> &'static str {
        match cat {
            FavoritesCategory::RecentlyPlayed => self.cat_recently_played,
            FavoritesCategory::Tracks => self.cat_tracks,
            FavoritesCategory::Artists => self.cat_artists,
            FavoritesCategory::Albums => self.cat_albums,
            FavoritesCategory::Playlists => self.cat_playlists,
            FavoritesCategory::Following => self.cat_following,
        }
    }

    pub fn favorites_category_headers(&self, cat: FavoritesCategory) -> [&'static str; 4] {
        match cat {
            FavoritesCategory::RecentlyPlayed | FavoritesCategory::Tracks => [
                self.header_title,
                self.header_artist,
                self.header_album,
                self.header_duration,
            ],
            FavoritesCategory::Artists => [self.header_artist, self.header_fans, "", ""],
            FavoritesCategory::Albums => [
                self.header_album,
                self.header_artist,
                "",
                self.header_tracks,
            ],
            FavoritesCategory::Playlists => [
                self.header_playlist,
                self.header_author,
                self.header_tracks,
                "",
            ],
            FavoritesCategory::Following => [self.header_profile, "", "", ""],
        }
    }

    pub fn offline_category_label(&self, cat: OfflineCategory) -> &'static str {
        match cat {
            OfflineCategory::Tracks => self.cat_tracks,
            OfflineCategory::Albums => self.cat_albums,
        }
    }

    /// Format "Waiting List (N tracks)"
    pub fn waiting_list_title(&self, count: usize) -> String {
        format!(" {} ({} {}) ", self.waiting_list, count, self.header_tracks)
    }

    /// Format "Results (N)"
    pub fn results_title(&self, count: usize) -> String {
        format!(" {} ({}) ", self.results, count)
    }

    /// Format "Favorites (N)"
    pub fn favorites_title(&self, count: usize) -> String {
        format!(" {} ({}) ", self.favorites, count)
    }

    /// Format "Album — N tracks"
    pub fn album_tracks_title(&self, album: &str, count: usize) -> String {
        format!(" {} — {} {} ", album, count, self.header_tracks)
    }

    /// Format "Creator — N titres" for playlist subtitle
    pub fn playlist_subtitle(&self, creator: &str, count: u64) -> String {
        format!("{} — {} {}", creator, count, self.header_tracks)
    }

    /// Format "Add \"title\" to playlist"
    pub fn add_to_playlist_fmt(&self, title: &str) -> String {
        format!(" {} \"{}\" ", self.add_to_playlist_title, title)
    }

    /// Format playlist item "name (N tracks)"
    pub fn playlist_item(&self, name: &str, count: u64) -> String {
        format!("{} ({} {})", name, count, self.header_tracks)
    }

    /// Format "Radios (N)"
    pub fn radios_count_title(&self, count: usize) -> String {
        format!(" {} ({}) ", self.radios_title, count)
    }

    /// Format "Connected as {name}"
    pub fn fmt_connected_as(&self, name: &str) -> String {
        format!("{} {}", self.status_connected_as, name)
    }

    /// Format "{N} results"
    pub fn fmt_results(&self, count: usize) -> String {
        format!("{} {}", count, self.status_results)
    }

    /// Format "{N} loaded"
    pub fn fmt_loaded(&self, count: usize) -> String {
        format!("{} {}", count, self.status_loaded)
    }

    /// Format "{N} playlists loaded"
    pub fn fmt_playlists_loaded(&self, count: usize) -> String {
        format!("{} {}", count, self.status_playlists_loaded)
    }

    /// Format "{N} radios loaded"
    pub fn fmt_radios_loaded(&self, count: usize) -> String {
        format!("{} {}", count, self.status_radios_loaded)
    }

    /// Format "Mix: {N} tracks"
    pub fn fmt_mix_tracks(&self, count: usize) -> String {
        format!("{} {}", self.status_mix_tracks, count)
    }

    /// Format "Radio: {N} tracks"
    pub fn fmt_radio_tracks(&self, count: usize) -> String {
        format!("{} {}", self.status_radio_tracks, count)
    }

    /// Format "Album — N tracks"
    pub fn fmt_album_tracks_status(&self, title: &str, count: usize) -> String {
        format!("{} — {} {}", title, count, self.header_tracks)
    }

    /// Format "Playlist — N tracks"
    pub fn fmt_playlist_tracks_status(&self, title: &str, count: usize) -> String {
        format!("{} — {} {}", title, count, self.header_tracks)
    }

    /// Format error with prefix
    pub fn fmt_error(&self, prefix: &str, err: &str) -> String {
        format!("{}: {}", prefix, err)
    }
}

// ── English ──────────────────────────────────────────────────────────
static EN: Strings = Strings {
    tab_search: " Search ",
    tab_favorites: " Favorites ",
    tab_radios: " Radios ",
    tab_offline: " Offline ",

    login_connecting: "Connecting...",
    login_button: "Login with Deezer",
    login_hint: "Enter: login | w: connect with ARL | Esc: quit",
    login_arl_title: " ARL Token ",
    login_arl_placeholder: "Paste your ARL token from browser cookies...",
    login_arl_hint: "Enter: connect | Esc: back",

    search_title_typing: " Search (Enter to submit, Esc to cancel) ",
    search_title_normal: " Search (press / to type) ",
    search_placeholder: "Press / to search tracks, artists, albums...",
    searching: "Searching...",
    no_results: "No results yet",
    results: "Results",

    shuffle_favorites: "Shuffle play my favorites",
    loading: "Loading...",
    no_favorites: "No favorites yet \u{2014} add some on Deezer!",
    favorites: "Favorites",

    no_track_loaded: "No track loaded",
    play_pause: "Play/Pause",
    next: "Next",
    prev: "Prev",
    shuffle: "Shuffle",
    repeat: "Repeat",
    repeat_all: "Repeat All",
    repeat_one: "Repeat One",
    vol: "Vol",
    help: "Help",

    menu_manage: "── Manage ──",
    menu_playback: "── Playback ──",
    menu_media: "── Media ──",
    remove_from_favorites: "Remove from favorites",
    add_to_favorites: "Add to favorites",
    add_to_playlist: "Add to playlist",
    dont_recommend: "Don't recommend this track",
    play_next: "Play next",
    add_to_queue: "Add to queue",
    mix_inspired: "Mix inspired by this track",
    track_album: "Track album",
    share: "Share",
    track_info: "Track info",

    info_title: "Title:    ",
    info_artist: "Artist:   ",
    info_album: "Album:    ",
    info_duration: "Duration: ",
    info_track_id: "Track ID: ",
    press_esc_close: "Press Esc to close",

    add_to_playlist_title: "Add to playlist",
    loading_playlists: "Loading playlists...",
    no_playlists: "No playlists found",

    keyboard_shortcuts: " Keyboard Shortcuts ",
    help_switch_tabs: "Switch tabs",
    help_search: "Search",
    help_play_submit: "Play / Submit",
    help_settings_back: "Settings / Back",
    help_navigate_list: "Navigate list",
    help_navigate_categories: "Navigate categories",
    help_play_pause: "Play / Pause",
    help_next_track: "Next track",
    help_prev_track: "Previous track",
    help_toggle_shuffle: "Toggle shuffle",
    help_cycle_repeat: "Cycle repeat mode",
    help_volume: "Volume up / down",
    help_album_detail: "Album detail page",
    help_waiting_list: "Waiting list (queue)",
    help_context_menu: "Track context menu",
    help_playing_menu: "Playing track menu",
    help_shuffle_favorites: "Shuffle favorites",
    help_this_help: "This help",
    help_settings: "Settings",
    help_quit: "Quit",
    help_detach: "Detach (keep playing)",
    help_info: "Application info",

    settings: " Settings ",
    settings_shortcuts: "Keyboard shortcuts",
    settings_themes: "Themes",
    settings_language: "Language",
    settings_logout: "Logout",

    about_title: " About Deezer TUI ",
    about_version: "Version",
    about_architecture: "Architecture",
    about_author: "Author",
    about_github: "GitHub",
    about_license: "License",

    themes: " Themes ",
    official_deezer_themes: "  Official Deezer themes",

    loading_album: "Loading album...",
    no_album_loaded: "No album loaded",
    date_label: "Date:    ",
    tracks_label: "Tracks:  ",
    label_label: "Label:   ",
    esc_back: "Esc  Back",
    enter_play_track: "Enter  Play track",
    no_tracks: "No tracks",

    playlist: " Playlist ",

    waiting_list: "Waiting List",
    queue_empty: "Queue is empty",

    hint_play: " play  ",
    hint_menu: " menu  ",
    hint_close: " close",
    hint_remove: " remove  ",
    hint_favorite: " favorite  ",

    radios_loading: "Loading radios...",
    radios_no_results: "No radios found",
    radios_filter_typing: " Filter (Enter to confirm, Esc to cancel) ",
    radios_filter_normal: " Filter (press / to type) ",
    radios_filter_placeholder: "Press / or Ctrl+F to filter radios...",
    radios_title: "Radios",
    header_radio: "Radio",

    offline_empty: "No offline content yet",
    download_for_offline: "Download for offline mode",
    remove_offline: "Remove from offline",
    status_downloading_track: "Downloading for offline...",
    status_track_saved_offline: "Track saved for offline",
    status_album_saved_offline: "Album saved for offline",
    status_offline_download_error: "Offline download error",
    status_removed_offline: "Removed from offline",
    hint_download_album: " offline  ",
    hint_expand_collapse: "→ expand/collapse",

    link_copied: "Link copied to clipboard!",
    no_album_info: "No album info available",
    daemon_disconnected: "Daemon disconnected",
    detach_message:
        "deezer-tui: music continues in background. Run \"deezer-tui\" to restore the player.",

    status_fetching_key: "Fetching decryption key...",
    status_loading_mix: "Loading mix...",
    status_loading_album: "Loading album...",
    status_loading_playlist: "Loading playlist...",
    status_loading_radio_tracks: "Loading radio tracks...",
    status_player_not_ready: "Player not ready yet",
    status_connected_as: "Connected as",
    status_ready: "Ready to play",
    status_audio_init_error: "Audio init error",
    status_key_error: "Key error",
    status_results: "results",
    status_loaded: "loaded",
    status_search_error: "Search error",
    status_favorites_error: "Favorites error",
    status_added_to_favorites: "Added to favorites",
    status_removed_from_favorites: "Removed from favorites",
    status_favorite_error: "Favorite error",
    status_playlists_loaded: "playlists loaded",
    status_playlists_error: "Playlists error",
    status_added_to_playlist: "Added to playlist",
    status_add_to_playlist_error: "Add to playlist error",
    status_track_disliked: "Track marked as disliked",
    status_dislike_error: "Dislike error",
    status_no_mix_tracks: "No mix tracks found",
    status_mix_tracks: "Mix:",
    status_mix_error: "Mix error",
    status_album_error: "Album error",
    status_playlist_error: "Playlist error",
    status_radios_loaded: "radios loaded",
    status_radios_error: "Radios error",
    status_no_radio_tracks: "No tracks in this radio",
    status_radio_tracks: "Radio:",
    status_radio_tracks_error: "Radio tracks error",
    status_playback_error: "Playback error",
    status_track_error: "Track error",
    status_play_next: "Will play next",
    status_added_to_queue: "Added to queue",

    header_title: "Title",
    header_artist: "Artist",
    header_album: "Album",
    header_duration: "Duration",
    header_fans: "Fans",
    header_tracks: "Tracks",
    header_author: "Author",
    header_description: "Description",
    header_profile: "Profile",
    header_playlist: "Playlist",
    header_podcast: "Podcast",
    header_episode: "Episode",

    cat_tracks: "Tracks",
    cat_artists: "Artists",
    cat_albums: "Albums",
    cat_playlists: "Playlists",
    cat_podcasts: "Podcasts",
    cat_episodes: "Episodes",
    cat_profiles: "Profiles",

    cat_recently_played: "Recently Played",
    cat_following: "Following",
};

// ── French ───────────────────────────────────────────────────────────
static FR: Strings = Strings {
    tab_search: " Recherche ",
    tab_favorites: " Favoris ",
    tab_radios: " Radios ",
    tab_offline: " Hors-ligne ",

    login_connecting: "Connexion...",
    login_button: "Se connecter avec Deezer",
    login_hint: "Entrée : connexion | w : connecter avec ARL | Esc : quitter",
    login_arl_title: " Jeton ARL ",
    login_arl_placeholder: "Collez votre jeton ARL depuis les cookies du navigateur...",
    login_arl_hint: "Entrée : connecter | Esc : retour",

    search_title_typing: " Recherche (Entrée pour valider, Esc pour annuler) ",
    search_title_normal: " Recherche (appuyez sur / pour écrire) ",
    search_placeholder: "Appuyez sur / pour chercher titres, artistes, albums...",
    searching: "Recherche...",
    no_results: "Aucun résultat",
    results: "Résultats",

    shuffle_favorites: "Jouer aléatoirement mes favoris",
    loading: "Chargement...",
    no_favorites: "Pas encore de favoris \u{2014} ajoutez-en sur Deezer !",
    favorites: "Favoris",

    no_track_loaded: "Aucun titre chargé",
    play_pause: "Lecture/Pause",
    next: "Suivant",
    prev: "Préc.",
    shuffle: "Aléatoire",
    repeat: "Répéter",
    repeat_all: "Répéter tout",
    repeat_one: "Répéter un",
    vol: "Vol",
    help: "Aide",

    menu_manage: "── Gérer ──",
    menu_playback: "── Lecture ──",
    menu_media: "── Média ──",
    remove_from_favorites: "Retirer des favoris",
    add_to_favorites: "Ajouter aux favoris",
    add_to_playlist: "Ajouter à une playlist",
    dont_recommend: "Ne pas recommander ce titre",
    play_next: "Lire ensuite",
    add_to_queue: "Ajouter à la file d'attente",
    mix_inspired: "Mix inspiré par ce titre",
    track_album: "Album du titre",
    share: "Partager",
    track_info: "Infos du titre",

    info_title: "Titre :   ",
    info_artist: "Artiste : ",
    info_album: "Album :   ",
    info_duration: "Durée :   ",
    info_track_id: "ID titre :",
    press_esc_close: "Esc pour fermer",

    add_to_playlist_title: "Ajouter à la playlist",
    loading_playlists: "Chargement des playlists...",
    no_playlists: "Aucune playlist trouvée",

    keyboard_shortcuts: " Raccourcis clavier ",
    help_switch_tabs: "Changer d'onglet",
    help_search: "Rechercher",
    help_play_submit: "Lire / Valider",
    help_settings_back: "Paramètres / Retour",
    help_navigate_list: "Naviguer dans la liste",
    help_navigate_categories: "Naviguer les catégories",
    help_play_pause: "Lecture / Pause",
    help_next_track: "Titre suivant",
    help_prev_track: "Titre précédent",
    help_toggle_shuffle: "Activer l'aléatoire",
    help_cycle_repeat: "Changer le mode répétition",
    help_volume: "Volume + / -",
    help_album_detail: "Page détail album",
    help_waiting_list: "File d'attente",
    help_context_menu: "Menu contextuel",
    help_playing_menu: "Menu titre en cours",
    help_shuffle_favorites: "Lecture aléatoire favoris",
    help_this_help: "Cette aide",
    help_settings: "Paramètres",
    help_quit: "Quitter",
    help_detach: "Détacher (continuer la lecture)",
    help_info: "Infos application",

    settings: " Paramètres ",
    settings_shortcuts: "Raccourcis clavier",
    settings_themes: "Thèmes",
    settings_language: "Langue",
    settings_logout: "Déconnexion",

    about_title: " À propos de Deezer TUI ",
    about_version: "Version",
    about_architecture: "Architecture",
    about_author: "Auteur",
    about_github: "GitHub",
    about_license: "Licence",

    themes: " Thèmes ",
    official_deezer_themes: "  Thèmes officiels Deezer",

    loading_album: "Chargement de l'album...",
    no_album_loaded: "Aucun album chargé",
    date_label: "Date :   ",
    tracks_label: "Titres : ",
    label_label: "Label :  ",
    esc_back: "Esc  Retour",
    enter_play_track: "Entrée  Lire le titre",
    no_tracks: "Aucun titre",

    playlist: " Playlist ",

    waiting_list: "File d'attente",
    queue_empty: "La file d'attente est vide",

    hint_play: " lire  ",
    hint_menu: " menu  ",
    hint_close: " fermer",
    hint_remove: " supprimer  ",
    hint_favorite: " favori  ",

    radios_loading: "Chargement des radios...",
    radios_no_results: "Aucune radio trouvée",
    radios_filter_typing: " Filtre (Entrée pour valider, Esc pour annuler) ",
    radios_filter_normal: " Filtre (appuyez sur / pour écrire) ",
    radios_filter_placeholder: "Appuyez sur / ou Ctrl+F pour filtrer les radios...",
    radios_title: "Radios",
    header_radio: "Radio",

    offline_empty: "Aucun contenu hors-ligne",
    download_for_offline: "Télécharger hors-ligne",
    remove_offline: "Supprimer du mode hors-ligne",
    status_downloading_track: "Téléchargement hors-ligne...",
    status_track_saved_offline: "Titre sauvegardé hors-ligne",
    status_album_saved_offline: "Album sauvegardé hors-ligne",
    status_offline_download_error: "Erreur de téléchargement hors-ligne",
    status_removed_offline: "Supprimé du mode hors-ligne",
    hint_download_album: " hors-ligne  ",
    hint_expand_collapse: "→ déplier/replier",

    link_copied: "Lien copié dans le presse-papiers !",
    no_album_info: "Aucune info d'album disponible",
    daemon_disconnected: "Démon déconnecté",
    detach_message: "deezer-tui : la musique continue en arrière-plan. Lancez \"deezer-tui\" pour restaurer le lecteur.",

    status_fetching_key: "Récupération de la clé de déchiffrement...",
    status_loading_mix: "Chargement du mix...",
    status_loading_album: "Chargement de l'album...",
    status_loading_playlist: "Chargement de la playlist...",
    status_loading_radio_tracks: "Chargement des titres de la radio...",
    status_player_not_ready: "Lecteur pas encore prêt",
    status_connected_as: "Connecté en tant que",
    status_ready: "Prêt à lire",
    status_audio_init_error: "Erreur d'initialisation audio",
    status_key_error: "Erreur de clé",
    status_results: "résultats",
    status_loaded: "chargé(s)",
    status_search_error: "Erreur de recherche",
    status_favorites_error: "Erreur des favoris",
    status_added_to_favorites: "Ajouté aux favoris",
    status_removed_from_favorites: "Retiré des favoris",
    status_favorite_error: "Erreur de favori",
    status_playlists_loaded: "playlists chargées",
    status_playlists_error: "Erreur des playlists",
    status_added_to_playlist: "Ajouté à la playlist",
    status_add_to_playlist_error: "Erreur d'ajout à la playlist",
    status_track_disliked: "Titre marqué comme non recommandé",
    status_dislike_error: "Erreur de non-recommandation",
    status_no_mix_tracks: "Aucun titre de mix trouvé",
    status_mix_tracks: "Mix :",
    status_mix_error: "Erreur du mix",
    status_album_error: "Erreur de l'album",
    status_playlist_error: "Erreur de la playlist",
    status_radios_loaded: "radios chargées",
    status_radios_error: "Erreur des radios",
    status_no_radio_tracks: "Aucun titre dans cette radio",
    status_radio_tracks: "Radio :",
    status_radio_tracks_error: "Erreur des titres radio",
    status_playback_error: "Erreur de lecture",
    status_track_error: "Erreur du titre",
    status_play_next: "Sera lu ensuite",
    status_added_to_queue: "Ajouté à la file d'attente",

    header_title: "Titre",
    header_artist: "Artiste",
    header_album: "Album",
    header_duration: "Durée",
    header_fans: "Fans",
    header_tracks: "Titres",
    header_author: "Auteur",
    header_description: "Description",
    header_profile: "Profil",
    header_playlist: "Playlist",
    header_podcast: "Podcast",
    header_episode: "Épisode",

    cat_tracks: "Titres",
    cat_artists: "Artistes",
    cat_albums: "Albums",
    cat_playlists: "Playlists",
    cat_podcasts: "Podcasts",
    cat_episodes: "Épisodes",
    cat_profiles: "Profils",

    cat_recently_played: "Écouté récemment",
    cat_following: "Abonnements",
};

// ── Spanish (Mexico) ─────────────────────────────────────────────────
static ES: Strings = Strings {
    tab_search: " Buscar ",
    tab_favorites: " Favoritos ",
    tab_radios: " Radios ",
    tab_offline: " Sin conexión ",

    login_connecting: "Conectando...",
    login_button: "Iniciar sesión con Deezer",
    login_hint: "Enter: iniciar sesión | w: conectar con ARL | Esc: salir",
    login_arl_title: " Token ARL ",
    login_arl_placeholder: "Pega tu token ARL de las cookies del navegador...",
    login_arl_hint: "Enter: conectar | Esc: volver",

    search_title_typing: " Buscar (Enter para enviar, Esc para cancelar) ",
    search_title_normal: " Buscar (presiona / para escribir) ",
    search_placeholder: "Presiona / para buscar canciones, artistas, álbumes...",
    searching: "Buscando...",
    no_results: "Sin resultados",
    results: "Resultados",

    shuffle_favorites: "Reproducir favoritos aleatoriamente",
    loading: "Cargando...",
    no_favorites: "Aún no hay favoritos \u{2014} ¡agrega algunos en Deezer!",
    favorites: "Favoritos",

    no_track_loaded: "Sin canción cargada",
    play_pause: "Reproducir/Pausa",
    next: "Siguiente",
    prev: "Anterior",
    shuffle: "Aleatorio",
    repeat: "Repetir",
    repeat_all: "Repetir todo",
    repeat_one: "Repetir uno",
    vol: "Vol",
    help: "Ayuda",

    menu_manage: "── Gestionar ──",
    menu_playback: "── Reproducción ──",
    menu_media: "── Medios ──",
    remove_from_favorites: "Quitar de favoritos",
    add_to_favorites: "Agregar a favoritos",
    add_to_playlist: "Agregar a playlist",
    dont_recommend: "No recomendar esta canción",
    play_next: "Reproducir siguiente",
    add_to_queue: "Agregar a la cola",
    mix_inspired: "Mix inspirado en esta canción",
    track_album: "Álbum de la canción",
    share: "Compartir",
    track_info: "Info de la canción",

    info_title: "Título:   ",
    info_artist: "Artista:  ",
    info_album: "Álbum:    ",
    info_duration: "Duración: ",
    info_track_id: "ID canción:",
    press_esc_close: "Esc para cerrar",

    add_to_playlist_title: "Agregar a playlist",
    loading_playlists: "Cargando playlists...",
    no_playlists: "No se encontraron playlists",

    keyboard_shortcuts: " Atajos de teclado ",
    help_switch_tabs: "Cambiar pestaña",
    help_search: "Buscar",
    help_play_submit: "Reproducir / Enviar",
    help_settings_back: "Ajustes / Volver",
    help_navigate_list: "Navegar lista",
    help_navigate_categories: "Navegar categorías",
    help_play_pause: "Reproducir / Pausa",
    help_next_track: "Siguiente canción",
    help_prev_track: "Canción anterior",
    help_toggle_shuffle: "Activar aleatorio",
    help_cycle_repeat: "Cambiar modo repetición",
    help_volume: "Volumen + / -",
    help_album_detail: "Detalle del álbum",
    help_waiting_list: "Cola de reproducción",
    help_context_menu: "Menú contextual",
    help_playing_menu: "Menú canción actual",
    help_shuffle_favorites: "Favoritos aleatorios",
    help_this_help: "Esta ayuda",
    help_settings: "Ajustes",
    help_quit: "Salir",
    help_detach: "Desacoplar (seguir reproduciendo)",
    help_info: "Info de la aplicación",

    settings: " Ajustes ",
    settings_shortcuts: "Atajos de teclado",
    settings_themes: "Temas",
    settings_language: "Idioma",
    settings_logout: "Cerrar sesión",

    about_title: " Acerca de Deezer TUI ",
    about_version: "Versión",
    about_architecture: "Arquitectura",
    about_author: "Autor",
    about_github: "GitHub",
    about_license: "Licencia",

    themes: " Temas ",
    official_deezer_themes: "  Temas oficiales de Deezer",

    loading_album: "Cargando álbum...",
    no_album_loaded: "Ningún álbum cargado",
    date_label: "Fecha:   ",
    tracks_label: "Canciones:",
    label_label: "Sello:   ",
    esc_back: "Esc  Volver",
    enter_play_track: "Enter  Reproducir canción",
    no_tracks: "Sin canciones",

    playlist: " Playlist ",

    waiting_list: "Cola de reproducción",
    queue_empty: "La cola está vacía",

    hint_play: " reproducir  ",
    hint_menu: " menú  ",
    hint_close: " cerrar",
    hint_remove: " quitar  ",
    hint_favorite: " favorito  ",

    radios_loading: "Cargando radios...",
    radios_no_results: "No se encontraron radios",
    radios_filter_typing: " Filtro (Enter para confirmar, Esc para cancelar) ",
    radios_filter_normal: " Filtro (presiona / para escribir) ",
    radios_filter_placeholder: "Presiona / o Ctrl+F para filtrar radios...",
    radios_title: "Radios",
    header_radio: "Radio",

    offline_empty: "Sin contenido sin conexión",
    download_for_offline: "Descargar para modo sin conexión",
    remove_offline: "Eliminar del modo sin conexión",
    status_downloading_track: "Descargando sin conexión...",
    status_track_saved_offline: "Pista guardada sin conexión",
    status_album_saved_offline: "Álbum guardado sin conexión",
    status_offline_download_error: "Error de descarga sin conexión",
    status_removed_offline: "Eliminado del modo sin conexión",
    hint_download_album: " sin conexión  ",
    hint_expand_collapse: "→ expandir/contraer",

    link_copied: "¡Enlace copiado al portapapeles!",
    no_album_info: "No hay info del álbum disponible",
    daemon_disconnected: "Demonio desconectado",
    detach_message: "deezer-tui: la música sigue en segundo plano. Ejecuta \"deezer-tui\" para restaurar el reproductor.",

    status_fetching_key: "Obteniendo clave de descifrado...",
    status_loading_mix: "Cargando mix...",
    status_loading_album: "Cargando álbum...",
    status_loading_playlist: "Cargando playlist...",
    status_loading_radio_tracks: "Cargando canciones de la radio...",
    status_player_not_ready: "Reproductor aún no listo",
    status_connected_as: "Conectado como",
    status_ready: "Listo para reproducir",
    status_audio_init_error: "Error de inicio de audio",
    status_key_error: "Error de clave",
    status_results: "resultados",
    status_loaded: "cargado(s)",
    status_search_error: "Error de búsqueda",
    status_favorites_error: "Error de favoritos",
    status_added_to_favorites: "Agregado a favoritos",
    status_removed_from_favorites: "Eliminado de favoritos",
    status_favorite_error: "Error de favorito",
    status_playlists_loaded: "playlists cargadas",
    status_playlists_error: "Error de playlists",
    status_added_to_playlist: "Agregado a la playlist",
    status_add_to_playlist_error: "Error al agregar a la playlist",
    status_track_disliked: "Canción marcada como no recomendada",
    status_dislike_error: "Error de no recomendar",
    status_no_mix_tracks: "No se encontraron canciones del mix",
    status_mix_tracks: "Mix:",
    status_mix_error: "Error del mix",
    status_album_error: "Error del álbum",
    status_playlist_error: "Error de la playlist",
    status_radios_loaded: "radios cargadas",
    status_radios_error: "Error de radios",
    status_no_radio_tracks: "No hay canciones en esta radio",
    status_radio_tracks: "Radio:",
    status_radio_tracks_error: "Error de canciones de radio",
    status_playback_error: "Error de reproducción",
    status_track_error: "Error de canción",
    status_play_next: "Se reproducirá después",
    status_added_to_queue: "Agregado a la cola",

    header_title: "Título",
    header_artist: "Artista",
    header_album: "Álbum",
    header_duration: "Duración",
    header_fans: "Fans",
    header_tracks: "Canciones",
    header_author: "Autor",
    header_description: "Descripción",
    header_profile: "Perfil",
    header_playlist: "Playlist",
    header_podcast: "Podcast",
    header_episode: "Episodio",

    cat_tracks: "Canciones",
    cat_artists: "Artistas",
    cat_albums: "Álbumes",
    cat_playlists: "Playlists",
    cat_podcasts: "Podcasts",
    cat_episodes: "Episodios",
    cat_profiles: "Perfiles",

    cat_recently_played: "Escuchado recientemente",
    cat_following: "Siguiendo",
};

// ── Portuguese (Brazil) ──────────────────────────────────────────────
static PT: Strings = Strings {
    tab_search: " Buscar ",
    tab_favorites: " Favoritos ",
    tab_radios: " Rádios ",
    tab_offline: " Offline ",

    login_connecting: "Conectando...",
    login_button: "Entrar com Deezer",
    login_hint: "Enter: entrar | w: conectar com ARL | Esc: sair",
    login_arl_title: " Token ARL ",
    login_arl_placeholder: "Cole seu token ARL dos cookies do navegador...",
    login_arl_hint: "Enter: conectar | Esc: voltar",

    search_title_typing: " Buscar (Enter para enviar, Esc para cancelar) ",
    search_title_normal: " Buscar (pressione / para digitar) ",
    search_placeholder: "Pressione / para buscar músicas, artistas, álbuns...",
    searching: "Buscando...",
    no_results: "Nenhum resultado",
    results: "Resultados",

    shuffle_favorites: "Reproduzir favoritos aleatoriamente",
    loading: "Carregando...",
    no_favorites: "Nenhum favorito ainda \u{2014} adicione alguns no Deezer!",
    favorites: "Favoritos",

    no_track_loaded: "Nenhuma música carregada",
    play_pause: "Play/Pausa",
    next: "Próxima",
    prev: "Anterior",
    shuffle: "Aleatório",
    repeat: "Repetir",
    repeat_all: "Repetir tudo",
    repeat_one: "Repetir uma",
    vol: "Vol",
    help: "Ajuda",

    menu_manage: "── Gerenciar ──",
    menu_playback: "── Reprodução ──",
    menu_media: "── Mídia ──",
    remove_from_favorites: "Remover dos favoritos",
    add_to_favorites: "Adicionar aos favoritos",
    add_to_playlist: "Adicionar à playlist",
    dont_recommend: "Não recomendar esta música",
    play_next: "Reproduzir em seguida",
    add_to_queue: "Adicionar à fila",
    mix_inspired: "Mix inspirado nesta música",
    track_album: "Álbum da música",
    share: "Compartilhar",
    track_info: "Info da música",

    info_title: "Título:   ",
    info_artist: "Artista:  ",
    info_album: "Álbum:    ",
    info_duration: "Duração:  ",
    info_track_id: "ID música:",
    press_esc_close: "Esc para fechar",

    add_to_playlist_title: "Adicionar à playlist",
    loading_playlists: "Carregando playlists...",
    no_playlists: "Nenhuma playlist encontrada",

    keyboard_shortcuts: " Atalhos do teclado ",
    help_switch_tabs: "Alternar abas",
    help_search: "Buscar",
    help_play_submit: "Reproduzir / Enviar",
    help_settings_back: "Configurações / Voltar",
    help_navigate_list: "Navegar na lista",
    help_navigate_categories: "Navegar categorias",
    help_play_pause: "Reproduzir / Pausar",
    help_next_track: "Próxima música",
    help_prev_track: "Música anterior",
    help_toggle_shuffle: "Ativar aleatório",
    help_cycle_repeat: "Alterar modo repetição",
    help_volume: "Volume + / -",
    help_album_detail: "Detalhe do álbum",
    help_waiting_list: "Fila de reprodução",
    help_context_menu: "Menu de contexto",
    help_playing_menu: "Menu da música atual",
    help_shuffle_favorites: "Favoritos aleatórios",
    help_this_help: "Esta ajuda",
    help_settings: "Configurações",
    help_quit: "Sair",
    help_detach: "Desanexar (continuar tocando)",
    help_info: "Info do aplicativo",

    settings: " Configurações ",
    settings_shortcuts: "Atalhos do teclado",
    settings_themes: "Temas",
    settings_language: "Idioma",
    settings_logout: "Sair",

    about_title: " Sobre Deezer TUI ",
    about_version: "Versão",
    about_architecture: "Arquitetura",
    about_author: "Autor",
    about_github: "GitHub",
    about_license: "Licença",

    themes: " Temas ",
    official_deezer_themes: "  Temas oficiais do Deezer",

    loading_album: "Carregando álbum...",
    no_album_loaded: "Nenhum álbum carregado",
    date_label: "Data:    ",
    tracks_label: "Músicas: ",
    label_label: "Selo:    ",
    esc_back: "Esc  Voltar",
    enter_play_track: "Enter  Reproduzir música",
    no_tracks: "Sem músicas",

    playlist: " Playlist ",

    waiting_list: "Fila de reprodução",
    queue_empty: "A fila está vazia",

    hint_play: " reproduzir  ",
    hint_menu: " menu  ",
    hint_close: " fechar",
    hint_remove: " remover  ",
    hint_favorite: " favorito  ",

    radios_loading: "Carregando rádios...",
    radios_no_results: "Nenhuma rádio encontrada",
    radios_filter_typing: " Filtro (Enter para confirmar, Esc para cancelar) ",
    radios_filter_normal: " Filtro (pressione / para digitar) ",
    radios_filter_placeholder: "Pressione / ou Ctrl+F para filtrar rádios...",
    radios_title: "Rádios",
    header_radio: "Rádio",

    offline_empty: "Sem conteúdo offline",
    download_for_offline: "Baixar para modo offline",
    remove_offline: "Remover do modo offline",
    status_downloading_track: "Baixando para offline...",
    status_track_saved_offline: "Faixa salva para offline",
    status_album_saved_offline: "Álbum salvo para offline",
    status_offline_download_error: "Erro ao baixar para offline",
    status_removed_offline: "Removido do modo offline",
    hint_download_album: " offline  ",
    hint_expand_collapse: "→ expandir/recolher",

    link_copied: "Link copiado para a área de transferência!",
    no_album_info: "Nenhuma info do álbum disponível",
    daemon_disconnected: "Daemon desconectado",
    detach_message: "deezer-tui: a música continua em segundo plano. Execute \"deezer-tui\" para restaurar o player.",

    status_fetching_key: "Obtendo chave de descriptografia...",
    status_loading_mix: "Carregando mix...",
    status_loading_album: "Carregando álbum...",
    status_loading_playlist: "Carregando playlist...",
    status_loading_radio_tracks: "Carregando músicas da rádio...",
    status_player_not_ready: "Player ainda não pronto",
    status_connected_as: "Conectado como",
    status_ready: "Pronto para reproduzir",
    status_audio_init_error: "Erro de inicialização de áudio",
    status_key_error: "Erro de chave",
    status_results: "resultados",
    status_loaded: "carregado(s)",
    status_search_error: "Erro de busca",
    status_favorites_error: "Erro dos favoritos",
    status_added_to_favorites: "Adicionado aos favoritos",
    status_removed_from_favorites: "Removido dos favoritos",
    status_favorite_error: "Erro de favorito",
    status_playlists_loaded: "playlists carregadas",
    status_playlists_error: "Erro das playlists",
    status_added_to_playlist: "Adicionado à playlist",
    status_add_to_playlist_error: "Erro ao adicionar à playlist",
    status_track_disliked: "Música marcada como não recomendada",
    status_dislike_error: "Erro de não recomendar",
    status_no_mix_tracks: "Nenhuma música do mix encontrada",
    status_mix_tracks: "Mix:",
    status_mix_error: "Erro do mix",
    status_album_error: "Erro do álbum",
    status_playlist_error: "Erro da playlist",
    status_radios_loaded: "rádios carregadas",
    status_radios_error: "Erro das rádios",
    status_no_radio_tracks: "Nenhuma música nesta rádio",
    status_radio_tracks: "Rádio:",
    status_radio_tracks_error: "Erro das músicas da rádio",
    status_playback_error: "Erro de reprodução",
    status_track_error: "Erro da música",
    status_play_next: "Será reproduzida em seguida",
    status_added_to_queue: "Adicionado à fila",

    header_title: "Título",
    header_artist: "Artista",
    header_album: "Álbum",
    header_duration: "Duração",
    header_fans: "Fãs",
    header_tracks: "Músicas",
    header_author: "Autor",
    header_description: "Descrição",
    header_profile: "Perfil",
    header_playlist: "Playlist",
    header_podcast: "Podcast",
    header_episode: "Episódio",

    cat_tracks: "Músicas",
    cat_artists: "Artistas",
    cat_albums: "Álbuns",
    cat_playlists: "Playlists",
    cat_podcasts: "Podcasts",
    cat_episodes: "Episódios",
    cat_profiles: "Perfis",

    cat_recently_played: "Ouvidos recentemente",
    cat_following: "Seguindo",
};

// ── German ───────────────────────────────────────────────────────────
static DE: Strings = Strings {
    tab_search: " Suche ",
    tab_favorites: " Favoriten ",
    tab_radios: " Radios ",
    tab_offline: " Offline ",

    login_connecting: "Verbindung...",
    login_button: "Mit Deezer anmelden",
    login_hint: "Enter: Anmelden | w: Mit ARL verbinden | Esc: Beenden",
    login_arl_title: " ARL-Token ",
    login_arl_placeholder: "ARL-Token aus den Browser-Cookies einfügen...",
    login_arl_hint: "Enter: Verbinden | Esc: Zurück",

    search_title_typing: " Suche (Enter zum Senden, Esc zum Abbrechen) ",
    search_title_normal: " Suche (/ drücken zum Tippen) ",
    search_placeholder: "/ drücken um Titel, Künstler, Alben zu suchen...",
    searching: "Suche...",
    no_results: "Keine Ergebnisse",
    results: "Ergebnisse",

    shuffle_favorites: "Favoriten zufällig abspielen",
    loading: "Laden...",
    no_favorites: "Noch keine Favoriten \u{2014} füge welche auf Deezer hinzu!",
    favorites: "Favoriten",

    no_track_loaded: "Kein Titel geladen",
    play_pause: "Play/Pause",
    next: "Nächster",
    prev: "Vorheriger",
    shuffle: "Zufällig",
    repeat: "Wiederholen",
    repeat_all: "Alle wiederholen",
    repeat_one: "Einen wiederholen",
    vol: "Lautst.",
    help: "Hilfe",

    menu_manage: "── Verwalten ──",
    menu_playback: "── Wiedergabe ──",
    menu_media: "── Medien ──",
    remove_from_favorites: "Aus Favoriten entfernen",
    add_to_favorites: "Zu Favoriten hinzufügen",
    add_to_playlist: "Zur Playlist hinzufügen",
    dont_recommend: "Diesen Titel nicht empfehlen",
    play_next: "Als Nächstes abspielen",
    add_to_queue: "Zur Warteschlange hinzufügen",
    mix_inspired: "Mix inspiriert von diesem Titel",
    track_album: "Album des Titels",
    share: "Teilen",
    track_info: "Titelinfo",

    info_title: "Titel:    ",
    info_artist: "Künstler: ",
    info_album: "Album:    ",
    info_duration: "Dauer:    ",
    info_track_id: "Titel-ID: ",
    press_esc_close: "Esc zum Schließen",

    add_to_playlist_title: "Zur Playlist hinzufügen",
    loading_playlists: "Playlists werden geladen...",
    no_playlists: "Keine Playlists gefunden",

    keyboard_shortcuts: " Tastenkürzel ",
    help_switch_tabs: "Tabs wechseln",
    help_search: "Suchen",
    help_play_submit: "Abspielen / Senden",
    help_settings_back: "Einstellungen / Zurück",
    help_navigate_list: "Liste navigieren",
    help_navigate_categories: "Kategorien navigieren",
    help_play_pause: "Abspielen / Pause",
    help_next_track: "Nächster Titel",
    help_prev_track: "Vorheriger Titel",
    help_toggle_shuffle: "Zufällig umschalten",
    help_cycle_repeat: "Wiederholungsmodus ändern",
    help_volume: "Lautstärke + / -",
    help_album_detail: "Albumdetailseite",
    help_waiting_list: "Warteschlange",
    help_context_menu: "Kontextmenü",
    help_playing_menu: "Menü laufender Titel",
    help_shuffle_favorites: "Favoriten zufällig",
    help_this_help: "Diese Hilfe",
    help_settings: "Einstellungen",
    help_quit: "Beenden",
    help_detach: "Trennen (weiterspielen)",
    help_info: "App-Informationen",

    settings: " Einstellungen ",
    settings_shortcuts: "Tastenkürzel",
    settings_themes: "Themen",
    settings_language: "Sprache",
    settings_logout: "Abmelden",

    about_title: " Über Deezer TUI ",
    about_version: "Version",
    about_architecture: "Architektur",
    about_author: "Autor",
    about_github: "GitHub",
    about_license: "Lizenz",

    themes: " Themen ",
    official_deezer_themes: "  Offizielle Deezer-Themen",

    loading_album: "Album wird geladen...",
    no_album_loaded: "Kein Album geladen",
    date_label: "Datum:   ",
    tracks_label: "Titel:   ",
    label_label: "Label:   ",
    esc_back: "Esc  Zurück",
    enter_play_track: "Enter  Titel abspielen",
    no_tracks: "Keine Titel",

    playlist: " Playlist ",

    waiting_list: "Warteschlange",
    queue_empty: "Die Warteschlange ist leer",

    hint_play: " abspielen  ",
    hint_menu: " Menü  ",
    hint_close: " schließen",
    hint_remove: " entfernen  ",
    hint_favorite: " Favorit  ",

    radios_loading: "Radios werden geladen...",
    radios_no_results: "Keine Radios gefunden",
    radios_filter_typing: " Filter (Enter zum Bestätigen, Esc zum Abbrechen) ",
    radios_filter_normal: " Filter (/ drücken zum Tippen) ",
    radios_filter_placeholder: "/ oder Ctrl+F drücken um Radios zu filtern...",
    radios_title: "Radios",
    header_radio: "Radio",

    offline_empty: "Keine Offline-Inhalte",
    download_for_offline: "Für Offline-Modus herunterladen",
    remove_offline: "Aus Offline entfernen",
    status_downloading_track: "Offline wird heruntergeladen...",
    status_track_saved_offline: "Titel offline gespeichert",
    status_album_saved_offline: "Album offline gespeichert",
    status_offline_download_error: "Offline-Download-Fehler",
    status_removed_offline: "Aus Offline entfernt",
    hint_download_album: " offline  ",
    hint_expand_collapse: "→ auf-/zuklappen",

    link_copied: "Link in die Zwischenablage kopiert!",
    no_album_info: "Keine Album-Info verfügbar",
    daemon_disconnected: "Daemon getrennt",
    detach_message: "deezer-tui: Musik läuft im Hintergrund weiter. Starte \"deezer-tui\" um den Player wiederherzustellen.",

    status_fetching_key: "Entschlüsselungsschlüssel wird abgerufen...",
    status_loading_mix: "Mix wird geladen...",
    status_loading_album: "Album wird geladen...",
    status_loading_playlist: "Playlist wird geladen...",
    status_loading_radio_tracks: "Radio-Titel werden geladen...",
    status_player_not_ready: "Player noch nicht bereit",
    status_connected_as: "Verbunden als",
    status_ready: "Bereit zur Wiedergabe",
    status_audio_init_error: "Audio-Initialisierungsfehler",
    status_key_error: "Schlüsselfehler",
    status_results: "Ergebnisse",
    status_loaded: "geladen",
    status_search_error: "Suchfehler",
    status_favorites_error: "Favoritenfehler",
    status_added_to_favorites: "Zu Favoriten hinzugefügt",
    status_removed_from_favorites: "Aus Favoriten entfernt",
    status_favorite_error: "Favoritenfehler",
    status_playlists_loaded: "Playlists geladen",
    status_playlists_error: "Playlist-Fehler",
    status_added_to_playlist: "Zur Playlist hinzugefügt",
    status_add_to_playlist_error: "Fehler beim Hinzufügen zur Playlist",
    status_track_disliked: "Titel als nicht empfohlen markiert",
    status_dislike_error: "Fehler beim Nicht-Empfehlen",
    status_no_mix_tracks: "Keine Mix-Titel gefunden",
    status_mix_tracks: "Mix:",
    status_mix_error: "Mix-Fehler",
    status_album_error: "Album-Fehler",
    status_playlist_error: "Playlist-Fehler",
    status_radios_loaded: "Radios geladen",
    status_radios_error: "Radio-Fehler",
    status_no_radio_tracks: "Keine Titel in diesem Radio",
    status_radio_tracks: "Radio:",
    status_radio_tracks_error: "Radio-Titel-Fehler",
    status_playback_error: "Wiedergabefehler",
    status_track_error: "Titelfehler",
    status_play_next: "Wird als Nächstes abgespielt",
    status_added_to_queue: "Zur Warteschlange hinzugefügt",

    header_title: "Titel",
    header_artist: "Künstler",
    header_album: "Album",
    header_duration: "Dauer",
    header_fans: "Fans",
    header_tracks: "Titel",
    header_author: "Autor",
    header_description: "Beschreibung",
    header_profile: "Profil",
    header_playlist: "Playlist",
    header_podcast: "Podcast",
    header_episode: "Episode",

    cat_tracks: "Titel",
    cat_artists: "Künstler",
    cat_albums: "Alben",
    cat_playlists: "Playlists",
    cat_podcasts: "Podcasts",
    cat_episodes: "Episoden",
    cat_profiles: "Profile",

    cat_recently_played: "Kürzlich gehört",
    cat_following: "Folge ich",
};

// ── Global accessor ──────────────────────────────────────────────────

thread_local! {
    static CURRENT_STRINGS: Cell<&'static Strings> = const { Cell::new(&EN) };
}

/// Get the current translation strings.
pub fn t() -> &'static Strings {
    CURRENT_STRINGS.with(|c| c.get())
}

/// Set the active locale.
pub fn set(locale: Locale) {
    let strings = match locale {
        Locale::English => &EN,
        Locale::French => &FR,
        Locale::Spanish => &ES,
        Locale::Portuguese => &PT,
        Locale::German => &DE,
    };
    CURRENT_STRINGS.with(|c| c.set(strings));
}

/// Get the current locale.
pub fn current_locale() -> Locale {
    let current = CURRENT_STRINGS.with(|c| c.get());
    let ptr = current as *const Strings;
    if ptr == &EN as *const Strings {
        Locale::English
    } else if ptr == &FR as *const Strings {
        Locale::French
    } else if ptr == &ES as *const Strings {
        Locale::Spanish
    } else if ptr == &PT as *const Strings {
        Locale::Portuguese
    } else {
        Locale::German
    }
}

/// Detect the system locale from environment variables.
/// Falls back to English if the language is not supported.
pub fn detect_locale() -> Locale {
    // Check in order: LANGUAGE, LC_ALL, LC_MESSAGES, LANG
    let lang_str = std::env::var("LANGUAGE")
        .ok()
        .and_then(|v| v.split(':').next().map(String::from))
        .or_else(|| std::env::var("LC_ALL").ok())
        .or_else(|| std::env::var("LC_MESSAGES").ok())
        .or_else(|| std::env::var("LANG").ok())
        .unwrap_or_default();

    // Parse: strip encoding (e.g., ".UTF-8"), take first 2 chars
    let code = lang_str.split('.').next().unwrap_or("");
    let lang = if code.len() >= 2 { &code[..2] } else { code };

    match lang {
        "fr" => Locale::French,
        "es" => Locale::Spanish,
        "pt" => Locale::Portuguese,
        "de" => Locale::German,
        _ => Locale::English,
    }
}
