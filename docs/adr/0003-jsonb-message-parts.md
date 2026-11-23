# Message parts are stored as JSONB in our own vocabulary

A message's parts are stored in a single `parts` JSONB column using vocabulary we own — `text`, `reasoning`, `tool` — rather than the shape of the UI library's message parts. The database, the backend and the desktop app all speak our vocabulary; the translation to the UI library's types happens in one place in TypeScript, at the UI boundary.

We chose this because the UI library is an alpha whose own README warns that its API "will change before reaching a stable release." Since its types exist only in TypeScript, there is nothing that would catch a breaking change on the Rust side at compile time — a renamed part type would surface at runtime, in the UI, with the Rust code still compiling cleanly. Keeping our vocabulary on the Rust and database side means an alpha upgrade breaks one TypeScript mapper, where the compiler is watching.

The cost is a translation layer that would not otherwise exist, and the risk that our vocabulary and the library's drift apart in ways nobody notices.

## Considered Options

**The UI library's vocabulary throughout.** No translation, and the fastest path from a database row to a rendered message. Rejected because of the runtime-only failure mode described above.

**One part per row.** Normalised, and queryable per part. Rejected because the parts of a message are only ever read and written as an ordered list — they are never filtered on independently.

## Consequences

- The persisted part shape is a deliberate subset: `text`, `reasoning`, and `tool`. Features the library supports that we do not use (files, sources, custom parts) have no representation in our vocabulary.
- A tool's input and output live together in one `tool` part. Rebuilding the model's context from stored messages therefore has to split that part back into rig's two-message form; see the turn lifecycle in `docs/design.md`.
