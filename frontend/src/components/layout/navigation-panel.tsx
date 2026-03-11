"use client";

import { useState, useCallback, useRef, useEffect } from "react";
import { useRouter } from "next/navigation";
import { PlusIcon } from "@heroicons/react/24/outline";
import { useNavigation } from "@/lib/navigation-context";
import { useSession } from "@/lib/session-context";
import { TabBar } from "./tab-bar";
import { PanelFooter } from "./panel-footer";
import { ChatsTab } from "../nav/chats-tab";
import { TasksTab } from "../nav/tasks-tab";
import { AgentsTab } from "../nav/agents-tab";
import { Logo } from "../logo";

const MIN_WIDTH = 200;
const MAX_WIDTH = 480;
const DEFAULT_WIDTH = 288; // w-72

export function NavigationPanel() {
  const { activeTab, addStandaloneChat } = useNavigation();
  const { createChat, inferring } = useSession();
  const router = useRouter();
  const [width, setWidth] = useState(DEFAULT_WIDTH);
  const dragging = useRef(false);
  const panelRef = useRef<HTMLDivElement>(null);

  const handleNewChat = async () => {
    const chat = await createChat({ agent_id: "system" });
    addStandaloneChat(chat);
    router.push(`/chat?id=${chat.id}`);
  };

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
      <div className="flex items-center px-2 py-1">
        <button
          onClick={() => router.push("/chat")}
          className="flex flex-1 items-center justify-center gap-1"
        >
          <Logo size={52} animate={inferring} />
          <span className="text-2xl font-bold text-text-primary tracking-wide" style={{ fontFamily: "var(--font-brand)" }}>FRONA</span>
        </button>
        <button
          onClick={handleNewChat}
          className="rounded-lg p-1.5 text-text-secondary hover:bg-surface-tertiary hover:text-text-primary transition"
          title="New chat"
        >
          <PlusIcon className="h-5 w-5" />
        </button>
      </div>

      <TabBar />

      <div className="flex-1 overflow-y-auto">
        {activeTab === "chat" && <ChatsTab />}
        {activeTab === "tasks" && <TasksTab />}
        {activeTab === "agents" && <AgentsTab />}
      </div>

      <PanelFooter />

      {/* Resize handle */}
      <div
        onMouseDown={onMouseDown}
        className="absolute top-0 right-0 bottom-0 w-1 cursor-col-resize hover:bg-accent/20 active:bg-accent/30 transition-colors"
      />
    </div>
  );
}
