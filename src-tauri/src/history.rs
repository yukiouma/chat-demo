//! Rebuilding the model's context from stored messages.
//!
//! This is the one place where our storage shape and the model's expectations
//! do not line up, so it is worth reading before changing it.

use protocol::{HistoryMessage, MessagePart, MessageRole, ToolStatus};
use rig::completion::{AssistantContent, AssistantMessage, Message as RigMessage};
use rig::message::{CallId, Text, ToolName, ToolResult, ToolResultContent, UserContent};

/// What a tool call that never came back is reported as.
const NO_RESULT: &str = "the call did not complete: no result was returned";

/// Turns stored messages into the conversation the model expects.
///
/// Four things happen here that are not obvious:
///
/// - The model wants a tool's call inside an assistant message and its
///   result in the message after it. Our stored message holds the call, the
///   result and the answer that followed, so it is split at the tool. Calls
///   the model made at once stay together in one assistant message, with
///   their results after it.
/// - Reasoning is not replayed. The model does not need to see its own
///   thinking again, and providers are not all willing to accept a message
///   that carries it.
/// - The answer that followed a tool call becomes its own assistant
///   message, so it comes after the tool result rather than before it.
/// - The Instructions are not replayed either: they travel as the preamble,
///   which the agent puts in front of the history.
pub fn to_rig_messages(stored: &[HistoryMessage]) -> Vec<RigMessage> {
    let mut out = Vec::new();

    for message in stored {
        match message.role {
            // The Instructions are the preamble, not part of the dialogue.
            MessageRole::System => continue,
            MessageRole::User => {
                out.push(RigMessage::user(protocol::Message::plain_text(&message.parts)))
            }
            MessageRole::Assistant => {
                // The assistant's content, up to the next tool result.
                let mut content: Vec<AssistantContent> = Vec::new();
                // The results of the tool calls in `content`, waiting to be
                // sent.
                let mut results: Vec<ToolResult> = Vec::new();

                for part in &message.parts {
                    match part {
                        MessagePart::Text { text } => {
                            // Anything said after a tool call comes after
                            // the result of it.
                            flush(&mut content, &mut results, &mut out);
                            content.push(AssistantContent::text(text.clone()));
                        }
                        MessagePart::Reasoning { .. } => {}
                        MessagePart::Tool {
                            call_id,
                            name,
                            input,
                            output,
                            status,
                        } => {
                            let Ok(tool_name) = ToolName::new(name.clone()) else {
                                continue;
                            };

                            content.push(AssistantContent::tool_call(
                                call_id.clone(),
                                tool_name.clone(),
                                input.clone(),
                            ));

                            results.push(match output {
                                Some(output) => ToolResult {
                                    call: CallId::from_wire(call_id.clone()),
                                    name: tool_name,
                                    content: vec![result_content(output.clone())],
                                    // The tool failed, and the model is told
                                    // so: a failure it cannot see is a
                                    // failure it will build on.
                                    is_error: failed(*status),
                                },
                                // The call never came back — the turn was
                                // interrupted. It still needs an answer, or
                                // the model is left holding a call nothing
                                // ever answered.
                                None => ToolResult {
                                    call: CallId::from_wire(call_id.clone()),
                                    name: tool_name,
                                    content: vec![ToolResultContent::Text(Text::new(NO_RESULT))],
                                    is_error: true,
                                },
                            });
                        }
                    }
                }

                flush(&mut content, &mut results, &mut out);

                if !content.is_empty() {
                    out.push(RigMessage::Assistant(AssistantMessage::new(content)));
                }
            }
        }
    }

    out
}

/// Whether a tool run went wrong.
fn failed(status: ToolStatus) -> bool {
    matches!(status, ToolStatus::OutputError | ToolStatus::OutputDenied)
}

/// Closes the assistant message and sends the results of its tool calls.
///
/// The model wants every call in one assistant message and every result
/// after it, so a batch of calls made at once is sent as one message
/// followed by one message per result — not interleaved.
fn flush(
    content: &mut Vec<AssistantContent>,
    results: &mut Vec<ToolResult>,
    out: &mut Vec<RigMessage>,
) {
    if results.is_empty() {
        return;
    }

    if !content.is_empty() {
        out.push(RigMessage::Assistant(AssistantMessage::new(std::mem::take(
            content,
        ))));
    }

    for result in results.drain(..) {
        out.push(RigMessage::User {
            content: vec![UserContent::ToolResult(result)],
        });
    }
}

