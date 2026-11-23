# CRF Agent Demo

A local desktop chat client for an LLM, backed by an HTTP service that owns user identity, conversation history, and the catalogue of available models.

## What it does

- **Desktop (Tauri v2 + React 19 + MUI 9 + `@mui/x-chat`)** — log in, create/switch/delete/fork conversations, chat with a local model, and watch its thinking, tool calls and answer stream in. Switch between the models the server offers.
- **Backend (Axum + sqlx + PostgreSQL)** — validates the user, stores conversations and their messages, and serves the model catalogue.

The LLM runs **on the desktop**, via `rig` talking to Ollama's native `/api/chat` endpoint on `localhost:11434`. The backend never runs inference; it is the system of record for conversation history.

## Getting started

```bash
# 1. Postgres — a database and role must exist
createdb crf_agent_demo

# 2. Configuration
cp .env.example .env     # DATABASE_URL

# 3. Backend — runs migrations on boot, listens on :3100
cargo run -p apis

# 4. Desktop
pnpm install
pnpm tauri dev
```

Ollama must be running with the model pulled:

```bash
ollama pull qwen3:32b
```

Demo credentials: **`yuki` / `250606`**.

## Layout

| Path | What |
|---|---|
| `src/` | React app |
| `src-tauri/` | Tauri app — commands, the `rig` client, chunk mapping |
| `apis/` | Axum backend, and `apis/migrations/` |
| `crates/protocol/` | Shared DTOs — Rust definitions, TypeScript generated via `ts-rs` into `src/generated/protocol.ts` |
| `docs/design.md` | The design: data model, endpoints, command surface, turn lifecycle |
| `docs/adr/` | Decision records |
| `GLOSSARY.md` | Domain vocabulary |

## Documents

- **[GLOSSARY.md](GLOSSARY.md)** — the domain vocabulary, and the words we deliberately rejected.
- **[docs/design.md](docs/design.md)** — how the pieces fit: schema, API, streaming contract, error handling, testing.
- **[docs/adr/](docs/adr/)** — the decisions worth remembering, and why.

## Tests

```bash
cargo test --workspace          # backend integration tests, history unit tests
cargo test -p crf-protocol      # regenerates src/generated/protocol.ts
pnpm build                      # type-checks and builds the frontend
```

Two tests need the outside world, and are ignored by default:

```bash
# a turn against a local Ollama with qwen3:32b pulled
cargo test -p crf-agent-demo --test turn -- --ignored

# the whole desktop path — needs the backend running
cargo run -p apis &
cargo test -p crf-agent-demo --test desktop -- --ignored
```

## Notes

This is a demo. Two things are knowingly unfinished:

- **The password is stored in plain text** in `users.password`. It should be hashed before this goes anywhere real — see [ADR-0002](docs/adr/0002-stateless-basic-authentication.md).
- **The model catalogue is static.** The server does not know which models are actually pulled locally, so choosing an absent model fails with an error from Ollama.
