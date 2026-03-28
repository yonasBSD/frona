"use client";

import { SectionHeader, SectionPanel, Toggle, Field } from "@/components/settings/field";
import { ShieldCheckIcon, TrashIcon, PlusIcon } from "@heroicons/react/24/outline";

interface SandboxSettings {
  network_access: boolean;
  allowed_network_destinations: string[];
  timeout_secs: number;
}

interface SandboxSectionProps {
  sandbox: SandboxSettings | null;
  onChange: (sandbox: SandboxSettings) => void;
}

const DEFAULT_SANDBOX: SandboxSettings = {
  network_access: true,
  allowed_network_destinations: [],
  timeout_secs: 30,
};

export function SandboxSection({ sandbox, onChange }: SandboxSectionProps) {
  const current = sandbox ?? DEFAULT_SANDBOX;

  const updateDest = (index: number, value: string) => {
    const updated = [...current.allowed_network_destinations];
    updated[index] = value;
    onChange({ ...current, allowed_network_destinations: updated });
  };

  const removeDest = (index: number) => {
    onChange({
      ...current,
      allowed_network_destinations: current.allowed_network_destinations.filter((_, i) => i !== index),
    });
  };

  const addDest = () => {
    onChange({
      ...current,
      allowed_network_destinations: [...current.allowed_network_destinations, ""],
    });
  };

  return (
    <div>
      <SectionHeader title="Sandbox" description="Execution environment constraints" icon={ShieldCheckIcon} />
      <SectionPanel>
        <Toggle
          label="Network Access"
          description="Allow the agent to make network requests"
          value={current.network_access}
          onChange={(v) => onChange({ ...current, network_access: v })}
        />
        <Field label="Timeout (seconds)" description="Maximum execution time for tool calls">
          <input
            type="number"
            value={current.timeout_secs}
            onChange={(e) => onChange({ ...current, timeout_secs: Number(e.target.value) })}
            min={1}
            max={300}
            className="block w-24 rounded-lg border border-border bg-surface px-3 py-2 text-sm text-text-primary placeholder:text-text-tertiary focus:border-accent focus:outline-none"
          />
        </Field>
        <div className="space-y-1">
          <label className="inline-flex items-center gap-1 text-sm font-medium text-text-secondary">
            Allowed Network Destinations
          </label>
          <div className="space-y-3">
            {current.allowed_network_destinations.map((dest, i) => (
              <div key={i} className="flex items-center gap-2 rounded-lg border border-border bg-surface p-3">
                <input
                  value={dest}
                  onChange={(e) => updateDest(i, e.target.value)}
                  placeholder="e.g. api.example.com"
                  className="flex-1 rounded-lg border border-border bg-surface px-3 py-2 text-sm text-text-primary placeholder:text-text-tertiary focus:border-accent focus:outline-none"
                />
                <button
                  onClick={() => removeDest(i)}
                  className="shrink-0 rounded-lg p-1.5 text-text-tertiary hover:bg-surface-tertiary hover:text-text-primary transition"
                >
                  <TrashIcon className="h-4 w-4" />
                </button>
              </div>
            ))}
          </div>
          <button
            onClick={addDest}
            className="flex items-center gap-1 text-sm text-accent hover:underline mt-2"
          >
            <PlusIcon className="h-4 w-4" />
            Add destination
          </button>
        </div>
      </SectionPanel>
    </div>
  );
}
