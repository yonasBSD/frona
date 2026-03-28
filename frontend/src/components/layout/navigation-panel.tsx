"use client";

import { useState, useCallback, useRef, useEffect } from "react";
import { useNavigation } from "@/lib/navigation-context";
import { TabBar } from "./tab-bar";
import { ChatsTab } from "../nav/chats-tab";
import { TasksTab } from "../nav/tasks-tab";

const MIN_WIDTH = 200;
const MAX_WIDTH = 480;
const DEFAULT_WIDTH = 289;

export function NavigationPanel() {
  const { activeTab } = useNavigation();
  const [width, setWidth] = useState(DEFAULT_WIDTH);
  const dragging = useRef(false);
  const panelRef = useRef<HTMLDivElement>(null);

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

  return (
    <div
      ref={panelRef}
      className="relative flex h-full flex-col border-r border-border bg-surface-nav"
      style={{ width }}
    >
      <TabBar />

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
