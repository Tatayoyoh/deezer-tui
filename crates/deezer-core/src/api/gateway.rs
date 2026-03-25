use serde_json::json;
use tracing::debug;

use super::models::{
    AlbumDetail, ArtistAlbumEntry, ArtistData, ArtistDetail, DeezerError, DisplayItem, EpisodeData,
    PlaylistData, PlaylistDetail, PodcastData, ProfileData, SearchResults, TrackData,
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
        let url =
            format!("{GW_LIGHT_URL}?method={method}&input=3&api_version=1.0&api_token={api_token}");

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

        let track: TrackData = serde_json::from_value(results)
            .map_err(|e| DeezerError::Api(format!("Failed to parse track data: {e}")))?;

        if let Some(ref fb) = track.fallback {
            debug!(track_id = %track.track_id, fallback_id = %fb.track_id, "Track has FALLBACK");
        }

        Ok(track)
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
        // Use the public API for album search (richer data: nested artist, nb_tracks).
        if category == "ALBUM" {
            return self.search_albums_public(query).await;
        }

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

    /// Search albums via the Deezer public API (returns richer data than gw-light).
    async fn search_albums_public(&self, query: &str) -> Result<Vec<DisplayItem>, DeezerError> {
        let resp: serde_json::Value = self
            .http
            .get("https://api.deezer.com/search/album")
            .query(&[("q", query), ("limit", "40")])
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        let data = resp
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| DeezerError::Api("Missing 'data' in album search".into()))?;

        let items = data
            .iter()
            .map(|entry| {
                let album_id = entry
                    .get("id")
                    .and_then(|v| v.as_u64())
                    .map(|v| v.to_string());
                let title = entry.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let artist = entry
                    .get("artist")
                    .and_then(|a| a.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let nb_tracks = entry.get("nb_tracks").and_then(|v| v.as_u64()).unwrap_or(0);

                DisplayItem {
                    col1: title.to_string(),
                    col2: artist.to_string(),
                    col3: String::new(),
                    col4: format!("{} titres", nb_tracks),
                    track: None,
                    album_id,
                    playlist_id: None,
                    artist_id: None,
                }
            })
            .collect();

        Ok(items)
    }

    /// Get favorite artists via the public API.
    pub async fn get_favorite_artists(&self) -> Result<Vec<DisplayItem>, DeezerError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| DeezerError::Auth("Not authenticated".into()))?;

        let url = format!(
            "https://api.deezer.com/user/{}/artists?limit=2000",
            session.user_id
        );
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        let data = resp
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| DeezerError::Api("Missing 'data' in favorite artists".into()))?;

        debug!("favorite_artists: {} items", data.len());

        let items = data
            .iter()
            .map(|entry| {
                let artist_id = entry
                    .get("id")
                    .and_then(|v| v.as_u64())
                    .map(|v| v.to_string());
                let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let nb_fan = entry.get("nb_fan").and_then(|v| v.as_u64()).unwrap_or(0);
                DisplayItem {
                    col1: name.to_string(),
                    col2: format_fans(nb_fan),
                    col3: String::new(),
                    col4: String::new(),
                    track: None,
                    album_id: None,
                    playlist_id: None,
                    artist_id,
                }
            })
            .collect();

        Ok(items)
    }

    /// Get favorite albums via the public API.
    pub async fn get_favorite_albums(&self) -> Result<Vec<DisplayItem>, DeezerError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| DeezerError::Auth("Not authenticated".into()))?;

        let url = format!(
            "https://api.deezer.com/user/{}/albums?limit=2000",
            session.user_id
        );
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        let data = resp
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| DeezerError::Api("Missing 'data' in favorite albums".into()))?;

        debug!("favorite_albums: {} items", data.len());

        let items = data
            .iter()
            .map(|entry| {
                let album_id = entry
                    .get("id")
                    .and_then(|v| v.as_u64())
                    .map(|v| v.to_string());
                let title = entry.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let artist = entry
                    .get("artist")
                    .and_then(|a| a.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let nb_tracks = entry.get("nb_tracks").and_then(|v| v.as_u64()).unwrap_or(0);
                let release_date = entry
                    .get("release_date")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                DisplayItem {
                    col1: title.to_string(),
                    col2: artist.to_string(),
                    col3: release_date.to_string(),
                    col4: format!("{} titres", nb_tracks),
                    track: None,
                    album_id,
                    playlist_id: None,
                    artist_id: None,
                }
            })
            .collect();

        Ok(items)
    }

    /// Get user playlists.
    /// Get user playlists via the public API.
    pub async fn get_playlists(&self) -> Result<Vec<DisplayItem>, DeezerError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| DeezerError::Auth("Not authenticated".into()))?;

        let url = format!(
            "https://api.deezer.com/user/{}/playlists?limit=2000",
            session.user_id
        );
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        let data = resp
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| DeezerError::Api("Missing 'data' in playlists".into()))?;

        debug!("favorite_playlists: {} items", data.len());

        let items = data
            .iter()
            .map(|entry| {
                let playlist_id = entry
                    .get("id")
                    .and_then(|v| v.as_u64())
                    .map(|v| v.to_string());
                let title = entry.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let nb_tracks = entry.get("nb_tracks").and_then(|v| v.as_u64()).unwrap_or(0);
                let author = entry
                    .get("creator")
                    .and_then(|c| c.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                DisplayItem {
                    col1: title.to_string(),
                    col2: author.to_string(),
                    col3: format!("{} titres", nb_tracks),
                    col4: String::new(),
                    track: None,
                    album_id: None,
                    playlist_id,
                    artist_id: None,
                }
            })
            .collect();

        Ok(items)
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
                    let tracks: Result<Vec<TrackData>, _> = serde_json::from_value(data.clone());
                    if let Ok(tracks) = tracks {
                        return Ok(tracks);
                    }
                }
                debug!(
                    "History response structure: {:?}",
                    res.as_object().map(|o| o.keys().collect::<Vec<_>>())
                );
                // Fallback to favorites
                self.get_favorites().await
            }
            Err(e) => {
                debug!("History API error: {e}, falling back to favorites");
                self.get_favorites().await
            }
        }
    }

    /// Get followed users/profiles via the public API.
    pub async fn get_following(&self) -> Result<Vec<DisplayItem>, DeezerError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| DeezerError::Auth("Not authenticated".into()))?;

        let url = format!(
            "https://api.deezer.com/user/{}/followings?limit=2000",
            session.user_id
        );
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        let data = resp
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| DeezerError::Api("Missing 'data' in following".into()))?;

        debug!("following: {} items", data.len());

        let items = data
            .iter()
            .map(|entry| {
                let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("");
                DisplayItem {
                    col1: name.to_string(),
                    col2: String::new(),
                    col3: String::new(),
                    col4: String::new(),
                    track: None,
                    album_id: None,
                    playlist_id: None,
                    artist_id: None,
                }
            })
            .collect();

        Ok(items)
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

    /// Add an artist to the user's favorites.
    pub async fn add_favorite_artist(&self, artist_id: &str) -> Result<(), DeezerError> {
        let params = json!({ "ART_ID": artist_id });
        self.gw_call("favorite_artist.add", params).await?;
        Ok(())
    }

    /// Remove an artist from the user's favorites.
    pub async fn remove_favorite_artist(&self, artist_id: &str) -> Result<(), DeezerError> {
        let params = json!({ "ART_ID": artist_id });
        self.gw_call("favorite_artist.remove", params).await?;
        Ok(())
    }

    /// Add an album to the user's favorites.
    pub async fn add_favorite_album(&self, album_id: &str) -> Result<(), DeezerError> {
        let params = json!({ "ALB_ID": album_id });
        self.gw_call("favorite_album.add", params).await?;
        Ok(())
    }

    /// Remove an album from the user's favorites.
    pub async fn remove_favorite_album(&self, album_id: &str) -> Result<(), DeezerError> {
        let params = json!({ "ALB_ID": album_id });
        self.gw_call("favorite_album.remove", params).await?;
        Ok(())
    }

    /// Add tracks to a playlist.
    pub async fn add_to_playlist(
        &self,
        playlist_id: &str,
        song_ids: &[&str],
    ) -> Result<(), DeezerError> {
        let songs: Vec<serde_json::Value> = song_ids.iter().map(|id| json!([id, 0])).collect();
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

    /// Get album details (title, artist, tracks, cover, release date) via the public API.
    pub async fn get_album_detail(&self, album_id: &str) -> Result<AlbumDetail, DeezerError> {
        let url = format!("https://api.deezer.com/album/{}", album_id);
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        if let Some(err) = resp.get("error") {
            return Err(DeezerError::Api(
                err.get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown album error")
                    .to_string(),
            ));
        }

        let title = resp
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let artist = resp
            .get("artist")
            .and_then(|a| a.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let nb_tracks = resp.get("nb_tracks").and_then(|v| v.as_u64()).unwrap_or(0);
        let release_date = resp
            .get("release_date")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let cover_url = resp
            .get("cover_medium")
            .or_else(|| resp.get("cover_small"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let label = resp
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Parse tracks from the "tracks.data" array
        let track_ids: Vec<String> = resp
            .get("tracks")
            .and_then(|t| t.get("data"))
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.get("id").and_then(|v| v.as_u64()).map(|v| v.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // Fetch full track data via gw-light (includes TRACK_TOKEN)
        let track_id_refs: Vec<&str> = track_ids.iter().map(|s| s.as_str()).collect();
        let tracks = if !track_id_refs.is_empty() {
            self.get_tracks(&track_id_refs).await.unwrap_or_default()
        } else {
            Vec::new()
        };

        Ok(AlbumDetail {
            album_id: album_id.to_string(),
            title,
            artist,
            nb_tracks,
            release_date,
            cover_url,
            label,
            tracks,
        })
    }

    /// Get artist details (name, fans, top tracks, albums) via the public API.
    pub async fn get_artist_detail(&self, artist_id: &str) -> Result<ArtistDetail, DeezerError> {
        // 1. Fetch artist info
        let url = format!("https://api.deezer.com/artist/{}", artist_id);
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        if let Some(err) = resp.get("error") {
            return Err(DeezerError::Api(
                err.get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown artist error")
                    .to_string(),
            ));
        }

        let name = resp
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let nb_fan = resp.get("nb_fan").and_then(|v| v.as_u64()).unwrap_or(0);
        let picture_url = resp
            .get("picture_medium")
            .or_else(|| resp.get("picture_small"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // 2. Fetch top tracks
        let top_url = format!("https://api.deezer.com/artist/{}/top?limit=50", artist_id);
        let top_resp: serde_json::Value = self
            .http
            .get(&top_url)
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        let track_ids: Vec<String> = top_resp
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.get("id").and_then(|v| v.as_u64()).map(|v| v.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let track_id_refs: Vec<&str> = track_ids.iter().map(|s| s.as_str()).collect();
        let top_tracks = if !track_id_refs.is_empty() {
            self.get_tracks(&track_id_refs).await.unwrap_or_default()
        } else {
            Vec::new()
        };

        // 3. Fetch albums (all types)
        let albums_url = format!(
            "https://api.deezer.com/artist/{}/albums?limit=500",
            artist_id
        );
        let albums_resp: serde_json::Value = self
            .http
            .get(&albums_url)
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        let mut albums: Vec<ArtistAlbumEntry> = albums_resp
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|entry| {
                        let album_id = entry
                            .get("id")
                            .and_then(|v| v.as_u64())
                            .map(|v| v.to_string())
                            .unwrap_or_default();
                        let title = entry
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let release_date = entry
                            .get("release_date")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let fans = entry.get("fans").and_then(|v| v.as_u64()).unwrap_or(0);
                        let record_type = entry
                            .get("record_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("album")
                            .to_string();
                        ArtistAlbumEntry {
                            album_id,
                            title,
                            release_date,
                            fans,
                            record_type,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Sort by release date descending (newest first)
        albums.sort_by(|a, b| b.release_date.cmp(&a.release_date));

        Ok(ArtistDetail {
            artist_id: artist_id.to_string(),
            name,
            nb_fan,
            picture_url,
            top_tracks,
            albums,
        })
    }

    /// Get playlist details (title, creator, tracks) via the public API.
    pub async fn get_playlist_detail(
        &self,
        playlist_id: &str,
    ) -> Result<PlaylistDetail, DeezerError> {
        let url = format!("https://api.deezer.com/playlist/{}", playlist_id);
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        if let Some(err) = resp.get("error") {
            return Err(DeezerError::Api(
                err.get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown playlist error")
                    .to_string(),
            ));
        }

        let title = resp
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let creator = resp
            .get("creator")
            .and_then(|c| c.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let nb_tracks = resp.get("nb_tracks").and_then(|v| v.as_u64()).unwrap_or(0);

        // Parse track IDs from the "tracks.data" array
        let track_ids: Vec<String> = resp
            .get("tracks")
            .and_then(|t| t.get("data"))
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.get("id").and_then(|v| v.as_u64()).map(|v| v.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // Fetch full track data via gw-light (includes TRACK_TOKEN)
        let track_id_refs: Vec<&str> = track_ids.iter().map(|s| s.as_str()).collect();
        let tracks = if !track_id_refs.is_empty() {
            self.get_tracks(&track_id_refs).await.unwrap_or_default()
        } else {
            Vec::new()
        };

        Ok(PlaylistDetail {
            playlist_id: playlist_id.to_string(),
            title,
            creator,
            nb_tracks,
            tracks,
        })
    }

    /// Get the list of radio stations from the Deezer public API.
    pub async fn get_radios(&self) -> Result<Vec<super::models::RadioData>, DeezerError> {
        let resp: serde_json::Value = self
            .http
            .get("https://api.deezer.com/radio")
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        let data = resp
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| DeezerError::Api("Missing 'data' in radios response".into()))?;

        let radios: Vec<super::models::RadioData> = data
            .iter()
            .filter_map(|entry| serde_json::from_value(entry.clone()).ok())
            .collect();

        Ok(radios)
    }

    /// Get tracks for a specific radio station from the Deezer public API.
    pub async fn get_radio_tracks(&self, radio_id: u64) -> Result<Vec<TrackData>, DeezerError> {
        let url = format!("https://api.deezer.com/radio/{}/tracks", radio_id);
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        let data = resp
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| DeezerError::Api("Missing 'data' in radio tracks response".into()))?;

        // The public API returns tracks in a different format than gw-light.
        // Extract track IDs and fetch full data via gw-light.
        let track_ids: Vec<String> = data
            .iter()
            .filter_map(|t| t.get("id").and_then(|v| v.as_u64()).map(|v| v.to_string()))
            .collect();

        let track_id_refs: Vec<&str> = track_ids.iter().map(|s| s.as_str()).collect();
        if track_id_refs.is_empty() {
            return Ok(Vec::new());
        }

        self.get_tracks(&track_id_refs).await
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
    let section =
        section.ok_or_else(|| DeezerError::Api("Section not found in search results".into()))?;
    let data = section
        .get("data")
        .ok_or_else(|| DeezerError::Api("Missing 'data' in search section".into()))?;
    serde_json::from_value(data.clone())
        .map_err(|e| DeezerError::Api(format!("Failed to parse search section: {e}")))
}

fn format_fans(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M fans", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K fans", n as f64 / 1_000.0)
    } else {
        format!("{n} fans")
    }
}
