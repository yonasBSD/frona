import { DefaultView } from "./default";
import { FileView } from "./file";
import { NodeView } from "./node";
import { PythonView } from "./python";
import { ShellView } from "./shell";
import type { ToolView } from "./types";

export type ToolMatcher = { match: (toolName: string) => boolean; view: ToolView };

export const TOOL_VIEWS_EXACT: Record<string, ToolView> = {
  shell: ShellView,
  python: PythonView,
  node: NodeView,
  read: FileView,
  write: FileView,
  edit: FileView,
  glob: FileView,
  grep: FileView,
};

export const TOOL_VIEWS_PATTERN: ToolMatcher[] = [];

export function pickView(toolName: string): ToolView {
  const exact = TOOL_VIEWS_EXACT[toolName];
  if (exact) return exact;
  for (const { match, view } of TOOL_VIEWS_PATTERN) {
    if (match(toolName)) return view;
  }
  return DefaultView;
}

export { DefaultView } from "./default";
export type { ToolView, ToolViewProps } from "./types";
