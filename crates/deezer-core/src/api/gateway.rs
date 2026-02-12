use serde_json::json;
use tracing::debug;

use super::models::{DeezerError, SearchResults, TrackData};
use super::DeezerClient;

const GW_LIGHT_URL: &str = "https://www.deezer.com/ajax/gw-light.php";

impl DeezerClient {
    fn api_token(&self) -> Result<&str, DeezerError> {
        self.session
            .as_ref()
            .map(|s| s.api_token.as_str())
            .ok_or_else(|| DeezerError::Auth("Not authenticated".into()))
    }

    /// Call a gateway API method.
    async fn gw_call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, DeezerError> {
        let api_token = self.api_token()?;
        let url = format!(
            "{GW_LIGHT_URL}?method={method}&input=3&api_version=1.0&api_token={api_token}"
        );

        debug!(method, "Gateway API call");

        let resp = self
            .http
            .post(&url)
            .json(&params)
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        // Check for API-level errors
        if let Some(error) = body.get("error") {
            if !error.as_object().map_or(true, |o| o.is_empty()) {
                return Err(DeezerError::Api(error.to_string()));
            }
        }

        body.get("results")
            .cloned()
            .ok_or_else(|| DeezerError::Api("Missing 'results' in response".into()))
    }

    /// Get full track data by song ID (includes TRACK_TOKEN and MD5_ORIGIN).
    pub async fn get_track(&self, song_id: &str) -> Result<TrackData, DeezerError> {
        let params = json!({ "sng_id": song_id });
        let results = self.gw_call("song.getData", params).await?;

        serde_json::from_value(results)
            .map_err(|e| DeezerError::Api(format!("Failed to parse track data: {e}")))
    }

    /// Get multiple tracks at once (includes TRACK_TOKEN and MD5_ORIGIN).
    pub async fn get_tracks(&self, song_ids: &[&str]) -> Result<Vec<TrackData>, DeezerError> {
        let sng_ids: Vec<serde_json::Value> = song_ids
            .iter()
            .filter_map(|id| id.parse::<u64>().ok().map(|n| json!(n)))
            .collect();

        let params = json!({ "sng_ids": sng_ids });
        let results = self.gw_call("song.getListData", params).await?;

        let data = results
            .get("data")
            .ok_or_else(|| DeezerError::Api("Missing 'data' in track list response".into()))?;

        serde_json::from_value(data.clone())
            .map_err(|e| DeezerError::Api(format!("Failed to parse track list: {e}")))
    }

    /// Ensure a TrackData has a TRACK_TOKEN (fetch from song.getData if missing).
    pub async fn ensure_track_token(&self, track: &TrackData) -> Result<TrackData, DeezerError> {
        if track.has_track_token() {
            Ok(track.clone())
        } else {
            debug!(track_id = %track.track_id, "Fetching full track data (missing TRACK_TOKEN)");
            self.get_track(&track.track_id).await
        }
    }

    /// Search for tracks.
    pub async fn search(&self, query: &str) -> Result<SearchResults, DeezerError> {
        let params = json!({
            "query": query,
            "filter": "ALL",
            "output": "TRACK",
            "start": 0,
            "nb": 40,
        });

        let results = self.gw_call("deezer.pageSearch", params).await?;

        let track_results = results
            .get("TRACK")
            .ok_or_else(|| DeezerError::Api("Missing 'TRACK' in search results".into()))?;

        serde_json::from_value(track_results.clone())
            .map_err(|e| DeezerError::Api(format!("Failed to parse search results: {e}")))
    }

    /// Get the user's favorite (loved) tracks.
    pub async fn get_favorites(&self) -> Result<Vec<TrackData>, DeezerError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| DeezerError::Auth("Not authenticated".into()))?;

        let params = json!({
            "user_id": session.user_id,
            "start": 0,
            "nb": 2000,
        });

        let results = self.gw_call("favorite_song.getList", params).await?;

        let data = results
            .get("data")
            .ok_or_else(|| DeezerError::Api("Missing 'data' in favorites response".into()))?;

        serde_json::from_value(data.clone())
            .map_err(|e| DeezerError::Api(format!("Failed to parse favorites: {e}")))
    }
}
