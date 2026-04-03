import type { ChatSSEEvent } from "./sse-event-bus";
import type { MessageResponse, Attachment, ToolExecution } from "./types";
import { api } from "./api-client";

interface ToolCallPart {
  type: "tool-call";
  toolCallId: string;
  toolName: string;
  args: Record<string, unknown>;
  argsText: string;
  result?: string;
  isError?: boolean;
}

export interface RetryInfo {
  reason: string;
  retryAfterSecs: number;
  startedAt: number;
}

export interface StoreSnapshot {
  messages: MessageResponse[];
  isRunning: boolean;
  loaded: boolean;
  retryInfo: RetryInfo | null;
  pendingTools: ToolExecution[];
}

/**
 * Per-chat reactive store that owns all message + streaming state.
 * SSE events are handled as simple state mutations + subscriber notifications.
 * React reads state via useSyncExternalStore.
 */
export class ChatStore {
  messages: MessageResponse[] = [];
  /** Text accumulated during the current inference stream. */
  streamingText = "";
  streamingReasoning = "";
  streamingToolCalls = new Map<string, ToolCallPart>();
  streamingToolResults = new Map<string, { result: string; isError: boolean }>();
  /** External tools waiting for user resolution (keyed by tool_call_id). */
  pendingExternalTools = new Map<string, ToolExecution>();
  isRunning = false;
  retryInfo: RetryInfo | null = null;
  loaded = false;

  /** Tracks text position at the time of each tool call for turnText extraction. */
  private lastTextSnapshot = 0;
  private retryTimer: ReturnType<typeof setTimeout> | null = null;
  private listeners = new Set<() => void>();
  private _snapshot: StoreSnapshot | null = null;

  subscribe(callback: () => void): () => void {
    this.listeners.add(callback);
    return () => this.listeners.delete(callback);
  }

  getSnapshot(): StoreSnapshot {
    if (!this._snapshot) {
      this._snapshot = {
        messages: this.getDisplayMessages(),
        isRunning: this.isRunning,
        loaded: this.loaded,
        retryInfo: this.retryInfo,
        pendingTools: this.getPendingExternalTools(),
      };
    }
    return this._snapshot;
  }

  private notify() {
    this._snapshot = null;
    for (const fn of this.listeners) fn();
  }

  markLoaded() {
    this.loaded = true;
    this.notify();
  }

  markRunning() {
    this.isRunning = true;
    this.notify();
  }

  getPendingExternalTools(): ToolExecution[] {
    return [...this.pendingExternalTools.values()].filter(
      (te) => te.tool_data?.data.status === "pending",
    );
  }

  /** Add a user message optimistically (before backend echo). */
  addUserMessage(content: string, attachments?: Attachment[]) {
    this.messages.push({
      id: `__user_${Date.now()}`,
      chat_id: "",
      role: "user",
      content,
      attachments,
      created_at: new Date().toISOString(),
    });
    this.isRunning = true;
    this.notify();
  }

  async loadMessages(chatId: string) {
    try {
      const { messages } = await api.get<{ messages: MessageResponse[]; has_more: boolean }>(
        `/api/chats/${chatId}/messages`,
      );
      // Don't overwrite if messages were added while we were fetching
      // (e.g. optimistic user message from onNew)
      if (this.messages.length === 0) {
        this.messages = messages;
      }
    } catch {
      // only clear if nothing was added in the meantime
      if (this.messages.length === 0) {
        this.messages = [];
      }
    }
    this.hydrateExternalTools();
    this.loaded = true;
    this.notify();
  }

  /** Scan loaded messages for pending external tool executions and populate the map. */
  private hydrateExternalTools() {
    for (const msg of this.messages) {
      if (!msg.tool_executions) continue;
      for (const te of msg.tool_executions) {
        if (te.tool_data && te.tool_data.data.status === "pending") {
          this.pendingExternalTools.set(te.tool_call_id, te);
        }
      }
    }
  }

