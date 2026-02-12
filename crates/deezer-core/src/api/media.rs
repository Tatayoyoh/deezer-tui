use serde_json::json;
use tracing::debug;

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

        // Try requested quality, then fall back
        let mut current_quality = Some(quality);

        while let Some(q) = current_quality {
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

            // Check for errors
            if let Some(errors) = body
                .get("data")
                .and_then(|d| d.get(0))
                .and_then(|d| d.get("errors"))
            {
                debug!(quality = q.as_api_format(), errors = %errors, "Quality unavailable, trying fallback");
            }

            current_quality = q.fallback();
        }

        Err(DeezerError::QualityUnavailable)
    }
}
