"use client";

import { memo, useState, type ReactNode } from "react";
import { ChevronDownIcon, DocumentIcon } from "@heroicons/react/24/outline";
import { PuffLoader } from "react-spinners";
import {
  type ToolCallMessagePartStatus,
  type ToolCallMessagePartComponent,
} from "@assistant-ui/react";
import { AnimatePresence, motion } from "motion/react";
import ReactMarkdown from "react-markdown";
import { cn } from "@/lib/utils";
import { useToolTimeline } from "./tool-timeline-context";
import { InlineCode } from "./inline-code";
import { CodeBlock } from "@/components/ui/code-block";

const FILE_TOOLS = new Set(["read", "write", "edit", "glob", "grep"]);

const EXT_TO_LANG: Record<string, string> = {
  ts: "ts", tsx: "tsx", js: "js", jsx: "jsx", mjs: "js", cjs: "js",
  py: "python", rs: "rust", go: "go", rb: "ruby", php: "php",
  java: "java", kt: "kotlin", swift: "swift", scala: "scala",
  c: "c", h: "c", cpp: "cpp", cc: "cpp", hpp: "cpp", cxx: "cpp",
  cs: "csharp", m: "objc", mm: "objc",
  sh: "bash", bash: "bash", zsh: "bash", fish: "fish",
  md: "markdown", mdx: "mdx", txt: "text", log: "text",
  json: "json", yaml: "yaml", yml: "yaml", toml: "toml", xml: "xml",
  html: "html", htm: "html", css: "css", scss: "scss", sass: "sass",
  sql: "sql", graphql: "graphql", gql: "graphql",
  lua: "lua", dart: "dart", r: "r", jl: "julia",
  vue: "vue", svelte: "svelte",
  dockerfile: "docker",
};

function inferLanguageFromPath(path: string | null | undefined): string {
  if (!path) return "text";
  const base = path.split("/").pop() ?? path;
  if (base.toLowerCase() === "dockerfile") return "docker";
  const dot = base.lastIndexOf(".");
  if (dot < 0) return "text";
  const ext = base.slice(dot + 1).toLowerCase();
  return EXT_TO_LANG[ext] ?? "text";
}

