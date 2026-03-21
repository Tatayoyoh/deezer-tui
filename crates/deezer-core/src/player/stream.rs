use tracing::{debug, info};

use crate::api::models::{AudioQuality, DeezerError, TrackData};
use crate::api::DeezerClient;
use crate::decrypt;

/// Create a shared HTTP client suitable for CDN downloads (no cookies needed).
pub fn new_cdn_client() -> Result<reqwest::Client, DeezerError> {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| DeezerError::Http(e.to_string()))
}

/// Download and decrypt a full track, returning raw audio bytes (MP3 or FLAC).
pub async fn fetch_track(
    client: &DeezerClient,
    track: &TrackData,
    quality: AudioQuality,
    master_key: &[u8; 16],
) -> Result<(Vec<u8>, AudioQuality), DeezerError> {
    // Get streaming URL (with quality fallback)
    let (url, actual_quality) = client.get_stream_url(track, quality).await?;

    let data = download_and_decrypt(&url, &track.track_id, master_key, client.http()).await?;

    Ok((data, actual_quality))
}

/// Download encrypted audio from a CDN URL and decrypt it.
/// This function does NOT need a `DeezerClient` reference, so it can run
/// without holding the client lock.
/// Pass a shared `reqwest::Client` to reuse connections across downloads.
pub async fn download_and_decrypt(
    url: &str,
    track_id: &str,
    master_key: &[u8; 16],
    http: &reqwest::Client,
) -> Result<Vec<u8>, DeezerError> {
    info!(track_id = %track_id, "Downloading track");

    let resp = http
        .get(url)
        .send()
        .await
        .map_err(|e| DeezerError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(DeezerError::Http(format!(
            "CDN returned status {}",
            resp.status()
        )));
    }

    let mut data = resp
        .bytes()
        .await
        .map_err(|e| DeezerError::Http(e.to_string()))?
        .to_vec();

    debug!(
        bytes = data.len(),
        track_id = %track_id,
        "Downloaded encrypted audio"
    );

    // Derive per-track key and decrypt
    let track_key = decrypt::derive_track_key(track_id, master_key);
    decrypt::decrypt_stream(&mut data, &track_key)?;

    debug!(track_id = %track_id, "Decrypted audio");

    Ok(data)
}
