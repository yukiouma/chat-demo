//! Integration tests for the HTTP API.
//!
//! Each test gets its own database, built from the real migrations, so the
//! schema under test is the schema that ships.

use apis::{app, state::AppState};
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const USER: &str = "yuki";
const PASSWORD: &str = "250606";

fn auth_header(user: &str, password: &str) -> String {
    format!("Basic {}", STANDARD.encode(format!("{user}:{password}")))
}

fn request(method: &str, uri: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(USER, PASSWORD));

    let body = match body {
        Some(value) => {
            builder = builder.header("Content-Type", "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };

    builder.body(body).expect("request builds")
}

/// Sends a request and returns the status and the parsed body.
async fn call(pool: &PgPool, request: Request<Body>) -> (StatusCode, Value) {
    let response = app(AppState {
        pool: pool.clone(),
    })
    .oneshot(request)
    .await
    .expect("router responds");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body reads");

    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };

    (status, body)
}

/// A conversation with one user message and one assistant message.
async fn seed_conversation(pool: &PgPool) -> (Uuid, Vec<Uuid>) {
    let conversation_id = Uuid::new_v4();
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(USER)
        .fetch_one(pool)
        .await
        .expect("seeded user exists");

    sqlx::query(
        "INSERT INTO conversations (id, user_id, title, model_name) VALUES ($1, $2, 'Seeded', 'qwen3:32b')",
    )
    .bind(conversation_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("conversation inserts");

    let mut ids = Vec::new();
    for (seq, role, text) in [
        (1, "user", "first"),
        (2, "assistant", "second"),
        (3, "user", "third"),
    ] {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO messages (id, conversation_id, role, seq, parts, content) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(id)
        .bind(conversation_id)
        .bind(role)
        .bind(seq)
        .bind(json!([{ "type": "text", "text": text }]))
        .bind(text)
        .execute(pool)
        .await
        .expect("message inserts");
        ids.push(id);
    }

    (conversation_id, ids)
}

// -- authentication ---------------------------------------------------------

#[sqlx::test]
async fn rejects_a_request_without_credentials(pool: PgPool) {
    let response = app(AppState { pool })
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn rejects_a_wrong_password(pool: PgPool) {
    let request = Request::builder()
        .uri("/api/me")
        .header("Authorization", auth_header(USER, "not the password"))
        .body(Body::empty())
        .unwrap();

    let (status, _) = call(&pool, request).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn returns_the_signed_in_user(pool: PgPool) {
    let (status, body) = call(&pool, request("GET", "/api/me", None)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["username"], json!(USER));
    assert!(body["id"].as_str().is_some());
}

// -- models -----------------------------------------------------------------

#[sqlx::test]
async fn lists_the_seeded_model(pool: PgPool) {
    let (status, body) = call(&pool, request("GET", "/api/models", None)).await;

    assert_eq!(status, StatusCode::OK);
    let models = body.as_array().expect("a list of models");
    assert_eq!(models.len(), 1);
    assert_eq!(models[0]["name"], json!("qwen3:32b"));
    assert_eq!(models[0]["supports_reasoning"], json!(true));
}

// -- conversations ----------------------------------------------------------

#[sqlx::test]
async fn creates_a_conversation(pool: PgPool) {
    let (status, body) = call(
        &pool,
        request("POST", "/api/conversations", Some(json!({ "model": "qwen3:32b" }))),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["model_name"], json!("qwen3:32b"));
    assert_eq!(body["title"], Value::Null);
}

#[sqlx::test]
async fn refuses_a_model_the_catalogue_does_not_offer(pool: PgPool) {
    let (status, _) = call(
        &pool,
        request("POST", "/api/conversations", Some(json!({ "model": "ghost:1b" }))),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn lists_a_users_conversations_most_recent_first(pool: PgPool) {
    let (conversation_id, _) = seed_conversation(&pool).await;

    let (status, body) = call(
        &pool,
        request("GET", "/api/conversations", None),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let conversations = body.as_array().expect("a list of conversations");
    assert_eq!(conversations.len(), 1);
    assert_eq!(conversations[0]["id"], json!(conversation_id.to_string()));
}

#[sqlx::test]
async fn switches_the_model_mid_conversation(pool: PgPool) {
    let (conversation_id, _) = seed_conversation(&pool).await;

    let (status, body) = call(
        &pool,
        request(
            "PATCH",
            &format!("/api/conversations/{conversation_id}"),
            Some(json!({ "model": "qwen3:32b" })),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["model_name"], json!("qwen3:32b"));
}

#[sqlx::test]
async fn deletes_a_conversation_and_its_messages(pool: PgPool) {
    let (conversation_id, _) = seed_conversation(&pool).await;

    let (status, _) = call(
        &pool,
        request(
            "DELETE",
            &format!("/api/conversations/{conversation_id}"),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE conversation_id = $1",
    )
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(remaining, 0, "the messages go with the conversation");
}

// -- messages ---------------------------------------------------------------

#[sqlx::test]
async fn appends_messages_and_numbers_them_in_order(pool: PgPool) {
    let (conversation_id, _) = seed_conversation(&pool).await;

    let new_id = Uuid::new_v4();
    let (status, body) = call(
        &pool,
        request(
            "POST",
            &format!("/api/conversations/{conversation_id}/messages"),
            Some(json!([{
                "id": new_id,
                "role": "user",
                "parts": [{ "type": "text", "text": "fourth" }],
            }])),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let messages = body.as_array().expect("the appended message");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["seq"], json!(4));
    assert_eq!(messages[0]["content"], json!("fourth"));
    assert_eq!(messages[0]["id"], json!(new_id.to_string()));
}

#[sqlx::test]
async fn titles_a_conversation_from_the_first_user_message(pool: PgPool) {
    let (_, body) = call(
        &pool,
        request("POST", "/api/conversations", Some(json!({ "model": "qwen3:32b" }))),
    )
    .await;
    let conversation_id = body["id"].as_str().unwrap().to_string();

    let long = "a".repeat(100);
    let (status, _) = call(
        &pool,
        request(
            "POST",
            &format!("/api/conversations/{conversation_id}/messages"),
            Some(json!([{
                "id": Uuid::new_v4(),
                "role": "user",
                "parts": [{ "type": "text", "text": long }],
            }])),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = call(
        &pool,
        request("GET", "/api/conversations", None),
    )
    .await;
    let title = body[0]["title"].as_str().unwrap();
    assert_eq!(title.chars().count(), 61, "the title is cut short");
    assert!(title.ends_with('…'));
}

#[sqlx::test]
async fn keeps_the_thinking_and_the_tool_call_of_a_message(pool: PgPool) {
    let (conversation_id, _) = seed_conversation(&pool).await;

    let (status, body) = call(
        &pool,
        request(
            "POST",
            &format!("/api/conversations/{conversation_id}/messages"),
            Some(json!([{
                "id": Uuid::new_v4(),
                "role": "assistant",
                "parts": [
                    { "type": "reasoning", "text": "thinking" },
                    {
                        "type": "tool",
                        "call_id": "call-1",
                        "name": "current_time",
                        "input": {},
                        "output": { "now": "2026-01-01T00:00:00Z" },
                        "status": "output_available",
                    },
                    { "type": "text", "text": "the answer" },
                ],
            }])),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let message = &body.as_array().unwrap()[0];
    let parts = message["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0]["type"], json!("reasoning"));
    assert_eq!(parts[1]["type"], json!("tool"));
    assert_eq!(parts[1]["name"], json!("current_time"));
    assert_eq!(parts[2]["type"], json!("text"));
    // The plain text is the answer alone.
    assert_eq!(message["content"], json!("the answer"));
}

// -- forking ----------------------------------------------------------------

#[sqlx::test]
async fn forks_a_conversation_copying_the_prefix(pool: PgPool) {
    let (conversation_id, message_ids) = seed_conversation(&pool).await;
    let fork_point = message_ids[0];

    let (status, body) = call(
        &pool,
        request(
            "POST",
            &format!("/api/conversations/{conversation_id}/fork"),
            Some(json!({ "from_message_id": fork_point })),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["forked_from_conversation_id"],
        json!(conversation_id.to_string())
    );
    assert_eq!(body["forked_from_message_id"], json!(fork_point.to_string()));
    assert_eq!(body["model_name"], json!("qwen3:32b"));

    let fork_id = body["id"].as_str().unwrap();
    let (_, messages) = call(
        &pool,
        request("GET", &format!("/api/conversations/{fork_id}/messages"), None),
    )
    .await;

    let messages = messages.as_array().unwrap();
    assert_eq!(messages.len(), 1, "only the prefix is copied");
    assert_eq!(messages[0]["seq"], json!(1));
    assert_eq!(messages[0]["content"], json!("first"));
    assert_ne!(
        messages[0]["id"],
        json!(fork_point.to_string()),
        "the copy gets its own identity"
    );
}

#[sqlx::test]
async fn a_fork_continues_the_sequence_it_was_forked_at(pool: PgPool) {
    let (conversation_id, message_ids) = seed_conversation(&pool).await;

    let (_, body) = call(
        &pool,
        request(
            "POST",
            &format!("/api/conversations/{conversation_id}/fork"),
            Some(json!({ "from_message_id": message_ids[1] })),
        ),
    )
    .await;
    let fork_id = body["id"].as_str().unwrap().to_string();

    let (_, body) = call(
        &pool,
        request(
            "POST",
            &format!("/api/conversations/{fork_id}/messages"),
            Some(json!([{
                "id": Uuid::new_v4(),
                "role": "user",
                "parts": [{ "type": "text", "text": "new" }],
            }])),
        ),
    )
    .await;

    assert_eq!(body.as_array().unwrap()[0]["seq"], json!(3));
}

#[sqlx::test]
async fn refuses_to_fork_from_a_message_of_another_conversation(pool: PgPool) {
    let (conversation_id, _) = seed_conversation(&pool).await;
    let (_, other_ids) = seed_conversation(&pool).await;

    let (status, _) = call(
        &pool,
        request(
            "POST",
            &format!("/api/conversations/{conversation_id}/fork"),
            Some(json!({ "from_message_id": other_ids[0] })),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

// -- isolation --------------------------------------------------------------

#[sqlx::test]
async fn does_not_show_another_user_a_conversation(pool: PgPool) {
    // A second user, and a conversation of their own.
    let other_user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, username, password) VALUES ($1, 'someone-else', 'x')")
        .bind(other_user_id)
        .execute(&pool)
        .await
        .unwrap();

    let their_conversation_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO conversations (id, user_id, title, model_name) \
         VALUES ($1, $2, 'Theirs', 'qwen3:32b')",
    )
    .bind(their_conversation_id)
    .bind(other_user_id)
    .execute(&pool)
    .await
    .unwrap();

    // `yuki` must not see it.
    let (status, _) = call(
        &pool,
        request(
            "GET",
            &format!("/api/conversations/{their_conversation_id}/messages"),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, body) = call(
        &pool,
        request("GET", "/api/conversations", None),
    )
    .await;
    assert_eq!(
        body.as_array().unwrap().len(),
        0,
        "and it is not in their list either"
    );
}

// -- instructions -----------------------------------------------------------

#[sqlx::test]
async fn sets_the_conversations_instructions(pool: PgPool) {
    let (conversation_id, _) = seed_conversation(&pool).await;

    let (status, body) = call(
        &pool,
        request(
            "PUT",
            &format!("/api/conversations/{conversation_id}/instructions"),
            Some(json!({ "instructions": "Answer in one word." })),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["role"], json!("system"));
    assert_eq!(body["content"], json!("Answer in one word."));

    let (_, messages) = call(
        &pool,
        request(
            "GET",
            &format!("/api/conversations/{conversation_id}/messages"),
            None,
        ),
    )
    .await;

    let messages = messages.as_array().unwrap();
    assert_eq!(messages[0]["role"], json!("system"), "they come first");
    assert_eq!(messages[0]["seq"], json!(0));
}

#[sqlx::test]
async fn setting_the_instructions_again_replaces_them(pool: PgPool) {
    let (conversation_id, _) = seed_conversation(&pool).await;
    let url = format!("/api/conversations/{conversation_id}/instructions");

    call(
        &pool,
        request("PUT", &url, Some(json!({ "instructions": "First." }))),
    )
    .await;
    let (_, body) = call(
        &pool,
        request("PUT", &url, Some(json!({ "instructions": "Second." }))),
    )
    .await;

    assert_eq!(body["content"], json!("Second."));

    let (_, messages) = call(
        &pool,
        request(
            "GET",
            &format!("/api/conversations/{conversation_id}/messages"),
            None,
        ),
    )
    .await;

    let messages = messages.as_array().unwrap();
    let instructions: Vec<_> = messages
        .iter()
        .filter(|message| message["role"] == json!("system"))
        .collect();
    assert_eq!(instructions.len(), 1, "there is only ever one");
}

#[sqlx::test]
async fn empty_instructions_are_empty_and_not_unset(pool: PgPool) {
    let (conversation_id, _) = seed_conversation(&pool).await;

    let (status, body) = call(
        &pool,
        request(
            "PUT",
            &format!("/api/conversations/{conversation_id}/instructions"),
            Some(json!({ "instructions": "   " })),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["content"], json!(""), "empty is not the same as unset");
}

#[sqlx::test]
async fn a_fork_inherits_the_instructions(pool: PgPool) {
    let (conversation_id, message_ids) = seed_conversation(&pool).await;

    call(
        &pool,
        request(
            "PUT",
            &format!("/api/conversations/{conversation_id}/instructions"),
            Some(json!({ "instructions": "Be brief." })),
        ),
    )
    .await;

    let (_, body) = call(
        &pool,
        request(
            "POST",
            &format!("/api/conversations/{conversation_id}/fork"),
            Some(json!({ "from_message_id": message_ids[0] })),
        ),
    )
    .await;

    let fork_id = body["id"].as_str().unwrap();
    let (_, messages) = call(
        &pool,
        request(
            "GET",
            &format!("/api/conversations/{fork_id}/messages"),
            None,
        ),
    )
    .await;

    let messages = messages.as_array().unwrap();
    assert_eq!(messages[0]["role"], json!("system"));
    assert_eq!(messages[0]["content"], json!("Be brief."));
}

#[sqlx::test]
async fn refuses_to_append_a_message_of_the_system_role(pool: PgPool) {
    let (conversation_id, _) = seed_conversation(&pool).await;

    let (status, _) = call(
        &pool,
        request(
            "POST",
            &format!("/api/conversations/{conversation_id}/messages"),
            Some(json!([{
                "id": Uuid::new_v4(),
                "role": "system",
                "parts": [{ "type": "text", "text": "Sneaky." }]
            }])),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}