function getFileToolSubtitle(toolName: string, args: unknown): string | null {
  if (!FILE_TOOLS.has(toolName) || !args || typeof args !== "object") return null;
  const a = args as Record<string, unknown>;
  if (toolName === "glob" || toolName === "grep") {
    const pattern = typeof a.pattern === "string" ? a.pattern : null;
    const scope = typeof a.path === "string" ? a.path : null;
    if (pattern && scope && scope !== ".") return `${pattern} in ${scope}`;
    return pattern;
  }
  return typeof a.path === "string" ? a.path : null;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

function ProduceFileExpanded({ args, result }: { args: unknown; result: unknown }) {
  const a = (args && typeof args === "object" ? args : {}) as Record<string, unknown>;
  const path = typeof a.path === "string" ? a.path : null;

  let parsed: Record<string, unknown> | null = null;
  if (typeof result === "string") {
    try {
      const v = JSON.parse(result);
      if (v && typeof v === "object") parsed = v as Record<string, unknown>;
    } catch {}
  } else if (result && typeof result === "object") {
    parsed = result as Record<string, unknown>;
  }

  const filename =
    parsed && typeof parsed.filename === "string" ? parsed.filename : path;
  const contentType =
    parsed && typeof parsed.content_type === "string" ? parsed.content_type : null;
  const sizeBytes =
    parsed && typeof parsed.size_bytes === "number" ? parsed.size_bytes : null;

  return (
    <div className="flex items-center gap-3 p-3 text-xs">
      <DocumentIcon className="h-8 w-8 shrink-0 text-text-tertiary" />
      <div className="flex flex-col gap-0.5 min-w-0">
        {filename && (
          <div className="font-mono text-text-primary truncate">{filename}</div>
        )}
        {(contentType || sizeBytes !== null) && (
          <div className="flex gap-3 text-text-tertiary">
            {sizeBytes !== null && <span>{formatBytes(sizeBytes)}</span>}
            {contentType && <span>{contentType}</span>}
          </div>
        )}
      </div>
    </div>
  );
}

function ShellExpanded({ args, result }: { args: unknown; result: unknown }) {
  const a = (args && typeof args === "object" ? args : {}) as Record<string, unknown>;
  const command = typeof a.command === "string" ? a.command : "";
  const resultText =
    typeof result === "string"
      ? result
      : result !== undefined
        ? JSON.stringify(result, null, 2)
        : "";
  return (
    <div className="flex flex-col gap-2">
      {command && <CodeBlock code={command} language="bash" wrap />}
      {resultText && (
        <pre className="whitespace-pre-wrap text-text-secondary text-xs overflow-x-auto px-3 pb-3">
          {resultText}
        </pre>
      )}
    </div>
  );
}

function globLiteralsToRegex(pattern: string): RegExp | null {
  try {
    const literals = pattern
      .split(/\*\*|\*|\?|\//)
      .map((p) => p.trim())
      .filter((p) => p.length >= 2);
    if (literals.length === 0) return null;
    const escaped = [...new Set(literals)].map((p) =>
      p.replace(/[.+^${}()|[\]\\]/g, "\\$&"),
    );
    return new RegExp(`(?:${escaped.join("|")})`, "g");
  } catch {
    return null;
  }
}

function highlightMatches(text: string, re: RegExp | null): ReactNode {
  if (!re) return text;
  const parts: ReactNode[] = [];
  let last = 0;
  re.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    if (m.index > last) parts.push(text.slice(last, m.index));
    parts.push(
      <span
        key={`m-${m.index}`}
        className="text-info font-semibold"
      >
        {m[0]}
      </span>,
    );
    last = m.index + m[0].length;
    if (m[0].length === 0) re.lastIndex++;
  }
  if (last < text.length) parts.push(text.slice(last));
  return parts.length > 0 ? parts : text;
}

function FileToolExpanded({
  toolName,
  args,
  result,
  description,
}: {
  toolName: string;
  args: unknown;
  result: unknown;
  description: string | null;
}) {
  const a = (args && typeof args === "object" ? args : {}) as Record<string, unknown>;
  const path = typeof a.path === "string" ? a.path : null;
  const lang = inferLanguageFromPath(path);
  const showPath = path && path !== description;

  if (toolName === "write") {
    const content = typeof a.content === "string" ? a.content : "";
    return (
      <div className="flex flex-col gap-2">
        {showPath && <p className="font-mono text-xs text-text-tertiary">{path}</p>}
        <CodeBlock code={content} language={lang} lineNumbers />
      </div>
    );
  }

  if (toolName === "edit") {
    const text = typeof result === "string" ? result : "";
    const marker = "Surrounding context:\n";
    const markerIdx = text.indexOf(marker);
    const summary = markerIdx >= 0 ? text.slice(0, markerIdx).trim() : null;
    const snippet = markerIdx >= 0 ? text.slice(markerIdx + marker.length) : null;
    return (
      <div className="flex flex-col gap-2">
        {showPath && <p className="font-mono text-xs text-text-tertiary">{path}</p>}
        {summary && (
          <p className="px-3 pt-2 text-xs text-text-tertiary">{summary}</p>
        )}
        {snippet !== null ? (
          <CodeBlock code={snippet} language={lang} />
        ) : (
          <pre className="whitespace-pre-wrap text-text-secondary text-xs overflow-x-auto px-3 py-2">
            {text}
          </pre>
        )}
      </div>
    );
  }

  if (toolName === "read") {
    const text = typeof result === "string" ? result : null;
    return (
      <div className="flex flex-col gap-2">
        {showPath && <p className="font-mono text-xs text-text-tertiary">{path}</p>}
        {text !== null && <CodeBlock code={text} language={lang} lineNumbers />}
      </div>
    );
  }

  const text =
    typeof result === "string"
      ? result
      : result !== undefined
        ? JSON.stringify(result, null, 2)
        : "";

  const truncMatch = text.match(/\n\n\[truncated[^\]]+\]$/);
  const trailing = truncMatch?.[0]?.trim() ?? null;
  const body = trailing ? text.slice(0, text.length - truncMatch![0].length) : text;
  const lines = body.split("\n").filter((l) => l.length > 0);

  const pattern = typeof a.pattern === "string" ? a.pattern : null;
  let matchRe: RegExp | null = null;
  if (toolName === "grep" && pattern) {
    try {
      matchRe = new RegExp(pattern, "g");
    } catch {
      matchRe = null;
    }
  } else if (toolName === "glob" && pattern) {
    matchRe = globLiteralsToRegex(pattern);
  }

  const grepLine = /^(.+?):(\d+):(.*)$/;
  return (
    <pre className="font-mono text-xs text-text-secondary bg-surface-nav rounded-b-md overflow-auto p-3 m-0 leading-relaxed">
      {lines.map((line, i) => {
        const m = toolName === "grep" ? line.match(grepLine) : null;
        if (m) {
          return (
            <div key={i} className="flex gap-3 whitespace-nowrap">
              <span className="text-text-tertiary shrink-0">
                {m[1]}:{m[2]}
              </span>
              <span>{highlightMatches(m[3], matchRe)}</span>
            </div>
          );
        }
        return (
          <div key={i} className="whitespace-nowrap">
            {highlightMatches(line, matchRe)}
          </div>
        );
      })}
      {trailing && (
        <div className="mt-2 italic text-text-tertiary whitespace-nowrap">
          {trailing}
        </div>
      )}
    </pre>
  );
}

