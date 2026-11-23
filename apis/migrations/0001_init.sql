-- Core schema for the CRF Agent Demo.
--
-- See docs/design.md for the reasoning behind each shape.

CREATE TABLE users (
    id            UUID        PRIMARY KEY,
    username      TEXT        NOT NULL UNIQUE,
    -- Plain text on purpose: this is a local demo with one seeded account.
    -- See docs/adr/0002-stateless-basic-authentication.md.
    password      TEXT        NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The catalogue of models the service offers. Static: rows are seeded, not
-- discovered from Ollama at runtime.
CREATE TABLE models (
    name               TEXT        PRIMARY KEY,
    display_name       TEXT        NOT NULL,
    supports_reasoning BOOLEAN     NOT NULL DEFAULT false,
    supports_tools     BOOLEAN     NOT NULL DEFAULT false,
    enabled            BOOLEAN     NOT NULL DEFAULT true,
    is_default         BOOLEAN     NOT NULL DEFAULT false
);

CREATE TABLE conversations (
    id                          UUID        PRIMARY KEY,
    user_id                     UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title                       TEXT,
    model_name                  TEXT        NOT NULL REFERENCES models (name),
    forked_from_conversation_id UUID        REFERENCES conversations (id) ON DELETE SET NULL,
    -- Points at messages(id), which points back here. The foreign key is added
    -- in 0003 once both tables exist.
    forked_from_message_id      UUID,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX conversations_user_recent_idx
    ON conversations (user_id, updated_at DESC);

CREATE TABLE messages (
    id              UUID        PRIMARY KEY,
    conversation_id UUID        NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
    role            TEXT        NOT NULL CHECK (role IN ('user', 'assistant')),
    -- Position within the conversation. Also defines the fork prefix.
    seq             INTEGER     NOT NULL,
    -- Typed parts: text, reasoning, tool. See docs/adr/0003-jsonb-message-parts.md.
    parts           JSONB       NOT NULL DEFAULT '[]'::jsonb,
    -- Denormalised plain text, for display and search without parsing parts.
    content         TEXT        NOT NULL DEFAULT '',
    status          TEXT        NOT NULL DEFAULT 'sent'
                                CHECK (status IN ('sent', 'error', 'cancelled')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (conversation_id, seq)
);
