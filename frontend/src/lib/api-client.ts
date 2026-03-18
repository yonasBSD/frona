export const API_URL = process.env.FRONA_SERVER_BACKEND_URL || "http://localhost:3001";

class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

let accessToken: string | null = null;
let refreshPromise: Promise<string | null> | null = null;

export function setAccessToken(token: string | null) {
  accessToken = token;
}

export function getAccessToken(): string | null {
  return accessToken;
}

async function refreshAccessToken(): Promise<string | null> {
  try {
    const res = await fetch(`${API_URL}/api/auth/refresh`, {
      method: "POST",
      credentials: "include",
    });
    if (!res.ok) return null;
    const data = await res.json();
    accessToken = data.token;
    return accessToken;
  } catch {
    return null;
  }
}

async function ensureAccessToken(): Promise<string | null> {
  if (accessToken) return accessToken;
  if (refreshPromise) return refreshPromise;
  refreshPromise = refreshAccessToken().finally(() => {
    refreshPromise = null;
  });
  return refreshPromise;
}

async function request<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...((options.headers as Record<string, string>) || {}),
  };

  const token = await ensureAccessToken();
  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }

  let res = await fetch(`${API_URL}${path}`, {
    ...options,
    headers,
    credentials: "include",
  });

  // On 401, try refreshing the access token and retry once
  if (res.status === 401 && token) {
    const newToken = await refreshAccessToken();
    if (newToken) {
      headers["Authorization"] = `Bearer ${newToken}`;
      res = await fetch(`${API_URL}${path}`, {
        ...options,
        headers,
        credentials: "include",
      });
    }
  }

  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }));
    throw new ApiError(res.status, body.error || "Request failed");
  }

  if (res.status === 204 || res.headers.get("content-length") === "0") {
    return undefined as T;
  }

  const text = await res.text();
  if (!text) {
    return undefined as T;
  }

  return JSON.parse(text);
}

import type { MessageResponse, Attachment, FileEntry, Notification } from "./types";

export async function uploadFile(file: File, relativePath?: string): Promise<Attachment> {
  const formData = new FormData();
  formData.append("file", file);
  if (relativePath) {
    formData.append("path", relativePath);
  }

  const token = await ensureAccessToken();
  const headers: Record<string, string> = {};
  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }

  const res = await fetch(`${API_URL}/api/files`, {
    method: "POST",
    body: formData,
    headers,
  });

  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }));
    throw new ApiError(res.status, body.error || "Upload failed");
  }

  return res.json();
}

function parseFileEntries(raw: Array<{ id: string; size: number; date: string; type: string; parent: string }>): FileEntry[] {
  return raw.map((e) => ({
    id: e.id,
    size: e.size,
    date: new Date(e.date),
    type: e.type as "folder" | "file",
    parent: e.parent,
  }));
}

export async function listUserFiles(path?: string): Promise<FileEntry[]> {
  const url = path ? `/api/files/browse/user/${path}` : "/api/files/browse/user";
  const raw = await request<Array<{ id: string; size: number; date: string; type: string; parent: string }>>(url);
  return parseFileEntries(raw);
}

export async function listAgentFiles(agentId: string, path?: string): Promise<FileEntry[]> {
  const url = path
    ? `/api/files/browse/agent/${agentId}/${path}`
    : `/api/files/browse/agent/${agentId}`;
  const raw = await request<Array<{ id: string; size: number; date: string; type: string; parent: string }>>(url);
  return parseFileEntries(raw);
}

export async function renameFile(path: string, newName: string): Promise<void> {
  await request("/api/files/rename", {
    method: "POST",
    body: JSON.stringify({ path, new_name: newName }),
  });
}

export async function copyFiles(sources: string[], destination: string): Promise<void> {
  await request("/api/files/copy", {
    method: "POST",
    body: JSON.stringify({ sources, destination }),
  });
}

export async function moveFiles(sources: string[], destination: string): Promise<void> {
  await request("/api/files/move", {
    method: "POST",
    body: JSON.stringify({ sources, destination }),
  });
}

export async function createFolder(path: string): Promise<void> {
  await request("/api/files/mkdir", {
    method: "POST",
    body: JSON.stringify({ path }),
  });
}

