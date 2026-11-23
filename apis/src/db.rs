//! Database rows and the conversions into the shared protocol types.

use chrono::{DateTime, Utc};
use protocol::{Conversation, Message, MessageRole, MessageStatus, Model};
use serde_json::Value;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub password: String,
}

#[derive(sqlx::FromRow)]
pub struct ModelRow {
    pub name: String,
    pub display_name: String,
    pub supports_reasoning: bool,
    pub supports_tools: bool,
    pub enabled: bool,
    pub is_default: bool,
}

impl From<ModelRow> for Model {
    fn from(row: ModelRow) -> Self {
        Self {
            name: row.name,
            display_name: row.display_name,
            supports_reasoning: row.supports_reasoning,
            supports_tools: row.supports_tools,
            enabled: row.enabled,
            is_default: row.is_default,
        }
    }
}

#[derive(sqlx::FromRow)]
pub struct ConversationRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub title: Option<String>,
    pub model_name: String,
    pub forked_from_conversation_id: Option<Uuid>,
    pub forked_from_message_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<ConversationRow> for Conversation {
    fn from(row: ConversationRow) -> Self {
        Self {
            id: row.id,
            title: row.title,
            model_name: row.model_name,
            forked_from_conversation_id: row.forked_from_conversation_id,
            forked_from_message_id: row.forked_from_message_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
pub struct MessageRow {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub role: String,
    pub seq: i32,
    pub parts: Value,
    pub content: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

impl MessageRow {
    pub fn into_message(self) -> Message {
        Message {
            id: self.id,
            conversation_id: self.conversation_id,
            role: role_from_str(&self.role),
            seq: self.seq,
            // Parts are stored in our own vocabulary; anything we cannot read
            // becomes an empty message rather than a failed request.
            parts: serde_json::from_value(self.parts).unwrap_or_default(),
            content: self.content,
            status: status_from_str(&self.status),
            created_at: self.created_at,
        }
    }
}

pub fn role_to_str(role: MessageRole) -> &'static str {
    match role {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
    }
}

fn role_from_str(role: &str) -> MessageRole {
    match role {
        "user" => MessageRole::User,
        _ => MessageRole::Assistant,
    }
}

pub fn status_to_str(status: MessageStatus) -> &'static str {
    match status {
        MessageStatus::Sent => "sent",
        MessageStatus::Error => "error",
        MessageStatus::Cancelled => "cancelled",
    }
}

fn status_from_str(status: &str) -> MessageStatus {
    match status {
        "error" => MessageStatus::Error,
        "cancelled" => MessageStatus::Cancelled,
        _ => MessageStatus::Sent,
    }
}
