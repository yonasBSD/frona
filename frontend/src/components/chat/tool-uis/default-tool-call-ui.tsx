"use client";

import { memo, useCallback, useRef, useState } from "react";
import {
  AlertCircleIcon,
  CheckIcon,
  ChevronDownIcon,
  LoaderIcon,
  XCircleIcon,
} from "lucide-react";
import {
  useScrollLock,
  type ToolCallMessagePartStatus,
  type ToolCallMessagePartComponent,
} from "@assistant-ui/react";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { cn } from "@/lib/utils";

const ANIMATION_DURATION = 200;

type ToolStatus = ToolCallMessagePartStatus["type"];

const statusIconMap: Record<ToolStatus, React.ElementType> = {
  running: LoaderIcon,
  complete: CheckIcon,
  incomplete: XCircleIcon,
  "requires-action": AlertCircleIcon,
};

type ToolFallbackRootProps = React.ComponentProps<typeof Collapsible> & {
  defaultOpen?: boolean;
};

function ToolFallbackRoot({
  className,
  defaultOpen = false,
  children,
  ...props
}: ToolFallbackRootProps) {
  const collapsibleRef = useRef<HTMLDivElement>(null);
  const [isOpen, setIsOpen] = useState(defaultOpen);
  const lockScroll = useScrollLock(collapsibleRef, ANIMATION_DURATION);

  const handleOpenChange = useCallback(
    (open: boolean) => {
      if (!open) lockScroll();
      setIsOpen(open);
    },
    [lockScroll],
  );

  return (
    <Collapsible
      ref={collapsibleRef}
      open={isOpen}
      onOpenChange={handleOpenChange}
      className={cn(
        "w-full rounded-lg border border-border py-3 my-1.5",
        className,
      )}
      style={{ "--animation-duration": `${ANIMATION_DURATION}ms` } as React.CSSProperties}
      {...props}
    >
      {children}
    </Collapsible>
  );
}

function ToolFallbackTrigger({
  toolName,
  description,
  status,
  className,
  ...props
}: React.ComponentProps<typeof CollapsibleTrigger> & {
  toolName: string;
  description?: string | null;
  status?: ToolCallMessagePartStatus;
}) {
  const statusType = status?.type ?? "complete";
  const isRunning = statusType === "running";
  const isCancelled = status?.type === "incomplete" && status.reason === "cancelled";
  const Icon = statusIconMap[statusType];

  return (
    <CollapsibleTrigger
      className={cn(
        "group/trigger flex w-full items-center gap-2 px-4 text-sm transition-colors",
        className,
      )}
      {...props}
    >
      <Icon
        className={cn(
          "h-4 w-4 shrink-0",
          isCancelled && "text-text-tertiary",
          isRunning && "animate-spin text-text-secondary",
          statusType === "complete" && "text-success",
          statusType === "incomplete" && !isCancelled && "text-danger",
        )}
      />
      <span
        className={cn(
          "grow text-left leading-none",
          isCancelled ? "text-text-tertiary line-through" : "text-text-secondary",
        )}
      >
        <b>{toolName}</b>
        {description && <span className="font-normal text-text-tertiary"> — {description}</span>}
      </span>
      <ChevronDownIcon
        className={cn(
          "h-4 w-4 shrink-0 text-text-tertiary transition-transform duration-200 ease-out",
          "group-data-[state=closed]/trigger:-rotate-90",
          "group-data-[state=open]/trigger:rotate-0",
        )}
      />
    </CollapsibleTrigger>
  );
}

function ToolFallbackContent({
  className,
  children,
  ...props
}: React.ComponentProps<typeof CollapsibleContent>) {
  return (
    <CollapsibleContent
      className={cn(
        "overflow-hidden text-sm",
        className,
      )}
      {...props}
    >
      <div className="mt-3 flex flex-col gap-2 border-t border-border pt-2">{children}</div>
    </CollapsibleContent>
  );
}

function ToolFallbackArgs({ argsText, className, ...props }: React.ComponentProps<"div"> & { argsText?: string }) {
  if (!argsText) return null;
  return (
    <div className={cn("px-4", className)} {...props}>
      <pre className="whitespace-pre-wrap text-text-secondary text-xs overflow-x-auto">{argsText}</pre>
    </div>
  );
}

function ToolFallbackResult({ result, className, ...props }: React.ComponentProps<"div"> & { result?: unknown }) {
  if (result === undefined) return null;
  return (
    <div className={cn("border-t border-dashed border-border px-4 pt-2", className)} {...props}>
      <p className="font-semibold text-text-secondary text-xs">Result:</p>
      <pre className="whitespace-pre-wrap text-text-secondary text-xs overflow-x-auto">
        {typeof result === "string" ? result : JSON.stringify(result, null, 2)}
      </pre>
    </div>
  );
}

function ToolFallbackError({ status, className, ...props }: React.ComponentProps<"div"> & { status?: ToolCallMessagePartStatus }) {
  if (status?.type !== "incomplete") return null;
  const error = (status as { error?: unknown }).error;
  const errorText = error
    ? typeof error === "string" ? error : JSON.stringify(error)
    : null;
  if (!errorText) return null;
  const isCancelled = status.reason === "cancelled";

  return (
    <div className={cn("px-4 text-xs", className)} {...props}>
      <p className="font-semibold text-text-tertiary">{isCancelled ? "Cancelled reason:" : "Error:"}</p>
      <p className="text-text-tertiary">{errorText}</p>
    </div>
  );
}

const ToolFallbackImpl: ToolCallMessagePartComponent = ({ toolName, args, argsText, result, status }) => {
  const isCancelled = status?.type === "incomplete" && status.reason === "cancelled";
  const description = typeof args?.description === "string" ? args.description : null;

  return (
    <ToolFallbackRoot className={cn(isCancelled && "border-border/50 opacity-60")}>
      <ToolFallbackTrigger toolName={toolName} description={description} status={status} />
      <ToolFallbackContent>
        <ToolFallbackError status={status} />
        <ToolFallbackArgs argsText={argsText} className={cn(isCancelled && "opacity-60")} />
        {!isCancelled && <ToolFallbackResult result={result} />}
      </ToolFallbackContent>
    </ToolFallbackRoot>
  );
};

export const DefaultToolCallUI = memo(ToolFallbackImpl) as unknown as ToolCallMessagePartComponent;
DefaultToolCallUI.displayName = "ToolFallback";
