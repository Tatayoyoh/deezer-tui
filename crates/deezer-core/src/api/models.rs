use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum DeezerError {
    #[error("HTTP error: {0}")]
    Http(String),

    #[error("API error: {0}")]
    Api(String),

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Decryption error: {0}")]
    Decrypt(String),

    #[error("Playback error: {0}")]
    Playback(String),

    #[error("No track available in requested quality")]
    QualityUnavailable,

    #[error("Track not available: {0}")]
    TrackUnavailable(String),

    #[error("Track is already in this playlist")]
    TrackAlreadyInPlaylist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AudioQuality {
    Mp3_64,
    #[default]
    Mp3_128,
    Mp3_320,
    Flac,
}

impl AudioQuality {
    pub fn as_api_format(&self) -> &'static str {
        match self {
            Self::Mp3_64 => "MP3_64",
            Self::Mp3_128 => "MP3_128",
            Self::Mp3_320 => "MP3_320",
            Self::Flac => "FLAC",
        }
    }

    pub fn fallback(&self) -> Option<Self> {
        match self {
            Self::Flac => Some(Self::Mp3_320),
            Self::Mp3_320 => Some(Self::Mp3_128),
            Self::Mp3_128 => Some(Self::Mp3_64),
            Self::Mp3_64 => None,
        }
    }

    /// Returns all qualities to try, starting from `self`, going down, then up.
    /// Example for Mp3_128: [Mp3_128, Mp3_64, Mp3_320, Flac]
    pub fn all_fallbacks(&self) -> Vec<Self> {
        let all_desc = [Self::Flac, Self::Mp3_320, Self::Mp3_128, Self::Mp3_64];
        let mut result = Vec::with_capacity(4);
        // First: self and below (preferred direction)
        let mut q = Some(*self);
        while let Some(quality) = q {
            result.push(quality);
            q = quality.fallback();
        }
        // Then: above self (try higher if lower fails)
        for &quality in &all_desc {
            if quality == *self {
                break;
            }
            if !result.contains(&quality) {
                result.push(quality);
            }
        }
        result
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserData {
    #[serde(rename = "USER")]
    pub user: UserInfo,
    #[serde(rename = "checkForm")]
    pub api_token: String,
    #[serde(rename = "OFFER")]
    #[serde(default)]
    pub offer: Option<UserOffer>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserInfo {
    #[serde(rename = "USER_ID")]
    #[serde(deserialize_with = "deserialize_string_or_number")]
    pub user_id: u64,
    #[serde(rename = "BLOG_NAME")]
    #[serde(default)]
    pub user_name: String,
    #[serde(rename = "OPTIONS")]
    pub options: UserOptions,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserOptions {
    pub license_token: String,
    #[serde(rename = "web_streaming")]
    #[serde(default)]
    pub web_streaming: bool,
    #[serde(rename = "web_hq")]
    #[serde(default)]
    pub web_hq: bool,
    #[serde(rename = "web_lossless")]
    #[serde(default)]
    pub web_lossless: bool,
    #[serde(rename = "mobile_offlinestreaming")]
    #[serde(default)]
    pub mobile_offline: bool,
    #[serde(rename = "license_country")]
    #[serde(default)]
    pub license_country: String,
    #[serde(rename = "expiration_timestamp")]
    #[serde(default)]
    pub expiration_timestamp: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserOffer {
    #[serde(rename = "OFFER_NAME")]
    #[serde(default)]
    pub offer_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackData {
    #[serde(rename = "SNG_ID")]
    pub track_id: String,
    #[serde(rename = "SNG_TITLE")]
    pub title: String,
    #[serde(rename = "ART_NAME")]
    pub artist: String,
    #[serde(rename = "ART_ID")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_optional_id")]
    pub artist_id: Option<String>,
    #[serde(rename = "ALB_TITLE")]
    #[serde(default)]
    pub album: String,
    #[serde(rename = "DURATION")]
    #[serde(default)]
    pub duration: String,
    #[serde(rename = "ALB_PICTURE")]
    #[serde(default)]
    pub album_picture: String,
    #[serde(rename = "ALB_ID")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_optional_id")]
    pub album_id: Option<String>,
    #[serde(rename = "TRACK_TOKEN")]
    #[serde(default)]
    pub track_token: Option<String>,
    #[serde(rename = "MD5_ORIGIN")]
    #[serde(default)]
    pub md5_origin: String,
    #[serde(rename = "FALLBACK")]
    #[serde(default)]
    pub fallback: Option<TrackFallback>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackFallback {
    #[serde(rename = "SNG_ID")]
    pub track_id: String,
}

impl TrackData {
    pub fn duration_secs(&self) -> u64 {
        self.duration.parse().unwrap_or(0)
    }

    pub fn has_track_token(&self) -> bool {
        self.track_token.as_ref().is_some_and(|t| !t.is_empty())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MediaUrl {
    pub sources: Vec<MediaSource>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MediaSource {
    pub url: String,
    pub provider: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchResults {
    pub data: Vec<TrackData>,
    #[serde(default)]
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtistData {
    #[serde(rename = "ART_ID")]
    #[serde(deserialize_with = "deserialize_id_to_string")]
    pub artist_id: String,
    #[serde(rename = "ART_NAME")]
    #[serde(default)]
    pub name: String,
    #[serde(rename = "NB_FAN")]
    #[serde(default)]
    pub nb_fan: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumData {
    #[serde(rename = "ALB_ID")]
    #[serde(deserialize_with = "deserialize_id_to_string")]
    pub album_id: String,
    #[serde(rename = "ALB_TITLE")]
    #[serde(default)]
    pub title: String,
    #[serde(rename = "ART_NAME")]
    #[serde(default)]
    pub artist: String,
    #[serde(rename = "NUMBER_TRACK", alias = "NB_SONG")]
    #[serde(default)]
    pub nb_tracks: u64,
    #[serde(rename = "PHYSICAL_RELEASE_DATE", alias = "DIGITAL_RELEASE_DATE")]
    #[serde(default)]
    pub release_date: String,
    #[serde(rename = "NB_FAN")]
    #[serde(default)]
    pub nb_fan: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistData {
    #[serde(rename = "PLAYLIST_ID")]
    #[serde(deserialize_with = "deserialize_id_to_string")]
    pub playlist_id: String,
    #[serde(rename = "TITLE")]
    #[serde(default)]
    pub title: String,
    #[serde(rename = "NB_SONG")]
    #[serde(default)]
    pub nb_songs: u64,
    #[serde(rename = "PARENT_USERNAME")]
    #[serde(default)]
    pub author: String,
    /// True if playlist is collaborative (writable by users other than the owner).
    /// Synthesized in `get_user_playlists_raw` from the raw STATUS field; not
    /// deserialized from the API directly.
    #[serde(default)]
    pub collaborative: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodcastData {
    #[serde(rename = "SHOW_ID")]
    #[serde(deserialize_with = "deserialize_id_to_string")]
    pub show_id: String,
    #[serde(rename = "SHOW_NAME")]
    #[serde(default)]
    pub name: String,
    #[serde(rename = "SHOW_DESCRIPTION")]
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeData {
    #[serde(rename = "EPISODE_ID")]
    #[serde(deserialize_with = "deserialize_id_to_string")]
    pub episode_id: String,
    #[serde(rename = "EPISODE_TITLE")]
    #[serde(default)]
    pub title: String,
    #[serde(rename = "SHOW_NAME")]
    #[serde(default)]
    pub show_name: String,
    #[serde(rename = "DURATION")]
    #[serde(default)]
    pub duration: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileData {
    #[serde(rename = "USER_ID")]
    #[serde(deserialize_with = "deserialize_string_or_number")]
    pub user_id: u64,
    #[serde(rename = "BLOG_NAME")]
    #[serde(default)]
    pub name: String,
}

/// A radio station from the Deezer public API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadioData {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub description: String,
}

/// Full album detail returned from the public API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumDetail {
    pub album_id: String,
    pub title: String,
    pub artist: String,
    pub nb_tracks: u64,
    pub release_date: String,
    pub cover_url: String,
    pub label: String,
    pub tracks: Vec<TrackData>,
}

/// An album entry within an artist detail page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtistAlbumEntry {
    pub album_id: String,
    pub title: String,
    pub release_date: String,
    pub fans: u64,
    pub record_type: String,
    #[serde(default)]
    pub cover_url: String,
}

/// A similar artist entry within an artist detail page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarArtistEntry {
    pub artist_id: String,
    pub name: String,
    pub nb_fan: u64,
}

/// Sub-tab category for the artist detail right panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ArtistSubTab {
    #[default]
    TopTracks,
    Albums,
    Lives,
    Other,
    Similar,
}

impl ArtistSubTab {
    pub const ALL: [Self; 5] = [
        Self::TopTracks,
        Self::Albums,
        Self::Lives,
        Self::Other,
        Self::Similar,
    ];

    pub fn next(&self) -> Self {
        let all = Self::ALL;
        let idx = all.iter().position(|c| c == self).unwrap_or(0);
        all[(idx + 1) % all.len()]
    }

    pub fn prev(&self) -> Self {
        let all = Self::ALL;
        let idx = all.iter().position(|c| c == self).unwrap_or(0);
        all[(idx + all.len() - 1) % all.len()]
    }
}

/// Full artist detail returned from the public API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtistDetail {
    pub artist_id: String,
    pub name: String,
    pub nb_fan: u64,
    pub picture_url: String,
    pub top_tracks: Vec<TrackData>,
    pub albums: Vec<ArtistAlbumEntry>,
    #[serde(default)]
    pub similar_artists: Vec<SimilarArtistEntry>,
}

impl ArtistDetail {
    /// Filter albums by sub-tab category.
    /// Lives are detected by title keywords since the API doesn't have a "live" record_type.
    pub fn albums_for_tab(&self, tab: ArtistSubTab) -> Vec<&ArtistAlbumEntry> {
        self.albums
            .iter()
            .filter(|a| match tab {
                ArtistSubTab::Albums => a.record_type == "album" && !a.is_live(),
                ArtistSubTab::Lives => a.is_live(),
                ArtistSubTab::Other => a.record_type != "album" && !a.is_live(),
                ArtistSubTab::TopTracks | ArtistSubTab::Similar => false,
            })
            .collect()
    }
}

impl ArtistAlbumEntry {
    /// Detect live albums by title keywords.
    pub fn is_live(&self) -> bool {
        let lower = self.title.to_lowercase();
        lower.contains("live") || lower.contains("concert") || lower.contains("en direct")
    }
}

/// Full playlist detail returned from the public API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistDetail {
    pub playlist_id: String,
    pub title: String,
    pub creator: String,
    pub nb_tracks: u64,
    pub tracks: Vec<TrackData>,
}

/// A unified display item for rendering in tables.
/// Adapts different Deezer data types into a common 4-column format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayItem {
    pub col1: String,
    pub col2: String,
    pub col3: String,
    pub col4: String,
    /// Original track data, if this item is playable.
    pub track: Option<TrackData>,
    /// Album ID, if this item represents or belongs to an album.
    #[serde(default)]
    pub album_id: Option<String>,
    /// Playlist ID, if this item represents a playlist.
    #[serde(default)]
    pub playlist_id: Option<String>,
    /// Artist ID, if this item represents an artist.
    #[serde(default)]
    pub artist_id: Option<String>,
}

impl DisplayItem {
    pub fn from_track(track: &TrackData) -> Self {
        let dur = track.duration_secs();
        Self {
            col1: track.title.clone(),
            col2: track.artist.clone(),
            col3: track.album.clone(),
            col4: format!("{}:{:02}", dur / 60, dur % 60),
            track: Some(track.clone()),
            album_id: None,
            playlist_id: None,
            artist_id: None,
        }
    }

    pub fn from_artist(artist: &ArtistData) -> Self {
        Self {
            col1: artist.name.clone(),
            col2: format!("{} fans", format_number(artist.nb_fan)),
            col3: String::new(),
            col4: String::new(),
            track: None,
            album_id: None,
            playlist_id: None,
            artist_id: Some(artist.artist_id.clone()),
        }
    }

    pub fn from_album(album: &AlbumData) -> Self {
        Self {
            col1: album.title.clone(),
            col2: album.artist.clone(),
            col3: album.release_date.clone(),
            col4: format!("{} titres", album.nb_tracks),
            track: None,
            album_id: Some(album.album_id.clone()),
            playlist_id: None,
            artist_id: None,
        }
    }

    pub fn from_playlist(playlist: &PlaylistData) -> Self {
        Self {
            col1: playlist.title.clone(),
            col2: playlist.author.clone(),
            col3: format!("{} titres", playlist.nb_songs),
            col4: String::new(),
            track: None,
            album_id: None,
            playlist_id: Some(playlist.playlist_id.clone()),
            artist_id: None,
        }
    }

    pub fn from_podcast(podcast: &PodcastData) -> Self {
        Self {
            col1: podcast.name.clone(),
            col2: podcast.description.clone(),
            col3: String::new(),
            col4: String::new(),
            track: None,
            album_id: None,
            playlist_id: None,
            artist_id: None,
        }
    }

    pub fn from_episode(episode: &EpisodeData) -> Self {
        let dur: u64 = episode.duration.parse().unwrap_or(0);
        Self {
            col1: episode.title.clone(),
            col2: episode.show_name.clone(),
            col3: String::new(),
            col4: format!("{}:{:02}", dur / 60, dur % 60),
            track: None,
            album_id: None,
            playlist_id: None,
            artist_id: None,
        }
    }

    pub fn from_profile(profile: &ProfileData) -> Self {
        Self {
            col1: profile.name.clone(),
            col2: String::new(),
            col3: String::new(),
            col4: String::new(),
            track: None,
            album_id: None,
            playlist_id: None,
            artist_id: None,
        }
    }
}

fn format_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Deezer sends numeric IDs as strings (e.g. "123456") or as integers (e.g. 0 for invalid).
fn deserialize_string_or_number<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;

    struct StringOrNumber;

    impl<'de> de::Visitor<'de> for StringOrNumber {
        type Value = u64;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a string or number")
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<u64, E> {
            Ok(v)
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<u64, E> {
            Ok(v as u64)
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<u64, E> {
            v.parse().map_err(de::Error::custom)
        }
    }

    deserializer.deserialize_any(StringOrNumber)
}

/// Deezer sends IDs as either strings or numbers, or may be missing. Returns Option<String>.
fn deserialize_optional_id<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;

    struct OptionalId;

    impl<'de> de::Visitor<'de> for OptionalId {
        type Value = Option<String>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a string, number, or null")
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Option<String>, E> {
            if v == 0 {
                Ok(None)
            } else {
                Ok(Some(v.to_string()))
            }
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Option<String>, E> {
            if v <= 0 {
                Ok(None)
            } else {
                Ok(Some(v.to_string()))
            }
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Option<String>, E> {
            if v.is_empty() || v == "0" {
                Ok(None)
            } else {
                Ok(Some(v.to_string()))
            }
        }

        fn visit_none<E: de::Error>(self) -> Result<Option<String>, E> {
            Ok(None)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Option<String>, E> {
            Ok(None)
        }
    }

    deserializer.deserialize_any(OptionalId)
}

/// Deezer sends IDs as either strings or numbers. This always returns a String.
fn deserialize_id_to_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;

    struct IdToString;

    impl<'de> de::Visitor<'de> for IdToString {
        type Value = String;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a string or number")
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<String, E> {
            Ok(v.to_string())
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<String, E> {
            Ok(v.to_string())
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<String, E> {
            Ok(v.to_string())
        }
    }

    deserializer.deserialize_any(IdToString)
}
