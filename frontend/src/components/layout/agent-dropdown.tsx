"use client";

import { useState, useRef, useEffect } from "react";
import { useRouter } from "next/navigation";
import {
  UserGroupIcon,
  Cog6ToothIcon,
  TrashIcon,
  UserCircleIcon,
  MagnifyingGlassIcon,
  CodeBracketIcon,
  BeakerIcon,
} from "@heroicons/react/24/outline";
import { useNavigation } from "@/lib/navigation-context";
import { agentDisplayName, type Agent } from "@/lib/types";

function RobotIcon({ className }: { className?: string }) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      className={className}
    >
      <rect x="5" y="7" width="14" height="12" rx="2" />
      <circle cx="9.5" cy="13" r="1.5" />
      <circle cx="14.5" cy="13" r="1.5" />
      <path d="M12 3v4" />
      <circle cx="12" cy="3" r="1" />
      <path d="M2 13h3M19 13h3" />
    </svg>
  );
}

const defaultIcons: Record<string, React.ComponentType<{ className?: string }>> = {
  system: UserCircleIcon,
  researcher: MagnifyingGlassIcon,
  developer: CodeBracketIcon,
  tester: BeakerIcon,
};

function AgentIcon({ agent }: { agent: Agent }) {
  if (agent.avatar) {
    return (
      <img
        src={agent.avatar}
        alt={agent.name}
        className="h-5 w-5 shrink-0 rounded-full object-cover"
      />
    );
  }
  const Icon = defaultIcons[agent.id] ?? RobotIcon;
  return <Icon className="h-5 w-5 shrink-0 text-text-tertiary" />;
}

export function AgentDropdown() {
  const router = useRouter();
  const { agents, deleteAgent } = useNavigation();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

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

  const handleDelete = async (e: React.MouseEvent, agent: Agent) => {
    e.stopPropagation();
    if (!confirm(`Delete agent "${agentDisplayName(agent.id, agent.name)}"?`)) return;
    await deleteAgent(agent.id);
  };

  return (
    <div ref={ref} className="relative flex items-center">
      <button
        onClick={() => setOpen((v) => !v)}
        className={`relative flex items-center justify-center h-10 w-10 transition cursor-pointer ${
          open ? "rounded-t-xl rounded-b-none bg-surface-secondary text-text-primary z-[2] border border-border border-b-0" : "rounded-full bg-surface-tertiary text-text-secondary hover:brightness-125"
        }`}
        title="Agents"
      >
        <UserGroupIcon className="h-5 w-5" />
      </button>

      {open && (
        <div className="absolute right-0 top-full z-[1] w-64 rounded-xl rounded-tr-none border border-border bg-surface-secondary shadow-lg">
          <div className="absolute -top-px right-0 w-[calc(theme(spacing.10)-3px)] h-[2px] bg-surface-secondary z-[1]" />
          <div className="pb-1">
            <div className="flex items-center justify-between px-4 py-2 border-b border-border shrink-0">
              <span className="text-sm font-medium text-text-secondary">Agents</span>
            </div>
            {[...agents].sort((a, b) => (a.id === "system" ? -1 : b.id === "system" ? 1 : 0)).map((agent) => (
              <div
                key={agent.id}
                className="group flex items-center gap-2 px-4 py-2 hover:bg-surface-secondary transition cursor-pointer"
                onClick={() => {
                  router.push(`/chat?agent=${agent.id}`);
                  setOpen(false);
                }}
              >
                <AgentIcon agent={agent} />
                <span className="flex-1 truncate text-sm text-text-secondary group-hover:text-text-primary">
                  {agentDisplayName(agent.id, agent.name)}
                </span>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    router.push(`/agents?id=${agent.id}`);
                    setOpen(false);
                  }}
                  className="p-1 rounded text-text-tertiary hover:text-text-primary transition opacity-0 group-hover:opacity-100"
                  title="Configure"
                >
                  <Cog6ToothIcon className="h-4 w-4" />
                </button>
                {!agent.is_shared && (
                  <button
                    onClick={(e) => handleDelete(e, agent)}
                    className="p-1 rounded text-text-tertiary hover:text-error-text transition opacity-0 group-hover:opacity-100"
                    title="Delete"
                  >
                    <TrashIcon className="h-4 w-4" />
                  </button>
                )}
              </div>
            ))}
            {agents.length === 0 && (
              <p className="px-3 py-4 text-center text-xs text-text-tertiary">
                No agents configured
              </p>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