  handleEvent(event: ChatSSEEvent) {
    switch (event.type) {
      case "token":
        this.isRunning = true;
        this.streamingText += event.content;
        break;

      case "reasoning":
        this.isRunning = true;
        this.streamingReasoning += event.content;
        break;

      case "tool_call": {
        this.isRunning = true;
        const args = tryParseJson(event.arguments);
        if (event.description) args.description = event.description;

        // Capture text spoken before this tool call as turnText
        if (this.streamingText.length > this.lastTextSnapshot) {
          const turnText = this.streamingText.slice(this.lastTextSnapshot).trim();
          if (turnText) args.turnText = turnText;
          this.lastTextSnapshot = this.streamingText.length;
        }

        this.streamingToolCalls.set(event.id, {
          type: "tool-call",
          toolCallId: event.id,
          toolName: event.name,
          args,
          argsText: typeof event.arguments === "string"
            ? event.arguments
            : JSON.stringify(event.arguments ?? {}),
        });
        break;
      }

      case "tool_result": {
        // Match by tool name to the first unresolved tool call
        for (const [id, tc] of this.streamingToolCalls) {
          if (tc.toolName === event.name && !this.streamingToolResults.has(id)) {
            this.streamingToolResults.set(id, {
              result: event.summary ?? (event.success ? "Done" : "Error"),
              isError: !event.success,
            });
            break;
          }
        }
        break;
      }

      case "tool_message": {
        if (event.tool_execution) {
          this.pendingExternalTools.set(event.tool_execution.tool_call_id, event.tool_execution);
        }
        break;
      }

      case "tool_resolved": {
        if (event.tool_execution && this.pendingExternalTools.has(event.tool_execution.tool_call_id)) {
          this.pendingExternalTools.set(event.tool_execution.tool_call_id, event.tool_execution);
        } else if (event.message) {
          const msg = event.message;
          const idx = this.messages.findIndex((m) => m.id === msg.id);
          if (idx >= 0) {
            // Preserve tool_executions — backend MessageResponse always has them empty
            if ((!msg.tool_executions || msg.tool_executions.length === 0) && this.messages[idx].tool_executions?.length) {
              msg.tool_executions = this.messages[idx].tool_executions;
            }
            this.messages[idx] = msg;
          }
        } else if (event.tool_execution) {
          this.updateToolExecution(event.tool_execution);
        }
        break;
      }

      case "inference_done": {
        const msg = event.message;

        // Merge tool_executions: existing (API-loaded) + streaming state + inference_done msg
        const existingIdx = this.messages.findIndex((m) => m.id === msg.id);
        const existingTools = existingIdx >= 0
          ? (this.messages[existingIdx].tool_executions ?? [])
          : [];
        const streamingTools = this.buildToolExecutions();

        // Combine all tools, dedup by id
        const seenIds = new Set<string>();
        const allTools: ToolExecution[] = [];
        for (const t of [...existingTools, ...streamingTools, ...(msg.tool_executions ?? [])]) {
          if (!seenIds.has(t.id)) {
            seenIds.add(t.id);
            allTools.push(t);
          }
        }
        if (allTools.length > 0) {
          msg.tool_executions = allTools;
        }

        if (existingIdx >= 0) {
          this.messages[existingIdx] = msg;
        } else {
          this.messages.push(msg);
        }
        this.clearStreaming();
        break;
      }

      case "chat_message": {
        // Replace optimistic user message with the real one from the backend
        if (event.message.role === "user") {
          const optIdx = this.messages.findIndex((m) => m.id.startsWith("__user_"));
          if (optIdx >= 0) {
            this.messages[optIdx] = event.message;
            break;
          }
        }
        // Skip if this message ID is already in the array
        if (!this.messages.some((m) => m.id === event.message.id)) {
          this.messages.push(event.message);
        }
        break;
      }

      case "retry": {
        this.retryInfo = {
          reason: event.reason,
          retryAfterSecs: event.retryAfterSecs,
          startedAt: Date.now(),
        };
        // Auto-clear after the retry period
        if (this.retryTimer) clearTimeout(this.retryTimer);
        const capturedStart = this.retryInfo.startedAt;
        this.retryTimer = setTimeout(() => {
          if (this.retryInfo?.startedAt === capturedStart) {
            this.retryInfo = null;
            this.notify();
          }
        }, event.retryAfterSecs * 1000 + 500);
        break;
      }

      case "inference_cancelled":
      case "inference_error":
        this.clearStreaming();
        break;
    }
    this.notify();
  }

  /**
   * Build the display message list. During streaming, appends a synthetic
   * "in-progress" assistant message built from the streaming state.
   */
  getDisplayMessages(): MessageResponse[] {
    const merged = mergeConsecutiveMessages(this.messages);
    if (!this.isRunning) return merged;

    const displayText = this.lastTextSnapshot > 0
      ? this.streamingText.slice(this.lastTextSnapshot)
      : this.streamingText;

    const streamingTools = this.buildToolExecutions();

    // If the last message is already executing (e.g. page refreshed mid-inference),
    // merge new streaming state into it instead of appending a separate message.
    const last = merged[merged.length - 1];
    if (last?.status === "executing" && last.role === "agent") {
      const updated = { ...last };
      if (displayText) {
        updated.content = [last.content, displayText].filter(Boolean).join("");
      }
      if (this.streamingReasoning) {
        updated.reasoning = [last.reasoning, this.streamingReasoning].filter(Boolean).join("");
      }
      if (streamingTools.length > 0) {
        updated.tool_executions = [...(last.tool_executions ?? []), ...streamingTools];
      }
      return [...merged.slice(0, -1), updated];
    }

    const syntheticMessage: MessageResponse = {
      id: "__streaming__",
      chat_id: "",
      role: "agent",
      content: displayText,
      reasoning: this.streamingReasoning || undefined,
      tool_executions: streamingTools.length > 0 ? streamingTools : undefined,
      status: "executing",
      created_at: new Date().toISOString(),
    };

    return [...merged, syntheticMessage];
  }

