pub mod auth;
pub mod gateway;
pub mod media;
pub mod models;

use std::sync::{Arc, Mutex};

use reqwest::cookie::Jar;
use reqwest::Client;

use crate::api::auth::Session;
use crate::api::models::DeezerError;

pub struct DeezerClient {
    pub(crate) http: Client,
    pub(crate) cookie_jar: Arc<Jar>,
    pub(crate) session: Option<Session>,
    /// Cached JWT for pipe.deezer.com GraphQL API. Lazily fetched.
    pub(crate) jwt_cache: Mutex<Option<String>>,
}

impl DeezerClient {
    pub fn new() -> Result<Self, DeezerError> {
        let cookie_jar = Arc::new(Jar::default());

        let http = Client::builder()
            .cookie_provider(Arc::clone(&cookie_jar))
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| DeezerError::Http(e.to_string()))?;

        Ok(Self {
            http,
            cookie_jar,
            session: None,
            jwt_cache: Mutex::new(None),
        })
    }

    pub fn http(&self) -> &Client {
        &self.http
    }

    pub fn is_authenticated(&self) -> bool {
        self.session.is_some()
    }

    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }
}