export interface SearchResult {
  id: string;
  size: number;
  date: Date;
  type: "folder" | "file";
  source: string;
  path: string;
}

export async function searchFiles(query: string, scope?: string): Promise<SearchResult[]> {
  let url = `/api/files/search?q=${encodeURIComponent(query)}`;
  if (scope) {
    url += `&scope=${encodeURIComponent(scope)}`;
  }
  const raw = await request<Array<{ id: string; size: number; date: string; type: string; parent: string }>>(
    url,
  );
  return raw.map((e) => {
    const [source, ...rest] = e.id.split(":");
    const path = rest.join(":");
    return {
      id: e.id,
      size: e.size,
      date: new Date(e.date),
      type: e.type as "folder" | "file",
      source,
      path,
    };
  });
}

export async function deleteFile(username: string, path: string): Promise<void> {
  await request(`/api/files/user/${username}/${path}`, { method: "DELETE" });
}

export async function presignFile(owner: string, path: string): Promise<string> {
  const data = await request<{ url: string }>("/api/files/presign", {
    method: "POST",
    body: JSON.stringify({ owner, path }),
  });
  return data.url;
}

export function fileDownloadUrl(attachment: Attachment, username: string): string {
  if (attachment.owner.startsWith("user:")) {
    return `${API_URL}/api/files/user/${username}/${attachment.path}`;
  } else if (attachment.owner.startsWith("agent:")) {
    const agentId = attachment.owner.replace("agent:", "");
    return `${API_URL}/api/files/agent/${agentId}/${attachment.path}`;
  }
  return "";
}

export async function sendMessage(
  chatId: string,
  body: { content: string; attachments?: Attachment[] },
): Promise<MessageResponse> {
  return request<MessageResponse>(`/api/chats/${chatId}/messages/stream`, {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export interface StreamSessionCallbacks {
  onToken?: (chatId: string, content: string) => void;
  onToolCall?: (chatId: string, name: string, args: unknown, description?: string) => void;
  onToolResult?: (chatId: string, name: string, success: boolean) => void;
  onEntityUpdated?: (chatId: string, table: string, recordId: string, fields: Record<string, unknown>) => void;
  onRetry?: (chatId: string, retryAfterSecs: number, reason: string) => void;
  onInferenceDone?: (chatId: string, message: MessageResponse) => void;
  onInferenceCancelled?: (chatId: string, reason: string) => void;
  onInferenceError?: (chatId: string, error: string) => void;
  onToolMessage?: (chatId: string, message: MessageResponse) => void;
  onToolResolved?: (chatId: string, message: MessageResponse) => void;
  onTitle?: (chatId: string, title: string) => void;
  onChatMessage?: (chatId: string, message: MessageResponse) => void;
  onTaskUpdate?: (taskId: string, status: string, sourceChatId: string | null, title: string, chatId: string | null, resultSummary: string | null) => void;
  onInferenceCount?: (count: number) => void;
  onNotification?: (notification: Notification) => void;
}

async function connectStream(
  callbacks: StreamSessionCallbacks,
  signal: AbortSignal,
): Promise<void> {
  const token = await ensureAccessToken();
  const headers: Record<string, string> = {};
  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }

  let res: Response;
  try {
    res = await fetch(`${API_URL}/api/stream`, { headers, signal });
  } catch (err) {
    if (err instanceof DOMException && err.name === "AbortError") return;
    throw err;
  }

  if (!res.ok) {
    throw new Error(`Stream connection failed: ${res.status}`);
  }

  const reader = res.body?.getReader();
  if (!reader) return;

  const decoder = new TextDecoder();
  let buffer = "";
  let currentEvent = "";

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split("\n");
      buffer = lines.pop() ?? "";

      for (const line of lines) {
        if (line.startsWith("event: ")) {
          currentEvent = line.slice(7).trim();
        } else if (line.startsWith("data: ")) {
          try {
            const parsed = JSON.parse(line.slice(6));
            const chatId = (parsed.chat_id as string) ?? "";
            switch (currentEvent) {
              case "token":
                callbacks.onToken?.(chatId, parsed.content as string);
                break;
              case "tool_call":
                callbacks.onToolCall?.(chatId, parsed.name as string, parsed.arguments, parsed.description as string | undefined);
                break;
              case "tool_result":
                callbacks.onToolResult?.(chatId, parsed.name as string, parsed.success as boolean);
                break;
              case "entity_updated":
                callbacks.onEntityUpdated?.(chatId, parsed.table as string, parsed.record_id as string, parsed.fields as Record<string, unknown>);
                break;
              case "retry":
                callbacks.onRetry?.(chatId, parsed.retry_after_secs as number, parsed.reason as string);
                break;
              case "inference_done":
                callbacks.onInferenceDone?.(chatId, parsed.message as MessageResponse);
                break;
              case "inference_cancelled":
                callbacks.onInferenceCancelled?.(chatId, parsed.reason as string);
                break;
              case "inference_error":
                callbacks.onInferenceError?.(chatId, parsed.error as string);
                break;
              case "tool_message":
                callbacks.onToolMessage?.(chatId, parsed.message as MessageResponse);
                break;
              case "tool_resolved":
                callbacks.onToolResolved?.(chatId, parsed.message as MessageResponse);
                break;
              case "title":
                callbacks.onTitle?.(chatId, parsed.title as string);
                break;
              case "chat_message":
                callbacks.onChatMessage?.(chatId, parsed.message as MessageResponse);
                break;
              case "task_update":
                callbacks.onTaskUpdate?.(
                  parsed.task_id as string,
                  parsed.status as string,
                  parsed.source_chat_id as string | null,
                  parsed.title as string,
                  parsed.chat_id as string | null,
                  parsed.result_summary as string | null,
                );
                break;
              case "inference_count":
                callbacks.onInferenceCount?.(parsed.count as number);
                break;
              case "notification":
                callbacks.onNotification?.(parsed.notification as Notification);
                break;
            }
          } catch {
            // skip malformed JSON
          }
          currentEvent = "";
        }
      }
    }
  } catch (err) {
    if (err instanceof DOMException && err.name === "AbortError") return;
    throw err;
  }
}

