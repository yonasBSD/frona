"use client";

import { useState } from "react";
import type { ModelGroupConfig, RetryConfig } from "@/lib/config-types";
import { NumberInput, SectionHeader } from "@/components/settings/field";
import { CubeIcon } from "@heroicons/react/24/outline";
import { ComboboxInput } from "@/components/settings/combobox";
import { ModelSelector } from "@/components/settings/model-selector";


interface ModelsSectionProps {
  models: Record<string, ModelGroupConfig>;
  enabledProviders: string[];
  providerConfigs?: Record<string, import("@/lib/config-types").ModelProviderConfig>;
  onChange: (models: Record<string, ModelGroupConfig>) => void;
}

const PREDEFINED_GROUPS = ["primary", "reasoning", "coding"];

function formatGroupName(name: string): string {
  const names: Record<string, string> = {
    primary: "Primary",
    reasoning: "Reasoning",
    coding: "Coding",
  };
  return names[name] ?? name;
}

function sortedGroupNames(names: string[]): string[] {
  const predefined = PREDEFINED_GROUPS.filter((g) => names.includes(g));
  const custom = names.filter((g) => !PREDEFINED_GROUPS.includes(g));
  return [...predefined, ...custom];
}

interface GroupNameInputProps {
  value: string;
  suggestions: string[];
  onRename: (newName: string) => void;
}

function GroupNameInput({ value, suggestions, onRename }: GroupNameInputProps) {
  const displayDraft = formatGroupName(value);
  const [draft, setDraft] = useState(displayDraft);

  const nameToId = new Map(suggestions.map((g) => [formatGroupName(g), g]));
  const items = suggestions.map((g) => {
    const display = formatGroupName(g);
    return { value: display, label: display };
  });

  return (
    <ComboboxInput
      label="Group ID"
      value={draft}
      items={items}
      placeholder="e.g. Primary, Coding"
      allowFreeText
      onChange={(v) => {
        setDraft(v);
        const id = nameToId.get(v);
        if (id) {
          onRename(id);
        }
      }}
      onBlur={() => {
        const resolved = nameToId.get(draft) ?? draft;
        const sanitized = resolved.trim().toLowerCase().replace(/\s+/g, "_").replace(/[^a-z0-9_]/g, "");
        if (sanitized && sanitized !== value) {
          setDraft(formatGroupName(sanitized));
          onRename(sanitized);
        } else {
          setDraft(formatGroupName(value));
        }
      }}
    />
  );
}

const DEFAULT_RETRY: RetryConfig = {
  max_retries: 10,
  initial_backoff_ms: 1000,
  backoff_multiplier: 2.0,
  max_backoff_ms: 60000,
};

function newModelGroup(): ModelGroupConfig {
  return {
    provider: "",
    model: "",
    fallbacks: [],
    max_tokens: null,
    temperature: null,
    context_window: null,
    retry: { ...DEFAULT_RETRY },
  };
}

function newFallback(): ModelGroupConfig {
  return {
    provider: "",
    model: "",
  };
}

