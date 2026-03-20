import { useSyncExternalStore, useCallback } from "react";

interface RetryInfo {
  reason: string;
  retryAfterSecs: number;
  startedAt: number; // Date.now() when retry started
}

let currentRetry: RetryInfo | null = null;
const listeners = new Set<() => void>();

function notify() {
  for (const fn of listeners) fn();
}

export function setRetryState(retry: { reason: string; retryAfterSecs: number } | null) {
  if (retry) {
    currentRetry = { ...retry, startedAt: Date.now() };
    // Auto-clear after the retry period
    const capturedStart = currentRetry.startedAt;
    setTimeout(() => {
      if (currentRetry?.startedAt === capturedStart) {
        currentRetry = null;
        notify();
      }
    }, retry.retryAfterSecs * 1000 + 500);
  } else {
    currentRetry = null;
  }
  notify();
}

export function clearRetryState() {
  currentRetry = null;
  notify();
}

export function useRetryState(): RetryInfo | null {
  const subscribe = useCallback((onStoreChange: () => void) => {
    listeners.add(onStoreChange);
    return () => listeners.delete(onStoreChange);
  }, []);

  return useSyncExternalStore(
    subscribe,
    () => currentRetry,
    () => null,
  );
}
