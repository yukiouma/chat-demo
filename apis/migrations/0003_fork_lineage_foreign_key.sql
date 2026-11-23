-- Add the lineage foreign key that could not be declared in 0001:
-- conversations references messages, and messages references conversations.

ALTER TABLE conversations
    ADD CONSTRAINT conversations_forked_from_message_id_fkey
    FOREIGN KEY (forked_from_message_id) REFERENCES messages (id) ON DELETE SET NULL;
