# Inference runs on the desktop, not on the backend

The LLM is called from the Tauri app via `rig`, talking to the local Ollama daemon on `http://localhost:11434`. The Axum backend never runs inference: it authenticates the user, stores conversations and messages, and serves the model catalogue. The desktop app streams a turn locally and then persists the result to the backend.

We chose this because the stated goal was to demonstrate the Tauri + `rig` + Ollama client, and because the backend stays a clean system of record for conversation history. The cost is real: the backend can never re-run, audit or account for a generation, and the client is responsible for supplying the model with the conversation history it needs.

## Considered Options

**Inference in the backend.** The backend would own the history and stream to the desktop over SSE. This centralises the agent loop and makes the stored transcript authoritative for what the model actually saw, but it adds an HTTP hop to every token and splits the LLM client across two processes.

**Both.** A desktop-side default with an optional backend proxy. More moving parts and more to test, for no gain at demo scale.

## Consequences

- The desktop app needs the backend running for login, history and the model list, but not for inference.
- Ollama must be reachable from wherever the desktop app runs; `OLLAMA_API_BASE_URL` overrides the default of `http://localhost:11434`.
