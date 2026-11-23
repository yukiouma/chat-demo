//! The CRF Agent Demo backend.
//!
//! It authenticates the user, stores conversations and their messages, and
//! serves the catalogue of models. It never runs inference — that happens on
//! the desktop. See docs/design.md.

pub mod auth;
pub mod db;
pub mod error;
pub mod routes;
pub mod state;

use axum::routing::{get, post};
use axum::Router;
use state::AppState;

/// The whole API.
pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/api/me", get(routes::me))
        .route("/api/models", get(routes::list_models))
        .route(
            "/api/conversations",
            get(routes::list_conversations).post(routes::create_conversation),
        )
        .route(
            "/api/conversations/{id}",
            axum::routing::delete(routes::delete_conversation)
                .patch(routes::update_conversation),
        )
        .route(
            "/api/conversations/{id}/messages",
            get(routes::list_messages).post(routes::append_messages),
        )
        .route("/api/conversations/{id}/fork", post(routes::fork_conversation))
        .with_state(state)
}
