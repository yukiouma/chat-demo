use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// An LLM the service offers for use.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "protocol.ts")]
pub struct Model {
    /// The name the inference engine knows the model by, e.g. `qwen3:32b`.
    pub name: String,
    pub display_name: String,
    pub supports_reasoning: bool,
    pub supports_tools: bool,
    pub enabled: bool,
    pub is_default: bool,
}
