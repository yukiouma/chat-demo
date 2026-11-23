import { Channel, invoke } from "@tauri-apps/api/core";

import type {
  Chunk,
  Conversation,
  HistoryMessage,
  Message,
  Model,
  User,
} from "./generated/protocol";

/**
 * The backend, reached through Rust. The browser never talks to it
 * directly: the credential lives in Rust.
 */
export const api = {
  login: (username: string, password: string) =>
    invoke<User>("login", { username, password }),

  currentUser: () => invoke<User | null>("current_user"),

  listModels: () => invoke<Model[]>("list_models"),

  listConversations: () => invoke<Conversation[]>("list_conversations"),

  createConversation: (model: string) =>
    invoke<Conversation>("create_conversation", { model }),

  deleteConversation: (conversationId: string) =>
    invoke<void>("delete_conversation", { conversationId }),

  forkConversation: (conversationId: string, fromMessageId: string) =>
    invoke<Conversation>("fork_conversation", { conversationId, fromMessageId }),

  loadMessages: (conversationId: string) =>
    invoke<Message[]>("load_messages", { conversationId }),

  updateConversation: (conversationId: string, model: string) =>
    invoke<Conversation>("update_conversation", { conversationId, model }),

  /**
   * Runs a turn. The chunks arrive on the channel as they happen.
   */
  sendMessage: (args: {
    conversationId: string;
    message: HistoryMessage;
    history: HistoryMessage[];
    model: string;
    onChunk: (chunk: Chunk) => void;
  }) => {
    const channel = new Channel<Chunk>();
    channel.onmessage = args.onChunk;

    return invoke<void>("send_message", {
      channel,
      conversationId: args.conversationId,
      message: args.message,
      history: args.history,
      model: args.model,
    });
  },
};
