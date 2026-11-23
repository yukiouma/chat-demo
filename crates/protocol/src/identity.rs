use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// A person who can sign in.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "protocol.ts")]
pub struct User {
    pub id: Uuid,
    pub username: String,
}
