"use client";

import { useMemo } from "react";
import { useNavigation } from "@/lib/navigation-context";
import { AgentItem } from "./agent-item";

export function AgentsTab() {
  const { agents } = useNavigation();

  const sorted = useMemo(
    () =>
      [...agents].sort((a, b) => {
        if (a.id === "system") return -1;
        if (b.id === "system") return 1;
        return (b.chat_count ?? 0) - (a.chat_count ?? 0);
      }),
    [agents],
  );

  return (
    <div className="space-y-1 p-2">
      {sorted.map((agent) => (
        <AgentItem key={agent.id} agent={agent} />
      ))}
      {agents.length === 0 && (
        <p className="px-2 py-4 text-center text-xs text-text-tertiary">
          No agents configured
        </p>
      )}
    </div>
  );
}
