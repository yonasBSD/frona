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

export async function ensureAccessToken(): Promise<string | null> {
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

import type { MessageResponse, Attachment, FileEntry } from "./types";

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

export function deleteAgent(agentId: string) {
  return request<void>(`/api/agents/${agentId}`, { method: "DELETE" });
}

export function getArchivedChats() {
  return request<import("./types").ChatResponse[]>("/api/chats/archived");
}

export function getContacts() {
  return request<import("./types").Contact[]>("/api/contacts");
}

// Skill types
export type SkillScope = "builtin" | "shared" | "agent";

export interface SkillSearchResult {
  name: string;
  repo: string;
  avatar_url: string;
  installs: number;
  installed: boolean;
}

export interface SkillPreview {
  name: string;
  description: string;
  body: string;
  metadata: Record<string, string>;
  repo: string;
  avatar_url: string;
  github_url: string;
  raw_base_url: string;
}

export interface SkillListItem {
  name: string;
  description: string;
  source: string | null;
  installed_at: string | null;
  scope: SkillScope;
}

export async function searchSkills(query: string): Promise<SkillSearchResult[]> {
  return request<SkillSearchResult[]>(`/api/skills/search?q=${encodeURIComponent(query)}`);
}

export async function previewSkill(repo: string, name: string): Promise<SkillPreview> {
  return request<SkillPreview>(`/api/skills/preview?repo=${encodeURIComponent(repo)}&name=${encodeURIComponent(name)}`);
}

export async function installSkills(repo: string, skillNames: string[], agentId?: string): Promise<SkillListItem[]> {
  return request<SkillListItem[]>("/api/skills/install", {
    method: "POST",
    body: JSON.stringify({ repo, skill_names: skillNames, agent_id: agentId }),
  });
}

export async function uninstallSkill(name: string, agentId?: string): Promise<void> {
  let url = `/api/skills/${encodeURIComponent(name)}`;
  if (agentId) url += `?agent_id=${encodeURIComponent(agentId)}`;
  return request<void>(url, { method: "DELETE" });
}

export async function listInstalledSkills(): Promise<SkillListItem[]> {
  return request<SkillListItem[]>("/api/skills");
}

export async function listAgentSkills(agentId: string): Promise<SkillListItem[]> {
  return request<SkillListItem[]>(`/api/agents/${agentId}/skills`);
}

export interface RepoBrowseSkill {
  name: string;
  description: string;
  sha: string;
  dir_path: string;
  installed: boolean;
}

export interface RepoBrowseResult {
  repo: string;
  description: string;
  avatar_url: string;
  skills: RepoBrowseSkill[];
}

export async function browseRepo(repo: string): Promise<RepoBrowseResult> {
  return request<RepoBrowseResult>(`/api/skills/browse?repo=${encodeURIComponent(repo)}`);
}

export const api = {
  get: <T>(path: string) => request<T>(path),
  post: <T>(path: string, body: unknown) =>
    request<T>(path, { method: "POST", body: JSON.stringify(body) }),
  put: <T>(path: string, body: unknown) =>
    request<T>(path, { method: "PUT", body: JSON.stringify(body) }),
  delete: <T>(path: string) => request<T>(path, { method: "DELETE" }),
  uploadFile: async <T = { url: string }>(path: string, file: File): Promise<T> => {
    const formData = new FormData();
    formData.append("file", file);
    const token = await ensureAccessToken();
    const headers: Record<string, string> = {};
    if (token) headers["Authorization"] = `Bearer ${token}`;
    const res = await fetch(`${API_URL}${path}`, { method: "PUT", body: formData, headers });
    if (!res.ok) {
      const body = await res.json().catch(() => ({ error: res.statusText }));
      throw new ApiError(res.status, body.error || "Upload failed");
    }
    return res.json();
  },
};
