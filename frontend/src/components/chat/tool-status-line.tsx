"use client";

import { useRef, useState, useEffect } from "react";
import {
  CheckCircleIcon,
  XCircleIcon,
  CommandLineIcon,
  CodeBracketIcon,
  GlobeAltIcon,
  MagnifyingGlassIcon,
  DocumentArrowDownIcon,
  CpuChipIcon,
  LightBulbIcon,
  UserIcon,
  ClockIcon,
  KeyIcon,
  PhoneIcon,
  ServerIcon,
  CalendarIcon,
  ArrowsRightLeftIcon,
  CursorArrowRaysIcon,
  WrenchScrewdriverIcon,
} from "@heroicons/react/16/solid";
import type { ToolCallStatus } from "@/lib/types";

const TOOL_META: Record<string, { icon: React.ComponentType<React.SVGProps<SVGSVGElement>>; label: string }> = {
  shell: { icon: CommandLineIcon, label: "Shell" },
  python: { icon: CodeBracketIcon, label: "Python" },
  web_search: { icon: MagnifyingGlassIcon, label: "Search" },
  web_fetch: { icon: GlobeAltIcon, label: "Fetch" },
  produce_file: { icon: DocumentArrowDownIcon, label: "File" },
  remember_agent_fact: { icon: LightBulbIcon, label: "Memory" },
  remember_user_fact: { icon: LightBulbIcon, label: "Memory" },
  update_agent: { icon: CpuChipIcon, label: "Settings" },
  update_identity: { icon: UserIcon, label: "Identity" },
  get_time: { icon: ClockIcon, label: "Time" },
  request_credentials: { icon: KeyIcon, label: "Credentials" },
  manage_service: { icon: ServerIcon, label: "Service" },
  schedule: { icon: CalendarIcon, label: "Schedule" },
  delegate_task: { icon: ArrowsRightLeftIcon, label: "Delegate" },
  run_subtask: { icon: ArrowsRightLeftIcon, label: "Subtask" },
  read_skill: { icon: DocumentArrowDownIcon, label: "Skill" },
  make_voice_call: { icon: PhoneIcon, label: "Call" },
  send_dtmf: { icon: PhoneIcon, label: "DTMF" },
  hangup_call: { icon: PhoneIcon, label: "Hangup" },
};

function getToolMeta(name: string): { icon: React.ComponentType<React.SVGProps<SVGSVGElement>>; label: string } {
  if (TOOL_META[name]) return TOOL_META[name];
  if (name.startsWith("browser_")) return { icon: CursorArrowRaysIcon, label: "Browser action" };
  return { icon: WrenchScrewdriverIcon, label: name.replace(/_/g, " ") };
}

export function ToolStatusLine({ toolCalls }: { toolCalls: ToolCallStatus[] }) {
  const [dismissed, setDismissed] = useState(false);
  const dismissTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Derive values from current toolCalls
  const running = toolCalls.filter((tc) => tc.status === "running");
  const errors = toolCalls.filter((tc) => tc.status === "error");
  const completedCount = toolCalls.filter(
    (tc) => tc.status === "success" || tc.status === "error" || tc.status === "fading"
  ).length;
  const lastTool = toolCalls.length > 0 ? toolCalls[toolCalls.length - 1] : null;

  const isActive = running.length > 0 || errors.length > 0;
  const isDone = !isActive && completedCount > 0;

  // Un-dismiss when new tools start running
  if (dismissed && isActive) {
    setDismissed(false);
  }

  // Start 5s dismiss timer when we enter "done" state
  useEffect(() => {
    if (isDone && !dismissed) {
      dismissTimer.current = setTimeout(() => setDismissed(true), 5000);
      return () => { if (dismissTimer.current) clearTimeout(dismissTimer.current); };
    }
    if (isActive && dismissTimer.current) {
      clearTimeout(dismissTimer.current);
      dismissTimer.current = null;
    }
  }, [isDone, isActive, dismissed]);

  const hidden = dismissed || (!isActive && completedCount === 0);

  if (hidden) return <div className="h-8 mb-2" />;

  // Derive what to display
  let displayKey: string;
  let LeftIcon: React.ComponentType<React.SVGProps<SVGSVGElement>>;
  let leftLabel: string;
  let statusIcon: React.ReactNode;
  let rightText: string;
  let rightStyle: string;

  if (running.length > 0) {
    const current = running[running.length - 1];
    const meta = getToolMeta(current.name);
    displayKey = `running-${current.id}`;
    LeftIcon = meta.icon;
    leftLabel = meta.label;
    rightText = current.description || current.name.replace(/_/g, " ");
    statusIcon = (
      <div className="h-3 w-3 shrink-0 animate-spin rounded-full border-[1.5px] border-current border-t-transparent" />
    );
    rightStyle = "bg-surface-tertiary text-text-secondary border-surface-tertiary";
  } else if (errors.length > 0) {
    const current = errors[errors.length - 1];
    const meta = getToolMeta(current.name);
    displayKey = `error-${current.id}`;
    LeftIcon = meta.icon;
    leftLabel = meta.label;
    rightText = current.description || current.name.replace(/_/g, " ");
    statusIcon = <XCircleIcon className="h-3 w-3 shrink-0" />;
    rightStyle = "bg-danger-bg text-danger-text border-danger-bg";
  } else {
    // Done summary — last tool's icon/label + description
    const meta = lastTool ? getToolMeta(lastTool.name) : { icon: WrenchScrewdriverIcon, label: "Tools" };
    displayKey = "done";
    LeftIcon = meta.icon;
    leftLabel = meta.label;
    rightText = lastTool?.description || lastTool?.name?.replace(/_/g, " ") || "done";
    statusIcon = <CheckCircleIcon className="h-3 w-3 shrink-0" />;
    rightStyle = "bg-success-bg text-success-text border-success-bg";
  }

  const showCount = completedCount > 1;

  const barBg = "bg-surface-secondary border-border";
  const barText = "text-text-secondary";
  const dividerColor = "border-border";

  return (
    <div className="h-8 overflow-hidden mb-2">
      <div
        className={`flex items-center rounded-2xl border ${barBg}`}
      >
        <span
          key={`left-${displayKey}`}
          className={`inline-flex items-center gap-1 border-r ${dividerColor} px-3 py-1.5 text-xs font-medium rounded-l-2xl ${barText}`}
          style={{ animation: "toolStatusSlideIn 200ms ease-out" }}
        >
          <LeftIcon className="h-3 w-3" />
          {leftLabel}
        </span>
        <div
          key={`mid-${displayKey}`}
          className={`inline-flex items-center gap-1.5 px-3 py-1.5 text-xs min-w-0 flex-1 ${barText}`}
          style={{ animation: "toolStatusSlideIn 200ms ease-out" }}
        >
          {statusIcon}
          <span className="truncate">{rightText}</span>
        </div>
        {showCount && (
          <span
            key={`count-${completedCount}`}
            className={`inline-flex items-center border-l ${dividerColor} px-3 py-1.5 text-xs font-medium rounded-r-2xl whitespace-nowrap ${barText}`}
            style={{ animation: "toolStatusSlideIn 200ms ease-out" }}
          >
            {completedCount} tools
          </span>
        )}
      </div>
    </div>
  );
}
