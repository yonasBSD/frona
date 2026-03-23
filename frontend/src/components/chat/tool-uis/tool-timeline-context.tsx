"use client";

import {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  useMemo,
  useRef,
} from "react";
import { useMessage } from "@assistant-ui/react";
import { useThreadIsRunning } from "@assistant-ui/core/react";
import { ChevronRightIcon, ChevronUpIcon, WrenchScrewdriverIcon } from "@heroicons/react/24/outline";
import { AnimatePresence, motion } from "motion/react";

const MAX_VISIBLE = 6;
const COLLAPSE_DELAY_MS = 10_000;

/** Tool calls with custom UIs that render outside the timeline. */
const EXCLUDED_TOOLS = new Set([
  "Question",
  "HumanInTheLoop",
  "VaultApproval",
  "ServiceApproval",
  "TaskCompletion",
]);

interface ToolTimelineContextValue {
  isVisible: (toolCallId: string) => boolean;
  isLastVisible: (toolCallId: string) => boolean;
  isFirstVisible: (toolCallId: string) => boolean;
  getToolIndex: (toolCallId: string) => number;
  hiddenCount: number;
}

const Ctx = createContext<ToolTimelineContextValue | null>(null);

export function useToolTimeline() {
  return useContext(Ctx);
}

export function ToolTimelineProvider({
  children,
}: {
  children: React.ReactNode;
}) {
  const message = useMessage();
  const isRunning = useThreadIsRunning();
  const wasRunning = useRef(false);

  const toolCallIds = useMemo(() => {
    if (message.role !== "assistant") return [];
    return (
      message.content as unknown as ReadonlyArray<{
        type: string;
        toolCallId?: string;
        toolName?: string;
      }>
    )
      .filter((p) => p.type === "tool-call" && p.toolCallId && !EXCLUDED_TOOLS.has(p.toolName ?? ""))
      .map((p) => p.toolCallId!);
  }, [message.role, message.content]);

  const totalTools = toolCallIds.length;

  // Messages loaded from history start collapsed; live messages start expanded
  const [collapsed, setCollapsed] = useState(
    () => !message.isLast || !isRunning,
  );
  const [userToggled, setUserToggled] = useState(false);

  // Window tools to last MAX_VISIBLE unless user explicitly expanded
  const visibleSet = useMemo(() => {
    if (userToggled) return null; // user toggled — show all
    if (toolCallIds.length <= MAX_VISIBLE) return null; // fits — show all
    const set = new Set<string>();
    const start = Math.max(0, toolCallIds.length - MAX_VISIBLE);
    for (let i = start; i < toolCallIds.length; i++) {
      set.add(toolCallIds[i]);
    }
    return set;
  }, [toolCallIds, userToggled]);

  // Expand while inference is running, collapse after it finishes
  useEffect(() => {
    if (isRunning && message.isLast) {
      wasRunning.current = true;
      if (!userToggled) {
        const timer = setTimeout(() => setCollapsed(false), 0);
        return () => clearTimeout(timer);
      }
      return;
    }
    if (wasRunning.current && totalTools > 0 && !userToggled) {
      wasRunning.current = false;
      const timer = setTimeout(() => setCollapsed(true), COLLAPSE_DELAY_MS);
      return () => clearTimeout(timer);
    }
  }, [isRunning, message.isLast, totalTools, userToggled]);

  const isVisible = useCallback(
    (toolCallId: string) => {
      if (collapsed) return false;
      if (visibleSet) return visibleSet.has(toolCallId);
      return true;
    },
    [collapsed, visibleSet],
  );

  const isLastVisible = useCallback(
    (toolCallId: string) => {
      return toolCallId === toolCallIds[toolCallIds.length - 1];
    },
    [toolCallIds],
  );

  const firstVisibleId = useMemo(() => {
    if (!visibleSet) return toolCallIds[0] ?? null;
    for (const id of toolCallIds) {
      if (visibleSet.has(id)) return id;
    }
    return null;
  }, [toolCallIds, visibleSet]);

  const isFirstVisible = useCallback(
    (toolCallId: string) => toolCallId === firstVisibleId,
    [firstVisibleId],
  );

  const toolIndexMap = useMemo(() => {
    const map = new Map<string, number>();
    toolCallIds.forEach((id, i) => map.set(id, i + 1));
    return map;
  }, [toolCallIds]);

  const getToolIndex = useCallback(
    (toolCallId: string) => toolIndexMap.get(toolCallId) ?? 0,
    [toolIndexMap],
  );

  const hiddenCount = visibleSet ? totalTools - visibleSet.size : 0;

  const handleToggle = useCallback(() => {
    setCollapsed((prev) => !prev);
    setUserToggled(true);
  }, []);

  const value = useMemo(
    () => ({ isVisible, isLastVisible, isFirstVisible, getToolIndex, hiddenCount }),
    [isVisible, isLastVisible, isFirstVisible, getToolIndex, hiddenCount],
  );

  if (totalTools === 0) return <>{children}</>;

  return (
    <Ctx.Provider value={value}>
      {children}
      <AnimatePresence mode="wait">
        {collapsed && (
          <motion.button
            key="expand"
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.2 }}
            onClick={handleToggle}
            className="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-xs text-text-tertiary hover:text-text-secondary hover:bg-surface-secondary transition-colors mt-1"
          >
            <WrenchScrewdriverIcon className="h-3 w-3" />
            <span>
              Used {totalTools} tool{totalTools !== 1 ? "s" : ""}
            </span>
            <ChevronRightIcon className="h-3 w-3" />
          </motion.button>
        )}
        {!collapsed && !isRunning && totalTools > 0 && (
          <motion.button
            key="collapse"
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.2 }}
            onClick={handleToggle}
            className="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-xs text-text-tertiary hover:text-text-secondary hover:bg-surface-secondary transition-colors mt-1"
          >
            <WrenchScrewdriverIcon className="h-3 w-3" />
            <span>
              Hide {totalTools} tool{totalTools !== 1 ? "s" : ""}
            </span>
            <ChevronUpIcon className="h-3 w-3" />
          </motion.button>
        )}
      </AnimatePresence>
    </Ctx.Provider>
  );
}
