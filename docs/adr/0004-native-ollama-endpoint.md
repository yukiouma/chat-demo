# Use rig's native Ollama endpoint, not the OpenAI-compatible one

`rig` exposes two ways to talk to Ollama. `Ollama::completion(model)` posts to Ollama's OpenAI-compatible `/v1/chat/completions`; `Ollama::native_completion(model)` posts to the native `/api/chat`. We use the native endpoint.

This looks like a step backwards — the OpenAI-compatible path is the one you'd reach for by default, and it is what a future reader will expect to see — so it is worth recording why. Only the native path sends Ollama's `think` parameter, and only the native path decodes the model's thinking back out of the response. The OpenAI-compatible path also *hard-errors* on `options`, so `num_ctx` and friends are unavailable there. Showing the model's thinking is an explicit goal of this project, so the native path is the only one that can satisfy it.

## Consequences

- The client is tied to Ollama's native API rather than to a portable OpenAI-shaped interface. Pointing the app at a hosted provider would mean revisiting this decision.
- `think` is a top-level parameter on the native endpoint, and Ollama's own notion of a thinking *level* is narrower than rig's: `Effort::Low`, `Medium` and `High` are supported, while `Minimal`, `XHigh` and `Reasoning::Budget` are rejected and fail the request outright.
- Ollama has no tool-choice parameter. `ToolChoice` is silently ignored with a warning, so the UI must not offer controls the backend cannot honour.
