"use client";

import { MessagePrimitive } from "@assistant-ui/react";
import { MarkdownText } from "./markdown-text";

export function FronaUserMessage() {
  return (
    <MessagePrimitive.Root>
      <div className="w-full">
        <div className="flex items-center gap-2.5 h-8">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-xs font-medium bg-accent text-surface">
            Y
          </div>
          <p className="text-xs font-medium text-text-tertiary">You</p>
        </div>
        <div className="pl-[42px] text-base text-text-primary">
          <MessagePrimitive.Parts
            components={{
              Text: MarkdownText,
            }}
          />
        </div>
      </div>
    </MessagePrimitive.Root>
  );
}
