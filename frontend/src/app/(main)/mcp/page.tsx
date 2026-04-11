"use client";

import { useState, useEffect, useCallback, useRef, Suspense } from "react";
import { useSearchParams, useRouter } from "next/navigation";
import { ArrowLeftIcon, CpuChipIcon, PlayIcon, StopIcon, TrashIcon, PlusIcon, InformationCircleIcon, CommandLineIcon, DocumentTextIcon } from "@heroicons/react/24/outline";
import { api, API_URL, getAccessToken } from "@/lib/api-client";
import { SectionHeader } from "@/components/settings/field";
import { formatDistanceToNow } from "date-fns";
import { SandboxSection } from "@/components/agents/configure/sandbox-section";
import { CredsSection } from "@/components/agents/configure/creds-section";

interface McpServer {
  id: string;
  slug: string;
  display_name: string;
  description: string | null;
  repository_url: string | null;
  registry_id: string | null;
  status: string;
  command: string;
  args: string[];
  tool_count: number;
  env: Record<string, string>;
  extra_read_paths: string[];
  extra_write_paths: string[];
  installed_at: string;
  last_started_at: string | null;
}

interface EnvVarDef {
  name: string;
  description: string | null;
  is_required: boolean;
  is_secret: boolean;
}

interface RegistryPackage {
  environment_variables: EnvVarDef[];
}

interface RegistryEntry {
  name: string;
  packages: RegistryPackage[];
}

const SECTIONS = [
  { id: "about", label: "About" },
  { id: "environment", label: "Environment" },
  { id: "sandbox", label: "Sandbox" },
  { id: "creds", label: "Credentials" },
  { id: "logs", label: "Logs" },
] as const;

type SectionId = (typeof SECTIONS)[number]["id"];

const STATUS_BADGE: Record<string, string> = {
  installed: "bg-surface-tertiary text-text-secondary",
  running: "bg-green-500/15 text-green-500",
  stopped: "bg-yellow-500/15 text-yellow-500",
  failed: "bg-red-500/15 text-red-500",
  starting: "bg-blue-400/15 text-blue-400",
};

