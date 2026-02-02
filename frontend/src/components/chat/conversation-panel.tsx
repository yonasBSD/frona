"use client";

import { useSession } from "@/lib/session-context";
import { ChatHeader } from "./chat-header";
import { TaskHeader } from "./task-header";
import { MessageList } from "./message-list";
import { MessageInput } from "./message-input";

export function ConversationPanel({ children }: { children?: React.ReactNode }) {
  const { activeChatId, activeTask, pendingAgentId } = useSession();

  if (activeTask) {
    return (
      <div className="flex-1 overflow-y-auto bg-surface">
        <div className="mx-auto flex min-h-full w-full max-w-3xl flex-col">
          <TaskHeader />
          {activeChatId ? (
            <>
              <MessageList />
              <MessageInput />
            </>
          ) : (
            <div className="flex flex-1 items-center justify-center">
              <p className="text-sm text-text-tertiary">Task has not started yet.</p>
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto bg-surface">
      {(activeChatId || pendingAgentId) ? (
        <div className="mx-auto flex min-h-full w-full max-w-3xl flex-col">
          <ChatHeader />
          <MessageList />
          <MessageInput />
        </div>
      ) : (
        children
      )}
    </div>
  );
}
