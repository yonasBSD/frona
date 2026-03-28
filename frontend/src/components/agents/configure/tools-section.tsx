"use client";

import { useState, useEffect, useMemo } from "react";
import { SectionHeader, SectionPanel } from "@/components/settings/field";
import { WrenchScrewdriverIcon } from "@heroicons/react/24/outline";
import { api } from "@/lib/api-client";
import ReactMarkdown from "react-markdown";

interface ToolInfo {
  id: string;
  group: string;
  description: string;
  configurable: boolean;
}

interface ToolGroup {
  group: string;
  description: string;
  configurable: boolean;
}

interface ToolsSectionProps {
  tools: string[];
  onChange: (tools: string[]) => void;
}

export function ToolsSection({ tools, onChange }: ToolsSectionProps) {
  const [available, setAvailable] = useState<ToolInfo[]>([]);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  useEffect(() => {
    api.get<ToolInfo[]>("/api/tools").then(setAvailable).catch(() => {});
  }, []);

  const groups = useMemo(() => {
    const map = new Map<string, ToolGroup>();
    for (const tool of available) {
      if (!map.has(tool.group)) {
        map.set(tool.group, {
          group: tool.group,
          description: tool.description,
          configurable: tool.configurable,
        });
      }
    }
    return Array.from(map.values()).sort((a, b) => {
      if (a.configurable !== b.configurable) return a.configurable ? 1 : -1;
      return a.group.localeCompare(b.group);
    });
  }, [available]);

  const toggle = (group: string) => {
    if (tools.includes(group)) {
      onChange(tools.filter((t) => t !== group));
    } else {
      onChange([...tools, group]);
    }
  };

  const toggleExpand = (group: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(group)) next.delete(group);
      else next.add(group);
      return next;
    });
  };

  const formatName = (name: string) =>
    name.split("_").map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(" ");

  return (
    <div>
      <SectionHeader title="Tools" description="Tools available to this agent" icon={WrenchScrewdriverIcon} />
      <SectionPanel>
        <div className="space-y-3">
          {groups.map((g) => {
            const isExpanded = expanded.has(g.group);
            return (
              <div
                key={g.group}
                className={`rounded-lg border border-border bg-surface ${
                  !g.configurable ? "opacity-60" : ""
                }`}
              >
                <div className="flex w-full items-center gap-3 px-4 py-3">
                  <input
                    type="checkbox"
                    checked={g.configurable ? tools.includes(g.group) : true}
                    onChange={() => g.configurable && toggle(g.group)}
                    disabled={!g.configurable}
                    className="h-4 w-4 rounded border-border text-accent focus:ring-accent disabled:opacity-50"
                  />
                  <button
                    type="button"
                    onClick={() => toggleExpand(g.group)}
                    className="flex flex-1 items-center justify-between text-sm font-medium text-text-primary hover:text-text-secondary transition"
                  >
                    <span>{formatName(g.group)}</span>
                    <svg
                      className={`h-4 w-4 text-text-tertiary transition-transform ${isExpanded ? "rotate-180" : ""}`}
                      fill="none"
                      viewBox="0 0 24 24"
                      stroke="currentColor"
                      strokeWidth={2}
                    >
                      <path strokeLinecap="round" strokeLinejoin="round" d="M19 9l-7 7-7-7" />
                    </svg>
                  </button>
                </div>
                {isExpanded && (
                  <div className="px-4 pb-4 pl-11">
                    <div className="text-xs text-text-tertiary prose prose-xs prose-invert max-w-none">
                      <ReactMarkdown>{g.description}</ReactMarkdown>
                    </div>
                  </div>
                )}
              </div>
            );
          })}
          {groups.length === 0 && (
            <p className="text-sm text-text-tertiary">Loading tools...</p>
          )}
        </div>
      </SectionPanel>
    </div>
  );
}
