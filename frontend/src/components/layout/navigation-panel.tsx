"use client";

import { useState, useCallback, useRef, useEffect } from "react";
import { ChevronLeftIcon, ChevronRightIcon } from "@heroicons/react/24/outline";
import { useNavigation } from "@/lib/navigation-context";
import { TabBar } from "./tab-bar";
import { ChatsTab } from "../nav/chats-tab";
import { TasksTab } from "../nav/tasks-tab";

const MIN_WIDTH = 200;
const MAX_WIDTH = 480;
const DEFAULT_WIDTH = 289;
const COOKIE_NAME = "nav_collapsed";

function getCookie(name: string): string | null {
  const match = document.cookie.match(new RegExp(`(?:^|; )${name}=([^;]*)`));
  return match ? decodeURIComponent(match[1]) : null;
}

function setCookie(name: string, value: string, days = 365) {
  const expires = new Date(Date.now() + days * 864e5).toUTCString();
  document.cookie = `${name}=${encodeURIComponent(value)}; expires=${expires}; path=/`;
}

export function NavigationPanel() {
  const { activeTab } = useNavigation();
  const [collapsed, setCollapsed] = useState(() => getCookie(COOKIE_NAME) === "1");
  const [width, setWidth] = useState(DEFAULT_WIDTH);
  const dragging = useRef(false);
  const panelRef = useRef<HTMLDivElement>(null);

  const toggleCollapsed = useCallback(() => {
    setCollapsed((prev) => {
      const next = !prev;
      setCookie(COOKIE_NAME, next ? "1" : "0");
      return next;
    });
  }, []);

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    dragging.current = true;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  }, []);

  useEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (!dragging.current) return;
      const newWidth = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, e.clientX));
      setWidth(newWidth);
    };

    const onMouseUp = () => {
      if (!dragging.current) return;
      dragging.current = false;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };

    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", onMouseUp);
    return () => {
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
    };
  }, []);

  if (collapsed) {
    return (
      <div
        onClick={toggleCollapsed}
        className="relative flex h-full flex-col items-center border-r border-border bg-surface-nav w-10 shrink-0 cursor-pointer hover:bg-surface-tertiary/50 transition"
        title="Expand panel"
      >
        <ChevronRightIcon className="h-4 w-4 mt-3 text-text-tertiary" />
      </div>
    );
  }

  return (
    <div
      ref={panelRef}
      className="relative flex h-full flex-col border-r border-border bg-surface-nav shrink-0"
      style={{ width }}
    >
      <div className="flex items-center border-b border-border">
        <div className="flex-1"><TabBar /></div>
        <button
          onClick={toggleCollapsed}
          className="mr-2 p-1 rounded text-text-tertiary hover:text-text-primary hover:bg-surface-tertiary transition"
          title="Collapse panel"
        >
          <ChevronLeftIcon className="h-4 w-4" />
        </button>
      </div>

      <div className="flex-1 overflow-y-auto">
        {activeTab === "chat" && <ChatsTab />}
        {activeTab === "tasks" && <TasksTab />}
      </div>

      {/* Resize handle */}
      <div
        onMouseDown={onMouseDown}
        className="absolute top-0 right-0 bottom-0 w-1 cursor-col-resize hover:bg-accent/20 active:bg-accent/30 transition-colors"
      />
    </div>
  );
}
