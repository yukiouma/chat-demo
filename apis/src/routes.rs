//! The HTTP API.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use protocol::{
    Conversation, CreateConversation, ForkConversation, Message, MessagePart, MessageRole, Model,
    NewMessage, UpdateConversation, UpdateInstructions, User,
};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::db::{role_to_str, status_to_str, ConversationRow, MessageRow, ModelRow};
use crate::error::ApiError;
use crate::state::AppState;

/// How much of the first message becomes the conversation's title.
const TITLE_LENGTH: usize = 60;

/// `GET /api/me`
pub async fn me(user: AuthUser) -> Json<User> {
    Json(user.0)
}

/// `GET /api/models`
pub async fn list_models(State(state): State<AppState>) -> Result<Json<Vec<Model>>, ApiError> {
    let rows = sqlx::query_as::<_, ModelRow>(
        "SELECT name, display_name, supports_reasoning, supports_tools, enabled, is_default \
         FROM models WHERE enabled ORDER BY is_default DESC, display_name",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::internal)?;

    Ok(Json(rows.into_iter().map(Model::from).collect()))
}

/// `GET /api/conversations`
pub async fn list_conversations(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<Conversation>>, ApiError> {
    let rows = sqlx::query_as::<_, ConversationRow>(
        "SELECT id, user_id, title, model_name, forked_from_conversation_id, \
                forked_from_message_id, created_at, updated_at \
         FROM conversations WHERE user_id = $1 ORDER BY updated_at DESC",
    )
    .bind(user.0.id)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::internal)?;

    Ok(Json(rows.into_iter().map(Conversation::from).collect()))
}

/// `POST /api/conversations`
pub async fn create_conversation(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<CreateConversation>,
) -> Result<Json<Conversation>, ApiError> {
    ensure_model_exists(&state, &body.model).await?;

    let row = sqlx::query_as::<_, ConversationRow>(
        "INSERT INTO conversations (id, user_id, title, model_name) \
         VALUES ($1, $2, NULL, $3) \
         RETURNING id, user_id, title, model_name, forked_from_conversation_id, \
                   forked_from_message_id, created_at, updated_at",
    )
    .bind(Uuid::new_v4())
    .bind(user.0.id)
    .bind(&body.model)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::internal)?;

    Ok(Json(Conversation::from(row)))
}

/// `PATCH /api/conversations/:id`
pub async fn update_conversation(
    State(state): State<AppState>,
    user: AuthUser,
    Path(conversation_id): Path<Uuid>,
    Json(body): Json<UpdateConversation>,
) -> Result<Json<Conversation>, ApiError> {
    ensure_model_exists(&state, &body.model).await?;

    let row = sqlx::query_as::<_, ConversationRow>(
        "UPDATE conversations SET model_name = $3, updated_at = now() \
         WHERE id = $1 AND user_id = $2 \
         RETURNING id, user_id, title, model_name, forked_from_conversation_id, \
                   forked_from_message_id, created_at, updated_at",
    )
    .bind(conversation_id)
    .bind(user.0.id)
    .bind(&body.model)
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::internal)?
    .ok_or(ApiError::NotFound("conversation"))?;

    Ok(Json(Conversation::from(row)))
}

