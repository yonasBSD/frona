"use client";

import { useState, useEffect } from "react";
import { MessagePrimitive, useMessage, useMessagePartText } from "@assistant-ui/react";
import { useThreadIsRunning } from "@assistant-ui/core/react";
import { MarkdownText } from "./markdown-text";
import { ChevronDownIcon, ChevronRightIcon } from "@heroicons/react/16/solid";
import { useSession } from "@/lib/session-context";
import { useNavigation } from "@/lib/navigation-context";
import { useRetryInfo } from "@/lib/retry-context";
import { agentDisplayName } from "@/lib/types";
import type { Attachment } from "@/lib/types";
import { DefaultToolCallUI } from "./tool-uis/default-tool-call-ui";
import { ToolTimelineProvider } from "./tool-uis/tool-timeline-context";
import { DocumentIcon, ArrowDownTrayIcon } from "@heroicons/react/24/outline";

function ReasoningPart({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  if (!text) return null;

  return (
    <div className="mb-2">
      <button
        onClick={() => setOpen((v) => !v)}
        className="inline-flex items-center gap-1 text-xs font-medium text-text-tertiary hover:text-text-secondary transition"
      >
        {open ? (
          <ChevronDownIcon className="h-3.5 w-3.5" />
        ) : (
          <ChevronRightIcon className="h-3.5 w-3.5" />
        )}
        Reasoning
      </button>
      {open && (
        <div className="mt-1 rounded-md border border-border bg-surface-secondary px-3 py-2 text-xs text-text-secondary whitespace-pre-wrap">
          {text}
        </div>
      )}
    </div>
  );
}

function StreamingIndicator() {
  return (
    <span className="inline-flex items-center gap-1 py-1 -order-1">
      <span className="h-1 w-1 rounded-full bg-text-tertiary animate-[wave_1.4s_ease-in-out_infinite]" />
      <span className="h-1 w-1 rounded-full bg-text-tertiary animate-[wave_1.4s_ease-in-out_0.2s_infinite]" />
      <span className="h-1 w-1 rounded-full bg-text-tertiary animate-[wave_1.4s_ease-in-out_0.4s_infinite]" />
    </span>
  );
}

function AvatarContent({ avatar, letter }: { avatar?: string | null; letter: string }) {
  if (avatar && (avatar.startsWith("data:") || avatar.startsWith("http") || avatar.startsWith("/api/"))) {
    // eslint-disable-next-line @next/next/no-img-element
    return <img src={avatar} alt="" className="h-8 w-8 rounded-full object-cover" />;
  }
  return (
    <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-xs font-medium bg-surface-tertiary text-text-secondary">
      {letter}
    </div>
  );
}

function AgentAvatar({ name, avatar }: { name: string; avatar?: string | null }) {
  const letter = name.charAt(0).toUpperCase();

  return (
    <>
      <MessagePrimitive.If last={false}>
        <AvatarContent avatar={avatar} letter={letter} />
      </MessagePrimitive.If>
      <MessagePrimitive.If last>
        <LastMessageAvatar avatar={avatar} letter={letter} />
      </MessagePrimitive.If>
    </>
  );
}

function LastMessageAvatar({ avatar, letter }: { avatar?: string | null; letter: string }) {
  const isRunning = useThreadIsRunning();

  return (
    <div className="relative shrink-0 h-8 w-8">
      {isRunning && (
        <>
          <div className="absolute inset-[-3px] rounded-full animate-spin" style={{
            background: "conic-gradient(from 0deg, transparent 0%, var(--accent) 30%, transparent 60%)",
            mask: "radial-gradient(farthest-side, transparent calc(100% - 2px), #fff calc(100% - 2px))",
            WebkitMask: "radial-gradient(farthest-side, transparent calc(100% - 2px), #fff calc(100% - 2px))",
            animationDuration: "1.2s",
          }} />
          <div className="absolute inset-[-3px] rounded-full animate-spin" style={{
            background: "conic-gradient(from 180deg, transparent 0%, var(--accent) 20%, transparent 50%)",
            mask: "radial-gradient(farthest-side, transparent calc(100% - 2px), #fff calc(100% - 2px))",
            WebkitMask: "radial-gradient(farthest-side, transparent calc(100% - 2px), #fff calc(100% - 2px))",
            animationDuration: "1.2s",
            opacity: 0.5,
          }} />
        </>
      )}
      <AvatarContent avatar={avatar} letter={letter} />
    </div>
  );
}

const RETRY_LABELS: Record<string, string> = {
  rate_limited: "Rate limited",
  server_error: "Server error",
  network_error: "Network error",
  empty_response: "Empty response",
  timeout: "Timeout",
  overloaded: "Overloaded",
};

function RetryBadge() {
  const retry = useRetryInfo();
  const [remaining, setRemaining] = useState(0);

  useEffect(() => {
    if (!retry) return;
    const update = () => {
      const elapsed = (Date.now() - retry.startedAt) / 1000;
      const left = Math.max(0, Math.ceil(retry.retryAfterSecs - elapsed));
      setRemaining(left);
    };
    update();
    const id = setInterval(update, 1000);
    return () => clearInterval(id);
  }, [retry]);

  if (!retry) return null;

  const label = RETRY_LABELS[retry.reason] ?? retry.reason;

  return (
    <span className="inline-flex items-center gap-1.5 rounded-full bg-surface-tertiary px-2 py-0.5 text-[10px] font-medium text-text-secondary">
      <span className="h-2.5 w-2.5 animate-spin rounded-full border border-text-tertiary border-t-text-secondary" />
      {label} · retry {remaining}s
    </span>
  );
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function AttachmentItem({ attachment }: { attachment: Attachment }) {
  const url = attachment.url;
  const isImage = attachment.content_type.startsWith("image/");

  if (!url) return null;

  if (isImage) {
    return (
      <a href={url} target="_blank" rel="noopener noreferrer">
        <img
          src={url}
          alt={attachment.filename}
          className="max-w-xs max-h-48 rounded-md border border-border"
        />
      </a>
    );
  }

  return (
    <a
      href={url}
      target="_blank"
      rel="noopener noreferrer"
      className="inline-flex items-center gap-1.5 rounded-md bg-surface-tertiary px-2.5 py-1.5 text-xs text-text-secondary hover:text-text-primary transition"
    >
      <span className="truncate max-w-[200px]">{attachment.filename}</span>
      <span className="text-text-tertiary">({formatFileSize(attachment.size_bytes)})</span>
    </a>
  );
}

function MessageAttachments() {
  const message = useMessage();
  const attachments = (message.metadata as Record<string, any>)?.custom?.attachments as Attachment[] | undefined;

  if (!attachments?.length) return null;

  return (
    <div className="flex flex-wrap gap-2 mt-2">
      {attachments.map((att, i) => (
        <AttachmentItem key={i} attachment={att} />
      ))}
    </div>
  );
}

export function FronaAssistantMessage() {
  const { agentId: sessionAgentId } = useSession();
  const message = useMessage();
  const messageAgentId = (message.metadata as Record<string, any>)?.custom?.agentId;
  const agentId = messageAgentId ?? sessionAgentId ?? undefined;
  const { agents } = useNavigation();

  const agent = agents.find((a) => a.id === agentId);
  const agentName = agentDisplayName(agentId, agent?.name);

  return (
    <MessagePrimitive.Root>
      <div className="w-full">
        <div className="flex items-center gap-2.5 h-8">
          <AgentAvatar name={agentName} avatar={agent?.identity?.avatar} />
          <p className="text-xs font-medium text-text-tertiary">
            {agentName}
          </p>
          <MessagePrimitive.If last>
            <RetryBadge />
          </MessagePrimitive.If>
        </div>
        <div className="pl-[42px] text-base text-text-primary flex flex-col items-start">
          <ToolTimelineProvider>
            <MessagePrimitive.Parts
              unstable_showEmptyOnNonTextEnd={false}
              components={{
                Text: SmoothMarkdownText,
                Reasoning: ReasoningPart,
                Empty: StreamingIndicator,
                tools: {
                  Fallback: DefaultToolCallUI,
                },
              }}
            />
          </ToolTimelineProvider>
          <MessageAttachments />
        </div>
      </div>
    </MessagePrimitive.Root>
  );
}

function SmoothMarkdownText() {
  const { text } = useMessagePartText();
  const isRunning = useThreadIsRunning();

  if (!text && isRunning) return <StreamingIndicator />;
  if (!text) return null;
  return <span className="-order-1 w-full"><MarkdownText smooth /></span>;
}
