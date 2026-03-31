"use client";

import { useState, useRef, useEffect, useCallback } from "react";
import {
  Squares2X2Icon,
  TrashIcon,
  StopIcon,
  PlayIcon,
} from "@heroicons/react/24/outline";
import { api, API_URL } from "@/lib/api-client";
import type { AppResponse } from "@/lib/types";

function statusDot(status: AppResponse["status"]): string {
  switch (status) {
    case "running":
    case "serving":
      return "bg-green-400";
    case "starting":
      return "bg-yellow-400";
    case "stopped":
    case "hibernated":
      return "bg-gray-400";
    case "failed":
      return "bg-red-400";
    default:
      return "bg-gray-400";
  }
}

export function AppDropdown() {
  const [open, setOpen] = useState(false);
  const [apps, setApps] = useState<AppResponse[]>([]);
  const ref = useRef<HTMLDivElement>(null);

  const fetchApps = useCallback(async () => {
    try {
      const data = await api.get<AppResponse[]>("/api/apps");
      setApps(data);
    } catch {
      /* ignore */
    }
  }, []);

  useEffect(() => {
    fetchApps();
  }, [fetchApps]);

  useEffect(() => {
    if (open) fetchApps();
  }, [open, fetchApps]);

  useEffect(() => {
    if (!open) return;
    function handleClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  const handleStop = async (e: React.MouseEvent, app: AppResponse) => {
    e.stopPropagation();
    await api.post(`/api/apps/${app.id}/stop`, {});
    fetchApps();
  };

  const handleRestart = async (e: React.MouseEvent, app: AppResponse) => {
    e.stopPropagation();
    await api.post(`/api/apps/${app.id}/restart`, {});
    fetchApps();
  };

  const handleDelete = async (e: React.MouseEvent, app: AppResponse) => {
    e.stopPropagation();
    if (!confirm(`Delete app "${app.name}"?`)) return;
    await api.delete(`/api/apps/${app.id}`);
    fetchApps();
  };

  return (
    <div ref={ref} className="relative flex items-center">
      <button
        onClick={() => setOpen((v) => !v)}
        className={`relative flex items-center justify-center h-10 w-10 transition cursor-pointer ${
          open ? "rounded-t-xl rounded-b-none bg-surface-secondary text-text-primary z-[61] border border-border border-b-0" : "rounded-full bg-surface-tertiary text-text-secondary hover:brightness-125"
        }`}
        title="Apps"
      >
        <Squares2X2Icon className="h-5 w-5" />
      </button>

      {open && (
        <div className="absolute right-0 top-full z-[60] w-64 rounded-xl rounded-tr-none border border-border bg-surface-secondary shadow-lg">
          <div className="absolute -top-px right-0 w-[calc(theme(spacing.10)-3px)] h-[2px] bg-surface-secondary z-[60]" />
          <div className="pb-1">
            <div className="flex items-center justify-between px-4 py-2 border-b border-border shrink-0">
              <span className="text-sm font-medium text-text-secondary">Apps</span>
            </div>
            {apps.map((app) => (
              <div
                key={app.id}
                className="group flex items-center gap-2 px-4 py-2 hover:bg-surface-tertiary transition cursor-pointer"
                onClick={() => {
                  window.open(`${API_URL}/apps/${app.id}/`, "_blank");
                  setOpen(false);
                }}
              >
                <span className={`h-2 w-2 shrink-0 rounded-full ${statusDot(app.status)}`} />
                <span className="flex-1 text-sm text-text-secondary group-hover:text-text-primary">
                  {app.name}
                </span>
                {app.status === "running" ? (
                  <button
                    onClick={(e) => handleStop(e, app)}
                    className="p-1 rounded text-text-tertiary hover:text-text-primary transition opacity-0 group-hover:opacity-100"
                    title="Stop"
                  >
                    <StopIcon className="h-4 w-4" />
                  </button>
                ) : (app.status === "stopped" || app.status === "failed" || app.status === "hibernated") && app.kind !== "static" ? (
                  <button
                    onClick={(e) => handleRestart(e, app)}
                    className="p-1 rounded text-text-tertiary hover:text-text-primary transition opacity-0 group-hover:opacity-100"
                    title="Start"
                  >
                    <PlayIcon className="h-4 w-4" />
                  </button>
                ) : null}
                <button
                  onClick={(e) => handleDelete(e, app)}
                  className="p-1 rounded text-text-tertiary hover:text-error-text transition opacity-0 group-hover:opacity-100"
                  title="Delete"
                >
                  <TrashIcon className="h-4 w-4" />
                </button>
              </div>
            ))}
            {apps.length === 0 && (
              <p className="px-4 py-4 text-center text-xs text-text-tertiary">
                No apps deployed
              </p>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
