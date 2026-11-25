import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Box,
  Button,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  List,
  ListItemButton,
  ListItemText,
  MenuItem,
  Select,

  TextField,
  Typography,
} from "@mui/material";
import { ChatBox, ChatMessageActions } from "@mui/x-chat";
import type { ChatMessage } from "@mui/x-chat/headless";

import { api } from "./api";
import { createAdapter } from "./chat/adapter";
import { ASSISTANT, USER, splitInstructions, toChatMessage } from "./chat/messages";
import type {
  Conversation,
  Message,
  Model,
  User as Account,
} from "./generated/protocol";

export default function App() {
  const [account, setAccount] = useState<Account | null>(null);
  const [checking, setChecking] = useState(true);

  useEffect(() => {
    // There is no stored session, so a fresh launch is always signed out.
    api
      .currentUser()
      .then(setAccount)
      .catch(() => setAccount(null))
      .finally(() => setChecking(false));
  }, []);

  if (checking) {
    return (
      <Box sx={{ display: "grid", placeItems: "center", height: "100vh" }}>
        <CircularProgress />
      </Box>
    );
  }

  if (!account) {
    return <SignIn onSignedIn={setAccount} />;
  }

  return <Chat account={account} onSignOut={() => setAccount(null)} />;
}

function SignIn({ onSignedIn }: { onSignedIn: (account: Account) => void }) {
  const [username, setUsername] = useState("yuki");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function signIn(event: React.FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError(null);

    try {
      onSignedIn(await api.login(username, password));
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Box sx={{ display: "grid", placeItems: "center", height: "100vh" }}>
      <Box
        component="form"
        onSubmit={signIn}
        sx={{ display: "flex", flexDirection: "column", gap: 2, minWidth: 280 }}
      >
        <Typography variant="h5">CRF Agent Demo</Typography>
        <TextField
          label="Username"
          value={username}
          onChange={(event) => setUsername(event.target.value)}
          autoFocus
        />
        <TextField
          label="Password"
          type="password"
          value={password}
          onChange={(event) => setPassword(event.target.value)}
        />
        {error && (
          <Typography color="error" variant="body2">
            {error}
          </Typography>
        )}
        <Button type="submit" variant="contained" disabled={busy}>
          {busy ? "Signing in…" : "Sign in"}
        </Button>
      </Box>
    </Box>
  );
}

function Chat({
  account,
  onSignOut,
}: {
  account: Account;
  onSignOut: () => void;
}) {
  const [models, setModels] = useState<Model[]>([]);
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [model, setModel] = useState<string>("");
  const [problem, setProblem] = useState<string | null>(null);
  /** The conversation's Instructions; null when it has none of its own. */
  const [instructions, setInstructions] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");

  const refreshConversations = useCallback(async () => {
    setConversations(await api.listConversations());
  }, []);

  // Load the catalogue, then make sure there is somewhere to talk.
  useEffect(() => {
    let cancelled = false;

    (async () => {
      try {
        const available = await api.listModels();
        const existing = await api.listConversations();
        if (cancelled) return;

        setModels(available);
        const preferred = available.find((m) => m.is_default) ?? available[0];
        setModel(preferred?.name ?? "");

        if (existing.length > 0) {
          setActiveId(existing[0].id);
          setModel(existing[0].model_name);
        } else if (preferred) {
          const created = await api.createConversation(preferred.name);
          if (cancelled) return;
          setConversations([created]);
          setActiveId(created.id);
        }
      } catch (err) {
        if (!cancelled) setProblem(String(err));
      }
    })();

    return () => {
      cancelled = true;
    };
  }, []);

  // Load the messages of whichever conversation is open.
  useEffect(() => {
    if (!activeId) {
      setMessages([]);
      setInstructions(null);
      return;
    }

    let cancelled = false;

    api
      .loadMessages(activeId)
      .then((stored: Message[]) => {
        if (cancelled) return;
        const split = splitInstructions(stored);
        setInstructions(split.instructions);
        setMessages(split.messages.map(toChatMessage));
      })
      .catch((err) => {
        if (!cancelled) setProblem(String(err));
      });

    return () => {
      cancelled = true;
    };
  }, [activeId]);

  // The adapter needs the current conversation, model and instructions, which
  // change.
  const context = useRef({
    conversationId: activeId ?? "",
    model,
    systemPrompt: instructions,
  });
  context.current = {
    conversationId: activeId ?? "",
    model,
    systemPrompt: instructions,
  };

  const adapter = useMemo(() => createAdapter(() => context.current), []);

  async function newConversation() {
    try {
      const created = await api.createConversation(model);
      await refreshConversations();
      setActiveId(created.id);
      setMessages([]);
      // The new conversation has the built-in instructions until they are
      // loaded; the previous conversation's must not leak into it.
      setInstructions(null);
    } catch (err) {
      setProblem(String(err));
    }
  }

  async function removeConversation(id: string) {
    try {
      await api.deleteConversation(id);
      const remaining = conversations.filter((c) => c.id !== id);
      setConversations(remaining);
      if (activeId === id) {
        setActiveId(remaining[0]?.id ?? null);
        setMessages([]);
        setInstructions(null);
      }
    } catch (err) {
      setProblem(String(err));
    }
  }

  async function forkFrom(messageId: string) {
    if (!activeId) return;
    try {
      const forked = await api.forkConversation(activeId, messageId);
      await refreshConversations();
      setActiveId(forked.id);
      setMessages([]);
      setInstructions(null);
      setModel(forked.model_name);
    } catch (err) {
      setProblem(String(err));
    }
  }

  async function changeModel(name: string) {
    setModel(name);
    if (!activeId) return;
    try {
      await api.updateConversation(activeId, name);
    } catch (err) {
      setProblem(String(err));
    }
  }

  function editInstructions() {
    setDraft(instructions ?? "");
    setEditing(true);
  }

  async function saveInstructions() {
    if (!activeId) return;
    try {
      const saved = await api.setInstructions(activeId, draft);
      setInstructions(saved.content);
      setEditing(false);
    } catch (err) {
      setProblem(String(err));
    }
  }

  const messageActions = useMemo(
    () =>
      function MessageActionsWithFork(props: React.ComponentProps<typeof ChatMessageActions>) {
        return (
          <ChatMessageActions
            {...props}
            extraActions={[
              {
                id: "fork",
                label: "Fork",
                onClick: (_event, { message }) => {
                  if (message) void forkFrom(message.id);
                },
              },
            ]}
          />
        );
      },
    [],
  );

  return (
    <Box sx={{ display: "flex", height: "100vh" }}>
      <Box
        sx={{
          width: 260,
          borderRight: 1,
          borderColor: "divider",
          display: "flex",
          flexDirection: "column",
          p: 1,
          gap: 1,
        }}
      >
        <Button variant="contained" onClick={newConversation}>
          New conversation
        </Button>

        <List dense sx={{ flex: 1, overflowY: "auto" }}>
          {conversations.map((conversation) => (
            <ListItemButton
              key={conversation.id}
              selected={conversation.id === activeId}
              onClick={() => setActiveId(conversation.id)}
            >
              <ListItemText
                primary={conversation.title ?? "New conversation"}
                secondary={
                  conversation.forked_from_conversation_id ? "forked" : undefined
                }
              />
              <Button
                size="small"
                onClick={(event) => {
                  event.stopPropagation();
                  void removeConversation(conversation.id);
                }}
              >
                ✕
              </Button>
            </ListItemButton>
          ))}
        </List>

        <Select
          size="small"
          value={model}
          onChange={(event) => void changeModel(event.target.value)}
        >
          {models.map((candidate) => (
            <MenuItem key={candidate.name} value={candidate.name}>
              {candidate.display_name}
            </MenuItem>
          ))}
        </Select>

        <Button size="small" onClick={editInstructions} disabled={!activeId}>
          Instructions
        </Button>

        <Box sx={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <Typography variant="caption">{account.username}</Typography>
          <Button size="small" onClick={onSignOut}>
            Sign out
          </Button>
        </Box>
      </Box>

      <Box sx={{ flex: 1, minWidth: 0, display: "flex" }}>
        {problem && (
          <Box sx={{ position: "absolute", top: 8, right: 8, zIndex: 10 }}>
            <Typography color="error" variant="body2">
              {problem}
            </Typography>
          </Box>
        )}

        <ChatBox
          adapter={adapter}
          messages={messages}
          onMessagesChange={setMessages}
          members={[USER, ASSISTANT]}
          currentUser={USER}
          features={{ streamingIndicator: "auto", dateDivider: true }}
          slots={{ messageActions }}
          density="comfortable"
          sx={{ flex: 1, minWidth: 0 }}
        />
      </Box>

      <Dialog open={editing} onClose={() => setEditing(false)} fullWidth maxWidth="sm">
        <DialogTitle>Instructions</DialogTitle>
        <DialogContent>
          <TextField
            label="Instructions"
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            multiline
            minRows={4}
            fullWidth
            autoFocus
            helperText="What the assistant runs with. Leave it empty for no instructions at all."
          />

          <Typography variant="subtitle2" sx={{ mt: 3, mb: 1 }}>
            What the model receives
          </Typography>
          <Box
            sx={{
              p: 1.5,
              borderRadius: 1,
              bgcolor: "action.hover",
              whiteSpace: "pre-wrap",
            }}
          >
            <Typography variant="body2">{effectiveInstructions(draft, instructions)}</Typography>
          </Box>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setEditing(false)}>Cancel</Button>
          <Button
            variant="contained"
            onClick={() => void saveInstructions()}
            disabled={draft === (instructions ?? "")}
          >
            Save
          </Button>
        </DialogActions>
      </Dialog>
    </Box>
  );
}

/**
 * What the model will be given, once the draft is saved.
 *
 * A conversation that has no Instructions of its own falls back to the
 * built-in ones, so an untouched empty field does not mean "no
 * instructions" — it means the default is still in charge.
 */
function effectiveInstructions(draft: string, stored: string | null): string {
  const unchanged = draft === (stored ?? "");

  if (unchanged) {
    if (stored === null) {
      return "The assistant's built-in default instructions.";
    }
    if (stored === "") {
      return "Nothing — the assistant will run with no instructions.";
    }
    return stored;
  }

  return draft.trim() === ""
    ? "Nothing — the assistant will run with no instructions."
    : draft;
}
