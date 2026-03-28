"use client";

import { useState, useEffect, useCallback, useRef, Suspense } from "react";
import { useSearchParams, useRouter } from "next/navigation";
import { ArrowLeftIcon } from "@heroicons/react/24/outline";
import { api } from "@/lib/api-client";
import { useNavigation } from "@/lib/navigation-context";
import type { Agent } from "@/lib/types";
import { agentDisplayName } from "@/lib/types";
import { ProfileSection } from "@/components/agents/configure/profile-section";
import { InstructionsSection } from "@/components/agents/configure/instructions-section";
import { ModelSection } from "@/components/agents/configure/model-section";
import { ToolsSection } from "@/components/agents/configure/tools-section";
import { SkillsSection } from "@/components/agents/configure/skills-section";
import type { SkillBrowserHandle } from "@/components/skills/skill-browser";
import { SandboxSection } from "@/components/agents/configure/sandbox-section";
import { CredsSection } from "@/components/agents/configure/creds-section";

const SECTIONS = [
  { id: "profile", label: "Profile" },
  { id: "model", label: "Model" },
  { id: "prompt", label: "Prompt" },
  { id: "tools", label: "Tools" },
  { id: "skills", label: "Skills" },
  { id: "sandbox", label: "Sandbox" },
  { id: "creds", label: "Credentials" },
] as const;

type SectionId = (typeof SECTIONS)[number]["id"];

