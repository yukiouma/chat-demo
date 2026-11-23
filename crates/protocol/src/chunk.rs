use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// A delta of a streamed message.
///
/// These are the chunks the desktop app sends over a Tauri channel. The
/// browser maps them onto the UI library's own chunk types; the library's
/// types never cross into Rust, because they are TypeScript-only and would
/// fail silently if they drifted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "protocol.ts")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Chunk {
    /// Opens a message. The first chunk of every message.
    Start { message_id: Uuid },
    /// A fragment of the message's text. `part_id` names the part, so that
    /// a second run of text — after a tool call, say — is a new part rather
    /// than more of the first.
    TextDelta {
        message_id: Uuid,
        part_id: String,
        text: String,
    },
    /// A fragment of the model's thinking. `part_id` names the part, as it
    /// does for text.
    ReasoningDelta {
        message_id: Uuid,
        part_id: String,
        text: String,
    },
    /// The model has asked for a named tool, and we know its arguments.
    ToolCall {
        message_id: Uuid,
        call_id: String,
        name: String,
        input: serde_json::Value,
    },
    /// The tool has run, with its output.
    ToolResult {
        message_id: Uuid,
        call_id: String,
        name: String,
        output: serde_json::Value,
        is_error: bool,
    },
    /// The message is complete.
    Finish { message_id: Uuid },
    /// The turn failed. Whatever was generated before this is kept.
    Error { message_id: Uuid, message: String },
}