export async function streamSession(
  callbacks: StreamSessionCallbacks,
  signal?: AbortSignal,
): Promise<void> {
  const ctrl = new AbortController();
  if (signal) {
    signal.addEventListener("abort", () => ctrl.abort());
  }

  let delay = 1000;
  const maxDelay = 30000;

  while (!ctrl.signal.aborted) {
    try {
      await connectStream(callbacks, ctrl.signal);
      delay = 1000;
    } catch {
      if (ctrl.signal.aborted) return;
    }
    if (ctrl.signal.aborted) return;
    await new Promise((r) => setTimeout(r, delay));
    delay = Math.min(delay * 2, maxDelay);
  }
}

export async function cancelGeneration(chatId: string): Promise<void> {
  const token = await ensureAccessToken();
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }
  await fetch(`${API_URL}/api/chats/${chatId}/cancel`, {
    method: "POST",
    headers,
  });
}

export async function cancelTask(taskId: string): Promise<void> {
  await request(`/api/tasks/${taskId}/cancel`, { method: "POST" });
}

export function deleteTask(taskId: string) {
  return request<void>(`/api/tasks/${taskId}`, { method: "DELETE" });
}

export function getTask(id: string) {
  return api.get<import("./types").TaskResponse>(`/api/tasks/${id}`);
}

export function archiveChat(chatId: string) {
  return request<import("./types").ChatResponse>(`/api/chats/${chatId}/archive`, { method: "POST" });
}

export function unarchiveChat(chatId: string) {
  return request<import("./types").ChatResponse>(`/api/chats/${chatId}/unarchive`, { method: "POST" });
}

export function deleteChat(chatId: string) {
  return request<void>(`/api/chats/${chatId}`, { method: "DELETE" });
}

export function getArchivedChats() {
  return request<import("./types").ChatResponse[]>("/api/chats/archived");
}

export function getContacts() {
  return request<import("./types").Contact[]>("/api/contacts");
}

export const api = {
  get: <T>(path: string) => request<T>(path),
  post: <T>(path: string, body: unknown) =>
    request<T>(path, { method: "POST", body: JSON.stringify(body) }),
  put: <T>(path: string, body: unknown) =>
    request<T>(path, { method: "PUT", body: JSON.stringify(body) }),
  delete: <T>(path: string) => request<T>(path, { method: "DELETE" }),
};
