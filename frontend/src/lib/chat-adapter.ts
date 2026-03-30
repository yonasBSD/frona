import type { ChatModelAdapter, ChatModelRunOptions, AttachmentAdapter, PendingAttachment, CompleteAttachment } from "@assistant-ui/react";
import { sendMessage as apiSendMessage, cancelGeneration, api, uploadFile } from "./api-client";
import { sseBus, type ChatSSEEvent } from "./sse-event-bus";
import { setRetryState, clearRetryState } from "./retry-state";
import type { Attachment as BackendAttachment, ChatResponse, MessageResponse } from "./types";

// Registry for mapping attachment IDs to backend Attachment objects
const backendAttachmentRegistry = new Map<string, BackendAttachment>();

export function registerBackendAttachment(id: string, attachment: BackendAttachment) {
  backendAttachmentRegistry.set(id, attachment);
}

export function getBackendAttachment(id: string): BackendAttachment | undefined {
  return backendAttachmentRegistry.get(id);
}

export const fronaAttachmentAdapter: AttachmentAdapter = {
  accept: "*/*",

  async add({ file }): Promise<PendingAttachment> {
    const uploaded = await uploadFile(file);
    backendAttachmentRegistry.set(uploaded.path, uploaded);
    return {
      id: uploaded.path,
      type: "file",
      name: uploaded.filename,
      contentType: uploaded.content_type,
      status: { type: "requires-action", reason: "composer-send" },
      content: [],
      file,
    };
  },

  async send(attachment: PendingAttachment): Promise<CompleteAttachment> {
    const isImage = attachment.contentType?.startsWith("image/");
    return {
      ...attachment,
      status: { type: "complete" },
      content: isImage && attachment.file
        ? [{ type: "image", image: URL.createObjectURL(attachment.file) }]
        : [{ type: "text", text: `[file: ${attachment.name}]` }],
    };
  },

  async remove(attachment) {
    backendAttachmentRegistry.delete(attachment.id);
  },
};

function tryParseJson(value: unknown): Record<string, string | number | boolean | null> {
  if (typeof value === "string") {
    try {
      const parsed = JSON.parse(value);
      if (typeof parsed === "object" && parsed !== null) return parsed;
      return {};
    } catch { return {}; }
  }
  if (typeof value === "object" && value !== null) return value as Record<string, string | number | boolean | null>;
  return {};
}

interface ToolCallPart {
  type: "tool-call";
  toolCallId: string;
  toolName: string;
  args: Record<string, string | number | boolean | null>;
  argsText: string;
  result?: string;
  isError?: boolean;
}

type ContentPart =
  | { type: "text"; text: string }
  | { type: "reasoning"; text: string }
  | ToolCallPart;

function buildParts(
  text: string,
  reasoning: string,
  toolCalls: Map<string, ToolCallPart>,
  toolResults: Map<string, { result: string; isError: boolean }>,
): ContentPart[] {
  const parts: ContentPart[] = [];
  if (reasoning) parts.push({ type: "reasoning", text: reasoning });
  // Always include text part when tool calls exist so the UI can show
  // a streaming indicator while tools are executing.
  if (text || toolCalls.size > 0) parts.push({ type: "text", text });
  for (const tc of toolCalls.values()) {
    const res = toolResults.get(tc.toolCallId);
    if (res) {
      parts.push({ ...tc, result: res.result, isError: res.isError });
    } else {
      parts.push(tc);
    }
  }
  return parts;
}

/**
 * If the text part is empty but a tool call has turnText, promote the last
 * turnText to the main text and strip it from all tool call args.
 * This handles the case where the agent spoke before a side-effect tool call
 * (e.g. store_user_memory) but produced no text after it.
 */
export function promoteTurnText<T extends { type: string; text?: string; args?: Record<string, unknown> }>(parts: T[]): T[] {
  const textPart = parts.find((p): p is T & { type: "text"; text: string } => p.type === "text");
  if (textPart?.text?.trim()) return parts;

  let lastTurnText = "";
  for (const p of parts) {
    if (p.type === "tool-call" && typeof p.args?.turnText === "string") {
      lastTurnText = p.args.turnText;
    }
  }
  if (!lastTurnText) return parts;

  return parts.map(p => {
    if (p.type === "text") return { ...p, text: lastTurnText };
    if (p.type === "tool-call" && p.args?.turnText) {
      const { turnText: _, ...rest } = p.args;
      return { ...p, args: rest };
    }
    return p;
  });
}

