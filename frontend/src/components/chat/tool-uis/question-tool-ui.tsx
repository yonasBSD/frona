"use client";

import { useState, useRef } from "react";
import { makeAssistantToolUI } from "@assistant-ui/react";
import { api } from "@/lib/api-client";
import { useChat } from "@/lib/chat-context";

interface QuestionArgs {
  question: string;
  options: string[];
  status: string;
  response: string | null;
}

export const QuestionToolUI = makeAssistantToolUI<QuestionArgs, string>({
  toolName: "Question",
  render: ({ args, result, addResult, toolCallId }) => {
    return (
      <QuestionRenderer
        toolCallId={toolCallId}
        question={args.question}
        options={args.options}
        serverStatus={args.status}
        serverResponse={args.response}
        result={result}
        addResult={addResult}
      />
    );
  },
});

function QuestionRenderer({
  toolCallId,
  question,
  options,
  serverStatus,
  serverResponse,
  result,
  addResult,
}: {
  toolCallId: string;
  question: string;
  options: string[];
  serverStatus: string;
  serverResponse: string | null;
  result?: string;
  addResult: (result: string) => void;
}) {
  const { chatId } = useChat();
  const [loading, setLoading] = useState(false);
  const acted = useRef(false);

  const resolved = serverStatus === "resolved" || result !== undefined;
  const selectedAnswer = result ?? serverResponse;

  const handleAnswer = async (answer: string) => {
    if (acted.current) return;
    acted.current = true;
    setLoading(true);
    await api.post(`/api/chats/${chatId}/tool-executions/resolve`, {
      resolutions: [{ tool_execution_id: toolCallId, response: answer }],
    });
    addResult(answer);
  };

  return (
    <div className="my-2 -order-1">
      <p className="text-base text-text-primary mb-2">{question}</p>
      <div className="flex flex-col gap-2">
        {options.map((option) => {
          const isSelected = selectedAnswer === option;
          return (
            <button
              key={option}
              onClick={() => handleAnswer(option)}
              disabled={loading || resolved}
              className={`rounded-lg border px-3 py-1.5 text-left text-base font-medium transition ${
                isSelected
                  ? "border-accent bg-accent/10 text-accent"
                  : resolved
                    ? "border-border text-text-tertiary opacity-50"
                    : "border-border text-text-secondary hover:border-accent hover:text-accent"
              }`}
            >
              {option}
            </button>
          );
        })}
      </div>
    </div>
  );
}
