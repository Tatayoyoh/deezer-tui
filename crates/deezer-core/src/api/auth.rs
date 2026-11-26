use tracing::info;

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
        self.login_arl_with_profile(arl, None).await
    }

    /// Log in with the shared ARL, then select and verify the requested Family profile.
    /// A failed switch must never leave the caller authenticated as the administrator.
    pub async fn login_arl_with_profile(
        &mut self,
        arl: &str,
        profile_id: Option<u64>,
    ) -> Result<Session, DeezerError> {
        *self = Self::new()?;
        let result = self.login_inner(arl, profile_id).await;
        if result.is_err() {
            self.session = None;
            self.profile_auth = None;
        }
        result
    }

    async fn login_inner(
        &mut self,
        arl: &str,
        profile_id: Option<u64>,
    ) -> Result<Session, DeezerError> {
        let cookie = format!("arl={arl}; Domain=.deezer.com; Path=/");
        self.cookie_jar
            .add_cookie_str(&cookie, &DEEZER_URL.parse().unwrap());
        let body = self.fetch_login_data().await?;
        let mut user_data = parse_user_data(&body, None)?;
        self.family_enabled = body["results"]["USER"]["MULTI_ACCOUNT"]["ENABLED"] == true
            && body["results"]["USER"]["MULTI_ACCOUNT"]["IS_SUB_ACCOUNT"] != true;
        self.session = Some(session_from_data(&user_data));

        if let Some(id) = profile_id {
            if id != user_data.user.user_id {
                self.gw_call("user.loginMulti", serde_json::json!({"account_id": id}))
                    .await?;
                let body = self.fetch_login_data().await?;
                user_data = parse_user_data(&body, Some(id))?;
                // GraphQL otherwise returns an administrator JWT even after
                // the gateway session has switched to a Family profile.
                self.profile_auth = Some((id, arl.to_owned()));
            }
        }
        let session = session_from_data(&user_data);
        self.session = Some(session.clone());
        info!(user_id = session.user_id, name = %session.user_name, "Authenticated successfully");
        Ok(session)
    }

    /// Selectable profiles only; independent members need their own login.
    pub async fn family_profiles(&self) -> Result<Vec<super::models::FamilyProfile>, DeezerError> {
        if !self.family_enabled {
            return Ok(Vec::new());
        }
        let results = self
            .gw_call("deezer.getChildAccounts", serde_json::json!({}))
            .await?;
        selectable_profiles(results, self.session.as_ref().map(|s| s.user_id))
    }

    async fn fetch_login_data(&self) -> Result<serde_json::Value, DeezerError> {
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

        Ok(body)
    }
}

fn session_from_data(data: &UserData) -> Session {
    Session {
        api_token: data.api_token.clone(),
        license_token: data.user.options.license_token.clone(),
        user_id: data.user.user_id,
        user_name: data.user.user_name.clone(),
    }
}

fn parse_user_data(
    body: &serde_json::Value,
    expected: Option<u64>,
) -> Result<UserData, DeezerError> {
    let results = body
        .get("results")
        .ok_or_else(|| DeezerError::Api("Missing 'results' in response".into()))?;
    let data: UserData = serde_json::from_value(results.clone())
        .map_err(|e| DeezerError::Api(format!("Failed to parse user data: {e}")))?;
    if data.user.user_id == 0 {
        return Err(DeezerError::Auth("Invalid ARL token".into()));
    }
    if expected.is_some_and(|id| id != data.user.user_id) {
        return Err(DeezerError::Auth(
            "Deezer did not select the requested Family profile".into(),
        ));
    }
    Ok(data)
}

fn selectable_profiles(
    results: serde_json::Value,
    current_id: Option<u64>,
) -> Result<Vec<super::models::FamilyProfile>, DeezerError> {
    let entries = results
        .as_array()
        .ok_or_else(|| DeezerError::Api("Invalid Family profile list".into()))?;
    let mut profiles = Vec::new();
    for entry in entries {
        let profile: super::models::FamilyProfile = serde_json::from_value(entry.clone())
            .map_err(|e| DeezerError::Api(format!("Invalid Family profile: {e}")))?;
        if Some(profile.user_id) == current_id || entry["EXTRA_FAMILY"]["IS_LOGGABLE_AS"] == true {
            profiles.push(profile);
        }
    }
    Ok(profiles)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_data(id: u64) -> serde_json::Value {
        serde_json::json!({ "results": {
            "checkForm": "test-csrf", "USER": {
                "USER_ID": id.to_string(), "BLOG_NAME": "Profile",
                "OPTIONS": {"license_token": "test-license"}
            }
        }})
    }

    #[test]
    fn rejected_profile_switch_cannot_fall_back_to_admin() {
        assert!(parse_user_data(&user_data(10), Some(20)).is_err());
        assert!(parse_user_data(&user_data(0), Some(20)).is_err());
        assert_eq!(
            parse_user_data(&user_data(20), Some(20))
                .unwrap()
                .user
                .user_id,
            20
        );
        assert!(parse_user_data(&user_data(10), None).is_ok());
    }

    #[test]
    fn family_picker_excludes_independent_accounts() {
        let profiles = selectable_profiles(serde_json::json!([
            {"USER_ID": "10", "BLOG_NAME": "Admin"},
            {"USER_ID": 20, "BLOG_NAME": "Profile", "EXTRA_FAMILY": {"IS_LOGGABLE_AS": true}},
            {"USER_ID": "30", "BLOG_NAME": "Independent", "EXTRA_FAMILY": {"IS_LOGGABLE_AS": false}}
        ]), Some(10)).unwrap();
        assert_eq!(
            profiles.iter().map(|p| p.user_id).collect::<Vec<_>>(),
            vec![10, 20]
        );
        assert!(selectable_profiles(serde_json::json!({}), Some(10)).is_err());
    }

    /// Live tests need a real ARL token, provided through the environment.
    /// NEVER hardcode a real ARL: it is a full-session credential.
    /// Run with: DEEZER_TEST_ARL=<your-arl> cargo test -- --ignored
    fn test_arl() -> String {
        std::env::var("DEEZER_TEST_ARL").expect("set DEEZER_TEST_ARL env var to run live API tests")
    }

    #[tokio::test]
    #[ignore = "live API: requires valid ARL token + network"]
    async fn test_arl_login() {
        let mut client = DeezerClient::new().unwrap();
        let session = client
            .login_arl(&test_arl())
            .await
            .expect("login_arl failed");

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
    #[ignore = "live API: requires valid ARL token + network"]
    async fn test_search() {
        let mut client = DeezerClient::new().unwrap();
        client.login_arl(&test_arl()).await.expect("login failed");

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
    #[ignore = "live API: requires valid ARL token + network"]
    async fn test_favorites() {
        let mut client = DeezerClient::new().unwrap();
        client.login_arl(&test_arl()).await.expect("login failed");

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
    #[ignore = "live API: requires valid ARL token + network"]
    async fn test_get_track_with_token() {
        let mut client = DeezerClient::new().unwrap();
        client.login_arl(&test_arl()).await.expect("login failed");

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
    #[ignore = "live API: requires network access to deezer.com"]
    async fn test_master_key() {
        let client = DeezerClient::new().unwrap();
        let key = crate::decrypt::fetch_master_key(client.http())
            .await
            .expect("fetch_master_key failed");
        println!("Master key: {:02x?}", key);
        assert_eq!(key.len(), 16);
    }
}
