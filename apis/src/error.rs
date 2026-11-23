use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// Everything that can go wrong while handling a request.
#[derive(Debug)]
pub enum ApiError {
    /// The credentials are missing, malformed or wrong.
    Unauthorized,
    /// The thing being asked for does not exist, or does not belong to the user.
    NotFound(&'static str),
    /// The request was understood but cannot be carried out as asked.
    BadRequest(String),
    /// Something went wrong on our side.
    Internal(anyhow::Error),
}

impl ApiError {
    pub fn internal(err: impl Into<anyhow::Error>) -> Self {
        Self::Internal(err.into())
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized => write!(f, "invalid credentials"),
            Self::NotFound(what) => write!(f, "{what} not found"),
            Self::BadRequest(message) => write!(f, "{message}"),
            Self::Internal(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "invalid credentials".to_string()),
            Self::NotFound(what) => (StatusCode::NOT_FOUND, format!("{what} not found")),
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::Internal(err) => {
                // The detail stays in the log; the client gets nothing to act on.
                eprintln!("internal error: {err:#}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
        };

        (status, Json(ErrorBody { error: message })).into_response()
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}