function AgentSettings() {
  const searchParams = useSearchParams();
  const router = useRouter();
  const { updateAgent } = useNavigation();
  const agentId = searchParams.get("id");

  const [agent, setAgent] = useState<Agent | null>(null);
  const [patch, setPatch] = useState<Record<string, unknown>>({});
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const sectionParam = searchParams.get("section");
  const initialSection = SECTIONS.some((s) => s.id === sectionParam) ? (sectionParam as SectionId) : "profile";
  const [activeSection, setActiveSectionState] = useState<SectionId>(initialSection);

  const setActiveSection = useCallback((id: SectionId) => {
    setActiveSectionState(id);
    const params = new URLSearchParams(searchParams.toString());
    params.set("section", id);
    router.replace(`/agents?${params.toString()}`);
  }, [searchParams, router]);
  const [hasAgentRemovals, setHasAgentRemovals] = useState(false);
  const skillBrowserRef = useRef<SkillBrowserHandle>(null);

  useEffect(() => {
    if (!agentId) return;
    setLoading(true);
    api
      .get<Agent>(`/api/agents/${agentId}`)
      .then(setAgent)
      .catch(() => setError("Agent not found"))
      .finally(() => setLoading(false));
  }, [agentId]);

  const merged = agent ? { ...agent, ...patch } : null;
  const hasPendingChanges = Object.keys(patch).length > 0 || hasAgentRemovals;

  const doSave = useCallback(async () => {
    if (!hasPendingChanges || !agent || !agentId) return;
    setSaving(true);
    setError(null);
    try {
      const payload = { ...patch };
      if (typeof payload.prompt === "string" && payload.prompt === agent.default_prompt) {
        payload.prompt = null;
      }
      if (payload.skills === null) {
        payload.skills = ["*"];
      }
      const updated = await api.put<Agent>(`/api/agents/${agentId}`, payload);
      setAgent(updated);
      setPatch({});
      updateAgent(agentId, { ...updated });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save");
    } finally {
      setSaving(false);
    }
  }, [patch, hasPendingChanges, agent, agentId, updateAgent]);

  const handleSave = useCallback(() => {
    if (skillBrowserRef.current) {
      skillBrowserRef.current.confirmRemovals(doSave);
    } else {
      doSave();
    }
  }, [doSave]);

  const handleDiscard = useCallback(() => {
    setPatch({});
    skillBrowserRef.current?.resetRemovals();
  }, []);

  const update = useCallback((fields: Record<string, unknown>) => {
    setPatch((prev) => ({ ...prev, ...fields }));
  }, []);

  if (!agentId) {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="text-sm text-text-tertiary">No agent selected</p>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="text-sm text-text-tertiary">Loading...</p>
      </div>
    );
  }

  if (!agent || !merged) {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="text-sm text-error-text">{error || "Agent not found"}</p>
      </div>
    );
  }

  return (
    <div className="flex h-full bg-surface">
      {/* Sidebar */}
      <div
        className="border-r border-border bg-surface-nav p-4 flex flex-col"
        style={{ width: 289 }}
      >
        <button
          onClick={() => router.push("/chat")}
          className="flex items-center gap-2 text-sm text-text-secondary hover:text-text-primary transition mb-4"
        >
          <ArrowLeftIcon className="h-4 w-4" />
          Back
        </button>
        <h2 className="text-lg font-semibold text-text-primary mb-4 truncate">
          {agentDisplayName(agent.id, agent.name)}
        </h2>
        <nav className="space-y-1 flex-1">
          {SECTIONS.map((s) => (
            <button
              key={s.id}
              onClick={() => setActiveSection(s.id)}
              className={`w-full text-left rounded-lg px-3 py-2 text-sm transition ${
                activeSection === s.id
                  ? "bg-accent/10 text-accent font-medium"
                  : "text-text-secondary hover:bg-surface-tertiary hover:text-text-primary"
              }`}
            >
              {s.label}
            </button>
          ))}
        </nav>
      </div>

      {/* Content */}
      <div className={`flex-1 ${activeSection === "prompt" ? "flex flex-col" : "overflow-y-auto"}`}>
        <div className={`max-w-2xl mx-auto p-8 space-y-6 ${activeSection === "prompt" ? "flex-1 flex flex-col" : ""}`}>
          {error && (
            <div className="rounded-lg bg-error-bg p-3 text-sm text-error-text">{error}</div>
          )}

          <div className={activeSection === "prompt" ? "flex-1 flex flex-col" : ""}>
            {activeSection === "profile" && (
              <ProfileSection
                description={(merged.description as string) ?? ""}
                enabled={(merged.enabled as boolean) ?? true}
                identity={(merged.identity as Record<string, string>) ?? {}}
                onChange={update}
                onIdentityChange={(v) => update({ identity: v })}
              />
            )}
            {activeSection === "model" && (
              <ModelSection
                modelGroup={(merged.model_group as string) ?? "primary"}
                onModelGroupChange={(v) => update({ model_group: v })}
              />
            )}
            {activeSection === "prompt" && (
              <InstructionsSection
                prompt={((merged.prompt as string) ?? agent.default_prompt)}
                onPromptChange={(v) => update({ prompt: v })}
              />
            )}
            {activeSection === "tools" && (
              <ToolsSection
                tools={(merged.tools as string[]) ?? []}
                onChange={(v) => update({ tools: v })}
              />
            )}
            {activeSection === "skills" && (
              <SkillsSection
                ref={skillBrowserRef}
                agentId={agentId}
                skills={"skills" in patch ? (patch.skills as string[] | null) : (agent.skills ?? null)}
                onSkillsChange={(v) => update({ skills: v })}
                onAgentRemovalsChange={setHasAgentRemovals}
              />
            )}
            {activeSection === "sandbox" && (
              <SandboxSection
                sandbox={merged.sandbox_config as { network_access: boolean; allowed_network_destinations: string[]; timeout_secs: number } | null}
                onChange={(v) => update({ sandbox_config: v })}
              />
            )}
            {activeSection === "creds" && <CredsSection agentId={agentId} />}
          </div>

          {/* Save bar */}
          {activeSection !== "creds" && (
            <div className="pt-4 pb-2 border-t border-border flex items-center justify-end gap-2">
              <button
                onClick={handleDiscard}
                disabled={!hasPendingChanges}
                className="w-28 rounded-lg border border-border py-2 text-sm font-medium text-text-secondary hover:bg-surface-tertiary disabled:opacity-50 transition"
              >
                Discard
              </button>
              <button
                onClick={handleSave}
                disabled={!hasPendingChanges || saving}
                className="w-28 rounded-lg bg-accent py-2 text-sm font-medium text-surface hover:bg-accent-hover disabled:opacity-50 transition"
              >
                {saving ? "Saving..." : "Save"}
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default function AgentSettingsPage() {
  return (
    <Suspense fallback={<div className="flex h-full items-center justify-center"><p className="text-sm text-text-tertiary">Loading...</p></div>}>
      <AgentSettings />
    </Suspense>
  );
}
