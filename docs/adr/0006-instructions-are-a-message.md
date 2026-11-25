# The conversation's Instructions are stored as a message

A conversation can carry its own instructions for the model. We store them as a message of their own, with the role `system`, sitting at the head of the conversation; the desktop app splits them off before rendering and hands them to the model as the preamble.

We chose this over a column on `conversations` because the instructions are a property of the conversation's *record*, not of its metadata: they are part of what the conversation is, they are inherited by a fork, and they belong in the same place as everything else the conversation holds. It also means the `messages` table has one shape — an ordered list of entries — rather than two kinds of thing to join.

Storing them at position zero, rather than at the end where a new message would normally go, is deliberate: it is what makes a fork inherit them without the fork knowing anything about them.

## Considered Options

**A column on `conversations`.** Simpler to read, simpler to write, and the fork inherits it by copying the column. Rejected because it splits the conversation's contents across two tables, and because a column has nowhere to say who wrote it — which matters, since the instructions are not said by anyone.

**A system-role message.** Chosen. The `system` role is the one the providers use for this, so the mapping to the wire is one to one.

**Nothing stored at all — the instructions live only in the desktop app.** Rejected because they would be lost on reload, and invisible to any other client of the same conversation.

## Consequences

- The role vocabulary is now three values, and the database's check constraint has to say so.
- The instructions are never shown in the transcript. They are configuration, not speech, and showing them would put a message in the dialogue that nobody said.
- A conversation that has no instructions of its own runs with the built-in default, which lives in the desktop app. A conversation whose instructions are set but empty runs with none at all. The two are different states, and the difference is visible.
- The instructions reach the model as a preamble, ahead of the history, rather than as a message within it. See the turn lifecycle in `docs/design.md`.
- Editing the instructions applies to the turns that come after it. The turns before it were answered under different instructions, and nothing rewrites them.