/** Only cancel generation when the user explicitly clicks Stop, not on component unmount. */
function onUserCancel(abortSignal: AbortSignal, chatId: string): () => void {
  return () => {
    const reason = abortSignal.reason;
    // assistant-ui's detach() passes AbortError with detach=true on unmount;
    // cancelRun() passes detach=false on explicit user cancel.
    if (reason && typeof reason === "object" && "detach" in reason && reason.detach) return;
    cancelGeneration(chatId).catch(() => {});
  };
}

export interface ChatAdapterOptions {
  chatId?: string;
  agentId: string;
  onChatCreated?: (chat: ChatResponse) => void;
  onChatMessage?: (msg: MessageResponse) => void;
}

/**
 * Shared event queue that persists across run() calls.
 * Wraps a single sseBus subscription. A background pump continuously pulls
 * events from the bus: chat_message events are delivered immediately via
 * callback (even between run() calls), while other events are buffered for
 * the next drain().
 */
class ChatEventQueue {
  private controller: AbortController;
  private onChatMessage?: (msg: MessageResponse) => void;
  /** Buffered non-chat_message events waiting for drain(). */
  private buffer: ChatSSEEvent[] = [];
  /** Resolve function for the active drain() waiting for the next event. */
  private drainResolve: ((event: ChatSSEEvent | null) => void) | null = null;
  /** True once the underlying subscription ends. */
  private ended = false;

  constructor(chatId: string, onChatMessage?: (msg: MessageResponse) => void) {
    this.controller = new AbortController();
    this.onChatMessage = onChatMessage;
    const events = sseBus.subscribe(chatId, this.controller.signal);
    this.startPump(events);
  }

  /** Continuously pull from the bus and route events. */
  private async startPump(events: AsyncIterable<ChatSSEEvent>) {
    try {
      for await (const event of events) {
        if (event.type === "chat_message" && this.onChatMessage) {
          this.onChatMessage(event.message);
        } else if (this.drainResolve) {
          const resolve = this.drainResolve;
          this.drainResolve = null;
          resolve(event);
        } else {
          this.buffer.push(event);
        }
      }
    } catch {
      // subscription ended
    }
    this.ended = true;
    // Wake any waiting drain
    if (this.drainResolve) {
      const resolve = this.drainResolve;
      this.drainResolve = null;
      resolve(null);
    }
  }

  /** Create an async iterable that drains buffered and incoming events. */
  drain(signal: AbortSignal): AsyncIterable<ChatSSEEvent> {
    const self = this;
    return {
      [Symbol.asyncIterator]() {
        return {
          next(): Promise<IteratorResult<ChatSSEEvent>> {
            // Drain buffered events first
            if (self.buffer.length > 0) {
              return Promise.resolve({ value: self.buffer.shift()!, done: false });
            }
            if (self.ended || signal.aborted) {
              return Promise.resolve({ value: undefined as unknown as ChatSSEEvent, done: true });
            }
            return new Promise<IteratorResult<ChatSSEEvent>>((resolve) => {
              let settled = false;
              const onAbort = () => {
                if (!settled) {
                  settled = true;
                  self.drainResolve = null;
                  resolve({ value: undefined as unknown as ChatSSEEvent, done: true });
                }
              };
              signal.addEventListener("abort", onAbort, { once: true });

              self.drainResolve = (event) => {
                if (settled) return;
                settled = true;
                signal.removeEventListener("abort", onAbort);
                if (event === null) {
                  resolve({ value: undefined as unknown as ChatSSEEvent, done: true });
                } else {
                  resolve({ value: event, done: false });
                }
              };
            });
          },
        };
      },
    };
  }

  destroy() {
    this.controller.abort();
  }
}

export interface ChatAdapterHandle {
  adapter: ChatModelAdapter;
  /** Sync last sent message ID after loading history (prevents re-sends). */
  syncLastSentMessageId: (messages: { id?: string; role: string }[]) => void;
  chatId: () => string | null;
  /** Tear down the SSE subscription. Call when the chat is permanently unmounted. */
  destroy: () => void;
}

