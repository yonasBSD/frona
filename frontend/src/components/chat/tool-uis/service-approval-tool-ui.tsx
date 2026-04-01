"use client";

import { useState, useEffect } from "react";
import { makeAssistantToolUI } from "@assistant-ui/react";
import { api, API_URL } from "@/lib/api-client";
import { useChat } from "@/lib/chat-context";
import { ApprovalResult, ApprovalButtons } from "./approval-parts";
import { Field, SectionHeader } from "@/components/settings/field";
import { RocketLaunchIcon } from "@heroicons/react/24/outline";
import type { AppResponse } from "@/lib/types";

interface ServiceApprovalArgs {
  action: string;
  manifest: Record<string, unknown>;
  previous_manifest: Record<string, unknown> | null;
  status: string;
  response: string | null;
}

export const ServiceApprovalToolUI = makeAssistantToolUI<ServiceApprovalArgs, string>({
  toolName: "ServiceApproval",
  render: ({ args, result, addResult }) => {
    return (
      <ServiceApprovalRenderer
        action={args.action}
        manifest={args.manifest}
        previousManifest={args.previous_manifest}
        serverStatus={args.status}
        result={result}
        addResult={addResult}
      />
    );
  },
});

function ServiceApprovalRenderer({
  action,
  manifest,
  previousManifest,
  serverStatus,
  result,
  addResult,
}: {
  action: string;
  manifest: Record<string, unknown>;
  previousManifest: Record<string, unknown> | null;
  serverStatus: string;
  result?: string;
  addResult: (result: string) => void;
}) {
  const { chatId } = useChat();
  const [loading, setLoading] = useState(false);
  const [appUrl, setAppUrl] = useState<string | null>(null);

  const denied = serverStatus === "denied" || result === "denied";
  const resolved = denied || serverStatus === "resolved" || result !== undefined;

  const name = String(manifest?.name || manifest?.id || "Unknown service");
  const description = manifest?.description ? String(manifest.description) : null;
  const command = manifest?.command ? String(manifest.command) : null;
  const isUpdate = !!previousManifest;

  // Fetch app URL on history load when already approved
  useEffect(() => {
    if (!resolved || denied || appUrl) return;
    api.get<AppResponse[]>("/api/apps").then((apps) => {
      const app = apps.find((a) => a.name === name);
      if (app?.url) setAppUrl(`${API_URL}${app.url}`);
    }).catch(() => {});
  }, [resolved, denied, appUrl, name]);

  if (resolved || denied) {
    return (
      <ApprovalResult
        denied={denied}
        label={denied ? "Service deployment denied" : "Service deployment approved"}
      >
        {appUrl && (
          <a
            href={appUrl}
            target="_blank"
            rel="noopener noreferrer"
            className="rounded-md border border-border bg-surface-secondary px-2.5 py-0.5 text-sm font-medium text-text-secondary shadow-sm hover:bg-surface-tertiary hover:text-text-primary transition"
          >
            Open
          </a>
        )}
      </ApprovalResult>
    );
  }

  const handleApprove = async () => {
    setLoading(true);
    try {
      const res = await api.post<{ approved: boolean; url?: string }>("/api/apps/approve", { chat_id: chatId });
      if (res?.url) setAppUrl(res.url);
      addResult("approved");
    } finally {
      setLoading(false);
    }
  };

  const handleDeny = async () => {
    setLoading(true);
    try {
      await api.post("/api/apps/deny", { chat_id: chatId });
      addResult("denied");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="rounded-xl border border-border bg-surface-secondary p-4 space-y-4 my-2 w-3/4 -order-1">
      <div className="mb-5 pb-3 border-b border-border flex items-end justify-between gap-3">
        <div>
          <div className="flex items-center gap-2">
            <h3 className="text-lg font-semibold text-text-primary">{name}</h3>
            <span className="rounded-full bg-surface-tertiary px-2.5 py-0.5 text-[11px] font-medium text-text-secondary uppercase tracking-wide">{action}</span>
          </div>
          {description && <p className="text-sm text-text-tertiary mt-1">{description}</p>}
        </div>
        <RocketLaunchIcon className="h-10 w-10 text-text-tertiary shrink-0" />
      </div>
      {command && (
        <Field label="Command">
          <code className="rounded-lg border border-border bg-surface px-3 py-2 text-sm text-text-primary block">{command}</code>
        </Field>
      )}

      <ApprovalButtons loading={loading} onApprove={handleApprove} onDeny={handleDeny} />
    </div>
  );
}
