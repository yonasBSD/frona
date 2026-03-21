import type { ChatModelAdapter, ChatModelRunOptions, AttachmentAdapter, PendingAttachment, CompleteAttachment } from "@assistant-ui/react";
import { sendMessage as apiSendMessage, cancelGeneration, api, uploadFile } from "./api-client";
import { sseBus, type ChatSSEEvent } from "./sse-event-bus";
import { setRetryState, clearRetryState } from "./retry-state";
import type { Attachment as BackendAttachment, ChatResponse } from "./types";

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
  if (text) parts.push({ type: "text", text });
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

export interface ChatAdapterOptions {
  chatId?: string;
  agentId: string;
  onChatCreated?: (chat: ChatResponse) => void;
}

/**
 * Shared event queue that persists across run() calls.
 * A single SSE subscriber pushes events into the queue.
 * Each run() drains from it via createDrain().
 */
class ChatEventQueue {
  private buffer: ChatSSEEvent[] = [];
  private waiter: ((event: ChatSSEEvent) => void) | null = null;
  private controller: AbortController;

  constructor(chatId: string) {
    this.controller = new AbortController();
    const self = this;
    const events = sseBus.subscribe(chatId, this.controller.signal);

    // Start consuming the subscription — push events into our buffer
    (async () => {
      for await (const event of events) {
        if (self.waiter) {
          const resolve = self.waiter;
          self.waiter = null;
          resolve(event);
        } else {
          self.buffer.push(event);
        }
      }
    })();
  }

  /** Create an async iterable that drains events from the shared queue. */
  drain(signal: AbortSignal): AsyncIterable<ChatSSEEvent> {
    const self = this;
    return {
      [Symbol.asyncIterator]() {
        return {
          next(): Promise<IteratorResult<ChatSSEEvent>> {
            if (signal.aborted) {
              return Promise.resolve({ value: undefined as unknown as ChatSSEEvent, done: true });
            }
            if (self.buffer.length > 0) {
              return Promise.resolve({ value: self.buffer.shift()!, done: false });
            }
            return new Promise<IteratorResult<ChatSSEEvent>>((resolve) => {
              const onAbort = () => {
                self.waiter = null;
                resolve({ value: undefined as unknown as ChatSSEEvent, done: true });
              };
              signal.addEventListener("abort", onAbort, { once: true });
              self.waiter = (event) => {
                signal.removeEventListener("abort", onAbort);
                resolve({ value: event, done: false });
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
  /** Signal that the next run() is an explicit user send (not a continuation). */
  send: () => void;
  chatId: () => string | null;
}

export function createChatAdapter(options: ChatAdapterOptions): ChatAdapterHandle {
  let currentChatId = options.chatId ?? null;
  let eventQueue: ChatEventQueue | null = null;
  let outgoingMessage = false;

  function ensureQueue(chatId: string): ChatEventQueue {
    if (eventQueue) return eventQueue;
    eventQueue = new ChatEventQueue(chatId);
    return eventQueue;
  }

  function send() {
    outgoingMessage = true;
  }

  const adapter: ChatModelAdapter = {
    async *run({ messages, abortSignal }: ChatModelRunOptions) {
      const toSend = outgoingMessage;
      outgoingMessage = false;

      if (toSend) {
        // Extract content and attachments from the last user message
        const lastMsg = messages[messages.length - 1];
        const content = lastMsg?.role === "user"
          ? lastMsg.content
              .filter((p): p is { type: "text"; text: string } => p.type === "text")
              .map((p) => p.text)
              .join("")
          : "";

        const attachments: BackendAttachment[] = [];
        if (lastMsg?.role === "user") {
          for (const att of lastMsg.attachments) {
            const backend = backendAttachmentRegistry.get(att.id);
            if (backend) {
              attachments.push(backend);
            }
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
        const onAbort = () => { cancelGeneration(chatId).catch(() => {}); };
        abortSignal.addEventListener("abort", onAbort);

        const body = attachments.length
          ? { content, attachments }
          : { content };

        await apiSendMessage(chatId, body);

        yield* streamEvents(queue.drain(abortSignal), abortSignal, onAbort);
        return;
      }

      // Continuation (addResult, etc.) — just stream SSE events.
      // The backend already resumed via resume_or_notify.
      if (!currentChatId) return;

      const chatId = currentChatId;
      const queue = ensureQueue(chatId);
      const onAbort = () => { cancelGeneration(chatId).catch(() => {}); };
      abortSignal.addEventListener("abort", onAbort);

      yield { content: [{ type: "text" as const, text: "" }] };
      yield* streamEvents(queue.drain(abortSignal), abortSignal, onAbort);
    },
  };

  return {
    adapter,
    send,
    chatId: () => currentChatId,
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

  try {
    for await (const event of events) {
      const result = handleEvent(event, text, reasoning, toolCalls, toolResults, toolCallCounter);
      text = result.text;
      reasoning = result.reasoning;
      toolCallCounter = result.toolCallCounter;

      if (result.retry) {
        setRetryState(result.retry);
      } else if (result.yield) {
        clearRetryState();
      }
      if (result.yield) {
        const update: Record<string, unknown> = {
          content: buildParts(text, reasoning, toolCalls, toolResults),
        };
        if (result.done && result.requiresAction) {
          update.status = { type: "requires-action", reason: "tool-calls" };
        }
        yield update;
      }
      if (result.done) return;
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
      return { text, reasoning, toolCallCounter: toolCallCounter + 1, yield: false, done: false };
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
      const msg = event.message;
      const toolName = msg.tool!.type;
      if (msg.tool_call_id) {
        toolCalls.delete(msg.tool_call_id);
      }
      toolCalls.set(msg.id, {
        type: "tool-call",
        toolCallId: msg.id,
        toolName,
        args: msg.tool!.data as Record<string, string | number | boolean | null>,
        argsText: JSON.stringify(msg.tool!.data),
      });
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
