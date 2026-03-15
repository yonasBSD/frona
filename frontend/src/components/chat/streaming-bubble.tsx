"use client";

import { MarkdownContent } from "./markdown-content";

interface StreamingBubbleProps {
  content: string;
  toolCalls?: { id: number; name: string; description: string | null; status: string }[];
  agentName: string;
}

export function StreamingBubble({ content, toolCalls, agentName }: StreamingBubbleProps) {
  return (
    <div className="flex justify-start min-h-[100px]">
      <div className="flex items-start gap-2.5 max-w-[85%]">
        <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-xs font-medium bg-surface-tertiary text-text-secondary">
          {agentName.charAt(0).toUpperCase()}
        </div>
        <div className="min-w-0 pt-0.5">
          <p className="text-xs font-medium text-text-tertiary mb-0.5">
            {agentName}
          </p>
          <div className="text-base text-text-primary">
            {content ? (
              <MarkdownContent content={content} />
            ) : !toolCalls?.length ? (
              <p className="animate-pulse text-text-tertiary">...</p>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}
