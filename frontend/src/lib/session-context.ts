"use client";

import {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  createElement,
} from "react";
import { useSearchParams, useRouter } from "next/navigation";
import { api, sendMessage as apiSendMessage, streamSession, cancelGeneration, getTask } from "./api-client";
import type { StreamSessionCallbacks } from "./api-client";
import { useNavigation } from "./navigation-context";
import { useNotifications } from "./notification-context";
import type { ChatResponse, MessageResponse, CreateChatRequest, ToolCallStatus, TaskResponse, Attachment } from "./types";

interface ChatStreamState {
  sending: boolean;
  streamingContent: string;
  toolCalls: ToolCallStatus[];
  nextToolId: number;
}

function emptyStreamState(): ChatStreamState {
  return { sending: false, streamingContent: "", toolCalls: [], nextToolId: 0 };
}

// --- Module-level streaming state (only one SessionProvider exists) ---
// Wrapped in an object so property mutations don't trigger the globals lint rule.

const sessionStore = {
  activeChatId: null as string | null,
  activeTaskId: null as string | null,
  pendingMessage: null as string | null,
  syncToReact: null as ((chatId: string, s: ChatStreamState) => void) | null,
  streams: new Map<string, ChatStreamState>(),
};

function getStream(chatId: string): ChatStreamState {
  let s = sessionStore.streams.get(chatId);
  if (!s) {
    s = emptyStreamState();
    sessionStore.streams.set(chatId, s);
  }
  return s;
}

function updateStream(chatId: string, fn: (s: ChatStreamState) => void) {
  const s = getStream(chatId);
  fn(s);
  if (chatId === sessionStore.activeChatId && sessionStore.syncToReact) {
    sessionStore.syncToReact(chatId, s);
  }
}

function resetStream(chatId: string) {
  const s = getStream(chatId);
  s.sending = false;
  s.streamingContent = "";
  s.toolCalls = [];
  if (chatId === sessionStore.activeChatId && sessionStore.syncToReact) {
    sessionStore.syncToReact(chatId, s);
  }
}

// --- Context ---

interface SessionContextValue {
  activeChatId: string | null;
  activeChat: ChatResponse | null;
  activeTaskId: string | null;
  activeTask: TaskResponse | null;
  pendingAgentId: string | null;
  messages: MessageResponse[];
  sending: boolean;
  inferring: boolean;
  streamingContent: string | null;
  activeToolCalls: ToolCallStatus[];
  sendMessage: (content: string, attachments?: Attachment[]) => Promise<void>;
  stopGeneration: () => void;
  createChat: (req: CreateChatRequest) => Promise<ChatResponse>;
  setPendingMessage: (message: string) => void;
  resolveToolMessage: (messageId: string, response?: string) => Promise<void>;
}

const SessionContext = createContext<SessionContextValue | null>(null);

