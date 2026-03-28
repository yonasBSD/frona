"use client";

import { useState, useEffect } from "react";
import { SectionHeader, SectionPanel } from "@/components/settings/field";
import { KeyIcon, TrashIcon } from "@heroicons/react/24/outline";
import { api } from "@/lib/api-client";

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

interface CredsSectionProps {
  agentId: string;
}

export function CredsSection({ agentId }: CredsSectionProps) {
  const [grants, setGrants] = useState<VaultGrant[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    api
      .get<VaultGrant[]>("/api/vaults/grants")
      .then((all) => setGrants(all.filter((g) => g.agent_id === agentId)))
      .catch(() => {})
      .finally(() => setLoading(false));
  }, [agentId]);

  const revoke = async (grantId: string) => {
    await api.delete(`/api/vaults/grants/${grantId}`);
    setGrants((prev) => prev.filter((g) => g.id !== grantId));
  };

  return (
    <div>
      <SectionHeader title="Credentials" description="Vault access grants for this agent" icon={KeyIcon} />
      <SectionPanel>
        {loading && <p className="text-sm text-text-tertiary">Loading...</p>}
        {!loading && grants.length === 0 && (
          <p className="text-sm text-text-tertiary">No credential grants for this agent.</p>
        )}
        <div className="space-y-3">
          {grants.map((grant) => (
            <div
              key={grant.id}
              className="flex items-center gap-3 rounded-lg border border-border bg-surface p-3"
            >
              <KeyIcon className="h-5 w-5 shrink-0 text-text-tertiary" />
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium text-text-primary truncate">{grant.query}</p>
                <p className="text-xs text-text-tertiary">
                  {grant.expires_at
                    ? `Expires ${new Date(grant.expires_at).toLocaleDateString()}`
                    : "No expiry"}
                  {grant.env_var_prefix && ` · Prefix: ${grant.env_var_prefix}`}
                </p>
              </div>
              <button
                onClick={() => revoke(grant.id)}
                className="shrink-0 rounded-lg p-1.5 text-text-tertiary hover:bg-surface-tertiary hover:text-text-primary transition"
                title="Revoke"
              >
                <TrashIcon className="h-4 w-4" />
              </button>
            </div>
          ))}
        </div>
      </SectionPanel>
    </div>
  );
}
