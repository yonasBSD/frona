"use client";

import { useMemo, useState, useEffect, useCallback, useRef } from "react";
import { useLocalRuntime } from "@assistant-ui/react";
import type { ThreadMessageLike } from "@assistant-ui/react";
import { createChatAdapter, fronaAttachmentAdapter, registerBackendAttachment, promoteTurnText } from "./chat-adapter";
import { sseBus } from "./sse-event-bus";
import { api, fileDownloadUrl } from "./api-client";
import type { MessageResponse, ChatResponse, Attachment } from "./types";
import type { CompleteAttachment } from "@assistant-ui/react";

function convertBackendAttachment(att: Attachment): CompleteAttachment {
  const username = att.owner.startsWith("user:") ? att.owner.substring(5) : "";
  const url = fileDownloadUrl(att, username);
  const isImage = att.content_type.startsWith("image/");

  registerBackendAttachment(att.path, att);

  return {
    id: att.path,
    type: isImage ? "image" : "file",
    name: att.filename,
    contentType: att.content_type,
    status: { type: "complete" },
    content: isImage
      ? [{ type: "image", image: url }]
      : [{ type: "text", text: `[file: ${att.filename}]` }],
  };
}

function convertMessage(msg: MessageResponse): ThreadMessageLike | null {
  if (msg.role === "user" || msg.role === "contact" || msg.role === "livecall") {
    const attachments = msg.attachments?.map(convertBackendAttachment);
    return {
      id: msg.id,
      role: "user" as const,
      content: [{ type: "text" as const, text: msg.content || "" }],
      createdAt: new Date(msg.created_at),
      ...(attachments?.length ? { attachments } : {}),
      metadata: {
        custom: {
          originalRole: msg.role,
          contactId: msg.contact_id,
        },
      },
    };
  }

  if (msg.role === "agent" || msg.role === "taskcompletion" || (msg.role === "system" && msg.event)) {
    const content: Array<
      | { type: "text"; text: string }
      | { type: "reasoning"; text: string }
      | { type: "tool-call"; toolCallId: string; toolName: string; args: Record<string, string | number | boolean | null>; argsText: string; result?: string }
    > = [];

    if (msg.reasoning) {
      content.push({ type: "reasoning" as const, text: msg.reasoning });
    }

    if (msg.content) {
      content.push({ type: "text" as const, text: msg.content });
    }

    if (!msg.content && !msg.reasoning && !msg.event) {
      content.push({ type: "text" as const, text: "" });
    }

    if (msg.tool_executions?.length) {
      for (const te of msg.tool_executions) {
        if (te.tool_data) {
          const toolName = te.tool_data.type;
          const status = te.tool_data.data.status;
          const resolved = status === "resolved" || status === "denied";
          const toolData = te.tool_data.data as Record<string, string | number | boolean | null>;
          const response = "response" in te.tool_data.data
            ? (te.tool_data.data as { response?: string | null }).response
            : null;
          content.push({
            type: "tool-call" as const,
            toolCallId: te.id,
            toolName,
            args: toolData,
            argsText: JSON.stringify(toolData),
            ...(resolved && response != null ? { result: response } : {}),
            ...(resolved && response == null ? { result: String(status) } : {}),
          });
        } else {
          content.push({
            type: "tool-call" as const,
            toolCallId: te.tool_call_id,
            toolName: te.name,
            args: {
              description: te.name,
              ...te.arguments,
              ...(te.turn_text ? { turnText: te.turn_text } : {}),
            } as Record<string, string | number | boolean | null>,
            argsText: JSON.stringify(te.arguments || {}),
            result: te.result,
          });
        }
      }
    }

    return {
      id: msg.id,
      role: "assistant" as const,
      content: promoteTurnText(content),
      createdAt: new Date(msg.created_at),
      status: msg.tool_executions?.some(te => te.tool_data && te.tool_data.data.status === "pending")
        || msg.status === "executing"
        ? { type: "requires-action" as const, reason: "tool-calls" as const }
        : { type: "complete" as const, reason: "stop" as const },
      metadata: {
        custom: {
          agentId: msg.agent_id,
          originalRole: msg.role,
        },
      },
    };
  }

  return null;
}

