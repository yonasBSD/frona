"use client";

import { useState, useEffect, useCallback } from "react";
import { RestartBanner } from "@/components/settings/restart-banner";
import { SettingsProvider } from "@/components/settings/settings-context";
import type { SectionHandlers } from "@/components/settings/settings-context";
import { ProfileSection } from "@/components/settings/sections/profile-section";
import { ThemeSection } from "@/components/settings/sections/theme-section";
import { ProvidersSection } from "@/components/settings/sections/providers-section";
import { ModelsSection } from "@/components/settings/sections/models-section";
import { ServerSection } from "@/components/settings/sections/server-section";
import { AuthSection } from "@/components/settings/sections/auth-section";
import { SsoSection } from "@/components/settings/sections/sso-section";
import { BrowserSection } from "@/components/settings/sections/browser-section";
import { SearchSection } from "@/components/settings/sections/search-section";
import { VoiceSection } from "@/components/settings/sections/voice-section";
import { VaultSection } from "@/components/settings/sections/vault-section";
import { AdvancedSection } from "@/components/settings/sections/advanced-section";
import { SkillsSection } from "@/components/settings/sections/skills-section";
import { AboutSection } from "@/components/settings/sections/about-section";
import { getConfig, updateConfig } from "@/lib/config-types";
import type { Config } from "@/lib/config-types";

const TABS = [
  { id: "profile", label: "Profile", group: "user", saveable: false },
  { id: "theme", label: "Theme", group: "user", saveable: false },
  { id: "providers", label: "Providers", group: "config", saveable: true },
  { id: "models", label: "Models", group: "config", saveable: true },
  { id: "skills", label: "Skills", group: "config", saveable: false },
  { id: "search", label: "Search", group: "config", saveable: true },
  { id: "voice", label: "Voice", group: "config", saveable: true },
  { id: "browser", label: "Browser", group: "config", saveable: true },
  { id: "vault", label: "Vault", group: "config", saveable: true },
  { id: "auth", label: "Authentication", group: "config", saveable: true },
  { id: "sso", label: "Single Sign-On", group: "config", saveable: true },
  { id: "server", label: "Server", group: "config", saveable: true },
  { id: "advanced", label: "Advanced", group: "config", saveable: true },
  { id: "about", label: "About", group: "config", saveable: false },
] as const;

type TabId = (typeof TABS)[number]["id"];

