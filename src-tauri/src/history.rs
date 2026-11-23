//! Rebuilding the model's context from stored messages.
//!
//! This is the one place where our storage shape and the model's expectations
//! do not line up, so it is worth reading before changing it.

use protocol::{HistoryMessage, MessagePart, MessageRole};
use rig::completion::{AssistantContent, AssistantMessage, Message as RigMessage};
use rig::message::{CallId, ToolName, ToolResult, ToolResultContent, UserContent};

/// Turns stored messages into the conversation the model expects.
///
/// Three things happen here that are not obvious:
///
/// - The model wants a tool's call inside an assistant message and its
///   result in the message after it. Our stored message holds the call, the
///   result and the answer that followed, so it is split at the tool.
/// - Reasoning is not replayed. The model does not need to see its own
///   thinking again, and providers are not all willing to accept a message
///   that carries it.
/// - The answer that followed a tool call becomes its own assistant
///   message, so it comes after the tool result rather than before it.
pub fn to_rig_messages(stored: &[HistoryMessage]) -> Vec<RigMessage> {
    let mut out = Vec::new();

    for message in stored {
        match message.role {
            MessageRole::User => {
                out.push(RigMessage::user(protocol::Message::plain_text(&message.parts)))
            }
            MessageRole::Assistant => {
                // The assistant's content, up to the next tool call.
                let mut content: Vec<AssistantContent> = Vec::new();

                for part in &message.parts {
                    match part {
                        MessagePart::Text { text } => {
                            content.push(AssistantContent::text(text.clone()));
                        }
                        MessagePart::Reasoning { .. } => {}
                        MessagePart::Tool {
                            call_id,
                            name,
                            input,
                            output,
                            ..
                        } => {
                            let Ok(tool_name) = ToolName::new(name.clone()) else {
                                continue;
                            };

                            content.push(AssistantContent::tool_call(
                                call_id.clone(),
                                tool_name.clone(),
                                input.clone(),
                            ));

                            let Some(output) = output else {
                                continue;
                            };

                            // Close the assistant message here: the tool
                            // result has to come after it, and the answer
                            // that followed has to come after that.
                            if !content.is_empty() {
                                out.push(RigMessage::Assistant(AssistantMessage::new(
                                    std::mem::take(&mut content),
                                )));
                            }

                            out.push(RigMessage::User {
                                content: vec![UserContent::ToolResult(ToolResult {
                                    call: CallId::from_wire(call_id.clone()),
                                    name: tool_name,
                                    content: vec![ToolResultContent::json(output.clone())],
                                    is_error: false,
                                })],
                            });
                        }
                    }
                }

                if !content.is_empty() {
                    out.push(RigMessage::Assistant(AssistantMessage::new(content)));
                }
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::ToolStatus;

    fn user(text: &str) -> HistoryMessage {
        HistoryMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::User,
            parts: vec![MessagePart::Text { text: text.into() }],
        }
    }

    fn assistant(parts: Vec<MessagePart>) -> HistoryMessage {
        HistoryMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::Assistant,
            parts,
        }
    }

    #[test]
    fn a_user_message_becomes_one_user_message() {
        let rig = to_rig_messages(&[user("hello")]);

        assert_eq!(rig.len(), 1);
        assert!(matches!(rig[0], RigMessage::User { .. }));
    }

    #[test]
    fn a_tool_call_becomes_an_assistant_message_and_a_result_message() {
        let stored = vec![assistant(vec![MessagePart::Tool {
            call_id: "call-1".into(),
            name: "current_time".into(),
            input: serde_json::json!({}),
            output: Some(serde_json::json!("2026-01-01T00:00:00Z")),
            status: ToolStatus::OutputAvailable,
        }])];

        let rig = to_rig_messages(&stored);

        assert_eq!(rig.len(), 2, "the call and its result are separate");

        let RigMessage::Assistant(assistant) = &rig[0] else {
            panic!("the call goes in an assistant message");
        };
        assert_eq!(assistant.content.len(), 1);
        assert!(matches!(
            assistant.content[0],
            AssistantContent::ToolCall(_)
        ));

        let RigMessage::User { content } = &rig[1] else {
            panic!("the result goes in a user message");
        };
        assert_eq!(content.len(), 1);
        assert!(matches!(content[0], UserContent::ToolResult(_)));
    }

    #[test]
    fn the_answer_after_a_tool_comes_after_the_tool_result() {
        // One stored message: the call, its result, and the answer that
        // followed.
        let stored = vec![assistant(vec![
            MessagePart::Tool {
                call_id: "call-1".into(),
                name: "current_time".into(),
                input: serde_json::json!({}),
                output: Some(serde_json::json!("2026-01-01T00:00:00Z")),
                status: ToolStatus::OutputAvailable,
            },
            MessagePart::Text {
                text: "It is noon.".into(),
            },
        ])];

        let rig = to_rig_messages(&stored);

        assert_eq!(rig.len(), 3, "the call, the result, and the answer");

        let RigMessage::Assistant(assistant) = &rig[0] else {
            panic!("the call goes in an assistant message");
        };
        assert!(matches!(
            assistant.content[0],
            AssistantContent::ToolCall(_)
        ));

        let RigMessage::User { content } = &rig[1] else {
            panic!("the result goes in a user message");
        };
        assert!(matches!(content[0], UserContent::ToolResult(_)));

        let RigMessage::Assistant(answer) = &rig[2] else {
            panic!("the answer is an assistant message of its own");
        };
        assert_eq!(answer.content.len(), 1);
        assert!(matches!(answer.content[0], AssistantContent::Text(_)));
    }

    #[test]
    fn a_tool_call_without_an_output_stays_on_its_own() {
        let stored = vec![assistant(vec![MessagePart::Tool {
            call_id: "call-1".into(),
            name: "current_time".into(),
            input: serde_json::json!({}),
            output: None,
            status: ToolStatus::InputAvailable,
        }])];

        let rig = to_rig_messages(&stored);

        assert_eq!(rig.len(), 1);
    }

    #[test]
    fn reasoning_is_not_replayed_to_the_model() {
        let stored = vec![assistant(vec![
            MessagePart::Reasoning {
                text: "thinking".into(),
            },
            MessagePart::Text {
                text: "the answer".into(),
            },
        ])];

        let rig = to_rig_messages(&stored);

        assert_eq!(rig.len(), 1);
        let RigMessage::Assistant(assistant) = &rig[0] else {
            panic!("an assistant message");
        };
        assert_eq!(assistant.content.len(), 1);
        assert!(matches!(assistant.content[0], AssistantContent::Text(_)));
    }
}
