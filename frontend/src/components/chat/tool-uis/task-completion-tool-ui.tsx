"use client";

import { makeAssistantToolUI } from "@assistant-ui/react";

interface TaskCompletionArgs {
  task_id: string;
  chat_id: string | null;
  status: string;
}

export const TaskCompletionToolUI = makeAssistantToolUI<TaskCompletionArgs, string>({
  toolName: "TaskCompletion",
  render: ({ args }) => {
    const isError = args.status === "failed";
    const displayContent = isError
      ? "Task marked as failed."
      : "Task marked as complete.";

    return (
      <div
        className={`flex items-start gap-3 rounded-lg border px-4 py-3 text-base my-2 ${
          isError
            ? "border-danger/30 bg-danger-bg text-danger-text"
            : "border-success/30 bg-success-bg text-success-text"
        }`}
      >
        <span className="flex-1">{displayContent}</span>
      </div>
    );
  },
});
