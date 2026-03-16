"use client";

import { MarkdownContent } from "./markdown-content";

interface StreamingBubbleProps {
  content: string;
  agentName: string;
}

export function StreamingBubble({ content, agentName }: StreamingBubbleProps) {
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
            ) : (
              <span className="inline-flex items-center gap-1 py-1">
                <span className="h-1 w-1 rounded-full bg-text-tertiary animate-[wave_1.4s_ease-in-out_infinite]" />
                <span className="h-1 w-1 rounded-full bg-text-tertiary animate-[wave_1.4s_ease-in-out_0.2s_infinite]" />
                <span className="h-1 w-1 rounded-full bg-text-tertiary animate-[wave_1.4s_ease-in-out_0.4s_infinite]" />
              </span>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
