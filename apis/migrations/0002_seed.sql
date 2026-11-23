-- Seed data: one model, one user.

INSERT INTO models (name, display_name, supports_reasoning, supports_tools, enabled, is_default)
VALUES ('qwen3:32b', 'Qwen3 32B', true, true, true, true);

-- Demo credentials. The password is stored in plain text; see
-- docs/adr/0002-stateless-basic-authentication.md.
INSERT INTO users (id, username, password)
VALUES (gen_random_uuid(), 'yuki', '250606');