function McpServerPage() {
  const searchParams = useSearchParams();
  const router = useRouter();
  const serverId = searchParams.get("id");

  const [server, setServer] = useState<McpServer | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [actionLoading, setActionLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [logs, setLogs] = useState<string>("");
  const [logsLoading, setLogsLoading] = useState(false);
  const [followLogs, setFollowLogs] = useState(true);
  const logsEndRef = useRef<HTMLDivElement>(null);

  const sectionParam = searchParams.get("section");
  const initialSection = SECTIONS.some((s) => s.id === sectionParam) ? (sectionParam as SectionId) : "about";
  const [activeSection, setActiveSection] = useState<SectionId>(initialSection);

  const [envValues, setEnvValues] = useState<Record<string, string>>({});
  const [allEnvDefs, setAllEnvDefs] = useState<EnvVarDef[]>([]);
  const [sandboxConfig, setSandboxConfig] = useState<{
    network_access: boolean;
    allowed_network_destinations: string[];
    shared_paths: string[];
  } | null>(null);

  const reload = useCallback(async () => {
    if (!serverId) return;
    try {
      const s = await api.get<McpServer>(`/api/mcp/servers/${serverId}`);
      setServer(s);
      setEnvValues(s.env ?? {});
      setSandboxConfig({
        network_access: true,
        allowed_network_destinations: [],
        shared_paths: [...s.extra_read_paths, ...s.extra_write_paths],
      });

      if (s.registry_id) {
        try {
          const entry = await api.get<RegistryEntry>(
            `/api/mcp/registry/${encodeURIComponent(s.registry_id)}`
          );
          if (entry?.packages?.[0]?.environment_variables) {
            setAllEnvDefs(entry.packages[0].environment_variables);
          }
        } catch { /* ignore */ }
      }
    } catch {
      setError("Failed to load server");
    } finally {
      setLoading(false);
    }
  }, [serverId]);

  useEffect(() => {
    reload();
  }, [reload]);

  useEffect(() => {
    if (activeSection !== "logs" || !serverId) return;
    setLogs("");
    setLogsLoading(true);

    const controller = new AbortController();
    (async () => {
      const token = getAccessToken();
      const headers: Record<string, string> = {};
      if (token) headers["Authorization"] = `Bearer ${token}`;

      let res: Response;
      try {
        res = await fetch(`${API_URL}/api/mcp/servers/${serverId}/logs/stream`, {
          headers,
          signal: controller.signal,
          credentials: "include",
        });
      } catch {
        setLogsLoading(false);
        return;
      }

      if (!res.ok || !res.body) {
        setLogsLoading(false);
        return;
      }

      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";

      try {
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          setLogsLoading(false);
          buffer += decoder.decode(value, { stream: true });
          const lines = buffer.split("\n");
          buffer = lines.pop() ?? "";
          for (const line of lines) {
            if (line.startsWith("data: ")) {
              setLogs((prev) => prev + line.slice(6) + "\n");
            }
          }
        }
      } catch {
        // aborted or connection lost
      }
    })();

    return () => controller.abort();
  }, [activeSection, serverId]);

  useEffect(() => {
    if (followLogs && logsEndRef.current) {
      logsEndRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [logs, followLogs]);

  const save = async () => {
    if (!server) return;
    setSaving(true);
    setError(null);
    try {
      const extra_env: Record<string, string> = {};
      for (const [k, v] of Object.entries(envValues)) {
        if (v.trim()) extra_env[k] = v.trim();
      }
      await api.patch(`/api/mcp/servers/${serverId}`, {
        extra_env: Object.keys(extra_env).length > 0 ? extra_env : undefined,
        extra_read_paths: sandboxConfig?.shared_paths,
        extra_write_paths: [],
      });
      setDirty(false);
      await reload();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Save failed");
    } finally {
      setSaving(false);
    }
  };

  const start = async () => {
    setActionLoading(true);
    setError(null);
    try {
      await api.post(`/api/mcp/servers/${serverId}/start`, {});
      await reload();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Start failed");
    } finally {
      setActionLoading(false);
    }
  };

  const stop = async () => {
    setActionLoading(true);
    try {
      await api.post(`/api/mcp/servers/${serverId}/stop`, {});
      await reload();
    } catch { /* ignore */ }
    finally { setActionLoading(false); }
  };

  const uninstall = async () => {
    if (!confirm("Uninstall this MCP server? This will remove all data and credential bindings.")) return;
    setActionLoading(true);
    try {
      await api.delete(`/api/mcp/servers/${serverId}`);
      router.push("/settings?tab=mcp");
    } catch { /* ignore */ }
    finally { setActionLoading(false); }
  };

  if (!serverId) {
    return <p className="p-8 text-sm text-error-text">No server ID provided.</p>;
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="h-5 w-5 animate-spin rounded-full border-2 border-accent border-t-transparent" />
      </div>
    );
  }

  if (!server) {
    return <p className="p-8 text-sm text-error-text">{error || "Server not found"}</p>;
  }

  const canStart = ["installed", "stopped", "failed"].includes(server.status);
  const canStop = server.status === "running";
  const ownerAvatar = server.repository_url
    ? (() => {
        const m = server.repository_url!.match(/github\.com\/([^/]+)/);
        return m ? `https://github.com/${m[1]}.png?size=64` : null;
      })()
    : null;
  const displayName = server.display_name.includes("/")
    ? server.display_name.split("/").pop()!
    : server.display_name;

  return (
    <div className="flex h-full bg-surface">
      {/* Sidebar */}
      <div
        className="border-r border-border bg-surface-nav p-4 flex flex-col"
        style={{ width: 289 }}
      >
        <button
          onClick={() => router.push("/settings?tab=mcp")}
          className="flex items-center gap-2 text-sm text-text-secondary hover:text-text-primary transition mb-4"
        >
          <ArrowLeftIcon className="h-4 w-4" />
          Back to MCP Servers
        </button>

        <div className="flex items-center gap-2 mb-4">
          {ownerAvatar ? (
            <img src={ownerAvatar} alt="" className="h-8 w-8 rounded-lg shrink-0" />
          ) : (
            <CpuChipIcon className="h-8 w-8 text-text-tertiary shrink-0" />
          )}
          <div className="min-w-0">
            <h2 className="text-sm font-semibold text-text-primary truncate">{displayName}</h2>
            <span className={`inline-block rounded-full px-2 py-0.5 text-[10px] font-medium ${STATUS_BADGE[server.status] ?? STATUS_BADGE.installed}`}>
              {server.status}
            </span>
          </div>
        </div>

        <nav className="space-y-1 flex-1">
          {SECTIONS.map((s) => (
            <button
              key={s.id}
              onClick={() => setActiveSection(s.id)}
              className={`w-full text-left rounded-lg px-3 py-2 text-sm transition ${
                activeSection === s.id
                  ? "bg-accent/10 text-accent font-medium"
                  : "text-text-secondary hover:bg-surface-tertiary hover:text-text-primary"
              }`}
            >
              {s.label}
            </button>
          ))}
        </nav>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto">
        <div className="max-w-2xl mx-auto p-8 space-y-6">
          {error && (
            <div className="rounded-lg bg-error-bg p-3 text-sm text-error-text">{error}</div>
          )}

          {activeSection === "about" && (
            <div className="space-y-6">
              <SectionHeader title="About" description="Server information and controls" icon={InformationCircleIcon} />
              <div className="rounded-xl border border-border bg-surface-secondary divide-y divide-border overflow-hidden">
                {server.registry_id && (
                  <div className="px-4 py-3 flex justify-between">
                    <span className="text-sm text-text-tertiary">Registry</span>
                    <span className="text-sm text-text-primary font-mono">{server.registry_id}</span>
                  </div>
                )}
                {server.repository_url && (
                  <div className="px-4 py-3 flex justify-between">
                    <span className="text-sm text-text-tertiary">Repository</span>
                    <a
                      href={server.repository_url}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-sm text-accent hover:text-accent-hover"
                    >
                      {server.repository_url.replace(/^https?:\/\/(www\.)?/, "").replace(/\.git$/, "")}
                    </a>
                  </div>
                )}
                <div className="px-4 py-3 flex justify-between">
                  <span className="text-sm text-text-tertiary">Command</span>
                  <span className="text-sm text-text-primary font-mono truncate ml-4">{server.command} {server.args.join(" ")}</span>
                </div>
                <div className="px-4 py-3 flex justify-between">
                  <span className="text-sm text-text-tertiary">Tools</span>
                  <span className="text-sm text-text-primary">{server.tool_count}</span>
                </div>
                <div className="px-4 py-3 flex justify-between">
                  <span className="text-sm text-text-tertiary">Installed</span>
                  <span className="text-sm text-text-primary">{formatDistanceToNow(new Date(server.installed_at), { addSuffix: true })}</span>
                </div>
                {server.last_started_at && (
                  <div className="px-4 py-3 flex justify-between">
                    <span className="text-sm text-text-tertiary">Last started</span>
                    <span className="text-sm text-text-primary">{formatDistanceToNow(new Date(server.last_started_at), { addSuffix: true })}</span>
                  </div>
                )}
              </div>

              <div className="flex items-center gap-2">
                {canStart && (
                  <button
                    onClick={start}
                    disabled={actionLoading}
                    className="inline-flex items-center gap-1.5 rounded-lg bg-green-600 px-4 py-2 text-sm font-medium text-white hover:bg-green-700 disabled:opacity-50 transition"
                  >
                    <PlayIcon className="h-4 w-4" />
                    {actionLoading ? "Starting..." : "Start"}
                  </button>
                )}
                {canStop && (
                  <button
                    onClick={stop}
                    disabled={actionLoading}
                    className="inline-flex items-center gap-1.5 rounded-lg bg-yellow-600 px-4 py-2 text-sm font-medium text-white hover:bg-yellow-700 disabled:opacity-50 transition"
                  >
                    <StopIcon className="h-4 w-4" />
                    {actionLoading ? "Stopping..." : "Stop"}
                  </button>
                )}
                <button
                  onClick={uninstall}
                  disabled={actionLoading}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-border px-4 py-2 text-sm font-medium text-danger hover:bg-surface-tertiary disabled:opacity-50 transition"
                >
                  <TrashIcon className="h-4 w-4" />
                  Uninstall
                </button>
              </div>
            </div>
          )}

          {activeSection === "environment" && (() => {
            const declaredNames = new Set(allEnvDefs.map((e) => e.name));
            const customKeys = Object.keys(envValues).filter((k) => !declaredNames.has(k));
            return (
              <div className="space-y-4">
                <SectionHeader title="Environment" description="Environment variables for this server" icon={CommandLineIcon} />

                {/* Registry-declared vars */}
                {allEnvDefs.length > 0 && (
                  <div className="space-y-3">
                    {allEnvDefs.map((ev) => (
                      <div key={ev.name}>
                        <label className="flex items-center gap-1.5 text-xs font-medium text-text-primary mb-1">
                          {ev.name}
                          {ev.is_required && <span className="text-red-400">*</span>}
                          {ev.is_secret && (
                            <span className="rounded-full bg-yellow-500/15 text-yellow-500 px-1.5 py-0.5 text-[10px] font-medium">secret</span>
                          )}
                        </label>
                        {ev.description && (
                          <p className="text-xs text-text-tertiary mb-1">{ev.description}</p>
                        )}
                        {ev.is_secret ? (
                          <p className="text-xs text-text-tertiary italic">
                            Configure this secret in the <button type="button" onClick={() => setActiveSection("creds")} className="text-accent hover:underline">Credentials</button> tab
                          </p>
                        ) : (
                          <input
                            type="text"
                            value={envValues[ev.name] ?? ""}
                            onChange={(e) => {
                              setEnvValues((prev) => ({ ...prev, [ev.name]: e.target.value }));
                              setDirty(true);
                            }}
                            placeholder={ev.is_required ? "Required" : "Optional"}
                            className="w-full rounded border border-border bg-background px-2.5 py-1.5 text-sm text-text-primary placeholder:text-text-tertiary focus:border-accent focus:outline-none font-mono"
                          />
                        )}
                      </div>
                    ))}
                  </div>
                )}

                {/* Custom env vars */}
                {customKeys.length > 0 && (
                  <div className="space-y-2">
                    {customKeys.map((key) => (
                      <div key={key} className="flex items-center gap-2">
                        <input
                          type="text"
                          defaultValue={key}
                          onBlur={(e) => {
                            const newKey = e.target.value.toUpperCase().replace(/[^A-Z0-9_]/g, "");
                            if (newKey && newKey !== key) {
                              setEnvValues((prev) => {
                                const next = { ...prev };
                                next[newKey] = next[key] ?? "";
                                delete next[key];
                                return next;
                              });
                              setDirty(true);
                            }
                          }}
                          placeholder="KEY"
                          className="w-1/3 rounded border border-border bg-background px-2.5 py-1.5 text-sm text-text-primary font-mono placeholder:text-text-tertiary focus:border-accent focus:outline-none"
                        />
                        <input
                          type="text"
                          value={envValues[key] ?? ""}
                          onChange={(e) => {
                            setEnvValues((prev) => ({ ...prev, [key]: e.target.value }));
                            setDirty(true);
                          }}
                          placeholder="Value"
                          className="flex-1 rounded border border-border bg-background px-2.5 py-1.5 text-sm text-text-primary placeholder:text-text-tertiary focus:border-accent focus:outline-none font-mono"
                        />
                        <button
                          onClick={() => {
                            setEnvValues((prev) => { const next = { ...prev }; delete next[key]; return next; });
                            setDirty(true);
                          }}
                          className="p-1 text-text-tertiary hover:text-danger transition"
                        >
                          <TrashIcon className="h-4 w-4" />
                        </button>
                      </div>
                    ))}
                  </div>
                )}

                {/* Add new env var */}
                <button
                  onClick={() => {
                    const key = `NEW_VAR_${Object.keys(envValues).length + 1}`;
                    setEnvValues((prev) => ({ ...prev, [key]: "" }));
                    setDirty(true);
                  }}
                  className="inline-flex items-center gap-1.5 text-xs text-accent hover:text-accent-hover transition"
                >
                  <PlusIcon className="h-3.5 w-3.5" />
                  Add environment variable
                </button>
              </div>
            );
          })()}

          {activeSection === "sandbox" && (
            <SandboxSection
              sandbox={sandboxConfig}
              onChange={(v) => { setSandboxConfig(v); setDirty(true); }}
            />
          )}

          {activeSection === "creds" && (
            <CredsSection principalKind="mcp_server" principalId={serverId} />
          )}

          {activeSection === "logs" && (
            <div className="space-y-4">
              <div className="mb-5 pb-3 border-b border-border flex items-end justify-between gap-3">
                <div>
                  <h3 className="text-lg font-semibold text-text-primary">Logs</h3>
                  <p className="text-sm text-text-tertiary mt-1">Server stderr output</p>
                </div>
                <label className="flex items-center gap-2 text-xs text-text-tertiary shrink-0">
                  <input
                    type="checkbox"
                    checked={followLogs}
                    onChange={(e) => setFollowLogs(e.target.checked)}
                    className="h-3.5 w-3.5 rounded border-border text-accent focus:ring-accent"
                  />
                  Follow
                </label>
              </div>
              {logsLoading && !logs && (
                <div className="flex items-center justify-center py-12">
                  <div className="h-5 w-5 animate-spin rounded-full border-2 border-accent border-t-transparent" />
                </div>
              )}
              {!logsLoading && !logs && (
                <p className="text-sm text-text-tertiary py-8 text-center">No logs available. Start the server to see output.</p>
              )}
              {logs && (
                <div
                  className="rounded-xl border border-border bg-[#0d1117] p-4 max-h-[600px] overflow-y-auto overflow-x-auto"
                  onScroll={(e) => {
                    const el = e.currentTarget;
                    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 30;
                    if (followLogs !== atBottom) setFollowLogs(atBottom);
                  }}
                >
                  <pre className="text-xs font-mono text-[#c9d1d9] whitespace-pre-wrap break-words leading-5">{logs}</pre>
                  <div ref={logsEndRef} />
                </div>
              )}
            </div>
          )}

          {/* Save bar */}
          {activeSection !== "creds" && activeSection !== "about" && activeSection !== "logs" && (
            <div className="pt-4 border-t border-border flex items-center justify-end gap-2">
              <button
                onClick={() => { setDirty(false); reload(); }}
                disabled={!dirty}
                className="rounded-lg border border-border px-4 py-2 text-sm font-medium text-text-secondary hover:bg-surface-tertiary disabled:opacity-50 transition"
              >
                Discard
              </button>
              <button
                onClick={save}
                disabled={!dirty || saving}
                className="rounded-lg bg-accent px-4 py-2 text-sm font-medium text-surface hover:bg-accent-hover disabled:opacity-50 transition"
              >
                {saving ? "Saving..." : "Save"}
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default function McpPage() {
  return (
    <Suspense>
      <McpServerPage />
    </Suspense>
  );
}
