//! Running a turn against the local model.

use futures::StreamExt;
use protocol::{Chunk, Message, MessagePart, MessageRole, MessageStatus, ToolStatus};
use rig::agent::AgentBuilder;
use rig::completion::{AssistantContent, Effort, Message as RigMessage, Reasoning};
use rig::prelude::MultiTurnStreamItem;
use rig::providers::ollama::Ollama;
use rig::streaming::{Item, StreamEvent};
use uuid::Uuid;

use crate::tools::CurrentTime;

const PREAMBLE: &str = "You are a helpful local assistant running on the user's machine. \
     Use the tools you have when they would help, and answer in plain prose.";

/// How many times the model may call a tool before it must answer.
const MAX_TURNS: usize = 4;

/// One assistant message, as it is being assembled.
struct Builder {
    id: Uuid,
    parts: Vec<MessagePart>,
    /// The part being written, and the name it is known by.
    open: Option<OpenPart>,
    /// How many parts have been opened, for naming them.
    opened: usize,
}

/// The part currently being written.
struct OpenPart {
    kind: PartKind,
    id: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PartKind {
    Text,
    Reasoning,
}

impl Builder {
    fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            parts: Vec::new(),
            open: None,
            opened: 0,
        }
    }

    /// Appends to the open text part, or opens one. Returns the part's name.
    fn push_text(&mut self, text: &str) -> String {
        if matches!(
            self.open,
            Some(OpenPart {
                kind: PartKind::Text,
                ..
            })
        ) {
            if let Some(MessagePart::Text { text: existing }) = self.parts.last_mut() {
                existing.push_str(text);
            }
            return self.open.as_ref().expect("it is open").id.clone();
        }

        let id = self.open_part(PartKind::Text);
        self.parts.push(MessagePart::Text {
            text: text.to_owned(),
        });
        id
    }

    /// Appends to the open reasoning part, or opens one. Returns the part's
    /// name.
    fn push_reasoning(&mut self, text: &str) -> String {
        if matches!(
            self.open,
            Some(OpenPart {
                kind: PartKind::Reasoning,
                ..
            })
        ) {
            if let Some(MessagePart::Reasoning { text: existing }) = self.parts.last_mut() {
                existing.push_str(text);
            }
            return self.open.as_ref().expect("it is open").id.clone();
        }

        let id = self.open_part(PartKind::Reasoning);
        self.parts.push(MessagePart::Reasoning {
            text: text.to_owned(),
        });
        id
    }

    /// Closes whatever was being written and names a new part.
    fn open_part(&mut self, kind: PartKind) -> String {
        let id = format!(
            "{}-{}",
            match kind {
                PartKind::Text => "text",
                PartKind::Reasoning => "reasoning",
            },
            self.opened
        );
        self.opened += 1;
        self.open = Some(OpenPart { kind, id: id.clone() });
        id
    }

    fn has_tool(&self, call_id: &str) -> bool {
        self.parts.iter().any(|part| match part {
            MessagePart::Tool { call_id: id, .. } => id == call_id,
            _ => false,
        })
    }

    fn push_tool(&mut self, call_id: &str, name: &str, input: serde_json::Value) {
        // A tool call ends whatever was being written: what comes after it
        // is a new part.
        self.open = None;
        self.parts.push(MessagePart::Tool {
            call_id: call_id.to_owned(),
            name: name.to_owned(),
            input,
            output: None,
            status: ToolStatus::InputAvailable,
        });
    }

    /// Records what the tool returned.
    fn set_tool_output(&mut self, call_id: &str, output: serde_json::Value, is_error: bool) {
        for part in self.parts.iter_mut() {
            if let MessagePart::Tool {
                call_id: id,
                output: slot,
                status,
                ..
            } = part
            {
                if id == call_id {
                    *slot = Some(output);
                    *status = if is_error {
                        ToolStatus::OutputError
                    } else {
                        ToolStatus::OutputAvailable
                    };
                    return;
                }
            }
        }
    }

    fn into_message(self, conversation_id: Uuid, status: MessageStatus) -> Message {
        Message {
            id: self.id,
            conversation_id,
            role: MessageRole::Assistant,
            // The backend gives the message its position.
            seq: 0,
            content: Message::plain_text(&self.parts),
            parts: self.parts,
            status,
            created_at: chrono::Utc::now(),
        }
    }
}

