import type {
  ChatAdapter,
  ChatMessageChunk,
} from "@mui/x-chat/headless";

import type { Chunk } from "../generated/protocol";
import { api } from "../api";
import { toHistoryMessage } from "./messages";

/** What the adapter needs to know that can change while the app runs. */
export interface AdapterContext {
  conversationId: string;
  model: string;
  /** The conversation's Instructions; null when it has none of its own. */
  systemPrompt: string | null;
}

/** Tracks the part of the message being written. */
interface MappingState {
  /** The open part, if any. Only one part is open at a time. */
  open: { kind: "text" | "reasoning"; id: string } | null;
}

/**
 * Opens a part, closing the one before it.
 *
 * The library resolves parts by the id in the chunk, so a new id means a new
 * part. That is what keeps a second run of thinking — after a tool call —
 * from being merged into the first.
 */
function openPart(
  state: MappingState,
  kind: "text" | "reasoning",
  id: string,
): ChatMessageChunk[] {
  if (state.open?.id === id) {
    return [];
  }

  const out: ChatMessageChunk[] = [];

  if (state.open) {
    out.push({
      type: state.open.kind === "text" ? "text-end" : "reasoning-end",
      id: state.open.id,
    });
  }

  out.push({
    type: kind === "text" ? "text-start" : "reasoning-start",
    id,
  });

  state.open = { kind, id };

  return out;
}

/**
 * Turns our chunks into the library's chunks.
 *
 * The library's chunks identify a *part* rather than a message, so the text
 * and the reasoning of one message need different ids.
 */
function mapChunk(chunk: Chunk, state: MappingState): ChatMessageChunk[] {
  switch (chunk.type) {
    case "start":
      state.open = null;
      return [{ type: "start", messageId: chunk.message_id }];

    case "text_delta": {
      const out = openPart(state, "text", chunk.part_id);
      out.push({
        type: "text-delta",
        id: chunk.part_id,
        delta: chunk.text,
      });
      return out;
    }

    case "reasoning_delta": {
      const out = openPart(state, "reasoning", chunk.part_id);
      out.push({
        type: "reasoning-delta",
        id: chunk.part_id,
        delta: chunk.text,
      });
      return out;
    }

    case "tool_call":
      return [
        {
          type: "tool-input-available",
          toolCallId: chunk.call_id,
          toolName: chunk.name,
          input: chunk.input,
        },
      ];

    case "tool_result":
      return chunk.is_error
        ? [
            {
              type: "tool-output-error",
              toolCallId: chunk.call_id,
              errorText: JSON.stringify(chunk.output),
            },
          ]
        : [
            {
              type: "tool-output-available",
              toolCallId: chunk.call_id,
              output: chunk.output,
            },
          ];

    case "finish": {
      const out: ChatMessageChunk[] = [];
      if (state.open) {
        out.push({
          type: state.open.kind === "text" ? "text-end" : "reasoning-end",
          id: state.open.id,
        });
        state.open = null;
      }
      out.push({ type: "finish", messageId: chunk.message_id });
      return out;
    }

    case "error":
      // Failing the stream is how the library learns the message went wrong.
      // It keeps what was generated and marks the message as errored.
      throw new Error(chunk.message);
  }
}

/**
 * Bridges the Tauri channel into the object stream the library expects.
 *
 * The library wants a stream of *parsed* chunks, not bytes.
 */
function createStream(
  run: (onChunk: (chunk: Chunk) => void) => Promise<void>,
): ReadableStream<ChatMessageChunk> {
  const queue: ChatMessageChunk[] = [];
  const state: MappingState = { open: null };

  let failure: Error | null = null;
  let finished = false;
  let wake: (() => void) | null = null;

  const push = (chunk: Chunk) => {
    try {
      queue.push(...mapChunk(chunk, state));
    } catch (error) {
      failure = error instanceof Error ? error : new Error(String(error));
    }
    wake?.();
  };

  run(push).then(
    () => {
      finished = true;
      wake?.();
    },
    (error) => {
      failure = error instanceof Error ? error : new Error(String(error));
      finished = true;
      wake?.();
    },
  );

  return new ReadableStream<ChatMessageChunk>({
    async pull(controller) {
      while (queue.length === 0 && !finished && !failure) {
        await new Promise<void>((resolve) => {
          wake = resolve;
        });
      }
      wake = null;

      if (queue.length > 0) {
        controller.enqueue(queue.shift()!);
        return;
      }
      if (failure) {
        controller.error(failure);
        return;
      }
      controller.close();
    },
  });
}

/**
 * The library's backend contract, implemented against our desktop app.
 */
export function createAdapter(context: () => AdapterContext): ChatAdapter {
  return {
    async sendMessage({ messages }) {
      const { conversationId, model, systemPrompt } = context();

      // The store already holds the message that was just sent, so it is
      // the last one; everything before it is the history.
      const sent = messages[messages.length - 1];
      if (!sent) {
        throw new Error("nothing to send");
      }

      const history = messages.slice(0, -1).map(toHistoryMessage);

      return createStream((onChunk) =>
        api.sendMessage({
          conversationId,
          message: toHistoryMessage(sent),
          history,
          model,
          systemPrompt,
          onChunk,
        }),
      );
    },
  };
}
