//! The backend client.
//!
//! It holds the credential, so the browser never sees it.

use protocol::{
    Conversation, CreateConversation, ForkConversation, Message, Model, NewMessage,
    UpdateConversation, User,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use uuid::Uuid;

/// Talks to the backend on the user's behalf.
#[derive(Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    base: String,
    username: String,
    password: String,
}

#[derive(Debug)]
pub enum ApiError {
    /// The credential is missing or wrong.
    Unauthorized,
    /// The thing asked for does not exist.
    NotFound(String),
    /// The request was refused.
    BadRequest(String),
    /// The backend could not be reached, or said something unexpected.
    Transport(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized => write!(f, "invalid credentials"),
            Self::NotFound(what) => write!(f, "{what} not found"),
            Self::BadRequest(message) => write!(f, "{message}"),
            Self::Transport(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ApiError {}

impl ApiClient {
    pub fn new(base: String, username: String, password: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            base: base.trim_end_matches('/').to_string(),
            username,
            password,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    /// Attaches the credential to every request.
    fn authed(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request.basic_auth(&self.username, Some(&self.password))
    }

    async fn send<T: DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<T, ApiError> {
        let response = self.send_raw(request).await?;

        response
            .json::<T>()
            .await
            .map_err(|err| ApiError::Transport(format!("unexpected response: {err}")))
    }

    /// Sends a request that is answered with no body.
    async fn send_empty(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<(), ApiError> {
        self.send_raw(request).await.map(|_| ())
    }

    async fn send_raw(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, ApiError> {
        let response = self
            .authed(request)
            .send()
            .await
            .map_err(|err| ApiError::Transport(format!("could not reach the backend: {err}")))?;

        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        // The backend's error body says what went wrong.
        let detail = response
            .text()
            .await
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|value| value["error"].as_str().map(str::to_owned))
            .unwrap_or_else(|| format!("HTTP {status}"));

        Err(match status.as_u16() {
            401 => ApiError::Unauthorized,
            404 => ApiError::NotFound(detail),
            400 => ApiError::BadRequest(detail),
            _ => ApiError::Transport(detail),
        })
    }

    fn json<T: Serialize + ?Sized>(value: &T) -> Result<serde_json::Value, ApiError> {
        serde_json::to_value(value).map_err(|err| ApiError::Transport(err.to_string()))
    }

    pub async fn me(&self) -> Result<User, ApiError> {
        self.send(self.http.get(self.url("/api/me"))).await
    }

    pub async fn list_models(&self) -> Result<Vec<Model>, ApiError> {
        self.send(self.http.get(self.url("/api/models"))).await
    }

    pub async fn list_conversations(&self) -> Result<Vec<Conversation>, ApiError> {
        self.send(self.http.get(self.url("/api/conversations"))).await
    }

    pub async fn create_conversation(&self, model: &str) -> Result<Conversation, ApiError> {
        let body = Self::json(&CreateConversation {
            model: model.to_owned(),
        })?;
        self.send(self.http.post(self.url("/api/conversations")).json(&body))
            .await
    }

    pub async fn update_conversation(
        &self,
        id: Uuid,
        model: &str,
    ) -> Result<Conversation, ApiError> {
        let body = Self::json(&UpdateConversation {
            model: model.to_owned(),
        })?;
        self.send(
            self.http
                .patch(self.url(&format!("/api/conversations/{id}")))
                .json(&body),
        )
        .await
    }

    pub async fn delete_conversation(&self, id: Uuid) -> Result<(), ApiError> {
        self.send_empty(self.http.delete(self.url(&format!("/api/conversations/{id}"))))
            .await
    }

    pub async fn list_messages(&self, id: Uuid) -> Result<Vec<Message>, ApiError> {
        self.send(self.http.get(self.url(&format!("/api/conversations/{id}/messages"))))
            .await
    }

    pub async fn append_messages(
        &self,
        id: Uuid,
        messages: &[NewMessage],
    ) -> Result<Vec<Message>, ApiError> {
        let body = Self::json(messages)?;
        self.send(
            self.http
                .post(self.url(&format!("/api/conversations/{id}/messages")))
                .json(&body),
        )
        .await
    }

    pub async fn fork_conversation(
        &self,
        id: Uuid,
        from_message_id: Uuid,
    ) -> Result<Conversation, ApiError> {
        let body = Self::json(&ForkConversation { from_message_id })?;
        self.send(
            self.http
                .post(self.url(&format!("/api/conversations/{id}/fork")))
                .json(&body),
        )
        .await
    }
}
