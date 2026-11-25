//! Shared vocabulary between the backend, the desktop app and the browser.
//!
//! These types are the single definition of the wire format. TypeScript
//! equivalents are generated from them by `ts-rs` into `src/generated/protocol.ts`.

mod chunk;
mod conversation;
mod identity;
mod message;
mod model;

pub use chunk::Chunk;
pub use conversation::{
    Conversation, CreateConversation, ForkConversation, UpdateConversation, UpdateInstructions,
};
pub use identity::User;
pub use message::{
    HistoryMessage, Message, MessagePart, MessageRole, MessageStatus, NewMessage, ToolStatus,
};
pub use model::Model;
