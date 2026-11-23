# The client supplies the conversation history to the model

When a turn starts, the model receives the conversation history that the desktop app already holds in memory, rather than the backend re-reading the stored transcript and handing it back. The desktop app is therefore the source of what the model sees, and the backend is consulted only to persist the result.

We chose this because the app already has the history — it loaded it to render the conversation — so re-fetching it would be a redundant round trip on every message.

**This is the one place where the backend's authority over history is deliberately given up.** The backend owns the stored transcript, but not the model's context. A stale client — one that has not reloaded after a fork, or a second window showing the same conversation — would feed the model a different context than what is on record, and nothing in the system would flag the divergence. We accepted that risk for this demo; a reviewer should assume it is an oversight unless told otherwise.

## Consequences

- The desktop app must load a conversation's messages before it can send to it, and must reload after a fork.
- The stored transcript and the model's context can disagree. If that matters later, the fix is to have the backend serve the history at turn start.