  private buildToolExecutions(): ToolExecution[] {
    const result: ToolExecution[] = [];
    for (const tc of this.streamingToolCalls.values()) {
      const pending = this.pendingExternalTools.get(tc.toolCallId);
      if (pending) {
        result.push(pending);
      } else {
        const res = this.streamingToolResults.get(tc.toolCallId);
        result.push({
          id: tc.toolCallId,
          chat_id: "",
          message_id: "",
          turn: 0,
          tool_call_id: tc.toolCallId,
          name: tc.toolName,
          arguments: tc.args as Record<string, unknown>,
          result: res?.result ?? "",
          success: res ? !res.isError : true,
          duration_ms: 0,
          description: tc.args.description as string | undefined,
          turn_text: tc.args.turnText as string | undefined,
          created_at: "",
        });
      }
    }
    return result;
  }

  resolveToolCall(toolCallId: string, result: string) {
    // Check pending external tools (streaming state) — optimistic update
    const pending = this.pendingExternalTools.get(toolCallId);
    if (pending?.tool_data) {
      this.pendingExternalTools.set(toolCallId, {
        ...pending,
        result,
        tool_data: { ...pending.tool_data, data: { ...pending.tool_data.data, status: "resolved", response: result } } as typeof pending.tool_data,
      });
      this.notify();
      return;
    }

    // Fall through to message-based resolution
    for (let i = this.messages.length - 1; i >= 0; i--) {
      const msg = this.messages[i];
      if (!msg.tool_executions) continue;
      const teIdx = msg.tool_executions.findIndex(
        (t) => t.id === toolCallId || t.tool_call_id === toolCallId,
      );
      if (teIdx < 0) continue;
      const te = msg.tool_executions[teIdx];
      const updatedTe = te.tool_data
        ? { ...te, result, tool_data: { ...te.tool_data, data: { ...te.tool_data.data, status: "resolved", response: result } } as typeof te.tool_data }
        : { ...te, result };
      const updatedExecutions = [...msg.tool_executions];
      updatedExecutions[teIdx] = updatedTe;
      const allResolved = updatedExecutions.every(
        (t) => !t.tool_data || t.tool_data.data.status !== "pending",
      );
      this.messages[i] = {
        ...msg,
        tool_executions: updatedExecutions,
        status: allResolved && msg.status === "executing" ? "completed" : msg.status,
      };
      this.notify();
      return;
    }
  }

  private updateToolExecution(te: ToolExecution) {
    for (let i = this.messages.length - 1; i >= 0; i--) {
      const msg = this.messages[i];
      if (!msg.tool_executions) continue;
      const teIdx = msg.tool_executions.findIndex((t) => t.id === te.id);
      if (teIdx >= 0) {
        const updatedExecutions = [...msg.tool_executions];
        updatedExecutions[teIdx] = te;
        const allResolved = updatedExecutions.every(
          (t) => !t.tool_data || t.tool_data.data.status !== "pending",
        );
        this.messages[i] = {
          ...msg,
          tool_executions: updatedExecutions,
          status: allResolved && msg.status === "executing" ? "completed" : msg.status,
        };
        break;
      }
    }
  }

  clearStreaming() {
    this.isRunning = false;
    this.streamingText = "";
    this.streamingReasoning = "";
    this.streamingToolCalls.clear();
    this.streamingToolResults.clear();
    this.pendingExternalTools.clear();
    this.lastTextSnapshot = 0;
    this.retryInfo = null;
    if (this.retryTimer) {
      clearTimeout(this.retryTimer);
      this.retryTimer = null;
    }
  }
}

/**
 * Merge consecutive agent messages from the same agent into a single message.
 * This prevents fragmentation when the backend sends multiple messages in sequence.
 */
export function mergeConsecutiveMessages(messages: MessageResponse[]): MessageResponse[] {
  const result: MessageResponse[] = [];
  for (const msg of messages) {
    const prev = result[result.length - 1];
    if (
      prev &&
      prev.role === "agent" &&
      (msg.role === "agent" || (msg.role === "system" && msg.event)) &&
      (prev.agent_id === msg.agent_id || msg.role === "system") &&
      prev.status !== "executing"
    ) {
      const mergedTools = [...(prev.tool_executions ?? []), ...(msg.tool_executions ?? [])];
      result[result.length - 1] = {
        ...prev,
        content: [prev.content, msg.content].filter(Boolean).join("\n\n"),
        reasoning: [prev.reasoning, msg.reasoning].filter(Boolean).join("\n\n") || undefined,
        tool_executions: mergedTools.length > 0 ? mergedTools : undefined,
        status: msg.status,
      };
    } else {
      result.push(msg);
    }
  }
  return result;
}

function tryParseJson(value: unknown): Record<string, unknown> {
  if (typeof value === "string") {
    try {
      const parsed = JSON.parse(value);
      if (typeof parsed === "object" && parsed !== null) return parsed;
      return {};
    } catch {
      return {};
    }
  }
  if (typeof value === "object" && value !== null) return value as Record<string, unknown>;
  return {};
}
