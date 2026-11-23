//! The tools the model may call.

use rig::prelude::{Tool, ToolContext};
use serde::Deserialize;

/// The current date and time, in UTC.
#[derive(Clone)]
pub struct CurrentTime;

/// No arguments: the model only has to ask for the time.
#[derive(Deserialize)]
pub struct CurrentTimeArgs {}

impl Tool for CurrentTime {
    const NAME: &'static str = "current_time";

    type Args = CurrentTimeArgs;
    type Output = String;
    type Error = std::convert::Infallible;

    fn description(&self) -> String {
        "Get the current date and time in UTC. Takes no arguments.".to_owned()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": [],
        })
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        _args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        Ok(chrono::Utc::now().to_rfc3339())
    }
}