/// `DELETE /api/conversations/:id`
pub async fn delete_conversation(
    State(state): State<AppState>,
    user: AuthUser,
    Path(conversation_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let deleted = sqlx::query("DELETE FROM conversations WHERE id = $1 AND user_id = $2")
        .bind(conversation_id)
        .bind(user.0.id)
        .execute(&state.pool)
        .await
        .map_err(ApiError::internal)?;

    if deleted.rows_affected() == 0 {
        return Err(ApiError::NotFound("conversation"));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/conversations/:id/messages`
pub async fn list_messages(
    State(state): State<AppState>,
    user: AuthUser,
    Path(conversation_id): Path<Uuid>,
) -> Result<Json<Vec<Message>>, ApiError> {
    require_conversation(&state, user.0.id, conversation_id).await?;

    let rows = sqlx::query_as::<_, MessageRow>(
        "SELECT id, conversation_id, role, seq, parts, content, status, created_at \
         FROM messages WHERE conversation_id = $1 ORDER BY seq",
    )
    .bind(conversation_id)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::internal)?;

    Ok(Json(rows.into_iter().map(MessageRow::into_message).collect()))
}

/// `PUT /api/conversations/:id/instructions`
///
/// The conversation's Instructions: one message of its own, written by the
/// application. Setting them again replaces what is there; there is never
/// more than one.
///
/// It sits at the head of the conversation, so every fork inherits it.
pub async fn set_instructions(
    State(state): State<AppState>,
    user: AuthUser,
    Path(conversation_id): Path<Uuid>,
    Json(body): Json<UpdateInstructions>,
) -> Result<Json<Message>, ApiError> {
    require_conversation(&state, user.0.id, conversation_id).await?;

    let instructions = body.instructions.trim().to_owned();

    let parts = serde_json::to_value(&[MessagePart::Text {
        text: instructions.clone(),
    }])
    .map_err(ApiError::internal)?;

    let mut tx = state.pool.begin().await.map_err(ApiError::internal)?;

    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM messages WHERE conversation_id = $1 AND role = 'system'",
    )
    .bind(conversation_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::internal)?;

    let row = match existing {
        Some(id) => {
            sqlx::query_as::<_, MessageRow>(
                "UPDATE messages SET parts = $2, content = $3 \
                 WHERE id = $1 \
                 RETURNING id, conversation_id, role, seq, parts, content, status, created_at",
            )
            .bind(id)
            .bind(&parts)
            .bind(&instructions)
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::internal)?
        }
        None => {
            sqlx::query_as::<_, MessageRow>(
                "INSERT INTO messages (id, conversation_id, role, seq, parts, content, status) \
                 VALUES ($1, $2, $3, 0, $4, $5, 'sent') \
                 RETURNING id, conversation_id, role, seq, parts, content, status, created_at",
            )
            .bind(Uuid::new_v4())
            .bind(conversation_id)
            .bind(role_to_str(MessageRole::System))
            .bind(&parts)
            .bind(&instructions)
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::internal)?
        }
    };

    sqlx::query("UPDATE conversations SET updated_at = now() WHERE id = $1")
        .bind(conversation_id)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::internal)?;

    tx.commit().await.map_err(ApiError::internal)?;

    Ok(Json(row.into_message()))
}

/// `POST /api/conversations/:id/messages`
///
/// Called twice per turn: once with the user's message, once with the
/// assistant's.
pub async fn append_messages(
    State(state): State<AppState>,
    user: AuthUser,
    Path(conversation_id): Path<Uuid>,
    Json(body): Json<Vec<NewMessage>>,
) -> Result<Json<Vec<Message>>, ApiError> {
    let conversation = require_conversation(&state, user.0.id, conversation_id).await?;

    let mut tx = state.pool.begin().await.map_err(ApiError::internal)?;

    let last_seq: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), 0) FROM messages WHERE conversation_id = $1",
    )
    .bind(conversation_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::internal)?;

    let mut seq = last_seq;
    let mut inserted = Vec::with_capacity(body.len());
    let mut title_from: Option<String> = None;

    for new in body {
        // The Instructions have an endpoint of their own: there is one of
        // them, and it belongs at the head of the conversation.
        if new.role == MessageRole::System {
            return Err(ApiError::BadRequest(
                "use PUT /api/conversations/:id/instructions".to_owned(),
            ));
        }

        seq += 1;

        let parts = serde_json::to_value(&new.parts).map_err(ApiError::internal)?;
        let content = Message::plain_text(&new.parts);

        let row = sqlx::query_as::<_, MessageRow>(
            "INSERT INTO messages (id, conversation_id, role, seq, parts, content, status) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             RETURNING id, conversation_id, role, seq, parts, content, status, created_at",
        )
        .bind(new.id)
        .bind(conversation_id)
        .bind(role_to_str(new.role))
        .bind(seq)
        .bind(&parts)
        .bind(&content)
        .bind(status_to_str(new.status))
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::internal)?;

        // The title comes from the first thing the user said.
        if conversation.title.is_none()
            && title_from.is_none()
            && new.role == MessageRole::User
            && !content.is_empty()
        {
            title_from = Some(truncate(&content, TITLE_LENGTH));
        }

        inserted.push(row.into_message());
    }

    if let Some(title) = title_from {
        sqlx::query("UPDATE conversations SET title = $2 WHERE id = $1 AND title IS NULL")
            .bind(conversation_id)
            .bind(title)
            .execute(&mut *tx)
            .await
            .map_err(ApiError::internal)?;
    }

    sqlx::query("UPDATE conversations SET updated_at = now() WHERE id = $1")
        .bind(conversation_id)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::internal)?;

    tx.commit().await.map_err(ApiError::internal)?;

    Ok(Json(inserted))
}

/// `POST /api/conversations/:id/fork`
///
/// Copies the conversation's messages up to and including the fork point into
/// a new conversation, which then continues independently.
pub async fn fork_conversation(
    State(state): State<AppState>,
    user: AuthUser,
    Path(conversation_id): Path<Uuid>,
    Json(body): Json<ForkConversation>,
) -> Result<Json<Conversation>, ApiError> {
    let source = require_conversation(&state, user.0.id, conversation_id).await?;

    let mut tx = state.pool.begin().await.map_err(ApiError::internal)?;

    let fork_seq: Option<i32> = sqlx::query_scalar(
        "SELECT seq FROM messages WHERE id = $1 AND conversation_id = $2",
    )
    .bind(body.from_message_id)
    .bind(conversation_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::internal)?;

    let Some(fork_seq) = fork_seq else {
        return Err(ApiError::NotFound("message"));
    };

    let new_id = Uuid::new_v4();

    let row = sqlx::query_as::<_, ConversationRow>(
        "INSERT INTO conversations \
         (id, user_id, title, model_name, forked_from_conversation_id, forked_from_message_id) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING id, user_id, title, model_name, forked_from_conversation_id, \
                   forked_from_message_id, created_at, updated_at",
    )
    .bind(new_id)
    .bind(user.0.id)
    .bind(&source.title)
    .bind(&source.model_name)
    .bind(conversation_id)
    .bind(body.from_message_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::internal)?;

    // The prefix keeps its order, timestamps and text, but each message gets
    // a new identity: ids are unique across the whole database.
    sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, seq, parts, content, status, created_at) \
         SELECT gen_random_uuid(), $1, role, seq, parts, content, status, created_at \
         FROM messages WHERE conversation_id = $2 AND seq <= $3",
    )
    .bind(new_id)
    .bind(conversation_id)
    .bind(fork_seq)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::internal)?;

    tx.commit().await.map_err(ApiError::internal)?;

    Ok(Json(Conversation::from(row)))
}

/// Fetches a conversation the user owns, or reports that it does not exist.
async fn require_conversation(
    state: &AppState,
    user_id: Uuid,
    conversation_id: Uuid,
) -> Result<ConversationRow, ApiError> {
    sqlx::query_as::<_, ConversationRow>(
        "SELECT id, user_id, title, model_name, forked_from_conversation_id, \
                forked_from_message_id, created_at, updated_at \
         FROM conversations WHERE id = $1 AND user_id = $2",
    )
    .bind(conversation_id)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::internal)?
    .ok_or(ApiError::NotFound("conversation"))
}

/// Rejects a model the catalogue does not offer.
async fn ensure_model_exists(state: &AppState, name: &str) -> Result<(), ApiError> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM models WHERE name = $1 AND enabled)",
    )
    .bind(name)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::internal)?;

    if exists {
        Ok(())
    } else {
        Err(ApiError::BadRequest(format!("unknown model '{name}'")))
    }
}

/// Cuts a string to `max` characters, marking that it was cut.
fn truncate(text: &str, max: usize) -> String {
    let mut out: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        out.push('…');
    }
    out
}
