"use client";

import { createContext, useContext } from "react";
import type { ToolExecution } from "./types";

export const PendingToolsContext = createContext<ToolExecution[]>([]);

export function usePendingTools(): ToolExecution[] {
  return useContext(PendingToolsContext);
}