/// What one turn produced.
pub struct Turn {
    /// The assistant's messages. The backend gives them their positions.
    pub messages: Vec<Message>,
    /// Why the turn stopped, if it stopped badly.
    pub failure: Option<String>,
}

/// Runs one turn, reporting what happens as it happens.
///
/// `history` is everything before the prompt. Whatever the model produced is
/// returned, even when the turn fails part way through.
pub async fn run_turn(
    conversation_id: Uuid,
    model: &str,
    history: Vec<RigMessage>,
    prompt: RigMessage,
    mut emit: impl FnMut(Chunk),
) -> Turn {
    // `from_env` honours OLLAMA_API_BASE_URL, so the daemon does not have to
    // be on this machine.
    let provider = match Ollama::from_env() {
        Ok(provider) => provider,
        Err(err) => {
            let message = format!("could not reach Ollama: {err}");
            emit(Chunk::Error {
                message_id: Uuid::nil(),
                message: message.clone(),
            });
            return Turn {
                messages: Vec::new(),
                failure: Some(message),
            };
        }
    };

    let agent = AgentBuilder::new(provider.native_completion(model))
        .preamble(PREAMBLE)
        .tool(CurrentTime)
        .default_max_turns(MAX_TURNS)
        .reasoning(Reasoning::Effort(Effort::Medium))
        .build();

    let mut stream = agent.prompt(prompt).history(history).stream();

    let mut messages: Vec<Message> = Vec::new();
    let mut current: Option<Builder> = None;
    let mut failure: Option<String> = None;

    while let Some(item) = stream.next().await {
        let item = match item {
            Ok(item) => item,
            Err(err) => {
                failure = Some(err.to_string());
                break;
            }
        };

        match item {
            MultiTurnStreamItem::StreamAssistantItem(Item::Event(event)) => match event {
                StreamEvent::Text { text, .. } => {
                    let builder = start(&mut current, &mut emit);
                    let part_id = builder.push_text(&text);
                    emit(Chunk::TextDelta {
                        message_id: builder.id,
                        part_id,
                        text,
                    });
                }
                StreamEvent::Reasoning { text, .. } => {
                    let builder = start(&mut current, &mut emit);
                    let part_id = builder.push_reasoning(&text);
                    emit(Chunk::ReasoningDelta {
                        message_id: builder.id,
                        part_id,
                        text,
                    });
                }
                StreamEvent::End { content, .. } => {
                    if let AssistantContent::ToolCall(call) = content {
                        let call_id = call.id.to_string();
                        let name = call.function.name.as_str().to_owned();
                        let input = serde_json::Value::Object(call.function.arguments.clone());

                        let builder = start(&mut current, &mut emit);
                        if !builder.has_tool(&call_id) {
                            builder.push_tool(&call_id, &name, input.clone());
                            emit(Chunk::ToolCall {
                                message_id: builder.id,
                                call_id,
                                name,
                                input,
                            });
                        }
                    }
                }
                _ => {}
            },

            MultiTurnStreamItem::ToolCall { tool_call } => {
                let call_id = tool_call.id.to_string();
                let name = tool_call.function.name.as_str().to_owned();
                let input = serde_json::Value::Object(tool_call.function.arguments.clone());

                let builder = start(&mut current, &mut emit);
                if !builder.has_tool(&call_id) {
                    builder.push_tool(&call_id, &name, input.clone());
                    emit(Chunk::ToolCall {
                        message_id: builder.id,
                        call_id,
                        name,
                        input,
                    });
                }
            }

            MultiTurnStreamItem::ToolResult { tool_result } => {
                let call_id = tool_result.call.to_string();
                let name = tool_result.name.as_str().to_owned();
                let output = tool_output(&tool_result.content);
                let is_error = tool_result.is_error;

                let Some(builder) = current.as_mut() else {
                    continue;
                };
                builder.set_tool_output(&call_id, output.clone(), is_error);
                emit(Chunk::ToolResult {
                    message_id: builder.id,
                    call_id,
                    name,
                    output,
                    is_error,
                });

                // The message keeps going: the model answers after the tool,
                // and that answer belongs to the same turn.
            }

            _ => {}
        }
    }

    // Whatever was generated before a failure is kept.
    let status = if failure.is_some() {
        MessageStatus::Error
    } else {
        MessageStatus::Sent
    };

    if let Some(builder) = current.take() {
        emit(Chunk::Finish {
            message_id: builder.id,
        });
        messages.push(builder.into_message(conversation_id, status));
    }

    if let Some(message) = &failure {
        emit(Chunk::Error {
            message_id: messages.last().map(|m| m.id).unwrap_or_else(Uuid::nil),
            message: message.clone(),
        });
    }

    Turn { messages, failure }
}

