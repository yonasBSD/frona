"use client";

import { useCallback, useState, useEffect, useRef } from "react";
import { AssistantRuntimeProvider } from "@assistant-ui/react";
import { useSession } from "@/lib/session-context";
import { useNavigation } from "@/lib/navigation-context";
import { ChatProvider } from "@/lib/chat-context";
import { useFronaRuntime } from "@/lib/assistant-runtime";
import { ChatHeader } from "./chat-header";
import { TaskHeader } from "./task-header";
import { AssistantThread } from "./assistant-thread";
import { ToolUIRegistry } from "./tool-uis";
import type { ChatResponse } from "@/lib/types";

function ChatView({ chatId, agentId }: { chatId?: string; agentId: string }) {
  const { setActiveChat, getPendingMessage } = useSession();
  const { addStandaloneChat } = useNavigation();
  const [currentChatId, setCurrentChatId] = useState<string | null>(chatId ?? null);

  const onChatCreated = useCallback((chat: ChatResponse) => {
    setCurrentChatId(chat.id);
    addStandaloneChat(chat);
    setActiveChat(chat);
  }, [addStandaloneChat, setActiveChat]);

  const { runtime, loaded, send, sendMessage } = useFronaRuntime({ chatId, agentId, onChatCreated });

  const pendingHandled = useRef(false);
  useEffect(() => {
    if (!loaded || pendingHandled.current) return;
    pendingHandled.current = true;
    const pending = getPendingMessage();
    if (pending) {
      sendMessage(pending);
    }
  }, [loaded, sendMessage, getPendingMessage]);

  const content = (
    <>
      <ToolUIRegistry />
      {loaded ? <AssistantThread onSend={send} /> : <div className="flex-1" />}
    </>
  );

  return (
    <AssistantRuntimeProvider runtime={runtime}>
      {currentChatId ? (
        <ChatProvider chatId={currentChatId} agentId={agentId}>
          {content}
        </ChatProvider>
      ) : (
        content
      )}
    </AssistantRuntimeProvider>
  );
}

export function ConversationPanel() {
  const { activeChatId, activeChat, activeTask, agentId } = useSession();

  const [pendingSessionId, setPendingSessionId] = useState(0);
  const [prevActiveChat, setPrevActiveChat] = useState(activeChat);

  if (activeChat !== prevActiveChat) {
    setPrevActiveChat(activeChat);
    if (!activeChat && prevActiveChat) {
      setPendingSessionId((n) => n + 1);
    }
  }

  const chatViewKey = activeChatId ?? `pending-${pendingSessionId}`;
  const effectiveAgentId = agentId ?? "system";

  if (activeTask) {
    return (
      <div className="flex-1 overflow-hidden bg-surface flex flex-col">
        <div className="mx-auto w-full max-w-3xl">
          <TaskHeader />
        </div>
        {activeChatId ? (
          <ChatView key={activeChatId} chatId={activeChatId} agentId={effectiveAgentId} />
        ) : (
          <div className="flex flex-1 items-center justify-center">
            <p className="text-sm text-text-tertiary">Task has not started yet.</p>
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-hidden bg-surface flex flex-col">
      <div className="mx-auto w-full max-w-3xl">
        <ChatHeader />
      </div>
      <ChatView
        key={chatViewKey}
        chatId={activeChatId ?? undefined}
        agentId={effectiveAgentId}
      />
    </div>
  );
}
