import { DefaultView } from "./default";
import type { ToolView } from "./types";

export type ToolMatcher = { match: (toolName: string) => boolean; view: ToolView };

export const TOOL_VIEWS_EXACT: Record<string, ToolView> = {};

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
