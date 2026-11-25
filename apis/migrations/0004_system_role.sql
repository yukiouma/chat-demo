-- Widen the role vocabulary: a conversation's Instructions are stored as a
-- message of their own, written by the application rather than said by
-- anyone. See docs/adr/0006-instructions-are-a-message.md.

ALTER TABLE messages
    DROP CONSTRAINT messages_role_check;

ALTER TABLE messages
    ADD CONSTRAINT messages_role_check
    CHECK (role IN ('user', 'assistant', 'system'));
