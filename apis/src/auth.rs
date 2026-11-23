//! HTTP Basic authentication.
//!
//! Every request carries the credential; there is no session and no token.
//! See docs/adr/0002-stateless-basic-authentication.md.

use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use crate::db::UserRow;
use crate::error::ApiError;
use crate::state::AppState;

/// A user whose credentials have been verified.
#[derive(Debug, Clone)]
pub struct AuthUser(pub protocol::User);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .ok_or(ApiError::Unauthorized)?;
        let raw = header.to_str().map_err(|_| ApiError::Unauthorized)?;
        let encoded = raw
            .strip_prefix("Basic ")
            .ok_or(ApiError::Unauthorized)?;
        let decoded = STANDARD
            .decode(encoded.trim())
            .map_err(|_| ApiError::Unauthorized)?;
        let decoded = String::from_utf8(decoded).map_err(|_| ApiError::Unauthorized)?;
        let (username, password) = decoded.split_once(':').ok_or(ApiError::Unauthorized)?;

        let row = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, password FROM users WHERE username = $1",
        )
        .bind(username)
        .fetch_optional(&state.pool)
        .await
        .map_err(ApiError::internal)?;

        let row = row.ok_or(ApiError::Unauthorized)?;
        if !secret_eq(row.password.as_bytes(), password.as_bytes()) {
            return Err(ApiError::Unauthorized);
        }

        Ok(AuthUser(protocol::User {
            id: row.id,
            username: row.username,
        }))
    }
}

/// Compares two secrets without leaking how many bytes matched.
///
/// The lengths are compared first, which does leak the length of the stored
/// password — harmless here, since the account is fixed and the password is
/// in the source anyway.
fn secret_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
