import type {
  ChatMessage,
  ChatMessagePart,
  ChatUser,
} from "@mui/x-chat/headless";

import type {
  HistoryMessage,
  Message,
  MessagePart,
  ToolStatus,
} from "../generated/protocol";

export const USER: ChatUser = {
  id: "user",
  displayName: "You",
  role: "user",
};

export const ASSISTANT: ChatUser = {
  id: "assistant",
  displayName: "Assistant",
  role: "assistant",
};

const TOOL_STATES: Record<ToolStatus, string> = {
  input_streaming: "input-streaming",
  input_available: "input-available",
  approval_requested: "approval-requested",
  approval_responded: "approval-responded",
  output_available: "output-available",
  output_error: "output-error",
  output_denied: "output-denied",
};

/** Our message, as the UI library wants it. */
export function toChatMessage(message: Message): ChatMessage {
  // The Instructions are split off before this is called: they are not part
  // of the dialogue and have no place in the transcript.
  const isUser = message.role === "user";

  return {
    id: message.id,
    role: isUser ? "user" : "assistant",
    parts: message.parts.map(toChatPart),
    status: "sent",
    createdAt: message.created_at,
    author: isUser ? USER : ASSISTANT,
  };
}

/**
 * The conversation's Instructions, and the messages that make up the
 * dialogue.
 *
 * The Instructions are stored as a message, but they are written by the
 * application, said by no one, and never shown in the transcript. A
 * conversation that has none of its own is `null` here, which is not the
 * same as one whose Instructions are empty.
 */
export function splitInstructions(stored: Message[]): {
  instructions: string | null;
  messages: Message[];
} {
  const instructions = stored.find((message) => message.role === "system");

  return {
    instructions: instructions ? plainText(instructions) : null,
    messages: stored.filter((message) => message.role !== "system"),
  };
}

/** A message's text, without its parts that are not text. */
function plainText(message: Message): string {
  return message.parts
    .map((part) => (part.type === "text" ? part.text : ""))
    .join("");
}

function toChatPart(part: MessagePart): ChatMessagePart {
  switch (part.type) {
    case "text":
      return { type: "text", text: part.text, state: "done" };
    case "reasoning":
      return { type: "reasoning", text: part.text, state: "done" };
    case "tool":
      return {
        type: "tool",
        toolInvocation: {
          toolCallId: part.call_id,
          toolName: part.name,
          state: TOOL_STATES[part.status] as never,
          input: part.input,
          output: part.output ?? undefined,
        },
      };
  }
}

/**
 * The UI library's message, as the desktop app holds it.
 *
 * Only what is needed to rebuild the model's context: the fields the server
 * owns are left to the server.
 */
export function toHistoryMessage(message: ChatMessage): HistoryMessage {
  return {
    id: message.id,
    role: message.role === "user" ? "user" : "assistant",
    parts: message.parts.map(toStoredPart),
  };
}

function toStoredPart(part: ChatMessagePart): MessagePart {
  switch (part.type) {
    case "text":
      return { type: "text", text: part.text };
    case "reasoning":
      return { type: "reasoning", text: part.text };
    case "tool": {
      const invocation = part.toolInvocation;
      return {
        type: "tool",
        call_id: invocation.toolCallId,
        name: invocation.toolName,
        input: (invocation.input ?? {}) as never,
        output: (invocation.output ?? null) as never,
        status: fromToolState(invocation.state),
      };
    }
    default:
      // The library can carry parts we do not store; they are dropped.
      return { type: "text", text: "" };
  }
}

function fromToolState(state: string): ToolStatus {
  switch (state) {
    case "input-streaming":
      return "input_streaming";
    case "approval-requested":
      return "approval_requested";
    case "approval-responded":
      return "approval_responded";
    case "output-available":
      return "output_available";
    case "output-error":
      return "output_error";
    case "output-denied":
      return "output_denied";
    default:
      return "input_available";
  }
}
