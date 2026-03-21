"use client";

import { ThreadPrimitive } from "@assistant-ui/react";
import { FronaUserMessage } from "./frona-user-message";
import { FronaAssistantMessage } from "./frona-assistant-message";
import { FronaComposer } from "./frona-composer";

export function AssistantThread() {
  return (
    <ThreadPrimitive.Root className="flex flex-1 flex-col min-h-0">
      <ThreadPrimitive.Viewport className="flex-1 overflow-y-auto min-h-0">
        <ThreadPrimitive.If empty>
          <div />
        </ThreadPrimitive.If>
        <ThreadPrimitive.If empty={false}>
          <div className="mx-auto w-full max-w-3xl px-6 py-4 space-y-3">
            <ThreadPrimitive.Messages
            components={{
              UserMessage: FronaUserMessage,
              AssistantMessage: FronaAssistantMessage,
            }}
          />
          </div>
        </ThreadPrimitive.If>
      </ThreadPrimitive.Viewport>
      <ThreadPrimitive.ViewportFooter className="sticky bottom-0">
        <ThreadPrimitive.ScrollToBottom asChild>
          <button className="absolute -top-10 left-1/2 -translate-x-1/2 rounded-full border border-border bg-surface px-3 py-1 text-xs text-text-secondary shadow-sm hover:bg-surface-secondary transition disabled:hidden">
            Scroll to bottom
          </button>
        </ThreadPrimitive.ScrollToBottom>
        <div className="mx-auto w-full max-w-3xl px-6 pb-4">
          <FronaComposer />
        </div>
      </ThreadPrimitive.ViewportFooter>
    </ThreadPrimitive.Root>
  );
}
