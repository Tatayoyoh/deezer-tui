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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioQuality {
    Mp3_64,
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
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserData {
    #[serde(rename = "USER")]
    pub user: UserInfo,
    #[serde(rename = "checkForm")]
    pub api_token: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackData {
    #[serde(rename = "SNG_ID")]
    pub track_id: String,
    #[serde(rename = "SNG_TITLE")]
    pub title: String,
    #[serde(rename = "ART_NAME")]
    pub artist: String,
    #[serde(rename = "ALB_TITLE")]
    #[serde(default)]
    pub album: String,
    #[serde(rename = "DURATION")]
    #[serde(default)]
    pub duration: String,
    #[serde(rename = "ALB_PICTURE")]
    #[serde(default)]
    pub album_picture: String,
    #[serde(rename = "TRACK_TOKEN")]
    #[serde(default)]
    pub track_token: Option<String>,
    #[serde(rename = "MD5_ORIGIN")]
    #[serde(default)]
    pub md5_origin: String,
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

#[derive(Debug, Clone, Deserialize)]
pub struct PlaylistData {
    #[serde(rename = "PLAYLIST_ID")]
    pub playlist_id: String,
    #[serde(rename = "TITLE")]
    pub title: String,
    #[serde(rename = "NB_SONG")]
    #[serde(default)]
    pub nb_songs: u64,
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