export function SessionProvider({ children }: { children: React.ReactNode }) {
  const searchParams = useSearchParams();
  const router = useRouter();
  const chatIdParam = searchParams.get("id");
  const taskIdParam = searchParams.get("task");
  const agentParam = searchParams.get("agent");
  const [activeTask, setActiveTask] = useState<TaskResponse | null>(null);
  const activeChatId = chatIdParam ?? activeTask?.chat_id ?? null;
  const activeTaskId = taskIdParam;
  const pendingAgentId = !activeChatId ? agentParam : null;
  const [activeChat, setActiveChat] = useState<ChatResponse | null>(null);
  const [messages, setMessages] = useState<MessageResponse[]>([]);
  const [inferring, setInferring] = useState(false);
  const { updateChatTitle, updateAgent, addStandaloneChat, updateTaskInList, setActiveTab } = useNavigation();
  const { addNotification } = useNotifications();

  // React state — always reflects the *active* chat's stream state.
  const [sending, setSending] = useState(false);
  const [streamingContent, setStreamingContent] = useState<string | null>(null);
  const [activeToolCalls, setActiveToolCalls] = useState<ToolCallStatus[]>([]);

  // Connect module-level sync to React state setters
  useEffect(() => {
    sessionStore.syncToReact = (_chatId: string, s: ChatStreamState) => {
      setSending(s.sending);
      setStreamingContent(s.streamingContent || null);
      setActiveToolCalls(s.toolCalls);
    };
    return () => { sessionStore.syncToReact = null; };
  }, []);

  // Render-time: project stream state and reset when active chat changes
  const [prevActiveChatId, setPrevActiveChatId] = useState(activeChatId);
  if (activeChatId !== prevActiveChatId) {
    setPrevActiveChatId(activeChatId);
    if (activeChatId) {
      const s = getStream(activeChatId);
      setSending(s.sending);
      setStreamingContent(s.streamingContent || null);
      setActiveToolCalls(s.toolCalls);
    } else {
      setSending(false);
      setStreamingContent(null);
      setActiveToolCalls([]);
      setActiveChat(null);
      setMessages([]);
    }
  }

  // Effect: sync module-level activeChatId for SSE callbacks
  useEffect(() => {
    sessionStore.activeChatId = activeChatId;
  }, [activeChatId]);

  // Render-time: reset task when param clears
  const [prevTaskId, setPrevTaskId] = useState(taskIdParam);
  if (taskIdParam !== prevTaskId) {
    setPrevTaskId(taskIdParam);
    if (!taskIdParam) {
      setActiveTask(null);
    }
  }

  // Effect: sync module-level activeTaskId for SSE callbacks
  useEffect(() => {
    sessionStore.activeTaskId = taskIdParam;
  }, [taskIdParam]);

  useEffect(() => {
    if (taskIdParam) {
      setActiveTab("tasks");
    }
  }, [taskIdParam, setActiveTab]);

  // Fetch task data when taskIdParam is set
  useEffect(() => {
    if (!taskIdParam) return;
    let cancelled = false;
    getTask(taskIdParam)
      .then((t) => { if (!cancelled) setActiveTask(t); })
      .catch(() => { if (!cancelled) setActiveTask(null); });
    return () => { cancelled = true; };
  }, [taskIdParam]);

  const setPendingMessage = useCallback((message: string) => {
    sessionStore.pendingMessage = message;
  }, []);

  const scheduleFade = useCallback((chatId: string, tcId: number) => {
    setTimeout(() => {
      updateStream(chatId, (s) => {
        const idx = s.toolCalls.findIndex((tc) => tc.id === tcId);
        if (idx === -1 || s.toolCalls[idx].status === "fading") return;
        s.toolCalls = [...s.toolCalls];
        s.toolCalls[idx] = { ...s.toolCalls[idx], status: "fading" as const };
      });
      setTimeout(() => {
        updateStream(chatId, (s) => {
          s.toolCalls = s.toolCalls.filter((tc) => tc.id !== tcId);
        });
      }, 300);
    }, 3000);
  }, []);

  // Unified SSE stream — handles all event types
  useEffect(() => {
    const controller = new AbortController();

    const callbacks: StreamSessionCallbacks = {
      onToken: (chatId, content) => {
        updateStream(chatId, (s) => {
          if (!s.sending) s.sending = true;
          s.streamingContent += content;
        });
      },
      onToolCall: (chatId, name, _args, description) => {
        if (name === "ask_user_question" || name === "request_user_takeover") return;
        updateStream(chatId, (s) => {
          const context = s.streamingContent.trim() || null;
          s.streamingContent = "";
          const hasDescription = description && description !== name;
          const entry: ToolCallStatus = {
            id: s.nextToolId++,
            name,
            description: hasDescription ? description : context,
            status: "running",
          };
          s.toolCalls = [entry, ...s.toolCalls];
        });
      },
      onToolResult: (chatId, name, success) => {
        updateStream(chatId, (s) => {
          const idx = s.toolCalls.findIndex(
            (tc) => tc.name === name && tc.status === "running",
          );
          if (idx !== -1) {
            s.toolCalls = [...s.toolCalls];
            s.toolCalls[idx] = { ...s.toolCalls[idx], status: success ? "success" : "error" };
            scheduleFade(chatId, s.toolCalls[idx].id);
          }
        });
      },
      onEntityUpdated: (_chatId, table, recordId, fields) => {
        if (table === "agent") {
          updateAgent(recordId, fields);
        }
      },
      onRetry: (chatId, retryAfterSecs, reason) => {
        const labels: Record<string, string> = {
          rate_limited: "Rate limited",
          server_error: "Server error",
          network_error: "Network error",
          empty_response: "Empty response",
          timeout: "Timeout",
          overloaded: "Server overloaded",
        };
        const label = labels[reason] ?? reason;
        updateStream(chatId, (s) => {
          const entry: ToolCallStatus = {
            id: s.nextToolId++,
            name: label,
            description: `Retrying in ${retryAfterSecs}s...`,
            status: "running",
          };
          s.toolCalls = [entry];
        });
      },
      onInferenceDone: (chatId, message) => {
        resetStream(chatId);
        if (chatId === sessionStore.activeChatId) {
          setMessages((prev) => [...prev, message]);
        }
      },
      onInferenceCancelled: (chatId) => {
        resetStream(chatId);
        if (chatId === sessionStore.activeChatId) {
          api.get<MessageResponse[]>(
            `/api/chats/${chatId}/messages`,
          ).then((msgs) => setMessages(msgs)).catch(() => {});
        }
      },
      onInferenceError: (chatId) => {
        resetStream(chatId);
      },
      onToolMessage: (chatId, message) => {
        resetStream(chatId);
        if (chatId === sessionStore.activeChatId) {
          setMessages((prev) => [...prev, message]);
        }
      },
      onToolResolved: (chatId, message) => {
        if (chatId !== sessionStore.activeChatId) return;
        setMessages((prev) =>
          prev.map((m) => (m.id === message.id ? message : m)),
        );
      },
      onTitle: (chatId, title) => {
        if (chatId === sessionStore.activeChatId) {
          setActiveChat((prev) => (prev ? { ...prev, title } : prev));
        }
        updateChatTitle(chatId, title);
      },
      onChatMessage: (chatId, message) => {
        if (chatId !== sessionStore.activeChatId) return;
        setMessages((prev) => {
          const idx = prev.findIndex((m) => m.id === message.id);
          if (idx >= 0) {
            const updated = [...prev];
            updated[idx] = message;
            return updated;
          }
          return [...prev, message];
        });
      },
      onTaskUpdate: (taskId, status, sourceChatId, title, chatId, resultSummary) => {
        updateTaskInList(taskId, { status, title, chat_id: chatId, result_summary: resultSummary });
        if (
          sourceChatId &&
          sourceChatId === sessionStore.activeChatId &&
          (status === "completed" || status === "failed")
        ) {
          api
            .get<MessageResponse[]>(`/api/chats/${sourceChatId}/messages`)
            .then((msgs) => setMessages(msgs))
            .catch(() => {});
        }
        if (taskId && sessionStore.activeTaskId && taskId === sessionStore.activeTaskId) {
          getTask(taskId)
            .then((t) => setActiveTask(t))
            .catch(() => {});
        }
      },
      onInferenceCount: (count) => {
        setInferring(count > 0);
      },
      onNotification: (notification) => {
        addNotification(notification);
      },
    };

    streamSession(callbacks, controller.signal);

    return () => {
      controller.abort();
    };
  }, [updateTaskInList, updateChatTitle, updateAgent, scheduleFade, addNotification]);

  // Fetch chat data when activeChatId is set
  useEffect(() => {
    if (!activeChatId) return;

    let cancelled = false;

    async function load() {
      try {
        const [chat, msgs] = await Promise.all([
          api.get<ChatResponse>(`/api/chats/${activeChatId}`),
          api.get<MessageResponse[]>(`/api/chats/${activeChatId}/messages`),
        ]);
        if (!cancelled) {
          setActiveChat(chat);
          setMessages(msgs);
        }
      } catch {
        if (!cancelled) {
          setActiveChat(null);
          setMessages([]);
        }
      }
    }

    load();
    return () => {
      cancelled = true;
    };
  }, [activeChatId]);

  const resolveToolMessage = useCallback(
    async (messageId: string, response?: string) => {
      if (!activeChatId) return;
      const updated = await api.post<MessageResponse>(
        `/api/chats/${activeChatId}/messages/${messageId}/resolve`,
        { response: response ?? null },
      );
      setMessages((prev) =>
        prev.map((m) => (m.id === messageId ? updated : m)),
      );
    },
    [activeChatId],
  );

  const sendMessage = useCallback(
    async (content: string, attachments?: Attachment[]) => {
      if (!activeChatId && pendingAgentId) {
        const chat = await api.post<ChatResponse>("/api/chats", { agent_id: pendingAgentId });
        addStandaloneChat(chat);
        sessionStore.pendingMessage = content;
        router.push(`/chat?id=${chat.id}`);
        return;
      }
      if (!activeChatId) return;
      updateStream(activeChatId, (s) => {
        s.sending = true;
        s.streamingContent = "";
        s.toolCalls = [];
      });

      const body = attachments?.length
        ? { content, attachments }
        : { content };

      try {
        const userMsg = await apiSendMessage(activeChatId, body);
        setMessages((prev) => [...prev, userMsg]);
      } catch {
        resetStream(activeChatId);
      }
    },
    [activeChatId, pendingAgentId, router, addStandaloneChat],
  );

  // Send pending message after navigation sets activeChatId (deferred to avoid sync setState in effect)
  useEffect(() => {
    if (!activeChatId || !sessionStore.pendingMessage) return;
    const content = sessionStore.pendingMessage;
    sessionStore.pendingMessage = null;
    const timer = setTimeout(() => sendMessage(content), 0);
    return () => clearTimeout(timer);
  }, [activeChatId, sendMessage]);

  const stopGeneration = useCallback(() => {
    if (!activeChatId) return;
    cancelGeneration(activeChatId).catch(() => {});
    resetStream(activeChatId);
    api.get<MessageResponse[]>(
      `/api/chats/${activeChatId}/messages`,
    ).then((msgs) => setMessages(msgs)).catch(() => {});
  }, [activeChatId]);

  const createChat = useCallback(async (req: CreateChatRequest) => {
    return await api.post<ChatResponse>("/api/chats", req);
  }, []);

  return createElement(
    SessionContext.Provider,
    {
      value: {
        activeChatId,
        activeChat,
        activeTaskId,
        activeTask,
        pendingAgentId,
        messages,
        sending,
        inferring,
        streamingContent,
        activeToolCalls,
        sendMessage,
        stopGeneration,
        createChat,
        setPendingMessage,
        resolveToolMessage,
      },
    },
    children,
  );
}

export function useSession(): SessionContextValue {
  const ctx = useContext(SessionContext);
  if (!ctx) throw new Error("useSession must be used within SessionProvider");
  return ctx;
}
