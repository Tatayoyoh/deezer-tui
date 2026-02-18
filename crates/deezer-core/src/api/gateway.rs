use serde_json::json;
use tracing::debug;

use super::models::{
    AlbumData, ArtistData, DeezerError, DisplayItem, EpisodeData, PlaylistData, PodcastData,
    ProfileData, SearchResults, TrackData,
};
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

    /// Search and return results as DisplayItems for a specific category.
    pub async fn search_category(
        &self,
        query: &str,
        category: &str,
    ) -> Result<Vec<DisplayItem>, DeezerError> {
        let params = json!({
            "query": query,
            "filter": "ALL",
            "output": "TRACK",
            "start": 0,
            "nb": 40,
        });

        let results = self.gw_call("deezer.pageSearch", params).await?;

        match category {
            "TRACK" => {
                let section = results.get("TRACK");
                let tracks: Vec<TrackData> = parse_search_section(section)?;
                Ok(tracks.iter().map(DisplayItem::from_track).collect())
            }
            "ARTIST" => {
                let section = results.get("ARTIST");
                let artists: Vec<ArtistData> = parse_search_section(section)?;
                Ok(artists.iter().map(DisplayItem::from_artist).collect())
            }
            "ALBUM" => {
                let section = results.get("ALBUM");
                let albums: Vec<AlbumData> = parse_search_section(section)?;
                Ok(albums.iter().map(DisplayItem::from_album).collect())
            }
            "PLAYLIST" => {
                let section = results.get("PLAYLIST");
                let playlists: Vec<PlaylistData> = parse_search_section(section)?;
                Ok(playlists.iter().map(DisplayItem::from_playlist).collect())
            }
            "SHOW" => {
                let section = results.get("SHOW");
                let podcasts: Vec<PodcastData> = parse_search_section(section)?;
                Ok(podcasts.iter().map(DisplayItem::from_podcast).collect())
            }
            "EPISODE" => {
                let section = results.get("EPISODE");
                let episodes: Vec<EpisodeData> = parse_search_section(section)?;
                Ok(episodes.iter().map(DisplayItem::from_episode).collect())
            }
            "USER" => {
                let section = results.get("USER");
                let profiles: Vec<ProfileData> = parse_search_section(section)?;
                Ok(profiles.iter().map(DisplayItem::from_profile).collect())
            }
            _ => Ok(Vec::new()),
        }
    }

    /// Get favorite artists.
    pub async fn get_favorite_artists(&self) -> Result<Vec<DisplayItem>, DeezerError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| DeezerError::Auth("Not authenticated".into()))?;

        let params = json!({
            "user_id": session.user_id,
            "start": 0,
            "nb": 2000,
        });

        let results = self.gw_call("favorite_artist.getList", params).await?;
        let data = results
            .get("data")
            .ok_or_else(|| DeezerError::Api("Missing 'data' in favorite artists".into()))?;

        debug!("favorite_artist sample: {:?}", data.as_array().and_then(|a| a.first()));

        let artists: Vec<ArtistData> = serde_json::from_value(data.clone())
            .map_err(|e| DeezerError::Api(format!("Failed to parse favorite artists: {e}")))?;
        Ok(artists.iter().map(DisplayItem::from_artist).collect())
    }

    /// Get favorite albums.
    pub async fn get_favorite_albums(&self) -> Result<Vec<DisplayItem>, DeezerError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| DeezerError::Auth("Not authenticated".into()))?;

        let params = json!({
            "user_id": session.user_id,
            "start": 0,
            "nb": 2000,
        });

        let results = self.gw_call("favorite_album.getList", params).await?;
        let data = results
            .get("data")
            .ok_or_else(|| DeezerError::Api("Missing 'data' in favorite albums".into()))?;

        let albums: Vec<AlbumData> = serde_json::from_value(data.clone())
            .map_err(|e| DeezerError::Api(format!("Failed to parse favorite albums: {e}")))?;
        Ok(albums.iter().map(DisplayItem::from_album).collect())
    }

    /// Get user playlists.
    pub async fn get_playlists(&self) -> Result<Vec<DisplayItem>, DeezerError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| DeezerError::Auth("Not authenticated".into()))?;

        let params = json!({
            "user_id": session.user_id,
            "start": 0,
            "nb": 2000,
        });

        let results = self.gw_call("playlist.getList", params).await?;
        let data = results
            .get("data")
            .ok_or_else(|| DeezerError::Api("Missing 'data' in playlists".into()))?;

        let playlists: Vec<PlaylistData> = serde_json::from_value(data.clone())
            .map_err(|e| DeezerError::Api(format!("Failed to parse playlists: {e}")))?;
        Ok(playlists.iter().map(DisplayItem::from_playlist).collect())
    }

    /// Get listening history (recently played tracks).
    pub async fn get_listening_history(&self) -> Result<Vec<TrackData>, DeezerError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| DeezerError::Auth("Not authenticated".into()))?;

        // Use deezer.pageProfile to fetch the user's listening history
        let params = json!({
            "user_id": session.user_id,
            "tab": "listening_history",
            "nb": 200,
        });

        let results = self.gw_call("deezer.pageProfile", params).await;

        match results {
            Ok(res) => {
                // The response contains TAB.listening_history.data
                if let Some(tab) = res.get("TAB") {
                    if let Some(history) = tab.get("listening_history") {
                        if let Some(data) = history.get("data") {
                            let tracks: Result<Vec<TrackData>, _> =
                                serde_json::from_value(data.clone());
                            if let Ok(tracks) = tracks {
                                return Ok(tracks);
                            }
                        }
                    }
                }
                // Try alternate structure: direct data array
                if let Some(data) = res.get("data") {
                    let tracks: Result<Vec<TrackData>, _> =
                        serde_json::from_value(data.clone());
                    if let Ok(tracks) = tracks {
                        return Ok(tracks);
                    }
                }
                debug!("History response structure: {:?}", res.as_object().map(|o| o.keys().collect::<Vec<_>>()));
                // Fallback to favorites
                self.get_favorites().await
            }
            Err(e) => {
                debug!("History API error: {e}, falling back to favorites");
                self.get_favorites().await
            }
        }
    }

    /// Get followed users/profiles.
    pub async fn get_following(&self) -> Result<Vec<DisplayItem>, DeezerError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| DeezerError::Auth("Not authenticated".into()))?;

        let params = json!({
            "user_id": session.user_id,
            "start": 0,
            "nb": 2000,
        });

        let results = self.gw_call("user.getFollowings", params).await?;
        let data = results
            .get("data")
            .ok_or_else(|| DeezerError::Api("Missing 'data' in following".into()))?;

        let profiles: Vec<ProfileData> = serde_json::from_value(data.clone())
            .map_err(|e| DeezerError::Api(format!("Failed to parse following: {e}")))?;
        Ok(profiles.iter().map(DisplayItem::from_profile).collect())
    }

    /// Add a track to the user's favorites.
    pub async fn add_favorite(&self, song_id: &str) -> Result<(), DeezerError> {
        let params = json!({ "SNG_ID": song_id });
        self.gw_call("favorite_song.add", params).await?;
        Ok(())
    }

    /// Remove a track from the user's favorites.
    pub async fn remove_favorite(&self, song_id: &str) -> Result<(), DeezerError> {
        let params = json!({ "SNG_ID": song_id });
        self.gw_call("favorite_song.remove", params).await?;
        Ok(())
    }

    /// Add tracks to a playlist.
    pub async fn add_to_playlist(
        &self,
        playlist_id: &str,
        song_ids: &[&str],
    ) -> Result<(), DeezerError> {
        let songs: Vec<serde_json::Value> = song_ids
            .iter()
            .map(|id| json!([id, 0]))
            .collect();
        let params = json!({
            "playlist_id": playlist_id,
            "songs": songs,
        });
        self.gw_call("playlist.addSongs", params).await?;
        Ok(())
    }

    /// Mark a track as disliked (don't recommend).
    pub async fn dislike_track(&self, song_id: &str) -> Result<(), DeezerError> {
        let params = json!({ "SNG_ID": song_id });
        self.gw_call("song.dislike", params).await?;
        Ok(())
    }

    /// Get user playlists as raw PlaylistData (for playlist picker).
    pub async fn get_user_playlists_raw(&self) -> Result<Vec<PlaylistData>, DeezerError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| DeezerError::Auth("Not authenticated".into()))?;

        let params = json!({
            "user_id": session.user_id,
            "start": 0,
            "nb": 2000,
        });

        let results = self.gw_call("playlist.getList", params).await?;
        let data = results
            .get("data")
            .ok_or_else(|| DeezerError::Api("Missing 'data' in playlists".into()))?;

        serde_json::from_value(data.clone())
            .map_err(|e| DeezerError::Api(format!("Failed to parse playlists: {e}")))
    }

    /// Get a smart radio mix inspired by a track.
    pub async fn get_smart_radio(&self, song_id: &str) -> Result<Vec<TrackData>, DeezerError> {
        let params = json!({ "SNG_ID": song_id });
        let results = self.gw_call("song.getSearchTrackMix", params).await?;

        let data = results
            .get("data")
            .ok_or_else(|| DeezerError::Api("Missing 'data' in smart radio response".into()))?;

        serde_json::from_value(data.clone())
            .map_err(|e| DeezerError::Api(format!("Failed to parse smart radio: {e}")))
    }
}

/// Parse a search section's "data" array into a typed vec.
fn parse_search_section<T: serde::de::DeserializeOwned>(
    section: Option<&serde_json::Value>,
) -> Result<Vec<T>, DeezerError> {
    let section = section.ok_or_else(|| DeezerError::Api("Section not found in search results".into()))?;
    let data = section
        .get("data")
        .ok_or_else(|| DeezerError::Api("Missing 'data' in search section".into()))?;
    serde_json::from_value(data.clone())
        .map_err(|e| DeezerError::Api(format!("Failed to parse search section: {e}")))
}
