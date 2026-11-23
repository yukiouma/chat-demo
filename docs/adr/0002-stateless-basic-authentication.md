# Stateless HTTP Basic authentication

Every request to the backend carries an `Authorization: Basic` header. The backend looks the user up by username and compares the supplied password against the stored one. There is no server-side session and no token: each request stands alone, and the credential lives only in the desktop app's Rust process, never in the React layer.

We chose this because the brief asked for basic auth and explicitly ruled out registration. The trade-off is that the password is transmitted on every request rather than exchanged once for a token.

**The password is stored in plain text.** This is a deliberate shortcut for a local demo with a single seeded account (`yuki` / `250606`) — it is not a design position and should be hashed (argon2 or bcrypt) before this goes anywhere real. The cost of not hashing is that a database dump reveals the password directly.

## Considered Options

**Token-based auth.** `POST /login` would exchange the credential once for an opaque token, after which the password crosses the wire only once and never sits in the backend's memory beyond that request. This is the better design, and we rejected it only because it introduces a server-side session and contradicts the stateless model this demo is demonstrating.

**Per-request hashing.** Verifying an argon2 hash on every request is deliberately slow (~50–100ms). At demo scale the request volume does not justify a verification cache.

## Consequences

- The React layer cannot call the backend directly — it has no credential. Every backend operation is a Tauri command that Rust performs. See the command surface in `docs/design.md`.
- The backend must never include the password in a response body.