/// Returns the message being built, starting one if there is none.
fn start<'a>(
    current: &'a mut Option<Builder>,
    emit: &mut impl FnMut(Chunk),
) -> &'a mut Builder {
    if current.is_none() {
        let builder = Builder::new();
        emit(Chunk::Start {
            message_id: builder.id,
        });
        *current = Some(builder);
    }
    current.as_mut().expect("just set")
}

/// The tool's output, as JSON.
fn tool_output(content: &[rig::message::ToolResultContent]) -> serde_json::Value {
    let values: Vec<serde_json::Value> = content
        .iter()
        .map(|part| match part {
            rig::message::ToolResultContent::Text(text) => {
                serde_json::Value::String(text.text().to_owned())
            }
            rig::message::ToolResultContent::Json { value } => value.clone(),
            other => serde_json::Value::String(format!("{other:?}")),
        })
        .collect();

    match values.len() {
        0 => serde_json::Value::Null,
        1 => values.into_iter().next().expect("one value"),
        _ => serde_json::Value::Array(values),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_of_thinking_is_one_part() {
        let mut builder = Builder::new();

        let first = builder.push_reasoning("thinking");
        let second = builder.push_reasoning(" more");

        assert_eq!(first, second, "the same run is one part");
        assert_eq!(builder.parts.len(), 1);
    }

    #[test]
    fn a_tool_call_closes_the_part_being_written() {
        let mut builder = Builder::new();

        let before = builder.push_reasoning("thinking");
        builder.push_tool("call-1", "current_time", serde_json::json!({}));
        let after = builder.push_reasoning("thinking again");

        assert_ne!(before, after, "what follows a tool is a new part");
        assert_eq!(builder.parts.len(), 3);
        assert!(matches!(builder.parts[0], MessagePart::Reasoning { .. }));
        assert!(matches!(builder.parts[1], MessagePart::Tool { .. }));
        assert!(matches!(builder.parts[2], MessagePart::Reasoning { .. }));
    }

    #[test]
    fn a_change_of_kind_starts_a_new_part() {
        let mut builder = Builder::new();

        let reasoning = builder.push_reasoning("thinking");
        let text = builder.push_text("the answer");

        assert_ne!(reasoning, text);
        assert_eq!(builder.parts.len(), 2);
    }

    #[test]
    fn the_parts_keep_the_order_they_arrived_in() {
        let mut builder = Builder::new();

        builder.push_reasoning("thinking");
        builder.push_tool("call-1", "current_time", serde_json::json!({}));
        builder.push_reasoning("thinking again");
        builder.push_text("the answer");

        let kinds: Vec<&str> = builder
            .parts
            .iter()
            .map(|part| match part {
                MessagePart::Text { .. } => "text",
                MessagePart::Reasoning { .. } => "reasoning",
                MessagePart::Tool { .. } => "tool",
            })
            .collect();

        assert_eq!(kinds, ["reasoning", "tool", "reasoning", "text"]);
    }
}