/// A tool's output, as the model should see it.
///
/// A tool that answered with a string answered with that string, not with
/// its JSON encoding — so it is sent as text.
fn result_content(output: serde_json::Value) -> ToolResultContent {
    match output {
        serde_json::Value::String(text) => ToolResultContent::Text(Text::new(text)),
        other => ToolResultContent::Json { value: other },
    }
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
    fn a_tool_call_that_never_came_back_is_given_an_error_result() {
        // The turn was interrupted between the call and the tool's answer.
        let stored = vec![assistant(vec![MessagePart::Tool {
            call_id: "call-1".into(),
            name: "current_time".into(),
            input: serde_json::json!({}),
            output: None,
            status: ToolStatus::InputAvailable,
        }])];

        let rig = to_rig_messages(&stored);

        assert_eq!(rig.len(), 2, "the call still needs an answer");

        let RigMessage::User { content } = &rig[1] else {
            panic!("the result goes in a user message");
        };
        let UserContent::ToolResult(result) = &content[0] else {
            panic!("the result is a tool result");
        };
        assert!(result.is_error, "a call that never came back is a failure");
    }

    #[test]
    fn a_failed_tool_is_replayed_as_a_failure() {
        let stored = vec![assistant(vec![MessagePart::Tool {
            call_id: "call-1".into(),
            name: "current_time".into(),
            input: serde_json::json!({}),
            output: Some(serde_json::json!("the tool gave up")),
            status: ToolStatus::OutputError,
        }])];

        let rig = to_rig_messages(&stored);

        let RigMessage::User { content } = &rig[1] else {
            panic!("the result goes in a user message");
        };
        let UserContent::ToolResult(result) = &content[0] else {
            panic!("the result is a tool result");
        };
        assert!(result.is_error, "the model is told the tool failed");
    }

    #[test]
    fn a_tool_that_worked_is_not_replayed_as_a_failure() {
        let stored = vec![assistant(vec![MessagePart::Tool {
            call_id: "call-1".into(),
            name: "current_time".into(),
            input: serde_json::json!({}),
            output: Some(serde_json::json!("2026-01-01T00:00:00Z")),
            status: ToolStatus::OutputAvailable,
        }])];

        let rig = to_rig_messages(&stored);

        let RigMessage::User { content } = &rig[1] else {
            panic!("the result goes in a user message");
        };
        let UserContent::ToolResult(result) = &content[0] else {
            panic!("the result is a tool result");
        };
        assert!(!result.is_error);
    }

    #[test]
    fn a_string_result_is_sent_as_text() {
        let stored = vec![assistant(vec![MessagePart::Tool {
            call_id: "call-1".into(),
            name: "current_time".into(),
            input: serde_json::json!({}),
            output: Some(serde_json::json!("2026-01-01T00:00:00Z")),
            status: ToolStatus::OutputAvailable,
        }])];

        let rig = to_rig_messages(&stored);

        let RigMessage::User { content } = &rig[1] else {
            panic!("the result goes in a user message");
        };
        let UserContent::ToolResult(result) = &content[0] else {
            panic!("the result is a tool result");
        };
        let ToolResultContent::Text(text) = &result.content[0] else {
            panic!("the tool answered with a string, so it is sent as text");
        };
        assert_eq!(text.text(), "2026-01-01T00:00:00Z");
    }

    #[test]
    fn calls_made_at_once_are_sent_together() {
        // Two calls in one message: the model wants both calls in one
        // assistant message, and both results after it.
        let stored = vec![assistant(vec![
            MessagePart::Tool {
                call_id: "call-1".into(),
                name: "current_time".into(),
                input: serde_json::json!({}),
                output: Some(serde_json::json!("noon")),
                status: ToolStatus::OutputAvailable,
            },
            MessagePart::Tool {
                call_id: "call-2".into(),
                name: "current_time".into(),
                input: serde_json::json!({}),
                output: Some(serde_json::json!("midnight")),
                status: ToolStatus::OutputAvailable,
            },
        ])];

        let rig = to_rig_messages(&stored);

        assert_eq!(rig.len(), 3, "one message of calls, and one per result");

        let RigMessage::Assistant(assistant) = &rig[0] else {
            panic!("the calls go in one assistant message");
        };
        assert_eq!(assistant.content.len(), 2, "both calls are in it");

        assert!(matches!(rig[1], RigMessage::User { .. }));
        assert!(matches!(rig[2], RigMessage::User { .. }));
    }

    #[test]
    fn the_instructions_are_not_replayed_to_the_model() {
        // The Instructions travel as the preamble, not as history.
        let stored = vec![HistoryMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::System,
            parts: vec![MessagePart::Text {
                text: "Be brief.".into(),
            }],
        }];

        assert!(to_rig_messages(&stored).is_empty());
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