const ANIMATION_DURATION = 200;

const TOOL_DISPLAY_NAMES: Record<string, string> = {
  web_fetch: "Web Fetch",
  web_search: "Web Search",
  cli: "Terminal",
  shell: "Shell",
  python: "Python",
  browser_navigate: "Browser",
  manage_app: "App",
  request_credentials: "Request Credentials",
  produce_file: "Produce File",
  store_agent_memory: "Remember",
  store_user_memory: "Remember",
  create_task: "Create Task",
  list_tasks: "List Tasks",
  delete_task: "Delete Task",
  task_control: "Task Control",
  complete_task: "Complete Task",
  defer_task: "Defer Task",
  fail_task: "Fail Task",
  update_identity: "Update Identity",
  update_entity: "Update Entity",
  set_heartbeat: "Set Heartbeat",
  notify_human: "Notify",
  request_user_takeover: "Request Takeover",
  ask_user_question: "Ask Question",
  make_voice_call: "Voice Call",
  send_dtmf: "Send DTMF",
  hangup_call: "Hang Up",
};

function displayToolName(name: string): string {
  if (TOOL_DISPLAY_NAMES[name]) return TOOL_DISPLAY_NAMES[name];
  const bare = name.includes("__") ? name.split("__").pop()! : name;
  return bare.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

type ToolStatus = ToolCallMessagePartStatus["type"];


function TimelineDot({
  status,
  isCancelled,
  index,
}: {
  status: ToolStatus;
  isCancelled: boolean;
  index: number;
}) {
  const isRunning = status === "running";
  const isComplete = status === "complete";
  const isFailed = (status === "incomplete" && !isCancelled) || status === "requires-action";

  return (
    <div className="absolute left-0 top-[-3px] z-10 h-6 w-6 flex items-center justify-center">
      <AnimatePresence mode="wait" initial={false}>
        {isRunning ? (
          <motion.div
            key="loader"
            initial={{ opacity: 0, scale: 0.5 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.5 }}
            transition={{ duration: 0.2 }}
          >
            <PuffLoader
              color="var(--text-tertiary)"
              size={24}
              speedMultiplier={0.8}
            />
          </motion.div>
        ) : (
          <motion.div
            key="number"
            initial={{ opacity: 0, scale: 0.5 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.5 }}
            transition={{ duration: 0.2 }}
            className={cn(
              "flex h-6 w-6 items-center justify-center rounded-full text-[10px] font-semibold leading-none",
              isComplete && "bg-success/20 text-success",
              isFailed && "bg-danger/20 text-danger",
              isCancelled && "bg-surface-tertiary text-text-tertiary",
            )}
          >
            {index}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

const ToolFallbackImpl: ToolCallMessagePartComponent = ({
  toolName,
  toolCallId,
  args,
  argsText,
  result,
  status,
}) => {
  const timeline = useToolTimeline();
  const [isOpen, setIsOpen] = useState(false);

  if (timeline && !timeline.isVisible(toolCallId)) return null;

  const isCancelled =
    status?.type === "incomplete" && status.reason === "cancelled";
  const rawDescription =
    typeof args?.description === "string" ? args.description : null;
  const userDescription =
    rawDescription && rawDescription !== toolName ? rawDescription : null;
  const description = userDescription ?? getFileToolSubtitle(toolName, args);
  const isFileTool = FILE_TOOLS.has(toolName);
  const isShellTool = toolName === "shell";
  const isProduceFileTool = toolName === "produce_file";
  const turnText =
    typeof args?.turnText === "string" ? args.turnText : null;
  const isToolError = args?.isError === true;
  const statusType = isToolError ? "incomplete" : (status?.type ?? "complete");
  const isLast = timeline ? timeline.isLastVisible(toolCallId) : false;
  const isFirst = timeline ? timeline.isFirstVisible(toolCallId) : false;
  const toolIndex = timeline ? timeline.getToolIndex(toolCallId) : 0;
  const hiddenCount = timeline?.hiddenCount ?? 0;

  const errorText =
    status?.type === "incomplete"
      ? (() => {
          const error = (status as { error?: unknown }).error;
          if (!error) return null;
          return typeof error === "string" ? error : JSON.stringify(error);
        })()
      : null;

  return (
    <>
      {isFirst && hiddenCount > 0 && (
        <div className="relative pl-8 pb-3 mt-3 flex items-center min-h-6">
          <div className="absolute left-[11px] top-[21px] bottom-0 w-px bg-border" />
          <div className="absolute left-0 z-10 flex h-6 w-6 items-center justify-center rounded-full bg-surface-tertiary text-[10px] font-semibold text-text-tertiary">
            +{hiddenCount}
          </div>
          <span className="text-sm text-text-tertiary leading-none">
            tools used
          </span>
        </div>
      )}
      {turnText && (
        <div className={cn("relative pb-2 flex items-start", isFirst && hiddenCount === 0 && "mt-3")}>
          <div className="absolute left-[11px] top-0 bottom-0 w-px bg-border" />
          <div className="inline-block rounded-r-md bg-surface-tertiary pl-4 pr-3 py-1.5 text-xs text-text-secondary leading-none [&_p]:m-0" style={{ marginLeft: "11px" }}>
            <ReactMarkdown components={{
              pre: ({ children }) => <>{children}</>,
              code: ({ className, children }) => {
                const lang = className?.replace("language-", "");
                const code = String(children).replace(/\n$/, "");
                if (!className) return <code className="text-xs">{children}</code>;
                return <InlineCode code={code} language={lang} />;
              },
            }}>{turnText}</ReactMarkdown>
          </div>
        </div>
      )}
      <motion.div
      initial={{ opacity: 0, height: 0 }}
      animate={{ opacity: 1, height: "auto" }}
      exit={{ opacity: 0, height: 0 }}
      transition={{ duration: 0.25, ease: "easeInOut" }}
      className={cn(
        "relative w-full pl-8 pb-2",
        isFirst && hiddenCount === 0 && !turnText && "mt-3",
        isLast && "pb-0",
        isCancelled && "opacity-60",
      )}
    >
      {!isLast && (
        <div className="absolute left-[11px] top-[21px] bottom-0 w-px bg-border" />
      )}

      {/* Status dot with number or spinner */}
      <TimelineDot
        status={statusType}
        isCancelled={!!isCancelled}
        index={toolIndex}
      />

      {/* Trigger */}
      <button
        onClick={() => setIsOpen((v) => !v)}
        className="flex w-full items-center gap-1.5 text-sm transition-colors"
      >
        <span
          className={cn(
            "grow text-left leading-snug",
            isCancelled
              ? "text-text-tertiary line-through"
              : "text-text-secondary",
          )}
        >
          <b>{displayToolName(toolName)}</b>
          {description && (
            <span className="font-normal text-text-tertiary">
              {" "}
              — {description}
            </span>
          )}
        </span>
        <motion.span
          animate={{ rotate: isOpen ? 0 : -90 }}
          transition={{ duration: ANIMATION_DURATION / 1000, ease: "easeOut" }}
          className="shrink-0 text-text-tertiary"
        >
          <ChevronDownIcon className="h-3.5 w-3.5" />
        </motion.span>
      </button>

      {/* Expandable content */}
      <AnimatePresence initial={false}>
        {isOpen && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: ANIMATION_DURATION / 1000, ease: "easeOut" }}
            className="overflow-hidden"
          >
            <div
              className={cn(
                "mt-2 flex flex-col gap-2 rounded-md border border-border bg-surface-secondary text-sm",
                (isFileTool || isShellTool || isProduceFileTool) && !errorText ? "p-0 overflow-hidden" : "p-3",
              )}
            >
              {errorText && (
                <div className="text-xs">
                  <p className="font-semibold text-text-tertiary">
                    {isCancelled ? "Cancelled reason:" : "Error:"}
                  </p>
                  <p className="text-text-tertiary">{errorText}</p>
                </div>
              )}
              {isFileTool ? (
                <div className={cn(isCancelled && "opacity-60")}>
                  <FileToolExpanded
                    toolName={toolName}
                    args={args}
                    result={result}
                    description={description}
                  />
                </div>
              ) : isShellTool ? (
                <div className={cn(isCancelled && "opacity-60")}>
                  <ShellExpanded args={args} result={result} />
                </div>
              ) : isProduceFileTool ? (
                <div className={cn(isCancelled && "opacity-60")}>
                  <ProduceFileExpanded args={args} result={result} />
                </div>
              ) : (
                <>
                  {argsText && (
                    <div className={cn(isCancelled && "opacity-60")}>
                      <pre className="whitespace-pre-wrap text-text-secondary text-xs overflow-x-auto">
                        {argsText}
                      </pre>
                    </div>
                  )}
                  {!isCancelled && result !== undefined && (
                    <div className="border-t border-dashed border-border pt-2">
                      <p className="font-semibold text-text-secondary text-xs">Result:</p>
                      <pre className="whitespace-pre-wrap text-text-secondary text-xs overflow-x-auto">
                        {typeof result === "string" ? result : JSON.stringify(result, null, 2)}
                      </pre>
                    </div>
                  )}
                </>
              )}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </motion.div>
    </>
  );
};

export const DefaultToolCallUI = memo(
  ToolFallbackImpl,
) as unknown as ToolCallMessagePartComponent;
DefaultToolCallUI.displayName = "ToolFallback";