export function createChatAdapter(options: ChatAdapterOptions): ChatAdapterHandle {
  let currentChatId = options.chatId ?? null;
  let eventQueue: ChatEventQueue | null = null;
  let lastSentMessageId: string | null = null;

  function ensureQueue(chatId: string): ChatEventQueue {
    if (eventQueue) return eventQueue;
    eventQueue = new ChatEventQueue(chatId, options.onChatMessage);
    return eventQueue;
  }

  /** Can this adapter send? Only if a chat exists or onChatCreated can create one. */
  function canSend(): boolean {
    return currentChatId != null || options.onChatCreated != null;
  }

  const adapter: ChatModelAdapter = {
    async *run({ messages, abortSignal }: ChatModelRunOptions) {
      const lastMsg = messages[messages.length - 1];
      const isNewUserMessage =
        canSend() &&
        lastMsg?.role === "user" &&
        lastMsg.id !== lastSentMessageId;

      console.log("[chat-adapter] run() called", {
        isNewUserMessage,
        lastMsgRole: lastMsg?.role,
        lastMsgId: lastMsg?.id,
        lastSentMessageId,
        chatId: currentChatId,
      });

      if (isNewUserMessage) {
        const content = lastMsg.content
          .filter((p): p is { type: "text"; text: string } => p.type === "text")
          .map((p) => p.text)
          .join("");

        const attachments: BackendAttachment[] = [];
        for (const att of lastMsg.attachments) {
          const backend = backendAttachmentRegistry.get(att.id);
          if (backend) {
            attachments.push(backend);
          }
        }

        if (!currentChatId) {
          const chat = await api.post<ChatResponse>("/api/chats", {
            agent_id: options.agentId,
          });
          currentChatId = chat.id;
          options.onChatCreated?.(chat);
        }

        const chatId = currentChatId;
        const queue = ensureQueue(chatId);
        const onAbort = onUserCancel(abortSignal, chatId);
        abortSignal.addEventListener("abort", onAbort);

        const body = attachments.length
          ? { content, attachments }
          : { content };

        await apiSendMessage(chatId, body);
        lastSentMessageId = lastMsg.id ?? null;

        yield* streamEvents(queue.drain(abortSignal), abortSignal, onAbort);
        return;
      }

      // Continuation (addResult, etc.) — just stream SSE events.
      // The backend already resumed via resume_or_notify.
      if (!currentChatId) {
        console.log("[chat-adapter] continuation: no chatId, returning");
        return;
      }

      console.log("[chat-adapter] continuation: starting drain for", currentChatId);
      const chatId = currentChatId;
      const queue = ensureQueue(chatId);
      const onAbort = onUserCancel(abortSignal, chatId);
      abortSignal.addEventListener("abort", onAbort);

      yield* streamEvents(queue.drain(abortSignal), abortSignal, onAbort);
    },
  };

  return {
    adapter,
    syncLastSentMessageId(messages) {
      for (let i = messages.length - 1; i >= 0; i--) {
        if (messages[i].role === "user" && messages[i].id) {
          lastSentMessageId = messages[i].id!;
          return;
        }
      }
    },
    chatId: () => currentChatId,
    destroy() {
      eventQueue?.destroy();
      eventQueue = null;
    },
  };
}

