"use client";

import { useState } from "react";
import { makeAssistantToolUI, useMessage } from "@assistant-ui/react";
import { ArrowDownTrayIcon, XMarkIcon } from "@heroicons/react/24/outline";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { CodeBlock } from "@/components/ui/code-block";
import type { Attachment } from "@/lib/types";

function FilePreviewModal({ attachment, onClose }: { attachment: Attachment; onClose: () => void }) {
  const [content, setContent] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useState(() => {
    if (!attachment.url) return;
    fetch(attachment.url)
      .then((r) => r.text())
      .then((text) => { setContent(text); setLoading(false); })
      .catch(() => { setContent("Failed to load file."); setLoading(false); });
  });

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={onClose}>
      <div
        className="relative flex flex-col w-[90vw] max-w-3xl max-h-[80vh] rounded-xl border border-border bg-surface shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-4 py-3 border-b border-border">
          <span className="text-sm font-medium text-text-primary truncate">{attachment.filename}</span>
          <div className="flex items-center gap-2">
            <a
              href={attachment.url}
              download={attachment.filename}
              className="flex items-center gap-1.5 rounded-md bg-surface-tertiary px-2.5 py-1.5 text-xs text-text-secondary hover:text-text-primary hover:bg-surface-secondary transition-colors"
            >
              <ArrowDownTrayIcon className="h-3.5 w-3.5" />
              Download
            </a>
            <button onClick={onClose} className="text-text-tertiary hover:text-text-primary transition-colors">
              <XMarkIcon className="h-5 w-5" />
            </button>
          </div>
        </div>
        <div className="flex-1 overflow-auto p-4">
          {loading ? (
            <p className="text-sm text-text-tertiary">Loading...</p>
          ) : attachment.content_type === "text/markdown" ? (
            <div className="prose prose-sm max-w-none text-text-primary prose-headings:text-text-primary prose-strong:text-text-primary prose-a:text-accent prose-code:text-text-primary prose-code:before:content-none prose-code:after:content-none prose-blockquote:text-text-secondary prose-blockquote:border-border">
              <ReactMarkdown remarkPlugins={[remarkGfm]} components={{
                code({ className, children, ...props }) {
                  const match = /language-(\w+)/.exec(className || "");
                  const code = String(children).replace(/\n$/, "");
                  if (match) return <CodeBlock code={code} language={match[1]} />;
                  return <code className={className} {...props}>{children}</code>;
                },
              }}>
                {content ?? ""}
              </ReactMarkdown>
            </div>
          ) : (
            <pre className="whitespace-pre-wrap text-sm text-text-primary font-mono">{content}</pre>
          )}
        </div>
      </div>
    </div>
  );
}

function AttachmentItem({ attachment }: { attachment: Attachment }) {
  const url = attachment.url;
  const isImage = attachment.content_type.startsWith("image/");
  const canPreview = attachment.content_type.startsWith("text/") || attachment.content_type === "application/json";
  const [showPreview, setShowPreview] = useState(false);

  if (!url) return null;

  if (isImage) {
    return (
      <a href={url} target="_blank" rel="noopener noreferrer">
        <img src={url} alt={attachment.filename} className="max-w-xs max-h-48 rounded-md border border-border" />
      </a>
    );
  }

  return (
    <>
      <button
        onClick={canPreview ? () => setShowPreview(true) : undefined}
        className="inline-flex items-center gap-1.5 rounded-lg border border-border bg-surface-tertiary px-3 py-2 text-xs text-text-secondary cursor-pointer hover:bg-surface-secondary transition-colors"
      >
        <span className="truncate max-w-[200px]">{attachment.filename}</span>
        {!canPreview && (
          <a href={url} download={attachment.filename} onClick={(e) => e.stopPropagation()}>
            <ArrowDownTrayIcon className="h-3.5 w-3.5 text-text-tertiary hover:text-text-primary" />
          </a>
        )}
      </button>
      {showPreview && <FilePreviewModal attachment={attachment} onClose={() => setShowPreview(false)} />}
    </>
  );
}

export const AttachmentsToolUI = makeAssistantToolUI<Record<string, never>, string>({
  toolName: "Attachments",
  render: () => {
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
  },
});