export default function SettingsPage() {
  const [config, setConfig] = useState<Config | null>(null);
  const [patch, setPatch] = useState<Record<string, unknown>>({});
  const [activeTab, setActiveTabState] = useState<TabId>(() => {
    if (typeof window !== "undefined") {
      const hash = window.location.hash.slice(1);
      if (TABS.some((t) => t.id === hash)) return hash as TabId;
    }
    return "profile";
  });

  const setActiveTab = useCallback((tab: TabId) => {
    setActiveTabState(tab);
    window.history.replaceState(null, "", `#${tab}`);
  }, []);

  useEffect(() => {
    const hash = window.location.hash.slice(1);
    if (TABS.some((t) => t.id === hash)) setActiveTabState(hash as TabId);
  }, []);

  const [saving, setSaving] = useState(false);
  const [showRestart, setShowRestart] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [configLoading, setConfigLoading] = useState(true);
  const [sectionModified, setSectionModified] = useState(false);
  const [sectionHandlers, setSectionHandlers] = useState<Map<string, SectionHandlers>>(new Map());

  const activeGroup = TABS.find((t) => t.id === activeTab)?.group;
  const isConfigTab = activeGroup === "config";

  const loadConfig = useCallback(async () => {
    try {
      const cfg = await getConfig();
      setConfig(cfg);
    } catch {
      setError("Failed to load configuration");
    } finally {
      setConfigLoading(false);
    }
  }, []);

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  const hasPendingChanges = Object.keys(patch).length > 0 || sectionModified;

  const handleSave = useCallback(async () => {
    if (!hasPendingChanges) return;
    setSaving(true);
    setError(null);
    try {
      if (Object.keys(patch).length > 0) {
        const result = await updateConfig(patch);
        setConfig(result.config);
        setPatch({});
        if (result.restart_required) setShowRestart(true);
      }
      for (const handler of sectionHandlers.values()) {
        await handler.save();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save");
    } finally {
      setSaving(false);
    }
  }, [patch, hasPendingChanges, sectionHandlers]);

  const handleDiscard = useCallback(() => {
    setPatch({});
    loadConfig();
    for (const handler of sectionHandlers.values()) {
      handler.discard();
    }
  }, [loadConfig, sectionHandlers]);

  const handleRefresh = useCallback(async () => {
    setPatch({});
    await loadConfig();
  }, [loadConfig]);

  const updatePatch = useCallback((section: string, value: unknown) => {
    setPatch((prev) => ({ ...prev, [section]: value }));
    setConfig((prev) => prev ? { ...prev, [section]: value } as Config : prev);
  }, []);

  const userTabs = TABS.filter((t) => t.group === "user");
  const configTabs = TABS.filter((t) => t.group === "config");

  return (
    <SettingsProvider
      onRefresh={handleRefresh}
      onModifiedChange={setSectionModified}
      onHandlersChange={setSectionHandlers}
    >
      <div className="flex h-full bg-surface">
        {/* Sidebar — matches nav panel style */}
        <div className="border-r border-border bg-surface-nav p-4 flex flex-col" style={{ width: 289 }}>
          <h2 className="text-lg font-semibold text-text-primary mb-4">Settings</h2>
          <nav className="space-y-1 flex-1">
            {userTabs.map((t) => (
              <button
                key={t.id}
                onClick={() => setActiveTab(t.id)}
                className={`w-full text-left rounded-lg px-3 py-2 text-sm transition ${
                  activeTab === t.id
                    ? "bg-accent/10 text-accent font-medium"
                    : "text-text-secondary hover:bg-surface-tertiary hover:text-text-primary"
                }`}
              >
                {t.label}
              </button>
            ))}
            <div className="border-b border-border my-2" />
            {configTabs.map((t) => (
              <button
                key={t.id}
                onClick={() => setActiveTab(t.id)}
                className={`w-full text-left rounded-lg px-3 py-2 text-sm transition ${
                  activeTab === t.id
                    ? "bg-accent/10 text-accent font-medium"
                    : "text-text-secondary hover:bg-surface-tertiary hover:text-text-primary"
                }`}
              >
                {t.label}
              </button>
            ))}
          </nav>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto">
          <div className="max-w-2xl mx-auto p-8 space-y-6">
            {showRestart && <RestartBanner visible={showRestart} />}

            {error && isConfigTab && (
              <div className="rounded-lg bg-error-bg p-3 text-sm text-error-text">{error}</div>
            )}

            <div className="min-h-[400px]">
              {activeTab === "profile" && <ProfileSection />}
              {activeTab === "theme" && <ThemeSection />}
              {isConfigTab && configLoading && (
                <p className="text-sm text-text-tertiary">Loading configuration...</p>
              )}

              {isConfigTab && !configLoading && !config && (
                <p className="text-sm text-error-text">{error || "Failed to load configuration"}</p>
              )}

              {activeTab === "skills" && <SkillsSection />}

              {config && (
                <>
                  {activeTab === "providers" && (
                    <ProvidersSection
                      providers={config.providers}
                      onChange={(v) => updatePatch("providers", v)}
                    />
                  )}
                  {activeTab === "models" && (
                    <ModelsSection
                      models={config.models}
                      enabledProviders={Object.keys(config.providers)}
                      providerConfigs={config.providers}
                      onChange={(v) => updatePatch("models", v)}
                    />
                  )}
                  {activeTab === "server" && (
                    <ServerSection
                      server={config.server}
                      onChange={(v) => updatePatch("server", v)}
                    />
                  )}
                  {activeTab === "auth" && (
                    <AuthSection
                      auth={config.auth}
                      onChange={(v) => updatePatch("auth", v)}
                    />
                  )}
                  {activeTab === "sso" && (
                    <SsoSection
                      sso={config.sso}
                      onChange={(v) => updatePatch("sso", v)}
                    />
                  )}
                  {activeTab === "browser" && (
                    <BrowserSection
                      browser={config.browser}
                      onChange={(v) => updatePatch("browser", v)}
                    />
                  )}
                  {activeTab === "search" && (
                    <SearchSection
                      search={config.search}
                      onChange={(v) => updatePatch("search", v)}
                    />
                  )}
                  {activeTab === "voice" && (
                    <VoiceSection
                      voice={config.voice}
                      onChange={(v) => updatePatch("voice", v)}
                    />
                  )}
                  {activeTab === "vault" && (
                    <VaultSection
                      vault={config.vault}
                      onChange={(v) => updatePatch("vault", v)}
                    />
                  )}
                  {activeTab === "about" && <AboutSection />}
                  {activeTab === "advanced" && (
                    <AdvancedSection
                      inference={config.inference}
                      scheduler={config.scheduler}
                      app={config.app}
                      onChange={(update) => {
                        if (update.inference) updatePatch("inference", update.inference);
                        if (update.scheduler) updatePatch("scheduler", update.scheduler);
                        if (update.app) updatePatch("app", update.app);
                      }}
                    />
                  )}
                </>
              )}
            </div>

            {/* Save bar — only for config tabs */}
            {TABS.find((t) => t.id === activeTab)?.saveable && config && (
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
    </SettingsProvider>
  );
}
