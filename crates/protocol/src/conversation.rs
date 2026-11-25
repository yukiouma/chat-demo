use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// A single chat thread belonging to one user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "protocol.ts")]
pub struct Conversation {
    pub id: Uuid,
    /// `None` until the conversation has its first message.
    pub title: Option<String>,
    /// The model this conversation uses. Switchable mid-conversation.
    pub model_name: String,
    pub forked_from_conversation_id: Option<Uuid>,
    pub forked_from_message_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Body of `POST /api/conversations`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "protocol.ts")]
pub struct CreateConversation {
    pub model: String,
}

/// Body of `POST /api/conversations/:id/fork`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "protocol.ts")]
pub struct ForkConversation {
    /// The last message of the prefix to copy. The fork starts from the
    /// message after it.
    pub from_message_id: Uuid,
}

/// Body of `PATCH /api/conversations/:id`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "protocol.ts")]
pub struct UpdateConversation {
    pub model: String,
}

/// Body of `PUT /api/conversations/:id/instructions`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "protocol.ts")]
pub struct UpdateInstructions {
    /// The conversation's Instructions. Empty means no instructions at all.
    pub instructions: String,
}
