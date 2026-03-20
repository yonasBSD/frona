"use client";

import { useRef, useState, useEffect, useCallback } from "react";
import { ComposerPrimitive, ThreadPrimitive, useComposerRuntime } from "@assistant-ui/react";
import { PaperAirplaneIcon, StopIcon, PlusIcon, XMarkIcon } from "@heroicons/react/24/solid";
import { ArrowUpTrayIcon, CloudIcon, FolderOpenIcon } from "@heroicons/react/24/outline";
import { FileBrowserModal } from "@/components/chat/file-browser-modal";
import type { Attachment } from "@/lib/types";

interface PendingFile {
  file: File;
  relativePath?: string;
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function FronaComposer({
  placeholder = "Send a message...",
  onSend,
}: {
  placeholder?: string;
  /** Called on form submit to set the outgoing flag before the runtime calls adapter.run(). */
  onSend?: (content: string, attachments?: Attachment[]) => void;
}) {
  const composerRuntime = useComposerRuntime();
  const [pendingFiles, setPendingFiles] = useState<PendingFile[]>([]);
  const [serverAttachments, setServerAttachments] = useState<Attachment[]>([]);
  const [uploading, setUploading] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [browseOpen, setBrowseOpen] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const menuButtonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (
        menuRef.current && !menuRef.current.contains(e.target as Node) &&
        menuButtonRef.current && !menuButtonRef.current.contains(e.target as Node)
      ) {
        setMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [menuOpen]);

  const handleFilesSelected = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (!files) return;
    const newFiles: PendingFile[] = Array.from(files).map((file) => ({
      file,
      relativePath: file.webkitRelativePath || undefined,
    }));
    setPendingFiles((prev) => [...prev, ...newFiles]);
    e.target.value = "";
  }, []);

  const removePendingFile = (index: number) => {
    setPendingFiles((prev) => prev.filter((_, i) => i !== index));
  };

  const removeServerAttachment = (index: number) => {
    setServerAttachments((prev) => prev.filter((_, i) => i !== index));
  };

  return (
    <div>
      {(pendingFiles.length > 0 || serverAttachments.length > 0) && (
        <div className="flex flex-wrap gap-1.5 mb-2 px-1">
          {pendingFiles.map((pf, i) => (
            <span
              key={`local-${i}`}
              className="inline-flex items-center gap-1 rounded-md bg-surface-tertiary px-2 py-1 text-xs text-text-secondary"
            >
              <span className="max-w-[200px] truncate">{pf.relativePath || pf.file.name}</span>
              <span className="text-text-tertiary">({formatFileSize(pf.file.size)})</span>
              <button
                type="button"
                onClick={() => removePendingFile(i)}
                className="ml-0.5 hover:text-text-primary"
              >
                <XMarkIcon className="h-3 w-3" />
              </button>
            </span>
          ))}
          {serverAttachments.map((att, i) => (
            <span
              key={`server-${i}`}
              className="inline-flex items-center gap-1 rounded-md bg-surface-tertiary px-2 py-1 text-xs text-text-secondary"
            >
              <span className="max-w-[200px] truncate">{att.filename}</span>
              <span className="text-text-tertiary">({formatFileSize(att.size_bytes)})</span>
              <button
                type="button"
                onClick={() => removeServerAttachment(i)}
                className="ml-0.5 hover:text-text-primary"
              >
                <XMarkIcon className="h-3 w-3" />
              </button>
            </span>
          ))}
        </div>
      )}
      <ComposerPrimitive.Root
        className="rounded-2xl border border-border bg-surface-secondary p-4 focus-within:border-accent transition-colors"
        {...(onSend ? { onSubmit: () => {
          const composer = composerRuntime.getState();
          const text = composer.text.trim();
          if (text) onSend(text, serverAttachments.length > 0 ? serverAttachments : undefined);
        }} : {})}
      >
        <input
          ref={fileInputRef}
          type="file"
          multiple
          className="hidden"
          onChange={handleFilesSelected}
        />
        <ComposerPrimitive.Input
          autoFocus
          placeholder={placeholder}
          rows={1}
          className="w-full bg-transparent text-sm leading-5 text-text-primary placeholder:text-text-tertiary focus:outline-none resize-none max-h-[200px] overflow-y-auto"
        />
        <div className="flex items-center justify-between pt-2">
          <div className="relative">
            <button
              ref={menuButtonRef}
              type="button"
              onClick={() => setMenuOpen((v) => !v)}
              disabled={uploading}
              className="rounded-lg p-1 text-text-tertiary hover:text-text-secondary hover:bg-surface-tertiary disabled:opacity-30 transition"
              title="Attach"
            >
              <PlusIcon className="h-5 w-5" />
            </button>
            {menuOpen && (
              <div
                ref={menuRef}
                className="absolute bottom-full left-0 mb-1 w-max rounded-lg border border-border bg-surface-secondary py-1 shadow-lg"
              >
                <button
                  type="button"
                  className="flex w-full items-center gap-2 px-3 py-2 text-sm text-text-secondary hover:bg-surface-tertiary transition-colors"
                  onClick={() => {
                    fileInputRef.current?.click();
                    setMenuOpen(false);
                  }}
                >
                  <ArrowUpTrayIcon className="h-4 w-4" />
                  Upload files
                </button>
                <button
                  type="button"
                  className="flex w-full items-center gap-2 px-3 py-2 text-sm text-text-secondary hover:bg-surface-tertiary transition-colors"
                  onClick={() => {
                    setBrowseOpen(true);
                    setMenuOpen(false);
                  }}
                >
                  <FolderOpenIcon className="h-4 w-4" />
                  Browse files
                </button>
                <button
                  type="button"
                  className="flex w-full items-center gap-2 px-3 py-2 text-sm text-text-tertiary cursor-default"
                  onClick={() => setMenuOpen(false)}
                >
                  <CloudIcon className="h-4 w-4" />
                  Connect to Google Drive
                </button>
              </div>
            )}
          </div>
          <div className="flex items-center gap-1">
            <ThreadPrimitive.If running>
              <ComposerPrimitive.Cancel asChild>
                <button
                  type="button"
                  className="shrink-0 rounded-lg p-1.5 text-text-secondary hover:text-text-primary transition"
                >
                  <StopIcon className="h-5 w-5" />
                </button>
              </ComposerPrimitive.Cancel>
            </ThreadPrimitive.If>
            <ThreadPrimitive.If running={false}>
              <ComposerPrimitive.Send asChild>
                <button
                  type="button"
                  disabled={uploading}
                  className="shrink-0 rounded-lg p-1.5 text-text-secondary hover:text-text-primary disabled:opacity-30 transition"
                >
                  <PaperAirplaneIcon className="h-5 w-5" />
                </button>
              </ComposerPrimitive.Send>
            </ThreadPrimitive.If>
          </div>
        </div>
      </ComposerPrimitive.Root>
      <FileBrowserModal
        open={browseOpen}
        onClose={() => setBrowseOpen(false)}
        onSelect={(attachments) => setServerAttachments((prev) => [...prev, ...attachments])}
      />
    </div>
  );
}

