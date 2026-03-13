import {
  CheckCircleIcon,
  XCircleIcon,
  CommandLineIcon,
  CodeBracketIcon,
  GlobeAltIcon,
  MagnifyingGlassIcon,
  DocumentArrowDownIcon,
  CpuChipIcon,
  LightBulbIcon,
  UserIcon,
  ClockIcon,
  KeyIcon,
  PhoneIcon,
  ServerIcon,
  CalendarIcon,
  ArrowsRightLeftIcon,
  CursorArrowRaysIcon,
  WrenchScrewdriverIcon,
} from "@heroicons/react/16/solid";
import type { ToolCallStatus } from "@/lib/types";

const TOOL_META: Record<string, { icon: React.ComponentType<React.SVGProps<SVGSVGElement>>; label: string }> = {
  shell: { icon: CommandLineIcon, label: "Shell" },
  python: { icon: CodeBracketIcon, label: "Python" },
  web_search: { icon: MagnifyingGlassIcon, label: "Search" },
  web_fetch: { icon: GlobeAltIcon, label: "Fetch" },
  produce_file: { icon: DocumentArrowDownIcon, label: "File" },
  remember_agent_fact: { icon: LightBulbIcon, label: "Memory" },
  remember_user_fact: { icon: LightBulbIcon, label: "Memory" },
  update_agent: { icon: CpuChipIcon, label: "Settings" },
  update_identity: { icon: UserIcon, label: "Identity" },
  get_time: { icon: ClockIcon, label: "Time" },
  request_credentials: { icon: KeyIcon, label: "Credentials" },
  manage_service: { icon: ServerIcon, label: "Service" },
  schedule: { icon: CalendarIcon, label: "Schedule" },
  delegate_task: { icon: ArrowsRightLeftIcon, label: "Delegate" },
  run_subtask: { icon: ArrowsRightLeftIcon, label: "Subtask" },
  read_skill: { icon: DocumentArrowDownIcon, label: "Skill" },
  make_voice_call: { icon: PhoneIcon, label: "Call" },
  send_dtmf: { icon: PhoneIcon, label: "DTMF" },
  hangup_call: { icon: PhoneIcon, label: "Hangup" },
};

function getToolMeta(name: string): { icon: React.ComponentType<React.SVGProps<SVGSVGElement>>; label: string } {
  if (TOOL_META[name]) return TOOL_META[name];
  if (name.startsWith("browser_")) return { icon: CursorArrowRaysIcon, label: "Browser" };
  return { icon: WrenchScrewdriverIcon, label: name.replace(/_/g, " ") };
}

export function ToolCallIndicator({ toolCall }: { toolCall: ToolCallStatus }) {
  const description =
    toolCall.description || toolCall.name.replace(/_/g, " ");

  const isFading = toolCall.status === "fading";
  const isError = toolCall.status === "error";
  const isSuccess = toolCall.status === "success";
  const isRunning = toolCall.status === "running";
  const meta = getToolMeta(toolCall.name);

  const badgeStyle = isError
    ? "bg-danger-bg text-danger-text border-danger-bg"
    : isSuccess
      ? "bg-success-bg text-success-text border-success-bg"
      : "bg-surface-tertiary text-text-secondary border-surface-tertiary";

  return (
    <div
      className="inline-flex transition-opacity duration-300 ease-out"
      style={{ opacity: isFading ? 0 : 1 }}
    >
      <span className="inline-flex items-center gap-1 rounded-l-full border border-r-0 border-info-bg bg-info-bg text-info-text px-2.5 py-1 text-[11px] font-medium">
        <meta.icon className="h-3 w-3" />
        {meta.label}
      </span>
      <div className={`inline-flex items-center gap-1.5 rounded-r-full border px-2.5 py-1 text-[11px] ${badgeStyle}`}>
        {isRunning ? (
          <div className="h-3 w-3 shrink-0 animate-spin rounded-full border-[1.5px] border-current border-t-transparent" />
        ) : isError ? (
          <XCircleIcon className="h-3 w-3 shrink-0" />
        ) : (
          <CheckCircleIcon className="h-3 w-3 shrink-0" />
        )}
        <span>{description}</span>
      </div>
    </div>
  );
}
