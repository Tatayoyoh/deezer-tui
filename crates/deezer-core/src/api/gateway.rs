use serde_json::json;
use tracing::debug;

use super::models::{
    AlbumDetail, ArtistAlbumEntry, ArtistData, ArtistDetail, DeezerError, DisplayItem, EpisodeData,
    MoodItem, PlaylistData, PlaylistDetail, PodcastData, ProfileData, SearchResults,
    SimilarArtistEntry, TrackData,
};
use super::DeezerClient;

const GW_LIGHT_URL: &str = "https://www.deezer.com/ajax/gw-light.php";
const AUTH_JWT_URL: &str = "https://auth.deezer.com/login/arl?jo=p&rto=c&i=c";
const PIPE_GRAPHQL_URL: &str = "https://pipe.deezer.com/api";

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

    /// Gateway call for write operations that don't return a meaningful `results` field.
    async fn gw_call_void(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(), DeezerError> {
        let api_token = self.api_token()?;
        let url =
            format!("{GW_LIGHT_URL}?method={method}&input=3&api_version=1.0&api_token={api_token}");

        debug!(method, "Gateway API call (void)");

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

        if let Some(error) = body.get("error") {
            if !error.as_object().map_or(true, |o| o.is_empty()) {
                return Err(DeezerError::Api(error.to_string()));
            }
        }

        Ok(())
    }

    /// Report that a track was played, so it shows up in the user's
    /// listening history. Mirrors what the web player sends after ~30s.
    pub async fn log_listen(
        &self,
        track_id: &str,
        format: &str,
        ctxt_id: &str,
        ctxt_type: &str,
    ) -> Result<(), DeezerError> {
        let ts_listen = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let track_num: serde_json::Value = parse_id(track_id);
        let ctxt_num: serde_json::Value = parse_id(ctxt_id);
        let params = json!({
            "params": {
                "ts_listen": ts_listen,
                "type": 0,
                "stat": { "seek": 0, "pause": 0, "sync": 1 },
                "media": { "id": track_num, "type": "song", "format": format },
                "ctxt": { "id": ctxt_num, "t": ctxt_type },
            }
        });
        self.gw_call_void("log.listen", params).await
    }

    /// Get full track data by song ID (includes TRACK_TOKEN and MD5_ORIGIN).
    pub async fn get_track(&self, song_id: &str) -> Result<TrackData, DeezerError> {
        let params = json!({ "sng_id": song_id });
        let results = self.gw_call("song.getData", params).await?;

        if song_id.starts_with('-') {
            debug!(track_id = %song_id, payload = %results, "song.getData (user-uploaded)");
        }

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
                    col4: nb_tracks.to_string(),
                    track: None,
                    album_id,
                    playlist_id: None,
                    artist_id: None,
                    show_id: None,
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
                    show_id: None,
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
                    col4: nb_tracks.to_string(),
                    track: None,
                    album_id,
                    playlist_id: None,
                    artist_id: None,
                    show_id: None,
                }
            })
            .collect();

        Ok(items)
    }

    /// Raw playlist entries from the user's own profile page, via the
    /// authenticated gateway.
    ///
    /// This is deliberately not `api.deezer.com/user/{id}/playlists`: the
    /// public REST API only ever exposes playlists whose visibility is public,
    /// so every private playlist is silently absent from its response even
    /// when the request carries a valid session.
    async fn fetch_profile_playlists(&self) -> Result<Vec<serde_json::Value>, DeezerError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| DeezerError::Auth("Not authenticated".into()))?;

        let params = json!({
            "user_id": session.user_id,
            "tab": "playlists",
            "nb": 2000,
        });

        let results = self.gw_call("deezer.pageProfile", params).await?;
        let data = results
            .get("TAB")
            .and_then(|t| t.get("playlists"))
            .and_then(|p| p.get("data"))
            .ok_or_else(|| DeezerError::Api("Missing TAB.playlists.data in profile".into()))?;

        serde_json::from_value(data.clone())
            .map_err(|e| DeezerError::Api(format!("Failed to parse playlists: {e}")))
    }

    /// Get user playlists for the Playlists tab — public and private alike.
    pub async fn get_playlists(&self) -> Result<Vec<DisplayItem>, DeezerError> {
        let items: Vec<DisplayItem> = self
            .fetch_profile_playlists()
            .await?
            .into_iter()
            .filter_map(|v| serde_json::from_value::<PlaylistData>(v).ok())
            .map(|pl| DisplayItem::from_playlist(&pl))
            .collect();

        debug!("favorite_playlists: {} items", items.len());
        Ok(items)
    }

    /// Get listening history (recently played tracks), most recent first.
    pub async fn get_listening_history(&self) -> Result<Vec<TrackData>, DeezerError> {
        // Try the dedicated history endpoint first. This returns rows that
        // include a TS / DATE field per play, ordered most-recent first.
        let params = json!({ "nb": 200 });
        if let Ok(res) = self.gw_call("user.getSongsHistory", params).await {
            if let Some(tracks) = extract_history_tracks(&res) {
                return Ok(tracks);
            }
            debug!(
                "user.getSongsHistory unexpected shape: keys={:?}",
                res.as_object().map(|o| o.keys().collect::<Vec<_>>())
            );
        }

        // Fallback: profile page listening_history tab (data grouped by day).
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| DeezerError::Auth("Not authenticated".into()))?;
        let params = json!({
            "user_id": session.user_id,
            "tab": "listening_history",
            "nb": 200,
        });
        if let Ok(res) = self.gw_call("deezer.pageProfile", params).await {
            if let Some(tab) = res.get("TAB").and_then(|t| t.get("listening_history")) {
                if let Some(tracks) = extract_history_tracks(tab) {
                    return Ok(tracks);
                }
                debug!(
                    "pageProfile listening_history tab keys={:?}",
                    tab.as_object().map(|o| o.keys().collect::<Vec<_>>())
                );
            }
        }

        debug!("History endpoints exhausted, falling back to favorites");
        self.get_favorites().await
    }

    /// Get a podcast show's name and its episodes, most recent first.
    pub async fn get_show_episodes(
        &self,
        show_id: &str,
    ) -> Result<(String, Vec<EpisodeData>), DeezerError> {
        let params = json!({
            "show_id": show_id,
            "start": 0,
            "nb": 200,
            "user_id": null,
        });

        let res = self.gw_call("deezer.pageShow", params).await?;

        let name = res
            .get("DATA")
            .and_then(|d| d.get("SHOW_NAME"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        let episodes: Vec<EpisodeData> = res
            .get("EPISODES")
            .and_then(|e| e.get("data"))
            .and_then(|d| d.as_array())
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|e| serde_json::from_value(e.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();

        debug!("show {show_id}: {} episodes", episodes.len());

        Ok((name, episodes))
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
                    show_id: None,
                }
            })
            .collect();

        Ok(items)
    }

    /// Add a track to the user's favorites.
    pub async fn add_favorite(&self, song_id: &str) -> Result<(), DeezerError> {
        let params = json!({ "SNG_ID": song_id });
        self.gw_call_void("favorite_song.add", params).await
    }

    /// Remove a track from the user's favorites.
    pub async fn remove_favorite(&self, song_id: &str) -> Result<(), DeezerError> {
        let params = json!({ "SNG_ID": song_id });
        self.gw_call_void("favorite_song.remove", params).await
    }

    /// Add an artist to the user's favorites.
    pub async fn add_favorite_artist(&self, artist_id: &str) -> Result<(), DeezerError> {
        let id = parse_id(artist_id);
        let params = json!({ "ART_ID": id });
        self.gw_call_void("artist.addFavorite", params).await
    }

    /// Remove an artist from the user's favorites.
    pub async fn remove_favorite_artist(&self, artist_id: &str) -> Result<(), DeezerError> {
        let id = parse_id(artist_id);
        let params = json!({ "ART_ID": id });
        self.gw_call_void("artist.deleteFavorite", params).await
    }

    /// Add an album to the user's favorites.
    pub async fn add_favorite_album(&self, album_id: &str) -> Result<(), DeezerError> {
        let id = parse_id(album_id);
        let params = json!({ "ALB_ID": id });
        self.gw_call_void("favorite_album.add", params).await
    }

    /// Remove an album from the user's favorites.
    pub async fn remove_favorite_album(&self, album_id: &str) -> Result<(), DeezerError> {
        let id = parse_id(album_id);
        let params = json!({ "ALB_ID": id });
        self.gw_call_void("favorite_album.remove", params).await
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
        match self.gw_call_void("playlist.addSongs", params).await {
            Ok(()) => Ok(()),
            Err(DeezerError::Api(msg)) => {
                let lower = msg.to_lowercase();
                if lower.contains("already") || lower.contains("duplicate") {
                    Err(DeezerError::TrackAlreadyInPlaylist)
                } else {
                    Err(DeezerError::Api(msg))
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Remove tracks from a playlist.
    pub async fn remove_from_playlist(
        &self,
        playlist_id: &str,
        song_ids: &[&str],
    ) -> Result<(), DeezerError> {
        let songs: Vec<serde_json::Value> = song_ids.iter().map(|id| json!([id, 0])).collect();
        let params = json!({
            "playlist_id": playlist_id,
            "songs": songs,
        });
        self.gw_call_void("playlist.deleteSongs", params).await
    }

    /// Mark a track as disliked (don't recommend).
    pub async fn dislike_track(&self, song_id: &str) -> Result<(), DeezerError> {
        let params = json!({ "SNG_ID": song_id });
        self.gw_call_void("song.dislike", params).await
    }

    /// Create a new (empty) personal playlist. Returns the new playlist's ID.
    pub async fn create_playlist(&self, title: &str) -> Result<String, DeezerError> {
        let params = json!({
            "title": title,
            "status": 0,
            "description": "",
            "songs": [],
        });
        let results = self.gw_call("playlist.create", params).await?;
        // Deezer returns the new playlist_id either as a bare value or wrapped.
        let pid = match &results {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => n.as_u64().map(|x| x.to_string()),
            serde_json::Value::Object(_) => results
                .get("PLAYLIST_ID")
                .or_else(|| results.get("playlist_id"))
                .and_then(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .or_else(|| v.as_u64().map(|n| n.to_string()))
                }),
            _ => None,
        };
        pid.ok_or_else(|| DeezerError::Api("Missing playlist_id in create response".into()))
    }

    /// Rename an existing playlist.
    pub async fn rename_playlist(
        &self,
        playlist_id: &str,
        new_title: &str,
    ) -> Result<(), DeezerError> {
        let params = json!({
            "playlist_id": playlist_id,
            "title": new_title,
        });
        self.gw_call_void("playlist.update", params).await
    }

    /// Delete a playlist owned by the current user.
    pub async fn delete_playlist(&self, playlist_id: &str) -> Result<(), DeezerError> {
        let params = json!({ "playlist_id": playlist_id });
        self.gw_call_void("playlist.delete", params).await
    }

    /// Get user playlists as raw PlaylistData (for playlist picker).
    pub async fn get_user_playlists_raw(&self) -> Result<Vec<PlaylistData>, DeezerError> {
        let owner_id = self
            .session
            .as_ref()
            .ok_or_else(|| DeezerError::Auth("Not authenticated".into()))?
            .user_id
            .to_string();

        let all = self.fetch_profile_playlists().await?;

        let writable: Vec<PlaylistData> = all
            .into_iter()
            .filter_map(|v| {
                let parent_id = v.get("PARENT_USER_ID").and_then(|x| {
                    x.as_str()
                        .map(str::to_string)
                        .or_else(|| x.as_u64().map(|n| n.to_string()))
                });
                let status = v
                    .get("STATUS")
                    .and_then(|x| {
                        x.as_u64()
                            .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
                    })
                    .unwrap_or(0);
                let ptype = v.get("TYPE").and_then(|x| x.as_str()).unwrap_or("");
                let is_owner = parent_id.as_deref() == Some(owner_id.as_str());
                let is_collaborative = status == 2;

                // Skip "loved tracks" pseudo-playlist (managed via favorites, not addSongs).
                if ptype.eq_ignore_ascii_case("loved") {
                    return None;
                }
                if !is_owner && !is_collaborative {
                    return None;
                }

                let mut pl: PlaylistData = serde_json::from_value(v).ok()?;
                pl.collaborative = is_collaborative;
                Some(pl)
            })
            .collect();

        debug!("user_playlists (writable): {} items", writable.len());
        Ok(writable)
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
            .get("cover_xl")
            .or_else(|| resp.get("cover_big"))
            .or_else(|| resp.get("cover_medium"))
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
            .get("picture_big")
            .or_else(|| resp.get("picture_medium"))
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
                        let cover_url = entry
                            .get("cover_xl")
                            .or_else(|| entry.get("cover_big"))
                            .or_else(|| entry.get("cover_medium"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        ArtistAlbumEntry {
                            album_id,
                            title,
                            release_date,
                            fans,
                            record_type,
                            cover_url,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Sort by release date descending (newest first)
        albums.sort_by(|a, b| b.release_date.cmp(&a.release_date));

        // 4. Fetch similar artists
        let related_url = format!(
            "https://api.deezer.com/artist/{}/related?limit=50",
            artist_id
        );
        let related_resp: serde_json::Value = self
            .http
            .get(&related_url)
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        let similar_artists: Vec<SimilarArtistEntry> = related_resp
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|entry| {
                        let artist_id = entry
                            .get("id")
                            .and_then(|v| v.as_u64())
                            .map(|v| v.to_string())
                            .unwrap_or_default();
                        let name = entry
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let nb_fan = entry.get("nb_fan").and_then(|v| v.as_u64()).unwrap_or(0);
                        SimilarArtistEntry {
                            artist_id,
                            name,
                            nb_fan,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(ArtistDetail {
            artist_id: artist_id.to_string(),
            name,
            nb_fan,
            picture_url,
            top_tracks,
            albums,
            similar_artists,
        })
    }

    /// Get playlist details (title, creator, tracks).
    ///
    /// Uses the authenticated gateway rather than `api.deezer.com/playlist/{id}`:
    /// the public endpoint refuses private playlists with "An active access
    /// token must be used to query information about the current user", so
    /// opening one from the Playlists tab failed once #17 made them visible.
    pub async fn get_playlist_detail(
        &self,
        playlist_id: &str,
    ) -> Result<PlaylistDetail, DeezerError> {
        let params = json!({
            "playlist_id": playlist_id,
            "start": 0,
            "nb": 2000,
            "lang": "en",
            "tab": 0,
            "tags": true,
            "header": true,
        });

        let res = self.gw_call("deezer.pagePlaylist", params).await?;
        let data = res.get("DATA");

        let title = data
            .and_then(|d| d.get("TITLE"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let creator = data
            .and_then(|d| d.get("PARENT_USERNAME"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        // SONGS holds full gateway track objects, so they deserialize straight
        // into TrackData — no second call to resolve IDs. Entries that come
        // back without a TRACK_TOKEN are topped up by ensure_track_token when
        // the track is actually played.
        let tracks: Vec<TrackData> = res
            .get("SONGS")
            .and_then(|s| s.get("data"))
            .and_then(|d| d.as_array())
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|t| serde_json::from_value(t.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();

        // NB_SONG comes back as a number or a string depending on the endpoint.
        let nb_tracks = data
            .and_then(|d| d.get("NB_SONG"))
            .and_then(|v| {
                v.as_u64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            })
            .unwrap_or(tracks.len() as u64);

        debug!("playlist {playlist_id}: {} tracks", tracks.len());

        Ok(PlaylistDetail {
            playlist_id: playlist_id.to_string(),
            title,
            creator,
            nb_tracks,
            tracks,
        })
    }

    /// Get the list of music genres/categories from the Deezer public API.
    pub async fn get_genres(&self) -> Result<Vec<super::models::GenreData>, DeezerError> {
        let resp: serde_json::Value = self
            .http
            .get("https://api.deezer.com/genre")
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        let data = resp
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| DeezerError::Api("Missing 'data' in genres response".into()))?;

        let mut genres: Vec<super::models::GenreData> = data
            .iter()
            .filter_map(|entry| serde_json::from_value(entry.clone()).ok())
            .collect();

        // Filter out the meta-genre "All" (id == 0) returned by Deezer.
        genres.retain(|g| g.id != 0);

        Ok(genres)
    }

    /// Get the full content of a music genre/category — top tracks, albums,
    /// artists, playlists from `/chart/{id}` plus radios from `/genre/{id}/radios`.
    pub async fn get_genre_detail(
        &self,
        genre_id: u64,
        name: String,
    ) -> Result<super::models::GenreDetail, DeezerError> {
        use super::models::{AlbumData, ArtistData, GenreDetail, PlaylistData, RadioData};

        let chart_url = format!("https://api.deezer.com/chart/{}", genre_id);
        let chart: serde_json::Value = self
            .http
            .get(&chart_url)
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        // Tracks: extract IDs, fetch full data via gw-light for streaming metadata.
        let track_ids: Vec<String> = chart
            .pointer("/tracks/data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.get("id").and_then(|v| v.as_u64()).map(|v| v.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let tracks = if track_ids.is_empty() {
            Vec::new()
        } else {
            let refs: Vec<&str> = track_ids.iter().map(String::as_str).collect();
            self.get_tracks(&refs).await.unwrap_or_default()
        };

        // Albums: parse public API format manually (lowercase, nested artist).
        let album_entries = chart
            .pointer("/albums/data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        let albums: Vec<AlbumData> = album_entries.iter().filter_map(parse_chart_album).collect();

        // Artists: Deezer's `/chart/{id}/artists` endpoint returns the global Top
        // artists ignoring the genre filter. Derive genre-specific artists from
        // the (correctly filtered) album list, deduping by id.
        let mut artists: Vec<ArtistData> = Vec::new();
        let mut seen_artist_ids = std::collections::HashSet::new();
        for entry in &album_entries {
            let Some(art) = entry.get("artist") else {
                continue;
            };
            let Some(id) = art.get("id").and_then(|v| v.as_u64()) else {
                continue;
            };
            if !seen_artist_ids.insert(id) {
                continue;
            }
            let name = art
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            artists.push(ArtistData {
                artist_id: id.to_string(),
                name,
                nb_fan: 0,
            });
        }

        // Playlists.
        let playlists: Vec<PlaylistData> = chart
            .pointer("/playlists/data")
            .and_then(|d| d.as_array())
            .map(|arr| arr.iter().filter_map(parse_chart_playlist).collect())
            .unwrap_or_default();

        // Radios for this genre (separate endpoint).
        let radios_url = format!("https://api.deezer.com/genre/{}/radios", genre_id);
        let radios_resp: serde_json::Value = self
            .http
            .get(&radios_url)
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;
        let radios: Vec<RadioData> = radios_resp
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|r| serde_json::from_value(r.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();

        Ok(GenreDetail {
            genre_id,
            name,
            tracks,
            albums,
            artists,
            playlists,
            radios,
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

    /// Acquire a JWT for pipe.deezer.com GraphQL API.
    /// Exchanges the ARL cookie (already set on the cookie jar by `login_arl`)
    /// for a JWT via `auth.deezer.com/login/arl`. Cached on the client.
    ///
    /// `force` drops the cached token first, to replace one the server has
    /// rejected. The ARL cookie outlives the JWT, so this re-exchange needs no
    /// user interaction.
    async fn ensure_jwt(&self, force: bool) -> Result<String, DeezerError> {
        if force {
            if let Ok(mut cache) = self.jwt_cache.lock() {
                *cache = None;
            }
        } else if let Ok(cache) = self.jwt_cache.lock() {
            if let Some(ref jwt) = *cache {
                return Ok(jwt.clone());
            }
        }

        debug!("Fetching JWT from auth.deezer.com");
        let resp = self
            .http
            .post(AUTH_JWT_URL)
            .header("Content-Length", "0")
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        let status = resp.status();
        let raw = resp
            .text()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;
        let _ = std::fs::write(
            "/tmp/deezer-jwt-debug.txt",
            format!("status={status}\n{raw}"),
        );
        let body: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| DeezerError::Http(format!("JWT response not JSON: {e}")))?;

        let jwt = body
            .get("jwt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                DeezerError::Auth(format!("Missing 'jwt' in auth response (status={status})"))
            })?
            .to_string();

        if let Ok(mut cache) = self.jwt_cache.lock() {
            *cache = Some(jwt.clone());
        }
        Ok(jwt)
    }

    /// Call the pipe.deezer.com GraphQL endpoint with a Bearer JWT.
    ///
    /// The JWT is cached for the whole session but Deezer expires it
    /// server-side, so a rejected token is refreshed and the call retried once
    /// instead of surfacing "JWT token has expired, please reauthenticate" to
    /// the user (#14). The retry is bounded to one attempt: if the fresh token
    /// is rejected too, the ARL itself is dead and only a real re-login helps.
    async fn graphql_call(&self, query: &str) -> Result<serde_json::Value, DeezerError> {
        match self.graphql_attempt(query, false).await {
            Err(e) if is_stale_jwt(&e) => {
                debug!("GraphQL rejected the JWT ({e}); refreshing and retrying once");
                self.graphql_attempt(query, true).await
            }
            other => other,
        }
    }

    /// One GraphQL round-trip. `force_new_jwt` bypasses the cached token.
    async fn graphql_attempt(
        &self,
        query: &str,
        force_new_jwt: bool,
    ) -> Result<serde_json::Value, DeezerError> {
        let jwt = self.ensure_jwt(force_new_jwt).await?;
        let body = json!({ "query": query });

        let resp = self
            .http
            .post(PIPE_GRAPHQL_URL)
            .bearer_auth(&jwt)
            .json(&body)
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        let status = resp.status();
        let raw_text = resp
            .text()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;
        let _ = std::fs::write(
            "/tmp/deezer-graphql-debug.txt",
            format!("status={status}\n{raw_text}"),
        );
        // A rejected token can come back as a bare 401/403 with no GraphQL
        // error envelope, so check the status before parsing.
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(DeezerError::Auth(format!(
                "GraphQL rejected the JWT (status={status})"
            )));
        }

        let body: serde_json::Value = serde_json::from_str(&raw_text)
            .map_err(|e| DeezerError::Http(format!("Non-JSON response: {e} body={raw_text}")))?;

        if let Some(errors) = body.get("errors").and_then(|e| e.as_array()) {
            if !errors.is_empty() {
                let msg = errors
                    .iter()
                    .filter_map(|e| e.get("message").and_then(|v| v.as_str()))
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(DeezerError::Api(format!("GraphQL error: {msg}")));
            }
        }

        body.get("data")
            .cloned()
            .ok_or_else(|| DeezerError::Api("Missing 'data' in GraphQL response".into()))
    }

    /// Get the catalog of "mood" Flow configurations (Happy, Chill, Workout, …).
    /// Queries pipe.deezer.com GraphQL via `me { flowConfigs { moods { … } } }`.
    /// Falls back to a hardcoded list of mood radios if the call fails.
    pub async fn get_moods(&self) -> Result<Vec<MoodItem>, DeezerError> {
        let query = r#"{ me { flowConfigs { moods { edges { node { id title } } } } } }"#;

        let data = self.graphql_call(query).await?;
        // Temporary diagnostic dump: full GraphQL response written to /tmp.
        let _ = std::fs::write(
            "/tmp/deezer-moods-debug.json",
            serde_json::to_string_pretty(&data).unwrap_or_default(),
        );
        let mut moods = Vec::new();
        if let Some(edges) = data
            .pointer("/me/flowConfigs/moods/edges")
            .and_then(|v| v.as_array())
        {
            for edge in edges {
                let id = edge
                    .pointer("/node/id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let title = edge
                    .pointer("/node/title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !id.is_empty() && !title.is_empty() {
                    moods.push(MoodItem {
                        id,
                        title,
                        target: String::new(),
                        radio_id: None,
                    });
                }
            }
        }
        if moods.is_empty() {
            debug!("GraphQL moods returned no entries, using hardcoded fallback");
            return Ok(hardcoded_moods());
        }
        Ok(moods)
    }

    /// Resolve a mood into a track list to play.
    /// Queries `flowConfig(flowConfigId: <id>) { tracks { track { id } } }`,
    /// then fetches full track data via gw-light.
    /// If the mood has a `radio_id` (hardcoded fallback path), uses that directly.
    pub async fn get_mood_tracks(&self, mood: &MoodItem) -> Result<Vec<TrackData>, DeezerError> {
        // Hardcoded fallback path: mood maps to a radio ID.
        if let Some(rid) = mood.radio_id {
            return self.get_radio_tracks(rid).await;
        }

        if mood.id.is_empty() {
            return Err(DeezerError::Api("Mood has no id".into()));
        }

        // Escape the ID for embedding in the GraphQL query string.
        let escaped_id = mood.id.replace('\\', "\\\\").replace('"', "\\\"");
        let query = format!(
            r#"{{ flowConfig(flowConfigId: "{escaped_id}") {{ tracks {{ track {{ id }} }} }} }}"#
        );

        let data = self.graphql_call(&query).await?;

        let track_ids: Vec<String> = data
            .pointer("/flowConfig/tracks")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        t.pointer("/track/id")
                            .and_then(|v| v.as_str())
                            .map(String::from)
                    })
                    .collect()
            })
            .unwrap_or_default();

        if track_ids.is_empty() {
            return Err(DeezerError::Api("No tracks in mood Flow config".into()));
        }

        let refs: Vec<&str> = track_ids.iter().map(String::as_str).collect();
        self.get_tracks(&refs).await
    }

    /// Get the user's personalized Flow (Deezer Flow).
    pub async fn get_flow(&self) -> Result<Vec<TrackData>, DeezerError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| DeezerError::Auth("Not authenticated".into()))?;

        let params = json!({ "user_id": session.user_id });
        let results = self.gw_call("radio.getUserRadio", params).await?;

        let data = results
            .get("data")
            .ok_or_else(|| DeezerError::Api("Missing 'data' in flow response".into()))?;

        serde_json::from_value(data.clone())
            .map_err(|e| DeezerError::Api(format!("Failed to parse flow: {e}")))
    }
}

/// Parse a search section's "data" array into a typed vec.
/// Pull the listening-history track list out of a gw-light response chunk,
/// sorted most-recent first when a timestamp field is present.
///
/// Accepts three shapes seen in the wild:
///   { "data": [ TrackData, ... ] }
///   { "data": [ { "DATE": "...", "data": [ TrackData, ... ] }, ... ] }   (day-grouped)
///   [ TrackData, ... ]
fn extract_history_tracks(node: &serde_json::Value) -> Option<Vec<TrackData>> {
    let array = node
        .get("data")
        .and_then(|d| d.as_array())
        .or_else(|| node.as_array())?;

    // Day-grouped: outer entries have their own "data" array of tracks. Flatten.
    if array
        .iter()
        .any(|e| e.get("data").and_then(|d| d.as_array()).is_some())
    {
        let mut out: Vec<(Option<i64>, TrackData)> = Vec::new();
        for group in array {
            let group_ts = extract_ts(group);
            if let Some(inner) = group.get("data").and_then(|d| d.as_array()) {
                for item in inner {
                    if let Ok(t) = serde_json::from_value::<TrackData>(item.clone()) {
                        out.push((extract_ts(item).or(group_ts), t));
                    }
                }
            }
        }
        if out.is_empty() {
            return None;
        }
        out.sort_by(|a, b| b.0.cmp(&a.0));
        return Some(out.into_iter().map(|(_, t)| t).collect());
    }

    // Flat: each entry is itself a track (possibly carrying TS/DATE).
    let mut out: Vec<(Option<i64>, TrackData)> = Vec::new();
    for item in array {
        if let Ok(t) = serde_json::from_value::<TrackData>(item.clone()) {
            out.push((extract_ts(item), t));
        }
    }
    if out.is_empty() {
        return None;
    }
    if out.iter().any(|(ts, _)| ts.is_some()) {
        out.sort_by(|a, b| b.0.cmp(&a.0));
    }
    Some(out.into_iter().map(|(_, t)| t).collect())
}

fn extract_ts(v: &serde_json::Value) -> Option<i64> {
    for key in ["TS", "DATE", "DATE_ADD", "TIMESTAMP"] {
        if let Some(val) = v.get(key) {
            if let Some(n) = val.as_i64() {
                return Some(n);
            }
            if let Some(s) = val.as_str() {
                if let Ok(n) = s.parse::<i64>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

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

/// Parse a string ID as a JSON integer if possible, otherwise keep it as a string.
/// Deezer's private gateway requires numeric IDs as integers, not strings.
fn parse_id(id: &str) -> serde_json::Value {
    if let Ok(n) = id.parse::<u64>() {
        serde_json::Value::Number(n.into())
    } else {
        serde_json::Value::String(id.to_string())
    }
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

/// Fallback list of mood radios used when `page.get` is unavailable or empty.
/// IDs are Deezer "smart radio" station IDs that play mood-fitting music.
fn hardcoded_moods() -> Vec<MoodItem> {
    let entries: &[(&str, u64)] = &[
        ("Chill", 132),
        ("Workout", 75),
        ("Pop", 137),
        ("Rock", 152),
        ("Electro", 144),
        ("Hip Hop", 116),
        ("Latin", 84),
        ("R&B", 153),
        ("Reggae", 117),
        ("Indie", 113),
    ];
    entries
        .iter()
        .map(|(title, rid)| MoodItem {
            id: rid.to_string(),
            title: (*title).to_string(),
            target: String::new(),
            radio_id: Some(*rid),
        })
        .collect()
}

// ── Chart-API parsers (public API uses lowercase fields, distinct from gw-light) ──

fn parse_chart_album(v: &serde_json::Value) -> Option<super::models::AlbumData> {
    let id = v.get("id")?.as_u64()?;
    let title = v.get("title").and_then(|x| x.as_str()).unwrap_or("");
    let artist = v
        .get("artist")
        .and_then(|a| a.get("name"))
        .and_then(|x| x.as_str())
        .unwrap_or("");
    Some(super::models::AlbumData {
        album_id: id.to_string(),
        title: title.to_string(),
        artist: artist.to_string(),
        nb_tracks: v.get("nb_tracks").and_then(|n| n.as_u64()).unwrap_or(0),
        release_date: String::new(),
        nb_fan: 0,
    })
}

fn parse_chart_playlist(v: &serde_json::Value) -> Option<super::models::PlaylistData> {
    let id = v.get("id")?.as_u64()?;
    let title = v.get("title").and_then(|x| x.as_str()).unwrap_or("");
    let nb_songs = v.get("nb_tracks").and_then(|n| n.as_u64()).unwrap_or(0);
    let author = v
        .get("user")
        .and_then(|u| u.get("name"))
        .and_then(|x| x.as_str())
        .unwrap_or("");
    Some(super::models::PlaylistData {
        playlist_id: id.to_string(),
        title: title.to_string(),
        nb_songs,
        author: author.to_string(),
        collaborative: false,
    })
}

/// True for the errors a fresh JWT can plausibly resolve.
///
/// Deezer reports an expired pipe.deezer.com token as a GraphQL error message
/// ("JWT token has expired, please reauthenticate") rather than a status code,
/// so the message has to be matched. Kept narrow on purpose: a retry must not
/// be triggered by unrelated GraphQL failures.
fn is_stale_jwt(err: &DeezerError) -> bool {
    match err {
        // Raised by graphql_attempt itself for a 401/403.
        DeezerError::Auth(msg) => msg.to_ascii_lowercase().contains("jwt"),
        DeezerError::Api(msg) => {
            let msg = msg.to_ascii_lowercase();
            msg.contains("jwt")
                && (msg.contains("expired")
                    || msg.contains("reauthenticate")
                    || msg.contains("invalid"))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::is_stale_jwt;
    use crate::api::models::DeezerError;
    use crate::api::DeezerClient;

    /// Live tests read the ARL from the environment so no token is committed.
    fn arl() -> String {
        std::env::var("DEEZER_ARL").expect("set DEEZER_ARL to run live tests")
    }

    #[test]
    fn stale_jwt_is_detected_from_the_graphql_message() {
        // The message Deezer actually returns, as reported in #14.
        assert!(is_stale_jwt(&DeezerError::Api(
            "GraphQL error: JWT token has expired, please reauthenticate".into()
        )));
        // The 401/403 path raises Auth instead.
        assert!(is_stale_jwt(&DeezerError::Auth(
            "GraphQL rejected the JWT (status=401 Unauthorized)".into()
        )));
    }

    #[test]
    fn unrelated_errors_do_not_trigger_a_jwt_retry() {
        assert!(!is_stale_jwt(&DeezerError::Api(
            "GraphQL error: field 'moods' not found".into()
        )));
        assert!(!is_stale_jwt(&DeezerError::Http("connection reset".into())));
        // "Not authenticated" is a missing session, not a stale token.
        assert!(!is_stale_jwt(&DeezerError::Auth(
            "Not authenticated".into()
        )));
    }

    #[tokio::test]
    #[ignore = "live API: requires valid ARL token + network"]
    async fn podcast_episodes_are_playable() {
        let mut client = DeezerClient::new().unwrap();
        client.login_arl(&arl()).await.expect("login failed");

        // A podcast search must yield shows with an ID to open...
        let shows = client
            .search_category("podcast", "SHOW")
            .await
            .expect("show search failed");
        let show_id = shows
            .iter()
            .find_map(|s| s.show_id.clone())
            .expect("no show with an ID in search results");

        // ...and that show must yield episodes carrying a playable source.
        let (name, episodes) = client
            .get_show_episodes(&show_id)
            .await
            .expect("get_show_episodes failed");
        println!("show '{name}' ({show_id}): {} episodes", episodes.len());
        assert!(!episodes.is_empty(), "show should have episodes");

        let track = episodes[0].to_track();
        assert!(
            track.direct_stream_url().is_some() || track.has_track_token(),
            "episode has neither a direct stream URL nor a track token"
        );

        // The direct stream must actually serve audio, unencrypted.
        if let Some(url) = track.direct_stream_url() {
            let audio = crate::player::stream::download_direct(url, client.http())
                .await
                .expect("direct stream download failed");
            println!("downloaded {} bytes for '{}'", audio.len(), track.title);
            assert!(audio.len() > 10_000, "suspiciously small audio payload");
            let is_mp3 = audio.starts_with(b"ID3") || (audio[0] == 0xFF && audio[1] & 0xE0 == 0xE0);
            assert!(is_mp3, "payload is not MP3: {:02X?}", &audio[..4]);
        }
    }
}
