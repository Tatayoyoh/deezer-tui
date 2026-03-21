use tracing::{debug, info};

use super::models::{DeezerError, UserData};
use super::DeezerClient;

const GW_LIGHT_URL: &str = "https://www.deezer.com/ajax/gw-light.php";
const DEEZER_URL: &str = "https://www.deezer.com";

#[derive(Debug, Clone)]
pub struct Session {
    pub api_token: String,
    pub license_token: String,
    pub user_id: u64,
    pub user_name: String,
}

impl DeezerClient {
    /// Authenticate using an ARL token (extracted from browser cookies).
    pub async fn login_arl(&mut self, arl: &str) -> Result<Session, DeezerError> {
        debug!("Authenticating with ARL token");

        // Inject ARL cookie into the cookie jar (persists across all subsequent requests)
        let cookie = format!("arl={arl}; Domain=.deezer.com; Path=/");
        self.cookie_jar
            .add_cookie_str(&cookie, &DEEZER_URL.parse().unwrap());

        // Call getUserData to validate the token and get session info.
        // The server will also set a session cookie (sid) in the response,
        // which the cookie jar will automatically store for subsequent calls.
        let url =
            format!("{GW_LIGHT_URL}?method=deezer.getUserData&input=3&api_version=1.0&api_token=");

        let resp = self
            .http
            .post(&url)
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        let results = body
            .get("results")
            .ok_or_else(|| DeezerError::Api("Missing 'results' in response".into()))?;

        let user_data: UserData = serde_json::from_value(results.clone())
            .map_err(|e| DeezerError::Api(format!("Failed to parse user data: {e}")))?;

        if user_data.user.user_id == 0 {
            return Err(DeezerError::Auth("Invalid ARL token — user_id is 0".into()));
        }

        let session = Session {
            api_token: user_data.api_token,
            license_token: user_data.user.options.license_token,
            user_id: user_data.user.user_id,
            user_name: user_data.user.user_name,
        };

        let offer_name = user_data.offer.as_ref().map_or("unknown", |o| &o.offer_name);
        info!(
            user_id = session.user_id,
            name = %session.user_name,
            offer = %offer_name,
            web_streaming = user_data.user.options.web_streaming,
            web_hq = user_data.user.options.web_hq,
            web_lossless = user_data.user.options.web_lossless,
            license_country = %user_data.user.options.license_country,
            "Authenticated successfully"
        );

        self.session = Some(session.clone());
        Ok(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ARL: &str = "eec5474088bcafa3e16be4492de9abf53472bda2d15527838cd685384c7c82b7daa0f5d745b9cd6756dfe83d65aaa8d6fd91aba6798c714c16a7dfbbe1235730ca72e7f3db0f58c8d9d1f84c29c2236a8ceaae6e639b275ac438379891e998f4";

    #[tokio::test]
    async fn test_arl_login() {
        let mut client = DeezerClient::new().unwrap();
        let session = client.login_arl(TEST_ARL).await.expect("login_arl failed");

        println!("Login OK!");
        println!("  user_id: {}", session.user_id);
        println!("  user_name: '{}'", session.user_name);
        assert!(session.user_id > 0, "user_id should be > 0");
        assert!(
            !session.api_token.is_empty(),
            "api_token should not be empty"
        );
        assert!(
            !session.license_token.is_empty(),
            "license_token should not be empty"
        );
    }

    #[tokio::test]
    async fn test_search() {
        let mut client = DeezerClient::new().unwrap();
        client.login_arl(TEST_ARL).await.expect("login failed");

        let results = client.search("Daft Punk").await.expect("search failed");
        println!("Search returned {} tracks", results.data.len());
        assert!(
            !results.data.is_empty(),
            "should find tracks for 'Daft Punk'"
        );

        let first = &results.data[0];
        println!(
            "  First: {} - {} (ID: {})",
            first.title, first.artist, first.track_id
        );
        println!("  TRACK_TOKEN present: {}", first.has_track_token());
    }

    #[tokio::test]
    async fn test_favorites() {
        let mut client = DeezerClient::new().unwrap();
        client.login_arl(TEST_ARL).await.expect("login failed");

        let favorites = client.get_favorites().await.expect("get_favorites failed");
        println!("Favorites: {} tracks", favorites.len());

        if !favorites.is_empty() {
            let first = &favorites[0];
            println!(
                "  First: {} - {} (ID: {})",
                first.title, first.artist, first.track_id
            );
            println!("  TRACK_TOKEN present: {}", first.has_track_token());
        }
    }

    #[tokio::test]
    async fn test_get_track_with_token() {
        let mut client = DeezerClient::new().unwrap();
        client.login_arl(TEST_ARL).await.expect("login failed");

        // Get full track data for a known track (Around the World by Daft Punk)
        let track = client.get_track("3135556").await.expect("get_track failed");
        println!("Track: {} - {}", track.title, track.artist);
        println!("  TRACK_TOKEN present: {}", track.has_track_token());
        println!("  MD5_ORIGIN: '{}'", track.md5_origin);
        assert!(
            track.has_track_token(),
            "song.getData should return TRACK_TOKEN"
        );
        assert!(
            !track.md5_origin.is_empty(),
            "song.getData should return MD5_ORIGIN"
        );
    }

    #[tokio::test]
    async fn test_master_key() {
        let client = DeezerClient::new().unwrap();
        let key = crate::decrypt::fetch_master_key(client.http())
            .await
            .expect("fetch_master_key failed");
        println!("Master key: {:02x?}", key);
        assert_eq!(key.len(), 16);
    }
}
