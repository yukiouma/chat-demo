import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Box,
  Button,
  CircularProgress,
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
import { ASSISTANT, USER, toChatMessage } from "./chat/messages";
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
      return;
    }

    let cancelled = false;

    api
      .loadMessages(activeId)
      .then((stored: Message[]) => {
        if (!cancelled) setMessages(stored.map(toChatMessage));
      })
      .catch((err) => {
        if (!cancelled) setProblem(String(err));
      });

    return () => {
      cancelled = true;
    };
  }, [activeId]);

  // The adapter needs the current conversation and model, which change.
  const context = useRef({ conversationId: activeId ?? "", model });
  context.current = { conversationId: activeId ?? "", model };

  const adapter = useMemo(() => createAdapter(() => context.current), []);

  async function newConversation() {
    try {
      const created = await api.createConversation(model);
      await refreshConversations();
      setActiveId(created.id);
      setMessages([]);
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
    </Box>
  );
}