export function ModelsSection({ models, enabledProviders, providerConfigs, onChange }: ModelsSectionProps) {
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [expandedRetry, setExpandedRetry] = useState<Set<string>>(new Set());
  const [confirmingRemove, setConfirmingRemove] = useState<string | null>(null);

  const groupNames = sortedGroupNames(Object.keys(models));

  function toggleExpanded(name: string) {
    setExpandedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  }

  function toggleRetry(name: string) {
    setExpandedRetry((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  }

  function updateGroup(name: string, update: Partial<ModelGroupConfig>) {
    onChange({ ...models, [name]: { ...models[name], ...update } });
  }

  function renameGroup(oldName: string, newName: string) {
    if (!newName.trim() || (newName !== oldName && newName in models)) return;
    const entries = Object.entries(models).map(([k, v]) =>
      k === oldName ? [newName, v] : [k, v]
    );
    onChange(Object.fromEntries(entries));
    setExpandedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(oldName)) {
        next.delete(oldName);
        next.add(newName);
      }
      return next;
    });
    setExpandedRetry((prev) => {
      const next = new Set(prev);
      if (next.has(oldName)) {
        next.delete(oldName);
        next.add(newName);
      }
      return next;
    });
  }

  function removeGroup(name: string) {
    const next = { ...models };
    delete next[name];
    onChange(next);
    setConfirmingRemove(null);
  }

  function addGroup() {
    let name = "";
    let i = 1;
    while (name in models) {
      name = `group_${i++}`;
    }
    onChange({ ...models, [name]: newModelGroup() });
    setExpandedGroups((prev) => new Set(prev).add(name));
  }

  function updateFallback(groupName: string, index: number, update: Partial<ModelGroupConfig>) {
    const fallbacks = [...(models[groupName].fallbacks ?? [])];
    fallbacks[index] = { ...fallbacks[index], ...update };
    updateGroup(groupName, { fallbacks });
  }

  function addFallback(groupName: string) {
    updateGroup(groupName, { fallbacks: [...(models[groupName].fallbacks ?? []), newFallback()] });
  }

  function removeFallback(groupName: string, index: number) {
    const fallbacks = (models[groupName].fallbacks ?? []).filter((_, i) => i !== index);
    updateGroup(groupName, { fallbacks });
  }

  function updateRetry(groupName: string, update: Partial<RetryConfig>) {
    updateGroup(groupName, {
      retry: { ...(models[groupName].retry ?? DEFAULT_RETRY), ...update },
    });
  }

  return (
    <div>
      <SectionHeader title="Model Groups" description="Configure model groups with fallback chains and inference parameters" icon={CubeIcon} />
      <div className="space-y-3">
        {groupNames.map((name) => {
          const group = models[name];
          const isExpanded = expandedGroups.has(name);
          const isRetryExpanded = expandedRetry.has(name);
          const retry = group.retry ?? DEFAULT_RETRY;

          return (
            <div
              key={name}
              className="rounded-lg border border-border bg-surface-secondary"
            >
              <button
                type="button"
                onClick={() => toggleExpanded(name)}
                className="flex w-full items-center justify-between px-4 py-3 text-sm font-medium text-text-primary hover:bg-surface-tertiary rounded-lg"
              >
                <span>{formatGroupName(name)}</span>
                <svg
                  className={`h-4 w-4 text-text-tertiary transition-transform ${isExpanded ? "rotate-180" : ""}`}
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth={2}
                >
                  <path strokeLinecap="round" strokeLinejoin="round" d="M19 9l-7 7-7-7" />
                </svg>
              </button>

              {isExpanded && (
                <div className="space-y-4 px-4 pb-4">
                  <GroupNameInput
                    value={name}
                    suggestions={PREDEFINED_GROUPS.filter((g) => g === name || !(g in models))}
                    onRename={(newName) => renameGroup(name, newName)}
                  />

                  <ModelSelector
                    label="Main Model"
                    provider={group.provider}
                    model={group.model}
                    enabledProviders={enabledProviders}
                    providerConfigs={providerConfigs}
                    onProviderChange={(v) => updateGroup(name, { provider: v })}
                    onModelChange={(v) => updateGroup(name, { model: v })}
                    onModelInfo={(info) => {
                      if (info) {
                        const update: Partial<ModelGroupConfig> = {};
                        if (info.max_tokens && info.max_tokens !== group.max_tokens) update.max_tokens = info.max_tokens;
                        if (info.context_window && info.context_window !== group.context_window) update.context_window = info.context_window;
                        if (Object.keys(update).length > 0) updateGroup(name, update);
                      }
                    }}
                  />

                  <div className="space-y-2">
                    <label className="block text-sm font-medium text-text-secondary">
                      Fallbacks
                    </label>
                    {(group.fallbacks ?? []).map((fb, i) => (
                      <div key={i} className="flex items-start gap-2">
                        <span className="text-xs text-text-tertiary w-5 text-right shrink-0 pt-8">
                          {i + 1}.
                        </span>
                        <div className="flex-1">
                          <ModelSelector
                            label=""
                            provider={fb.provider}
                            model={fb.model}
                            enabledProviders={enabledProviders}
                            providerConfigs={providerConfigs}
                            onProviderChange={(v) => updateFallback(name, i, { provider: v })}
                            onModelChange={(v) => updateFallback(name, i, { model: v })}
                          />
                        </div>
                        <button
                          type="button"
                          onClick={() => removeFallback(name, i)}
                          className="shrink-0 rounded-lg p-1.5 text-text-tertiary hover:bg-surface-tertiary hover:text-text-primary mt-6"
                        >
                          <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                          </svg>
                        </button>
                      </div>
                    ))}
                    <button
                      type="button"
                      onClick={() => addFallback(name)}
                      className="mt-1 text-xs text-accent hover:underline"
                    >
                      + Add fallback
                    </button>
                  </div>

                  <NumberInput
                    label="Max Tokens"
                    value={group.max_tokens ?? null}
                    onChange={(v) => updateGroup(name, { max_tokens: v || null })}
                    min={1}
                    placeholder="Default"
                  />

                  <div className="space-y-1">
                    <div className="flex items-center justify-between">
                      <label className="block text-sm font-medium text-text-secondary">
                        Temperature
                      </label>
                      <span className="text-xs text-text-tertiary tabular-nums">
                        {group.temperature != null ? group.temperature.toFixed(1) : "Default"}
                      </span>
                    </div>
                    <input
                      type="range"
                      min={0}
                      max={2}
                      step={0.1}
                      value={group.temperature ?? 0}
                      onChange={(e) => {
                        const v = parseFloat(e.target.value);
                        updateGroup(name, { temperature: v === 0 ? null : v });
                      }}
                      className="w-full accent-accent"
                    />
                  </div>

                  <NumberInput
                    label="Context Window"
                    value={group.context_window ?? null}
                    onChange={(v) => updateGroup(name, { context_window: v || null })}
                    min={1}
                    placeholder="Default"
                  />

                  <button
                    type="button"
                    onClick={() => toggleRetry(name)}
                    className="flex items-center gap-1 text-sm font-medium text-text-secondary hover:text-text-primary transition"
                  >
                    <svg
                      className={`h-4 w-4 text-text-tertiary transition-transform ${isRetryExpanded ? "rotate-90" : ""}`}
                      fill="none"
                      viewBox="0 0 24 24"
                      stroke="currentColor"
                      strokeWidth={2}
                    >
                      <path strokeLinecap="round" strokeLinejoin="round" d="M9 5l7 7-7 7" />
                    </svg>
                    Retry Config
                  </button>
                  {isRetryExpanded && (
                    <div className="space-y-4">
                      <NumberInput
                        label="Max Retries"
                        value={retry.max_retries}
                        onChange={(v) => updateRetry(name, { max_retries: v })}
                        min={0}
                      />
                      <NumberInput
                        label="Initial Backoff (ms)"
                        value={retry.initial_backoff_ms}
                        onChange={(v) => updateRetry(name, { initial_backoff_ms: v })}
                        min={0}
                      />
                      <NumberInput
                        label="Backoff Multiplier"
                        value={retry.backoff_multiplier}
                        onChange={(v) => updateRetry(name, { backoff_multiplier: v })}
                        min={1}
                        step={0.1}
                      />
                      <NumberInput
                        label="Max Backoff (ms)"
                        value={retry.max_backoff_ms}
                        onChange={(v) => updateRetry(name, { max_backoff_ms: v })}
                        min={0}
                      />
                    </div>
                  )}

                  <div className="pt-3 border-t border-border flex items-center gap-2">
                    {confirmingRemove === name ? (
                      <>
                        <span className="text-sm text-text-secondary">Remove this group?</span>
                        <button
                          type="button"
                          onClick={() => removeGroup(name)}
                          className="rounded-lg bg-danger px-3 py-1.5 text-xs font-medium text-surface hover:opacity-90 transition"
                        >
                          Confirm
                        </button>
                        <button
                          type="button"
                          onClick={() => setConfirmingRemove(null)}
                          className="rounded-lg px-3 py-1.5 text-xs font-medium text-text-secondary hover:bg-surface-tertiary transition"
                        >
                          Cancel
                        </button>
                      </>
                    ) : (
                      <button
                        type="button"
                        onClick={() => setConfirmingRemove(name)}
                        className="rounded-lg border border-border px-3 py-1.5 text-xs font-medium text-danger hover:bg-surface-tertiary transition"
                      >
                        Remove Group
                      </button>
                    )}
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </div>

      <button
        type="button"
        onClick={addGroup}
        className="mt-4 rounded-lg bg-accent px-4 py-2 text-sm text-white hover:opacity-90"
      >
        + Add Model Group
      </button>
    </div>
  );
}
