//! The whole desktop path: sign in, ask a question, get an answer, keep it.
//!
//! This needs the backend running. Run it with:
//!
//! ```text
//! cargo run -p apis &
//! cargo test -p crf-agent-demo --test desktop -- --ignored --nocapture
//! ```

use std::sync::{Arc, Mutex};

use crf_agent_demo_lib::api::ApiClient;
use crf_agent_demo_lib::commands::run_turn;
use protocol::{Chunk, HistoryMessage, MessagePart, MessageRole};
use tauri::ipc::{Channel, InvokeResponseBody};
use uuid::Uuid;

fn backend() -> String {
    std::env::var("CRF_API_BASE_URL").unwrap_or_else(|_| "http://localhost:3100".to_string())
}

#[tokio::test]
#[ignore = "needs the backend running"]
async fn a_turn_is_streamed_and_stored() {
    let client = ApiClient::new(backend(), "yuki".into(), "250606".into());

    // The credential is checked, and the user comes back.
    let user = client.me().await.expect("the credential works");
    assert_eq!(user.username, "yuki");

    // Somewhere to talk.
    let conversation = client
        .create_conversation("qwen3:32b")
        .await
        .expect("a conversation");

    // Ask a question.
    let question = HistoryMessage {
        id: Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![MessagePart::Text {
            text: "Say hello in one short sentence.".into(),
        }],
    };

    // Collect everything that streams back.
    let seen: Arc<Mutex<Vec<Chunk>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let channel = Channel::new(move |body: InvokeResponseBody| {
        if let InvokeResponseBody::Json(raw) = body {
            if let Ok(chunk) = serde_json::from_str::<Chunk>(&raw) {
                sink.lock().expect("the sink is not poisoned").push(chunk);
            }
        }
        Ok(())
    });

    run_turn(
        &client,
        &channel,
        conversation.id,
        question.clone(),
        Vec::new(),
        "qwen3:32b".into(),
    )
    .await
    .expect("the turn succeeds");

    // Something was streamed, and it was a start and an answer.
    let seen = seen.lock().expect("the sink is not poisoned").clone();
    assert!(
        seen.iter().any(|chunk| matches!(chunk, Chunk::Start { .. })),
        "the message started: {seen:?}"
    );
    assert!(
        seen.iter()
            .any(|chunk| matches!(chunk, Chunk::TextDelta { .. })),
        "the model answered: {seen:?}"
    );

    // And the whole turn was kept.
    let stored = client
        .list_messages(conversation.id)
        .await
        .expect("the messages");
    assert_eq!(stored.len(), 2, "the question and the answer");
    assert_eq!(stored[0].role, MessageRole::User);
    assert_eq!(stored[0].id.to_string(), question.id);
    assert_eq!(stored[1].role, MessageRole::Assistant);
    assert!(!stored[1].content.is_empty());

    // The answer carries the model's thinking.
    assert!(
        stored[1]
            .parts
            .iter()
            .any(|part| matches!(part, MessagePart::Reasoning { .. })),
        "the thinking was kept"
    );

    // Tidy up.
    client
        .delete_conversation(conversation.id)
        .await
        .expect("the conversation is deleted");
}