async function* streamEvents(
  events: AsyncIterable<ChatSSEEvent>,
  abortSignal: AbortSignal,
  onAbort: () => void,
) {
  let text = "";
  let reasoning = "";
  const toolCalls = new Map<string, ToolCallPart>();
  const toolResults = new Map<string, { result: string; isError: boolean }>();
  let toolCallCounter = 0;
  let lastTextSnapshot = 0;

  try {
    for await (const event of events) {
      console.log("[chat-adapter] event:", event.type, event);
      const result = handleEvent(event, text, reasoning, toolCalls, toolResults, toolCallCounter);
      // Capture text segment that arrived before this tool call
      if (event.type === "tool_call" && result.text.length > lastTextSnapshot) {
        const turnText = result.text.slice(lastTextSnapshot).trim();
        if (turnText) {
          const tc = toolCalls.get((event as { id: string }).id);
          if (tc) tc.args = { ...tc.args, turnText: turnText };
        }
        lastTextSnapshot = result.text.length;
      }
      text = result.text;
      reasoning = result.reasoning;
      toolCallCounter = result.toolCallCounter;

      if (result.retry) {
        setRetryState(result.retry);
      } else if (result.yield) {
        clearRetryState();
      }
      if (result.yield) {
        // Only show text from the current turn (after the last tool call).
        // Earlier turn text is already captured as turnText bubbles on tool calls.
        const displayText = lastTextSnapshot > 0
          ? text.slice(lastTextSnapshot)
          : text;
        let content = buildParts(displayText, reasoning, toolCalls, toolResults);
        if (result.done) content = promoteTurnText(content);
        const update: Record<string, unknown> = { content };
        if (result.done && result.requiresAction) {
          update.status = { type: "requires-action", reason: "tool-calls" };
        }
        yield update;
      }
      if (result.done) {
        console.log("[chat-adapter] stream done", { requiresAction: result.requiresAction });
        return;
      }
    }
  } finally {
    abortSignal.removeEventListener("abort", onAbort);
  }
}

interface HandleResult {
  text: string;
  reasoning: string;
  toolCallCounter: number;
  yield: boolean;
  done: boolean;
  requiresAction?: boolean;
  retry?: { reason: string; retryAfterSecs: number };
}

function handleEvent(
  event: ChatSSEEvent,
  text: string,
  reasoning: string,
  toolCalls: Map<string, ToolCallPart>,
  toolResults: Map<string, { result: string; isError: boolean }>,
  toolCallCounter: number,
): HandleResult {
  switch (event.type) {
    case "token":
      return { text: text + event.content, reasoning, toolCallCounter, yield: true, done: false };

    case "reasoning":
      return { text, reasoning: reasoning + event.content, toolCallCounter, yield: true, done: false };

    case "tool_call": {
      const args = tryParseJson(event.arguments);
      if (event.description) {
        args.description = event.description;
      }
      toolCalls.set(event.id, {
        type: "tool-call",
        toolCallId: event.id,
        toolName: event.name,
        args,
        argsText: typeof event.arguments === "string" ? event.arguments : JSON.stringify(event.arguments ?? {}),
      });
      return { text, reasoning, toolCallCounter: toolCallCounter + 1, yield: true, done: false };
    }

    case "tool_result": {
      let matchedId: string | null = null;
      for (const [id, tc] of toolCalls) {
        if (tc.toolName === event.name && !toolResults.has(id)) {
          matchedId = id;
          break;
        }
      }
      if (matchedId) {
        toolResults.set(matchedId, {
          result: event.summary ?? (event.success ? "Done" : "Error"),
          isError: !event.success,
        });
      }
      return { text, reasoning, toolCallCounter, yield: !!matchedId, done: false };
    }

    case "tool_message": {
      if (event.tool_execution) {
        const te = event.tool_execution;
        const toolName = te.tool_data?.type ?? te.name;
        // Remove the original tool_call entry (keyed by Anthropic tool_call_id)
        // so we don't show both the timeline item and the custom UI
        toolCalls.delete(te.tool_call_id);
        toolCalls.set(te.id, {
          type: "tool-call",
          toolCallId: te.id,
          toolName,
          args: (te.tool_data?.data ?? te.arguments) as Record<string, string | number | boolean | null>,
          argsText: JSON.stringify(te.tool_data?.data ?? te.arguments),
        });
      }
      return { text, reasoning, toolCallCounter, yield: true, done: true, requiresAction: true };
    }

    case "retry":
      return { text, reasoning, toolCallCounter, yield: true, done: false,
        retry: { reason: event.reason, retryAfterSecs: event.retryAfterSecs } };

    case "tool_resolved":
    case "chat_message":
      return { text, reasoning, toolCallCounter, yield: false, done: false };

    case "inference_done":
      return { text, reasoning, toolCallCounter, yield: true, done: true };

    case "inference_cancelled":
    case "inference_error":
      return { text, reasoning, toolCallCounter, yield: false, done: true };
  }
}
