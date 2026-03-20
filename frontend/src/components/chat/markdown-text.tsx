"use client";

import { MarkdownTextPrimitive } from "@assistant-ui/react-markdown";

export function MarkdownText({ smooth }: { smooth?: boolean; [key: string]: unknown }) {
  return (
    <MarkdownTextPrimitive
      className="prose prose-base max-w-none leading-normal [&>*:first-child]:mt-0 prose-p:my-1.5 prose-headings:my-2 prose-ul:my-1.5 prose-ol:my-1.5 prose-li:my-0 prose-pre:my-2 prose-blockquote:my-1.5 prose-hr:my-2 text-[var(--text-primary)] prose-headings:text-[var(--text-primary)] prose-strong:text-[var(--text-primary)] prose-a:text-[var(--accent)] hover:prose-a:text-[var(--accent-hover)] prose-code:text-[var(--text-primary)] prose-code:before:content-none prose-code:after:content-none prose-blockquote:text-[var(--text-secondary)] prose-blockquote:border-[var(--border)] prose-hr:border-[var(--border)] prose-th:border-[var(--border)] prose-td:border-[var(--border)]"
      smooth={smooth}
    />
  );
}
