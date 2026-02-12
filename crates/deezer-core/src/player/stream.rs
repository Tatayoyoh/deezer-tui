use tracing::{debug, info};

use crate::api::models::{AudioQuality, DeezerError, TrackData};
use crate::api::DeezerClient;
use crate::decrypt;

/// Download and decrypt a full track, returning raw audio bytes (MP3 or FLAC).
pub async fn fetch_track(
    client: &DeezerClient,
    track: &TrackData,
    quality: AudioQuality,
    master_key: &[u8; 16],
) -> Result<(Vec<u8>, AudioQuality), DeezerError> {
    // Get streaming URL (with quality fallback)
    let (url, actual_quality) = client.get_stream_url(track, quality).await?;

    info!(
        track_id = %track.track_id,
        title = %track.title,
        quality = actual_quality.as_api_format(),
        "Fetching track"
    );

    // Download the encrypted stream
    let http = reqwest::Client::new();
    let resp = http
        .get(&url)
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
        track_id = %track.track_id,
        "Downloaded encrypted audio"
    );

    // Derive per-track key and decrypt
    let track_key = decrypt::derive_track_key(&track.track_id, master_key);
    decrypt::decrypt_stream(&mut data, &track_key)?;

    debug!(track_id = %track.track_id, "Decrypted audio");

    Ok((data, actual_quality))
}
