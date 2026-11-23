use crate::api::ApiClient;

/// What the app holds while it runs.
pub struct AppState {
    /// Where the backend is.
    pub base_url: String,
    /// The signed-in client, once there is one. It holds the credential, so
    /// the browser never sees it.
    pub client: tokio::sync::RwLock<Option<ApiClient>>,
}

impl AppState {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: tokio::sync::RwLock::new(None),
        }
    }
}
