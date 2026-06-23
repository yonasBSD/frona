"use client";

import { useState, useEffect, useMemo } from "react";
import { UserCircleIcon } from "@heroicons/react/24/outline";
import { useAuth } from "@/lib/auth";
import { useTheme } from "@/lib/theme";
import { api } from "@/lib/api-client";
import { Field, SectionHeader, SectionPanel } from "../field";
import { ComboboxInput } from "../combobox";

const themeModes = [
  { value: "system" as const, label: "System" },
  { value: "light" as const, label: "Light" },
  { value: "dark" as const, label: "Dark" },
];

interface SystemInfo {
  server_timezone: string;
}

export function ProfileSection() {
  const { user, revalidate } = useAuth();
  const { mode, setMode } = useTheme();
  const [timezone, setTimezone] = useState(user?.timezone ?? "");
  const [saving, setSaving] = useState(false);
  const [timezones, setTimezones] = useState<string[]>([]);
  const [serverTimezone, setServerTimezone] = useState<string>("");

  useEffect(() => {
    api.get<string[]>("/api/system/timezones").then(setTimezones).catch(() => {});
    api.get<SystemInfo>("/api/system/info").then((i) => setServerTimezone(i.server_timezone ?? "")).catch(() => {});
  }, []);

  const effectiveTimezone = timezone || serverTimezone;
  const usingServerDefault = !timezone && !!serverTimezone;

  const timezoneItems = useMemo(
    () => timezones.map((tz) => ({ value: tz, label: tz.replace(/_/g, " ") })),
    [timezones],
  );

  const detectedTimezone = useMemo(() => {
    try {
      return Intl.DateTimeFormat().resolvedOptions().timeZone;
    } catch {
      return null;
    }
  }, []);

  const saveTimezone = async (tz: string) => {
    setTimezone(tz);
    if (!timezones.includes(tz)) return;
    setSaving(true);
    try {
      await api.put("/api/auth/profile", { timezone: tz || null });
      await revalidate();
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="space-y-6">
      <SectionHeader title="Profile" description="Your account information" icon={UserCircleIcon} />

      {user && (
        <SectionPanel title="Account">
          <div className="space-y-4">
            <Field label="Name">
              <p className="text-sm text-text-primary">{user.name}</p>
            </Field>
            <Field label="Username">
              <p className="text-sm text-text-primary">@{user.handle}</p>
            </Field>
            <Field label="Email">
              <p className="text-sm text-text-primary">{user.email}</p>
            </Field>
          </div>
        </SectionPanel>
      )}

      {user && (
        <SectionPanel title="Preferences">
          <div className="space-y-4">
            <div className="space-y-1">
              <ComboboxInput
                label="Timezone"
                value={timezone}
                items={timezoneItems}
                onChange={saveTimezone}
                placeholder="Select timezone..."
                allowFreeText={false}
              />
              {effectiveTimezone && (
                <p className="text-xs text-text-tertiary">
                  {usingServerDefault
                    ? `Currently using server default: ${effectiveTimezone}`
                    : `Currently using: ${effectiveTimezone}`}
                </p>
              )}
              {detectedTimezone && timezone !== detectedTimezone && (
                <button
                  type="button"
                  onClick={() => saveTimezone(detectedTimezone)}
                  className="text-xs text-accent hover:underline"
                >
                  Use detected: {detectedTimezone}
                </button>
              )}
              {saving && (
                <p className="text-xs text-text-tertiary">Saving...</p>
              )}
            </div>

            <Field label="Theme" description="Applies to this browser only.">
              <div className="flex gap-2">
                {themeModes.map(({ value, label }) => (
                  <button
                    key={value}
                    onClick={() => setMode(value)}
                    className={`flex-1 rounded-lg px-3 py-2 text-sm font-medium transition ${
                      mode === value
                        ? "bg-accent text-surface"
                        : "bg-surface text-text-secondary hover:bg-surface-tertiary"
                    }`}
                  >
                    {label}
                  </button>
                ))}
              </div>
            </Field>
          </div>
        </SectionPanel>
      )}
    </div>
  );
}
