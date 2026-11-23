//! The desktop app.
//!
//! It talks to the backend for identity and history, and to Ollama for the
//! model. See docs/design.md.

pub mod agent;
pub mod api;
pub mod commands;
pub mod history;
pub mod state;
pub mod tools;

use state::AppState;

/// Where the backend is. Set at build time; override with `CRF_API_BASE_URL`.
const DEFAULT_API_BASE_URL: &str = "http://localhost:3100";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let base_url = option_env!("CRF_API_BASE_URL")
        .unwrap_or(DEFAULT_API_BASE_URL)
        .to_string();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new(base_url))
        .invoke_handler(tauri::generate_handler![
            commands::login,
            commands::current_user,
            commands::list_models,
            commands::list_conversations,
            commands::create_conversation,
            commands::delete_conversation,
            commands::fork_conversation,
            commands::load_messages,
            commands::update_conversation,
            commands::send_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
