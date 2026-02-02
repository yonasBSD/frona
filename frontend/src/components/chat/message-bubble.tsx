"use client";

import type { MessageResponse } from "@/lib/types";
import { useNavigation } from "@/lib/navigation-context";
import { MarkdownContent } from "./markdown-content";

interface MessageBubbleProps {
  message: MessageResponse;
  agentName: string;
}

export function MessageBubble({ message, agentName }: MessageBubbleProps) {
  const isUser = message.role === "user";
  const { agents } = useNavigation();

  const displayName = isUser
    ? "You"
    : message.agent_id
      ? (agents.find((a) => a.id === message.agent_id)?.name ?? agentName)
      : agentName;

  return (
    <div className="flex justify-start">
      <div className="flex items-start gap-2.5 max-w-[85%]">
        <div
          className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-xs font-medium ${
            isUser
              ? "bg-accent text-surface"
              : "bg-surface-tertiary text-text-secondary"
          }`}
        >
          {isUser ? "U" : displayName.charAt(0).toUpperCase()}
        </div>
        <div className="min-w-0 pt-0.5">
          <p className="text-[11px] font-medium text-text-tertiary mb-0.5">
            {displayName}
          </p>
          <div className="text-sm text-text-primary">
            {isUser ? (
              <p className="whitespace-pre-wrap">{message.content}</p>
            ) : (
              <MarkdownContent content={message.content} />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