/** Merge consecutive messages with the same role into a single message. */
function mergeConsecutive(messages: ThreadMessageLike[]): ThreadMessageLike[] {
  const result: ThreadMessageLike[] = [];
  for (const msg of messages) {
    const prev = result[result.length - 1];
    const prevAgentId = (prev?.metadata as Record<string, any>)?.custom?.agentId;
    const msgAgentId = (msg.metadata as Record<string, any>)?.custom?.agentId;
    const msgOriginalRole = (msg.metadata as Record<string, any>)?.custom?.originalRole;
    if (
      prev &&
      prev.role === msg.role &&
      msg.role === "assistant" &&
      (prevAgentId === msgAgentId || msgOriginalRole === "system") &&
      prev.status?.type !== "requires-action"
    ) {
      // Merge content parts into the previous message
      const prevContent = Array.isArray(prev.content) ? prev.content : [prev.content];
      const msgContent = Array.isArray(msg.content) ? msg.content : [msg.content];
      result[result.length - 1] = {
        ...prev,
        content: [...prevContent, ...msgContent] as ThreadMessageLike["content"],
        status: msg.status,
      };
    } else {
      result.push(msg);
    }
  }
  return result;
}

export interface FronaRuntimeOptions {
  chatId?: string;
  agentId: string;
  onChatCreated?: (chat: ChatResponse) => void;
}

export function useFronaRuntime({ chatId, agentId, onChatCreated }: FronaRuntimeOptions) {
  const runtimeRef = useRef<ReturnType<typeof useLocalRuntime>>(null!);

  const handle = useMemo(
    () => createChatAdapter({
      chatId, agentId, onChatCreated,
      onChatMessage: (msg) => {
        const converted = convertMessage(msg);
        if (converted) {
          runtimeRef.current.thread.append({
            ...converted,
            startRun: false,
          } as Parameters<typeof runtimeRef.current.thread.append>[0]);
        }
      },
    }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [chatId, agentId],
  );

  const runtime = useLocalRuntime(handle.adapter, {
    unstable_humanToolNames: [
      "Question",
      "HumanInTheLoop",
      "VaultApproval",
      "ServiceApproval",
      "TaskCompletion",
    ],
    adapters: {
      attachments: fronaAttachmentAdapter,
    },
  });
  runtimeRef.current = runtime;

  // Clean up SSE subscription when the chat adapter is permanently unmounted (LRU eviction)
  useEffect(() => {
    return () => handle.destroy();
  }, [handle]);

  // Load messages for existing chats and reset the runtime with them
  const [loaded, setLoaded] = useState(!chatId);

  const loadMessages = useCallback((id: string) => {
    return api.get<MessageResponse[]>(`/api/chats/${id}/messages`)
      .then((msgs) => {
        const converted = mergeConsecutive(msgs.map(convertMessage).filter(Boolean) as ThreadMessageLike[]);
        handle.syncLastSentMessageId(converted);
        runtime.thread.reset(converted);
      });
  }, [handle, runtime]);

  useEffect(() => {
    if (!chatId) {
      setLoaded(true);
      return;
    }
    let cancelled = false;
    loadMessages(chatId)
      .then(() => { if (!cancelled) setLoaded(true); })
      .catch(() => { if (!cancelled) setLoaded(true); });
    return () => { cancelled = true; };
  }, [chatId, loadMessages]);

  // Reload messages when SSE reconnects after a drop to pick up anything missed.
  // Skip if a stream is active — it manages its own state.
  useEffect(() => {
    return sseBus.onReconnect(() => {
      const id = chatId ?? handle.chatId();
      if (!id) return;
      try {
        if (runtime.thread.getState().isRunning) return;
      } catch { /* thread not initialized yet */ }
      loadMessages(id).catch(() => {});
    });
  }, [chatId, handle, runtime, loadMessages]);

  /** Append to thread — used for programmatic sends (pending messages). */
  const sendMessage = useCallback((content: string, attachments?: Attachment[]) => {
    if (attachments?.length) {
      for (const att of attachments) {
        registerBackendAttachment(att.path, att);
      }
    }
    runtime.thread.append({
      role: "user",
      content: [{ type: "text", text: content }],
      attachments: attachments?.map(convertBackendAttachment),
    });
  }, [runtime]);

  return { runtime, loaded, sendMessage };
}
