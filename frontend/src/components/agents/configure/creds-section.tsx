"use client";

import { useState, useEffect, useCallback } from "react";
import { SectionHeader } from "@/components/settings/field";
import { KeyIcon, TrashIcon } from "@heroicons/react/24/outline";
import { CheckIcon, MinusIcon } from "@heroicons/react/16/solid";
import * as Checkbox from "@radix-ui/react-checkbox";
import { Si1password, SiBitwarden, SiVault, SiKeepassxc, SiKeeper } from "@icons-pack/react-simple-icons";
import { api } from "@/lib/api-client";

const PROVIDER_ICONS: Record<string, React.ComponentType<{ className?: string; size?: number }>> = {
  one_password: Si1password,
  bitwarden: SiBitwarden,
  hashicorp: SiVault,
  keepass: SiKeepassxc,
  keeper: SiKeeper,
};

const PROVIDER_LABELS: Record<string, string> = {
  local: "Local",
  one_password: "1Password",
  bitwarden: "Bitwarden",
  hashicorp: "HashiCorp",
  keepass: "KeePass",
  keeper: "Keeper",
};

interface VaultGrant {
  id: string;
  connection_id: string;
  vault_item_id: string;
  agent_id: string;
  query: string;
  env_var_prefix: string | null;
  expires_at: string | null;
  created_at: string;
}

interface VaultConnection {
  id: string;
  name: string;
  provider: string;
  enabled: boolean;
}

interface CredsSectionProps {
  agentId: string;
}

export function CredsSection({ agentId }: CredsSectionProps) {
  const [grants, setGrants] = useState<VaultGrant[]>([]);
  const [connections, setConnections] = useState<Map<string, VaultConnection>>(new Map());
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [revoking, setRevoking] = useState(false);

  useEffect(() => {
    Promise.all([
      api.get<VaultGrant[]>("/api/vaults/grants"),
      api.get<VaultConnection[]>("/api/vaults"),
    ])
      .then(([allGrants, allConns]) => {
        setGrants(allGrants.filter((g) => g.agent_id === agentId));
        setConnections(new Map(allConns.map((c) => [c.id, c])));
      })
      .catch(() => {})
      .finally(() => setLoading(false));
  }, [agentId]);

  const toggleSelect = useCallback((id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  }, []);

  const toggleSelectAll = useCallback(() => {
    setSelected((prev) => {
      if (prev.size === grants.length) return new Set();
      return new Set(grants.map((g) => g.id));
    });
  }, [grants]);

  const revokeSelected = useCallback(async () => {
    if (selected.size === 0) return;
    setRevoking(true);
    try {
      for (const id of selected) {
        await api.delete(`/api/vaults/grants/${id}`);
      }
      setGrants((prev) => prev.filter((g) => !selected.has(g.id)));
      setSelected(new Set());
    } catch {
      // ignore
    } finally {
      setRevoking(false);
    }
  }, [selected]);

  return (
    <div>
      <SectionHeader title="Credentials" description="Vault access grants for this agent" icon={KeyIcon} />
      {loading && <p className="text-sm text-text-tertiary py-8 text-center">Loading...</p>}
      {!loading && grants.length === 0 && (
        <p className="text-sm text-text-tertiary py-8 text-center">No credential grants for this agent.</p>
      )}
      {!loading && grants.length > 0 && (
        <div>
          <div className="flex items-center justify-between mb-2 min-h-[36px]">
            <h4 className="text-base font-medium text-text-secondary">Grants</h4>
            {selected.size > 0 && (
              <button
                onClick={revokeSelected}
                disabled={revoking}
                className="inline-flex items-center gap-1.5 rounded-lg border border-border px-3 py-1.5 text-xs font-medium text-danger hover:bg-surface-tertiary disabled:opacity-50 transition"
              >
                <TrashIcon className="h-3.5 w-3.5" />
                {revoking ? "Revoking..." : `Revoke ${selected.size} selected`}
              </button>
            )}
          </div>
          <div className="rounded-xl border border-border bg-surface-secondary divide-y divide-border">
            <div className="px-4 py-2 flex items-center gap-3 bg-surface-tertiary/30">
              <Checkbox.Root
                checked={grants.every((g) => selected.has(g.id)) ? true : grants.some((g) => selected.has(g.id)) ? "indeterminate" : false}
                onCheckedChange={toggleSelectAll}
                className="h-4 w-4 rounded border border-border bg-surface flex items-center justify-center data-[state=checked]:bg-accent data-[state=checked]:border-accent data-[state=indeterminate]:bg-accent data-[state=indeterminate]:border-accent transition shrink-0"
              >
                <Checkbox.Indicator>
                  {grants.every((g) => selected.has(g.id))
                    ? <CheckIcon className="h-3 w-3 text-surface" />
                    : <MinusIcon className="h-3 w-3 text-surface" />}
                </Checkbox.Indicator>
              </Checkbox.Root>
              <span className="text-xs text-text-secondary">
                {selected.size > 0 ? `${selected.size} selected` : "Select all"}
              </span>
            </div>
            {grants.map((grant) => {
              const conn = connections.get(grant.connection_id);
              const provider = conn?.provider ?? "local";
              const ProviderIcon = PROVIDER_ICONS[provider];
              return (
                <div
                  key={grant.id}
                  onClick={(e) => { if (!(e.target as HTMLElement).closest("button")) toggleSelect(grant.id); }}
                  className="px-4 py-3 flex items-center gap-3 transition hover:bg-surface-tertiary cursor-pointer"
                >
                  <Checkbox.Root
                    checked={selected.has(grant.id)}
                    onCheckedChange={() => toggleSelect(grant.id)}
                    className="h-4 w-4 rounded border border-border bg-surface flex items-center justify-center data-[state=checked]:bg-accent data-[state=checked]:border-accent transition shrink-0"
                  >
                    <Checkbox.Indicator>
                      <CheckIcon className="h-3 w-3 text-surface" />
                    </Checkbox.Indicator>
                  </Checkbox.Root>
                  <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-surface-tertiary shrink-0">
                    {ProviderIcon
                      ? <ProviderIcon size={18} className="text-text-secondary" />
                      : <KeyIcon className="h-5 w-5 text-text-tertiary" />}
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium text-text-primary truncate">{grant.query}</span>
                      <span className="rounded-full bg-surface-tertiary px-2 py-0.5 text-[11px] font-medium text-text-secondary">
                        {PROVIDER_LABELS[provider] ?? provider}
                      </span>
                    </div>
                    <span className="text-xs text-text-tertiary">
                      {grant.expires_at
                        ? `Expires ${new Date(grant.expires_at).toLocaleDateString()}`
                        : "No expiry"}
                      {grant.env_var_prefix && ` · Prefix: ${grant.env_var_prefix}`}
                    </span>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
