"use client";

import { useState, useEffect, useRef } from "react";
import { makeAssistantToolUI } from "@assistant-ui/react";
import { api } from "@/lib/api-client";
import { useChat } from "@/lib/chat-context";
import { ApprovalResult, ApprovalButtons } from "./approval-parts";
import { Field, TextInput, SelectInput, SectionHeader } from "@/components/settings/field";
import { KeyIcon } from "@heroicons/react/24/outline";

interface VaultApprovalArgs {
  query: string;
  reason: string;
  env_var_prefix: string | null;
  status: string;
  response: string | null;
}

interface VaultItem {
  id: string;
  name: string;
  username?: string;
}

interface VaultConnection {
  id: string;
  name: string;
  provider: string;
  enabled: boolean;
}

type GrantDuration = "once" | { hours: number } | { days: number } | "permanent";

export const VaultApprovalToolUI = makeAssistantToolUI<VaultApprovalArgs, string>({
  toolName: "VaultApproval",
  render: ({ args, result, addResult }) => {
    return (
      <VaultApprovalRenderer
        query={args.query}
        reason={args.reason}
        envVarPrefix={args.env_var_prefix}
        serverStatus={args.status}
        result={result}
        addResult={addResult}
      />
    );
  },
});

function VaultApprovalRenderer({
  query,
  reason,
  serverStatus,
  result,
  addResult,
  envVarPrefix,
}: {
  query: string;
  reason: string;
  envVarPrefix: string | null;
  serverStatus: string;
  result?: string;
  addResult: (result: string) => void;
}) {
  const { chatId } = useChat();
  const [loading, setLoading] = useState(false);
  const acted = useRef(false);
  const [connections, setConnections] = useState<VaultConnection[]>([]);
  const [selectedConnection, setSelectedConnection] = useState("");
  const [items, setItems] = useState<VaultItem[]>([]);
  const [selectedItem, setSelectedItem] = useState("");
  const [duration, setDuration] = useState<GrantDuration>("once");
  const [searchQuery, setSearchQuery] = useState(query);
  const [searching, setSearching] = useState(false);

  const denied = serverStatus === "denied" || result === "denied";
  const resolved = denied || serverStatus === "resolved" || result !== undefined;

  useEffect(() => {
    if (resolved) return;
    api.get<VaultConnection[]>("/api/vaults").then((conns) => {
      setConnections(conns.filter((c) => c.enabled));
      if (conns.length > 0) setSelectedConnection(conns[0].id);
    });
  }, [resolved]);

  useEffect(() => {
    if (resolved || !selectedConnection || !searchQuery) return;
    setSearching(true);
    api
      .get<VaultItem[]>(`/api/vaults/${selectedConnection}/items?q=${encodeURIComponent(searchQuery)}`)
      .then((results) => {
        setItems(results);
        if (results.length > 0) setSelectedItem(results[0].id);
      })
      .finally(() => setSearching(false));
  }, [resolved, selectedConnection, searchQuery]);

  if (resolved || denied) {
    return (
      <ApprovalResult
        denied={denied}
        label={denied ? "Credential request denied" : "Credential request approved"}
      />
    );
  }

  const handleApprove = async () => {
    if (!selectedItem || acted.current) return;
    acted.current = true;
    setLoading(true);
    try {
      await api.post("/api/vaults/approve", {
        chat_id: chatId,
        connection_id: selectedConnection,
        vault_item_id: selectedItem,
        grant_duration: duration,
        env_var_prefix: envVarPrefix,
      });
      addResult("approved");
    } finally {
      setLoading(false);
    }
  };

  const handleDeny = async () => {
    if (acted.current) return;
    acted.current = true;
    setLoading(true);
    try {
      await api.post("/api/vaults/deny", { chat_id: chatId });
      addResult("denied");
    } finally {
      setLoading(false);
    }
  };

  const durationValue = typeof duration === "string" ? duration : "hours" in duration ? "hours" : "days";

  return (
    <div className="rounded-xl border border-border bg-surface-secondary p-4 space-y-4 my-2 w-3/4 -order-1">
      <SectionHeader title="Credential Request" description={reason} icon={KeyIcon} />

      <SelectInput
        label="Vault"
        value={selectedConnection}
        onChange={(v) => setSelectedConnection(v ?? "")}
        options={connections.map((c) => ({ value: c.id, label: c.name }))}
        allowEmpty={false}
      />

      <TextInput
        label="Search"
        value={searchQuery}
        onChange={setSearchQuery}
        placeholder="Search vault items..."
      />

      <Field label="Item">
        {searching ? (
          <p className="text-xs text-text-tertiary">Searching...</p>
        ) : items.length > 0 ? (
          <div className="space-y-1">
            {items.map((item) => (
              <button
                key={item.id}
                onClick={() => setSelectedItem(item.id)}
                className={`w-full rounded-lg border px-3 py-2 text-left text-sm transition ${
                  selectedItem === item.id
                    ? "border-accent bg-accent/10 text-accent"
                    : "border-border text-text-secondary hover:border-accent"
                }`}
              >
                <span className="font-medium">{item.name}</span>
                {item.username && (
                  <span className="ml-2 text-text-tertiary">({item.username})</span>
                )}
              </button>
            ))}
          </div>
        ) : (
          <p className="text-xs text-text-tertiary">No items found</p>
        )}
      </Field>

      <SelectInput
        label="Duration"
        value={durationValue}
        onChange={(v) => {
          if (v === "once") setDuration("once");
          else if (v === "permanent") setDuration("permanent");
          else if (v === "hours") setDuration({ hours: 24 });
          else if (v === "days") setDuration({ days: 7 });
        }}
        options={[
          { value: "once", label: "Allow once" },
          { value: "hours", label: "Allow for 24 hours" },
          { value: "days", label: "Allow for 7 days" },
          { value: "permanent", label: "Allow permanently" },
        ]}
        allowEmpty={false}
      />

      <ApprovalButtons
        loading={loading}
        onApprove={handleApprove}
        onDeny={handleDeny}
        approveDisabled={!selectedItem}
      />
    </div>
  );
}
