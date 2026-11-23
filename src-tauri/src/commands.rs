//! The commands the browser can call.
//!
//! Every backend operation goes through Rust, because the credential lives
//! here and not in the browser.

use protocol::{
    Chunk, Conversation, HistoryMessage, Message, MessageStatus, Model, NewMessage, User,
};
use tauri::ipc::Channel;
use tauri::State;
use uuid::Uuid;

use crate::agent;
use crate::api::ApiClient;
use crate::history;
use crate::state::AppState;

/// The id to store a client message under.
///
/// The desktop keeps the id it gave the message, so its optimistic copy and
/// the stored message agree. An id that is not a UUID — which the UI library
/// falls back to when it cannot make one — is given a fresh one.
fn storage_id(id: &str) -> Uuid {
    Uuid::parse_str(id).unwrap_or_else(|_| Uuid::new_v4())
}

/// The signed-in client, or a complaint.
async fn client(state: &State<'_, AppState>) -> Result<ApiClient, String> {
    state
        .client
        .read()
        .await
        .clone()
        .ok_or_else(|| "not signed in".to_string())
}

/// Checks the credential and keeps it.
#[tauri::command]
pub async fn login(
    state: State<'_, AppState>,
    username: String,
    password: String,
) -> Result<User, String> {
    let client = ApiClient::new(state.base_url.clone(), username, password);
    let user = client.me().await.map_err(|err| err.to_string())?;

    *state.client.write().await = Some(client);

    Ok(user)
}

/// The signed-in user, if there is one.
#[tauri::command]
pub async fn current_user(state: State<'_, AppState>) -> Result<Option<User>, String> {
    let Some(client) = client(&state).await.ok() else {
        return Ok(None);
    };

    client.me().await.map(Some).map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn list_models(state: State<'_, AppState>) -> Result<Vec<Model>, String> {
    client(&state)
        .await?
        .list_models()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn list_conversations(
    state: State<'_, AppState>,
) -> Result<Vec<Conversation>, String> {
    client(&state)
        .await?
        .list_conversations()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn create_conversation(
    state: State<'_, AppState>,
    model: String,
) -> Result<Conversation, String> {
    client(&state)
        .await?
        .create_conversation(&model)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn delete_conversation(
    state: State<'_, AppState>,
    conversation_id: Uuid,
) -> Result<(), String> {
    client(&state)
        .await?
        .delete_conversation(conversation_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn fork_conversation(
    state: State<'_, AppState>,
    conversation_id: Uuid,
    from_message_id: Uuid,
) -> Result<Conversation, String> {
    client(&state)
        .await?
        .fork_conversation(conversation_id, from_message_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn load_messages(
    state: State<'_, AppState>,
    conversation_id: Uuid,
) -> Result<Vec<Message>, String> {
    client(&state)
        .await?
        .list_messages(conversation_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn update_conversation(
    state: State<'_, AppState>,
    conversation_id: Uuid,
    model: String,
) -> Result<Conversation, String> {
    client(&state)
        .await?
        .update_conversation(conversation_id, &model)
        .await
        .map_err(|err| err.to_string())
}

/// Runs a turn: stores the question, streams the answer, stores the answer.
#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    channel: Channel<Chunk>,
    conversation_id: Uuid,
    message: HistoryMessage,
    history: Vec<HistoryMessage>,
    model: String,
) -> Result<(), String> {
    let client = client(&state).await?;

    run_turn(&client, &channel, conversation_id, message, history, model).await
}

/// The work of a turn, without the command wrapper.
pub async fn run_turn(
    client: &ApiClient,
    channel: &Channel<Chunk>,
    conversation_id: Uuid,
    message: HistoryMessage,
    history: Vec<HistoryMessage>,
    model: String,
) -> Result<(), String> {
    // The question is stored first, so it survives a crash mid-answer.
    client
        .append_messages(
            conversation_id,
            &[NewMessage {
                id: storage_id(&message.id),
                role: message.role,
                parts: message.parts.clone(),
                status: MessageStatus::Sent,
            }],
        )
        .await
        .map_err(|err| err.to_string())?;

    let rig_history = history::to_rig_messages(&history);
    let prompt = rig::completion::Message::user(protocol::Message::plain_text(&message.parts));

    let turn = agent::run_turn(conversation_id, &model, rig_history, prompt, |chunk| {
        // The browser may have gone away; that is not the model's problem.
        let _ = channel.send(chunk);
    })
    .await;

    if !turn.messages.is_empty() {
        let stored: Vec<NewMessage> = turn
            .messages
            .iter()
            .map(|message| NewMessage {
                id: message.id,
                role: message.role,
                parts: message.parts.clone(),
                status: message.status,
            })
            .collect();

        client
            .append_messages(conversation_id, &stored)
            .await
            .map_err(|err| err.to_string())?;
    }

    match turn.failure {
        Some(message) => Err(message),
        None => Ok(()),
    }
}
