"use client";

import { useState, useRef } from "react";
import { makeAssistantToolUI } from "@assistant-ui/react";
import { api } from "@/lib/api-client";
import { useChat } from "@/lib/chat-context";

interface HumanInTheLoopArgs {
  reason: string;
  debugger_url: string;
  status: string;
  response: string | null;
}

export const HumanInTheLoopToolUI = makeAssistantToolUI<HumanInTheLoopArgs, string>({
  toolName: "HumanInTheLoop",
  render: ({ args, result, addResult, toolCallId }) => {
    return (
      <HumanInTheLoopRenderer
        toolCallId={toolCallId}
        reason={args.reason}
        debuggerUrl={args.debugger_url}
        serverStatus={args.status}
        result={result}
        addResult={addResult}
      />
    );
  },
});

function HumanInTheLoopRenderer({
  toolCallId,
  reason,
  debuggerUrl,
  serverStatus,
  result,
  addResult,
}: {
  toolCallId: string;
  reason: string;
  debuggerUrl: string;
  serverStatus: string;
  result?: string;
  addResult: (result: string) => void;
}) {
  const { chatId } = useChat();
  const [loading, setLoading] = useState(false);
  const acted = useRef(false);
  const resolved = serverStatus === "resolved" || result !== undefined;

  const handleResume = async () => {
    if (acted.current) return;
    acted.current = true;
    setLoading(true);
    await api.post(`/api/chats/${chatId}/tool-executions/${toolCallId}/resolve`, {
      response: "resumed",
    }).catch(() =>
      api.post(`/api/chats/${chatId}/messages/${toolCallId}/resolve`, {
        response: "resumed",
      })
    );
    addResult("resumed");
  };

  return (
    <div className="my-2">
      <p className="text-base text-text-primary mb-2">{reason}</p>
      <div className="flex flex-wrap gap-2">
        {debuggerUrl && (
          <a
            href={debuggerUrl}
            target="_blank"
            rel="noopener noreferrer"
            className="rounded-lg border border-border px-3 py-1.5 text-base font-medium text-text-secondary hover:border-accent hover:text-accent transition"
          >
            Open Browser Debugger
          </a>
        )}
        <button
          onClick={handleResume}
          disabled={loading || resolved}
          className={`rounded-lg border px-3 py-1.5 text-base font-medium transition ${
            resolved
              ? "border-accent bg-accent/10 text-accent"
              : "border-border text-text-secondary hover:border-accent hover:text-accent"
          }`}
        >
          {loading ? "Resuming..." : resolved ? "Resumed" : "Resume Agent"}
        </button>
      </div>
    </div>
  );
}
