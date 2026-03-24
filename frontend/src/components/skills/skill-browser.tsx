"use client";

import { useState, useCallback, useRef, useMemo, useEffect } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import {
  MagnifyingGlassIcon,
  ArrowDownTrayIcon,
  CheckCircleIcon,
  ArrowLeftIcon,
  ExclamationTriangleIcon,
  TrashIcon,
  PuzzlePieceIcon,
} from "@heroicons/react/24/outline";
import * as Checkbox from "@radix-ui/react-checkbox";
import { CheckIcon, MinusIcon } from "@heroicons/react/16/solid";
import {
  searchSkills,
  previewSkill,
  installSkills,
  uninstallSkill,
  listInstalledSkills,
  browseRepo,
  type SkillSearchResult,
  type SkillPreview,
  type SkillListItem,
  type RepoBrowseResult,
  type RepoBrowseSkill,
} from "@/lib/api-client";

interface SkillBrowserProps {
  agentId?: string;
}

function MetadataTable({ meta }: { meta: Record<string, string> }) {
  const entries = Object.entries(meta);
  if (entries.length === 0) return null;

  return (
    <div className="rounded-lg border border-border bg-surface-secondary overflow-hidden">
      <table className="w-full text-sm">
        <tbody>
          {entries.map(([key, value]) => (
            <tr key={key} className="border-b border-border last:border-b-0">
              <td className="px-4 py-2 font-medium text-text-secondary whitespace-nowrap align-top w-32 bg-surface-tertiary/50">
                {key}
              </td>
              <td className="px-4 py-2 text-text-primary break-words prose prose-sm max-w-none text-[var(--text-primary)] prose-p:my-1 prose-ul:my-1 prose-li:my-0 prose-headings:my-1 [&>*:first-child]:mt-0 [&>*:last-child]:mb-0">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{value}</ReactMarkdown>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function SkillBrowser({ agentId }: SkillBrowserProps) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SkillSearchResult[]>([]);
  const [preview, setPreview] = useState<SkillPreview | null>(null);
  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [searching, setSearching] = useState(false);
  const [installing, setInstalling] = useState<string | null>(null);
  const [uninstalling, setUninstalling] = useState<string | null>(null);
  const [uninstallingBatch, setUninstallingBatch] = useState(false);
  const [selectedInstalled, setSelectedInstalled] = useState(false);
  const [confirmInstall, setConfirmInstall] = useState<{ repo: string; names: string[] } | null>(null);
  const [confirmUninstall, setConfirmUninstall] = useState<string[] | null>(null);
  const [reviewed, setReviewed] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [installed, setInstalled] = useState<SkillListItem[]>([]);
  const [repoBrowse, setRepoBrowse] = useState<RepoBrowseResult | null>(null);
  const [installingAll, setInstallingAll] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const debounceRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  const loadInstalled = useCallback(async () => {
    try {
      const items = await listInstalledSkills();
      setInstalled(items);
    } catch {
      // ignore
    }
  }, []);

  useEffect(() => { loadInstalled(); }, [loadInstalled]);

  const installableSkills = useMemo(
    () => repoBrowse?.skills.filter((s) => !s.installed) ?? [],
    [repoBrowse],
  );

  const toggleSelect = useCallback((name: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name); else next.add(name);
      return next;
    });
  }, []);

  const toggleSelectAll = useCallback((items: { name: string }[]) => {
    setSelected((prev) => {
      if (prev.size === items.length) return new Set();
      return new Set(items.map((s) => s.name));
    });
  }, []);

  const previewMeta = useMemo(() => {
    if (!preview) return {};
    return {
      name: preview.name,
      ...(preview.description ? { description: preview.description } : {}),
      ...preview.metadata,
    };
  }, [preview]);

  const parseRepoPath = (q: string): string | null => {
    const trimmed = q.trim().replace(/\/+$/, "");
    const ghMatch = trimmed.match(/^https?:\/\/github\.com\/([a-zA-Z0-9._-]+\/[a-zA-Z0-9._-]+)/);
    if (ghMatch) return ghMatch[1];
    if (/^[a-zA-Z0-9._-]+\/[a-zA-Z0-9._-]+$/.test(trimmed)) return trimmed;
    return null;
  };

  const handleBrowseRepo = useCallback(async (repo: string) => {
    setSearching(true);
    setRepoBrowse(null);
    setResults([]);
    setError(null);
    setSelected(new Set());
    try {
      const result = await browseRepo(repo);
      setRepoBrowse(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to browse repo");
    } finally {
      setSearching(false);
    }
  }, []);

  const handleInstallAll = useCallback(async (repo: string, skills: RepoBrowseSkill[]) => {
    const toInstall = skills.filter((s) => !s.installed);
    if (toInstall.length === 0) return;
    setInstallingAll(true);
    setError(null);
    try {
      await installSkills(repo, toInstall.map((s) => s.name), agentId);
      setRepoBrowse((prev) => prev ? { ...prev, skills: prev.skills.map((s) => ({ ...s, installed: true })) } : prev);
      loadInstalled();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Install failed");
    } finally {
      setInstallingAll(false);
    }
  }, [agentId, loadInstalled]);

  const handleSearch = useCallback(async (q: string) => {
    setQuery(q);
    setError(null);
    setRepoBrowse(null);
    setSelected(new Set());

    if (debounceRef.current) clearTimeout(debounceRef.current);

    if (q.trim().length < 2) {
      setResults([]);
      return;
    }

    debounceRef.current = setTimeout(async () => {
      const repo = parseRepoPath(q);
      if (repo) {
        handleBrowseRepo(repo);
        return;
      }
      setSearching(true);
      try {
        const r = await searchSkills(q.trim());
        setResults(r);
      } catch (err) {
        setError(err instanceof Error ? err.message : "Search failed");
      } finally {
        setSearching(false);
      }
    }, 300);
  }, [handleBrowseRepo]);

  const handleSelect = useCallback(async (result: SkillSearchResult) => {
    setSelectedName(result.name);
    setSelectedInstalled(installed.some((s) => s.name === result.name && s.source === result.repo));
    setPreview(null);
    setError(null);
    try {
      const p = await previewSkill(result.repo, result.name);
      setPreview(p);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load preview");
    }
  }, [installed]);

  const handleInstall = useCallback(async (repo: string, name: string) => {
    setInstalling(name);
    setError(null);
    try {
      await installSkills(repo, [name], agentId);
      setResults((prev) =>
        prev.map((r) => (r.name === name && r.repo === repo ? { ...r, installed: true } : r))
      );
      setRepoBrowse((prev) => prev ? { ...prev, skills: prev.skills.map((s) => (s.name === name ? { ...s, installed: true } : s)) } : prev);
      if (preview?.name === name) {
        setPreview(null);
        setSelectedName(null);
      }
      loadInstalled();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Install failed");
    } finally {
      setInstalling(null);
    }
  }, [agentId, preview, loadInstalled]);

  const handleUninstall = useCallback(async (name: string) => {
    setUninstalling(name);
    setError(null);
    try {
      await uninstallSkill(name);
      setResults((prev) =>
        prev.map((r) => (r.name === name ? { ...r, installed: false } : r))
      );
      setSelectedInstalled(false);
      setSelectedName(null);
      setPreview(null);
      loadInstalled();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Uninstall failed");
    } finally {
      setUninstalling(null);
    }
  }, [loadInstalled]);

  const handleUninstallBatch = useCallback(async (names: string[]) => {
    if (names.length === 0) return;
    setUninstallingBatch(true);
    setError(null);
    try {
      for (const name of names) {
        await uninstallSkill(name);
      }
      setSelected(new Set());
      loadInstalled();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Uninstall failed");
    } finally {
      setUninstallingBatch(false);
    }
  }, [loadInstalled]);

  const handleBack = useCallback(() => {
    setSelectedName(null);
    setPreview(null);
  }, []);

  const confirmDialog = confirmInstall && (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50" onClick={() => setConfirmInstall(null)} />
      <div className="relative rounded-xl border border-border bg-surface-secondary p-4 space-y-4 max-w-lg w-full mx-4 shadow-xl">
        <div className="mb-5 pb-3 border-b border-border flex items-end justify-between gap-3">
          <div>
            <div className="flex items-center gap-2">
              <h3 className="text-lg font-semibold text-text-primary">
                {confirmInstall.names.length === 1 ? confirmInstall.names[0] : `${confirmInstall.names.length} skills`}
              </h3>
              <span className="rounded-full bg-surface-tertiary px-2.5 py-0.5 text-[11px] font-medium text-text-secondary uppercase tracking-wide">install</span>
            </div>
            <p className="text-sm text-text-tertiary mt-1">{confirmInstall.repo}</p>
          </div>
          <ExclamationTriangleIcon className="h-10 w-10 text-yellow-500 shrink-0" />
        </div>

        <div className="text-sm text-text-secondary space-y-2">
          <p>Community skills are not audited by Frona. Once installed, this skill is integrated into the agent&apos;s system prompt and can direct it to:</p>
          <ul className="list-disc list-inside space-y-1 pl-1">
            <li>Execute system commands</li>
            <li>Access or modify local files</li>
            <li>Make outbound network requests</li>
          </ul>
          <p>To protect your data, always verify the source code on GitHub before proceeding.</p>
        </div>

        <label className="flex items-start gap-2 cursor-pointer">
          <input
            type="checkbox"
            checked={reviewed}
            onChange={(e) => setReviewed(e.target.checked)}
            className="h-4 w-4 mt-0.5 rounded border-border accent-accent shrink-0"
          />
          <span className="text-sm text-text-secondary">
            I&apos;ve reviewed the{" "}
            <a
              href={`https://github.com/${confirmInstall.repo}`}
              target="_blank"
              rel="noopener noreferrer"
              className="text-accent hover:text-accent-hover"
              onClick={(e) => e.stopPropagation()}
            >
              source code on GitHub
              <svg className="inline h-3.5 w-3.5 ml-0.5 -mt-0.5 align-middle" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" d="M13.5 6H5.25A2.25 2.25 0 003 8.25v10.5A2.25 2.25 0 005.25 21h10.5A2.25 2.25 0 0018 18.75V10.5m-4.5-6H21m0 0v7.5m0-7.5l-9 9" />
              </svg>
            </a>
          </span>
        </label>

        <div className="flex gap-2">
          <button
            onClick={() => {
              const { repo, names } = confirmInstall;
              setConfirmInstall(null);
              setReviewed(false);
              if (names.length === 1) {
                handleInstall(repo, names[0]);
              } else {
                handleInstallAll(repo, (repoBrowse?.skills ?? []).filter((s) => names.includes(s.name)));
              }
            }}
            disabled={!reviewed}
            className="w-28 inline-flex items-center justify-center gap-1.5 rounded-lg bg-accent py-2 text-sm font-medium text-surface shadow-sm hover:bg-accent-hover transition disabled:opacity-50"
          >
            <ArrowDownTrayIcon className="h-4 w-4" />
            Install
          </button>
          <button
            onClick={() => setConfirmInstall(null)}
            className="w-28 inline-flex items-center justify-center gap-1.5 rounded-lg border border-border py-2 text-sm font-medium text-text-secondary shadow-sm hover:bg-surface-tertiary transition"
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );

  const uninstallDialog = confirmUninstall && (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50" onClick={() => setConfirmUninstall(null)} />
      <div className="relative rounded-xl border border-border bg-surface-secondary p-4 space-y-4 max-w-lg w-full mx-4 shadow-xl">
        <div className="mb-5 pb-3 border-b border-border flex items-end justify-between gap-3">
          <div>
            <div className="flex items-center gap-2">
              <h3 className="text-lg font-semibold text-text-primary">
                {confirmUninstall.length === 1 ? confirmUninstall[0] : `${confirmUninstall.length} skills`}
              </h3>
              <span className="rounded-full bg-surface-tertiary px-2.5 py-0.5 text-[11px] font-medium text-text-secondary uppercase tracking-wide">uninstall</span>
            </div>
          </div>
          <TrashIcon className="h-10 w-10 text-danger shrink-0" />
        </div>

        <div className="text-sm text-text-secondary space-y-2">
          <p>
            {confirmUninstall.length === 1
              ? "This will remove the skill and all its files. The agent will no longer have access to it."
              : `This will remove ${confirmUninstall.length} skills and all their files. The agent will no longer have access to them.`}
          </p>
        </div>

        <div className="flex gap-2">
          <button
            onClick={() => {
              const names = confirmUninstall;
              setConfirmUninstall(null);
              if (names.length === 1) {
                handleUninstall(names[0]);
              } else {
                handleUninstallBatch(names);
              }
            }}
            className="w-28 inline-flex items-center justify-center gap-1.5 rounded-lg border border-border py-2 text-sm font-medium text-danger hover:bg-surface-tertiary transition"
          >
            <TrashIcon className="h-4 w-4" />
            Uninstall
          </button>
          <button
            onClick={() => setConfirmUninstall(null)}
            className="w-28 inline-flex items-center justify-center gap-1.5 rounded-lg border border-border py-2 text-sm font-medium text-text-secondary shadow-sm hover:bg-surface-tertiary transition"
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );

  if (selectedName) {
    return (
      <div className="space-y-4">
        {confirmDialog}
      {uninstallDialog}
        <button
          onClick={handleBack}
          className="inline-flex items-center gap-1.5 text-sm text-text-secondary hover:text-text-primary transition"
        >
          <ArrowLeftIcon className="h-4 w-4" />
          Back to results
        </button>

        {error && (
          <div className="rounded-lg bg-error-bg p-3 text-sm text-error-text">{error}</div>
        )}

        {!preview && !error && (
          <div className="flex items-center justify-center py-12">
            <div className="h-5 w-5 animate-spin rounded-full border-2 border-accent border-t-transparent" />
          </div>
        )}

        {preview && (
          <div className="space-y-4">
            <div className="flex items-start justify-between gap-4">
              <div className="flex items-center gap-3">
                {preview.avatar_url && (
                  <img
                    src={preview.avatar_url}
                    alt=""
                    className="h-10 w-10 rounded-lg"
                  />
                )}
                <div>
                  <h3 className="text-base font-semibold text-text-primary">{preview.name}</h3>
                  <p className="text-sm text-text-tertiary">{preview.repo}</p>
                </div>
              </div>
              <div className="shrink-0 flex items-center gap-2">
                <a
                  href={preview.github_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex items-center gap-1.5 rounded-lg border border-border px-3 py-2 text-sm font-medium text-text-secondary hover:bg-surface-tertiary transition"
                >
                  <svg className="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
                    <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z" />
                  </svg>
                  GitHub
                </a>
                {selectedInstalled ? (
                  <button
                    onClick={() => setConfirmUninstall([preview.name])}
                    disabled={uninstalling === preview.name}
                    className="inline-flex items-center gap-1.5 rounded-lg border border-border px-4 py-2 text-sm font-medium text-danger hover:bg-surface-tertiary disabled:opacity-50 transition"
                  >
                    <TrashIcon className="h-4 w-4" />
                    {uninstalling === preview.name ? "Removing..." : "Uninstall"}
                  </button>
                ) : (
                  <button
                    onClick={() => { setReviewed(false); setConfirmInstall({ repo: preview.repo, names: [preview.name] }); }}
                    disabled={installing === preview.name}
                    className="inline-flex items-center gap-1.5 rounded-lg bg-accent px-4 py-2 text-sm font-medium text-surface hover:bg-accent-hover disabled:opacity-50 transition"
                  >
                    <ArrowDownTrayIcon className="h-4 w-4" />
                    {installing === preview.name ? "Installing..." : "Install"}
                  </button>
                )}
              </div>
            </div>

            <MetadataTable meta={previewMeta} />

            <div className="prose prose-sm max-w-none text-[var(--text-primary)] prose-headings:text-[var(--text-primary)] prose-strong:text-[var(--text-primary)] prose-a:text-[var(--accent)] prose-code:text-[var(--text-primary)] prose-code:before:content-none prose-code:after:content-none prose-pre:bg-transparent prose-pre:p-0 prose-blockquote:text-[var(--text-secondary)] prose-blockquote:border-[var(--border)] prose-th:border-[var(--border)] prose-td:border-[var(--border)]">
              <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                components={{
                  a({ href, children, ...props }) {
                    const resolved = href && !href.startsWith("http") && !href.startsWith("#")
                      ? `${preview.github_url}/${href}`
                      : href;
                    return <a href={resolved} target="_blank" rel="noopener noreferrer" {...props}>{children}</a>;
                  },
                  img({ src, alt, ...props }) {
                    const resolved = typeof src === "string" && !src.startsWith("http")
                      ? `${preview.raw_base_url}/${src}`
                      : src;
                    return <img src={resolved as string} alt={alt} {...props} />;
                  },
                  code({ className, children, ...props }) {
                    const match = /language-(\w+)/.exec(className || "");
                    const code = String(children).replace(/\n$/, "");
                    if (match) {
                      return (
                        <SyntaxHighlighter
                          language={match[1]}
                          PreTag="div"
                          style={{
                            ...oneDark,
                            'pre[class*="language-"]': { ...oneDark['pre[class*="language-"]'], background: "var(--surface-nav)" },
                            'code[class*="language-"]': { ...oneDark['code[class*="language-"]'], background: "var(--surface-nav)" },
                          }}
                          customStyle={{ margin: 0, borderRadius: "0.5rem", fontSize: "0.8125rem" }}
                        >
                          {code}
                        </SyntaxHighlighter>
                      );
                    }
                    return (
                      <code className={className} {...props}>
                        {children}
                      </code>
                    );
                  },
                }}
              >
                {preview.body}
              </ReactMarkdown>
            </div>
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {confirmDialog}
      {uninstallDialog}
      <div className="relative">
        <MagnifyingGlassIcon className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-text-tertiary" />
        <input
          type="text"
          value={query}
          onChange={(e) => handleSearch(e.target.value)}
          placeholder="Search skills or enter owner/repo..."
          className="w-full rounded-lg border border-border bg-surface pl-9 pr-3 py-2 text-sm text-text-primary placeholder:text-text-tertiary focus:border-accent focus:outline-none"
        />
        {searching && (
          <div className="absolute right-3 top-1/2 -translate-y-1/2">
            <div className="h-4 w-4 animate-spin rounded-full border-2 border-accent border-t-transparent" />
          </div>
        )}
      </div>

      {error && (
        <div className="rounded-lg bg-error-bg p-3 text-sm text-error-text">{error}</div>
      )}

      {results.length > 0 && (
        <div className="rounded-xl border border-border bg-surface-secondary divide-y divide-border">
          {results.map((result) => (
            <button
              key={`${result.repo}/${result.name}`}
              onClick={() => handleSelect(result)}
              className="w-full text-left px-4 py-3 flex items-center gap-3 transition hover:bg-surface-tertiary cursor-pointer"
            >
              {result.avatar_url && (
                <img
                  src={result.avatar_url}
                  alt=""
                  className="h-8 w-8 rounded-lg shrink-0"
                />
              )}
              <div className="flex-1 min-w-0">
                <div className="text-sm font-medium text-text-primary truncate">{result.name}</div>
                <div className="text-xs text-text-tertiary truncate">{result.repo}</div>
              </div>
              <div className="shrink-0 flex items-center gap-2">
                {result.installs > 0 && (
                  <span className="text-xs text-text-tertiary">
                    {result.installs.toLocaleString()} installs
                  </span>
                )}
                {result.installed ? (
                  <CheckCircleIcon className="h-5 w-5 text-green-500" />
                ) : (
                  <ArrowDownTrayIcon className="h-4 w-4 text-text-tertiary" />
                )}
              </div>
            </button>
          ))}
        </div>
      )}

      {query.trim().length >= 2 && !searching && results.length === 0 && !repoBrowse && (
        <p className="text-sm text-text-tertiary text-center py-8">No skills found</p>
      )}

      {repoBrowse && repoBrowse.skills.length > 0 && (
        <div>
          <div className="flex items-center justify-between mb-4 min-h-[36px]">
            <div className="flex items-center gap-3 min-w-0">
              <img src={repoBrowse.avatar_url} alt="" className="h-10 w-10 rounded-lg shrink-0" />
              <div className="min-w-0">
                <h4 className="text-base font-medium text-text-primary">{repoBrowse.repo}</h4>
                {repoBrowse.description && (
                  <p className="text-xs text-text-tertiary truncate">{repoBrowse.description}</p>
                )}
              </div>
            </div>
            <button
              onClick={() => {
                const names = selected.size > 0
                  ? Array.from(selected)
                  : installableSkills.map((s) => s.name);
                if (names.length > 0) {
                  setReviewed(false);
                  setConfirmInstall({ repo: repoBrowse.repo, names });
                }
              }}
              disabled={installingAll || installableSkills.length === 0}
              className={`inline-flex items-center gap-1.5 rounded-lg bg-accent px-3 py-1.5 text-xs font-medium text-surface shadow-sm hover:bg-accent-hover disabled:opacity-50 transition shrink-0 ${selected.size === 0 ? "invisible" : ""}`}
            >
              <ArrowDownTrayIcon className="h-3.5 w-3.5" />
              {installingAll
                ? "Installing..."
                : selected.size > 0
                  ? `Install ${selected.size} selected`
                  : "Install all"}
            </button>
          </div>
          <div className="rounded-xl border border-border bg-surface-secondary divide-y divide-border">
            {installableSkills.length > 0 && (
              <div className="px-4 py-2 flex items-center gap-3 bg-surface-tertiary/30">
                <Checkbox.Root
                  checked={selected.size === installableSkills.length ? true : selected.size > 0 ? "indeterminate" : false}
                  onCheckedChange={() => toggleSelectAll(installableSkills)}
                  className="h-4 w-4 rounded border border-border bg-surface flex items-center justify-center data-[state=checked]:bg-accent data-[state=checked]:border-accent data-[state=indeterminate]:bg-accent data-[state=indeterminate]:border-accent transition shrink-0"
                >
                  <Checkbox.Indicator>
                    {selected.size === installableSkills.length
                      ? <CheckIcon className="h-3 w-3 text-surface" />
                      : <MinusIcon className="h-3 w-3 text-surface" />}
                  </Checkbox.Indicator>
                </Checkbox.Root>
                <span className="text-xs text-text-secondary">
                  {selected.size > 0 && `${selected.size} of ${installableSkills.length} selected`}
                </span>
              </div>
            )}
            {repoBrowse.skills.map((skill) => (
              <div
                key={skill.name}
                className="px-4 py-3 flex items-center gap-3 group"
              >
                {skill.installed ? (
                  <CheckCircleIcon className="h-4 w-4 text-green-500 shrink-0" />
                ) : (
                  <Checkbox.Root
                    checked={selected.has(skill.name)}
                    onCheckedChange={() => toggleSelect(skill.name)}
                    className="h-4 w-4 rounded border border-border bg-surface flex items-center justify-center data-[state=checked]:bg-accent data-[state=checked]:border-accent transition shrink-0"
                  >
                    <Checkbox.Indicator>
                      <CheckIcon className="h-3 w-3 text-surface" />
                    </Checkbox.Indicator>
                  </Checkbox.Root>
                )}
                <img
                  src={repoBrowse.avatar_url}
                  alt=""
                  className="h-8 w-8 rounded-lg shrink-0"
                />
                <div
                  className="flex-1 min-w-0 cursor-pointer"
                  onClick={() => handleSelect({ name: skill.name, repo: repoBrowse.repo, avatar_url: repoBrowse.avatar_url, installs: 0, installed: skill.installed })}
                >
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium text-text-primary truncate hover:text-accent transition">{skill.name}</span>
                  </div>
                  {skill.description && (
                    <div className="text-xs text-text-tertiary line-clamp-3">{skill.description}</div>
                  )}
                </div>
                {skill.installed && (
                  <CheckCircleIcon className="h-5 w-5 text-green-500 shrink-0" />
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {query.trim().length < 2 && installed.length > 0 && (
        <div>
          <div className="flex items-center justify-between mb-2 min-h-[36px]">
            <h4 className="text-base font-medium text-text-secondary">Installed</h4>
            <button
              onClick={() => setConfirmUninstall(Array.from(selected))}
              disabled={uninstallingBatch || selected.size === 0}
              className={`inline-flex items-center gap-1.5 rounded-lg border border-border px-3 py-1.5 text-xs font-medium text-danger hover:bg-surface-tertiary disabled:opacity-50 transition ${selected.size === 0 ? "invisible" : ""}`}
            >
              <TrashIcon className="h-3.5 w-3.5" />
              {uninstallingBatch ? "Removing..." : `Uninstall ${selected.size} selected`}
            </button>
          </div>
          <div className="rounded-xl border border-border bg-surface-secondary divide-y divide-border">
            <div className="px-4 py-2 flex items-center gap-3 bg-surface-tertiary/30">
              <Checkbox.Root
                checked={selected.size === installed.length ? true : selected.size > 0 ? "indeterminate" : false}
                onCheckedChange={() => toggleSelectAll(installed)}
                className="h-4 w-4 rounded border border-border bg-surface flex items-center justify-center data-[state=checked]:bg-accent data-[state=checked]:border-accent data-[state=indeterminate]:bg-accent data-[state=indeterminate]:border-accent transition shrink-0"
              >
                <Checkbox.Indicator>
                  {selected.size === installed.length
                    ? <CheckIcon className="h-3 w-3 text-surface" />
                    : <MinusIcon className="h-3 w-3 text-surface" />}
                </Checkbox.Indicator>
              </Checkbox.Root>
              <span className="text-xs text-text-secondary">
                {selected.size > 0 && `${selected.size} of ${installed.length} selected`}
              </span>
            </div>
            {installed.map((skill) => {
              const owner = skill.source?.split("/")[0];
              return (
                <div
                  key={skill.name}
                  className="px-4 py-3 flex items-center gap-3 transition hover:bg-surface-tertiary"
                >
                  <Checkbox.Root
                    checked={selected.has(skill.name)}
                    onCheckedChange={() => toggleSelect(skill.name)}
                    className="h-4 w-4 rounded border border-border bg-surface flex items-center justify-center data-[state=checked]:bg-accent data-[state=checked]:border-accent transition shrink-0"
                  >
                    <Checkbox.Indicator>
                      <CheckIcon className="h-3 w-3 text-surface" />
                    </Checkbox.Indicator>
                  </Checkbox.Root>
                  {owner ? (
                    <img
                      src={`https://github.com/${owner}.png`}
                      alt=""
                      className="h-8 w-8 rounded-lg shrink-0"
                    />
                  ) : (
                    <PuzzlePieceIcon className="h-8 w-8 text-text-tertiary shrink-0" />
                  )}
                  <div
                    className="flex-1 min-w-0 cursor-pointer"
                    onClick={() => handleSelect({ name: skill.name, repo: skill.source || "", avatar_url: owner ? `https://github.com/${owner}.png` : "", installs: 0, installed: true })}
                  >
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium text-text-primary truncate hover:text-accent transition">{skill.name}</span>
                      {skill.source && (
                        <span className="rounded-full bg-surface-tertiary px-2 py-0.5 text-[11px] text-text-tertiary whitespace-nowrap">{skill.source}</span>
                      )}
                    </div>
                    {skill.description && (
                      <div className="text-xs text-text-tertiary line-clamp-3">{skill.description}</div>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {query.trim().length < 2 && installed.length === 0 && (
        <p className="text-sm text-text-tertiary text-center py-8">
          Search skills or enter a GitHub repo (e.g. anthropics/skills)
        </p>
      )}
    </div>
  );
}
