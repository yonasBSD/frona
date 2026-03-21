"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import { useRouter } from "next/navigation";
import { EllipsisVerticalIcon } from "@heroicons/react/24/outline";
import { AssistantRuntimeProvider } from "@assistant-ui/react";
import { api, API_URL } from "@/lib/api-client";
import { useFronaRuntime } from "@/lib/assistant-runtime";
import { useNavigation } from "@/lib/navigation-context";
import { FronaComposer } from "@/components/chat/frona-composer";
import type { AppResponse, ChatResponse } from "@/lib/types";

const statusColors: Record<string, string> = {
  running: "bg-success",
  serving: "bg-success",
  starting: "bg-warning",
  hibernated: "bg-warning",
  stopped: "bg-text-tertiary",
  failed: "bg-danger",
};

function AppCard({
  app,
  onUpdate,
  onDelete,
}: {
  app: AppResponse;
  onUpdate: (app: AppResponse) => void;
  onDelete: (id: string) => void;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [acting, setActing] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    const handleClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [menuOpen]);

  const handleStop = async () => {
    setActing(true);
    try {
      const updated = await api.post<AppResponse>(`/api/apps/${app.id}/stop`, {});
      onUpdate(updated);
    } catch {}
    setActing(false);
    setMenuOpen(false);
  };

  const handleDelete = async () => {
    setActing(true);
    try {
      await api.delete(`/api/apps/${app.id}`);
      onDelete(app.id);
    } catch {}
    setActing(false);
    setMenuOpen(false);
  };

  const icon = app.manifest?.icon as string | undefined;
  const href = app.url ? `${API_URL}${app.url}` : undefined;

  return (
    <div className="relative flex items-center gap-3 rounded-lg border border-border px-4 py-3 transition hover:border-accent">
      {icon ? (
        <span className="text-lg shrink-0">{icon}</span>
      ) : (
        <span
          className={`h-2 w-2 shrink-0 rounded-full ${statusColors[app.status] || "bg-text-tertiary"}`}
        />
      )}
      <a
        href={href}
        target="_blank"
        rel="noopener noreferrer"
        className={`min-w-0 flex-1 ${href ? "cursor-pointer" : "cursor-default opacity-60"}`}
      >
        <p className="text-sm font-medium text-text-primary truncate">
          {app.name}
        </p>
        {app.description && (
          <p className="text-xs text-text-tertiary truncate">
            {app.description}
          </p>
        )}
      </a>
      <span className="text-[10px] text-text-tertiary capitalize shrink-0">
        {app.status}
      </span>
      <div ref={menuRef} className="relative shrink-0">
        <button
          onClick={() => setMenuOpen((o) => !o)}
          className="rounded p-1 text-text-tertiary hover:text-text-primary hover:bg-surface-tertiary transition"
        >
          <EllipsisVerticalIcon className="h-4 w-4" />
        </button>
        {menuOpen && (
          <div className="absolute right-0 top-full z-10 mt-1 w-32 rounded-lg border border-border bg-surface-secondary py-1 shadow-lg">
            {(app.status === "running" || app.status === "serving") && (
              <button
                onClick={handleStop}
                disabled={acting}
                className="w-full px-3 py-1.5 text-left text-xs text-text-secondary hover:bg-surface-tertiary transition"
              >
                Stop
              </button>
            )}
            <button
              onClick={handleDelete}
              disabled={acting}
              className="w-full px-3 py-1.5 text-left text-xs text-danger hover:bg-surface-tertiary transition"
            >
              Delete
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function HomeComposer() {
  const router = useRouter();
  const { addStandaloneChat } = useNavigation();

  const onChatCreated = useCallback((chat: ChatResponse) => {
    addStandaloneChat(chat);
    router.push(`/chat?id=${chat.id}`);
  }, [addStandaloneChat, router]);

  const { runtime } = useFronaRuntime({ agentId: "system", onChatCreated });

  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <FronaComposer placeholder="What's on your mind?" />
    </AssistantRuntimeProvider>
  );
}

export default function HomePage() {
  const [apps, setApps] = useState<AppResponse[]>([]);

  useEffect(() => {
    api.get<AppResponse[]>("/api/apps").then(setApps).catch(() => {});
  }, []);

  return (
    <div className="flex h-full items-start justify-center overflow-y-auto">
      <div className="w-full max-w-3xl px-8 py-12 space-y-8">
        <HomeComposer />
        {apps.length > 0 && (
          <div>
            <p className="text-[11px] font-semibold uppercase tracking-wider text-text-tertiary pb-2">
              Services
            </p>
            <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
              {apps.map((app) => (
                <AppCard
                  key={app.id}
                  app={app}
                  onUpdate={(updated) =>
                    setApps((prev) =>
                      prev.map((a) => (a.id === updated.id ? updated : a)),
                    )
                  }
                  onDelete={(id) =>
                    setApps((prev) => prev.filter((a) => a.id !== id))
                  }
                />
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
