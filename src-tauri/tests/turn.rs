//! A turn against a real model.
//!
//! This needs Ollama running with `qwen3:32b` pulled, so it is ignored by
//! default. Run it with:
//!
//! ```text
//! cargo test -p crf-agent-demo --test turn -- --ignored --nocapture
//! ```

use crf_agent_demo_lib::agent::run_turn;
use protocol::Chunk;
use uuid::Uuid;

#[tokio::test]
#[ignore = "needs a local Ollama with qwen3:32b pulled"]
async fn streams_the_answer_thinking_and_the_tool() {
    let conversation_id = Uuid::new_v4();
    let mut seen: Vec<Chunk> = Vec::new();

    let turn = run_turn(
        conversation_id,
        "qwen3:32b",
        Vec::new(),
        rig::completion::Message::user(
            "What time is it right now? Use the current_time tool to find out, then tell me.",
        ),
        None,
        |chunk| seen.push(chunk),
    )
    .await;

    assert!(
        turn.failure.is_none(),
        "the turn failed: {:?}",
        turn.failure
    );

    assert!(
        seen.iter()
            .any(|chunk| matches!(chunk, Chunk::TextDelta { .. })),
        "the model said something"
    );

    assert!(
        seen.iter()
            .any(|chunk| matches!(chunk, Chunk::ToolCall { .. })),
        "the model called the tool: {seen:?}"
    );

    assert!(
        seen.iter()
            .any(|chunk| matches!(chunk, Chunk::ToolResult { .. })),
        "the tool ran"
    );

    // The whole turn is one assistant message, so the UI can show it as one
    // bubble: the thinking, the tool call, and the answer.
    assert_eq!(turn.messages.len(), 1, "one message for the turn");

    let parts = &turn.messages[0].parts;
    assert!(
        parts
            .iter()
            .any(|part| matches!(part, protocol::MessagePart::Tool { .. })),
        "the tool call was kept: {parts:?}"
    );
    assert!(
        parts
            .iter()
            .any(|part| matches!(part, protocol::MessagePart::Reasoning { .. })),
        "the thinking was kept: {parts:?}"
    );
    assert!(
        parts
            .iter()
            .any(|part| matches!(part, protocol::MessagePart::Text { .. })),
        "the answer was kept: {parts:?}"
    );

    // The tool's result is kept with the call, as the UI expects them.
    let tool_output = parts.iter().find_map(|part| match part {
        protocol::MessagePart::Tool { output, .. } => output.clone(),
        _ => None,
    });
    assert!(tool_output.is_some(), "the tool's output was kept");

    // One start, one finish: the whole turn is one message to the UI.
    assert_eq!(
        seen.iter()
            .filter(|chunk| matches!(chunk, Chunk::Start { .. }))
            .count(),
        1
    );
    assert_eq!(
        seen.iter()
            .filter(|chunk| matches!(chunk, Chunk::Finish { .. }))
            .count(),
        1
    );

    println!("chunks: {:#?}", seen);
}
