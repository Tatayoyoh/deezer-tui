use serde_json::json;
use tracing::{debug, warn};

use super::models::{AudioQuality, DeezerError, TrackData};
use super::DeezerClient;

const MEDIA_URL: &str = "https://media.deezer.com/v1/get_url";

impl DeezerClient {
    /// Get a streaming URL for a track at the requested quality.
    /// Falls back to lower qualities if the requested one is unavailable.
    pub async fn get_stream_url(
        &self,
        track: &TrackData,
        quality: AudioQuality,
    ) -> Result<(String, AudioQuality), DeezerError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| DeezerError::Auth("Not authenticated".into()))?;

        let track_token = track
            .track_token
            .as_deref()
            .ok_or_else(|| DeezerError::TrackUnavailable("Missing TRACK_TOKEN".into()))?;

        debug!(
            track_id = %track.track_id,
            token_len = track_token.len(),
            token_prefix = &track_token[..track_token.len().min(20)],
            "get_stream_url: track_token info"
        );

        // Try requested quality, then fall back to lower, then try higher
        let qualities_to_try = quality.all_fallbacks();

        for q in qualities_to_try {
            debug!(quality = q.as_api_format(), track_id = %track.track_id, "Requesting stream URL");

            let payload = json!({
                "license_token": session.license_token,
                "media": [{
                    "type": "FULL",
                    "formats": [{
                        "cipher": "BF_CBC_STRIPE",
                        "format": q.as_api_format(),
                    }]
                }],
                "track_tokens": [track_token],
            });

            let resp = self
                .http
                .post(MEDIA_URL)
                .json(&payload)
                .send()
                .await
                .map_err(|e| DeezerError::Http(e.to_string()))?;

            let body: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| DeezerError::Http(e.to_string()))?;

            // Extract the URL from the response
            if let Some(url) = body
                .get("data")
                .and_then(|d| d.get(0))
                .and_then(|d| d.get("media"))
                .and_then(|m| m.get(0))
                .and_then(|m| m.get("sources"))
                .and_then(|s| s.get(0))
                .and_then(|s| s.get("url"))
                .and_then(|u| u.as_str())
            {
                debug!(quality = q.as_api_format(), "Got stream URL");
                return Ok((url.to_string(), q));
            }

            // Log the full response for debugging
            warn!(
                quality = q.as_api_format(),
                track_id = %track.track_id,
                response = %body,
                "get_stream_url: no streaming URL in response"
            );
        }

        Err(DeezerError::QualityUnavailable)
    }
}
