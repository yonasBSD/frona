"use client";

import { useMemo, useState, useEffect, useCallback } from "react";
import { useLocalRuntime } from "@assistant-ui/react";
import type { ThreadMessageLike } from "@assistant-ui/react";
import { createChatAdapter, fronaAttachmentAdapter, registerBackendAttachment } from "./chat-adapter";
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
  if (msg.role === "toolresult" && !msg.tool) return null;
  if (msg.role === "agent" && !msg.content && msg.tool_calls) return null;

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

  if (msg.role === "agent" || msg.role === "taskcompletion" || (msg.role === "toolresult" && msg.tool)) {
    const content: Array<
      | { type: "text"; text: string }
      | { type: "reasoning"; text: string }
      | { type: "tool-call"; toolCallId: string; toolName: string; args: Record<string, string | number | boolean | null>; argsText: string; result?: string }
    > = [];

    if (msg.reasoning) {
      content.push({ type: "reasoning" as const, text: msg.reasoning });
    }

    if (msg.content && !(msg.role === "toolresult" && msg.tool)) {
      content.push({ type: "text" as const, text: msg.content });
    }

    if (!msg.content && !msg.reasoning && !msg.tool) {
      content.push({ type: "text" as const, text: "" });
    }

    if (msg.tool) {
      const toolName = msg.tool.type;
      const resolved = msg.tool.data.status === "resolved" || msg.tool.data.status === "denied";
      const toolData = msg.tool.data as Record<string, string | number | boolean | null>;
      const response = "response" in msg.tool.data ? (msg.tool.data as { response?: string | null }).response : null;
      content.push({
        type: "tool-call" as const,
        toolCallId: msg.id,
        toolName,
        args: toolData,
        argsText: JSON.stringify(toolData),
        ...(resolved && response != null ? { result: response } : {}),
        ...(resolved && response == null ? { result: String(msg.tool.data.status) } : {}),
      });
    }

    return {
      id: msg.id,
      role: "assistant" as const,
      content,
      createdAt: new Date(msg.created_at),
      status: msg.tool && msg.tool.data.status === "pending"
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
    if (
      prev &&
      prev.role === msg.role &&
      msg.role === "assistant" &&
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
  const handle = useMemo(
    () => createChatAdapter({ chatId, agentId, onChatCreated }),
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

  // Load messages for existing chats and reset the runtime with them
  const [loaded, setLoaded] = useState(!chatId);

  useEffect(() => {
    if (!chatId) {
      setLoaded(true);
      return;
    }
    let cancelled = false;
    api.get<MessageResponse[]>(`/api/chats/${chatId}/messages`)
      .then((msgs) => {
        if (cancelled) return;
        const converted = mergeConsecutive(msgs.map(convertMessage).filter(Boolean) as ThreadMessageLike[]);
        runtime.thread.reset(converted);
        setLoaded(true);
      })
      .catch(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => { cancelled = true; };
  }, [chatId, runtime]);

  /** Set the outgoing flag only — used by the composer's onSubmit (runtime handles append). */
  const send = useCallback(() => {
    handle.send();
  }, [handle]);

  /** Set flag + append to thread — used for programmatic sends (pending messages). */
  const sendMessage = useCallback((content: string, attachments?: Attachment[]) => {
    if (attachments?.length) {
      for (const att of attachments) {
        registerBackendAttachment(att.path, att);
      }
    }
    handle.send();
    runtime.thread.append({
      role: "user",
      content: [{ type: "text", text: content }],
      attachments: attachments?.map(convertBackendAttachment),
    });
  }, [handle, runtime]);

  return { runtime, loaded, send, sendMessage };
}
