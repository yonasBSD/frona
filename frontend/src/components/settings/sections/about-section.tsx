"use client";

import { useState, useEffect } from "react";
import { InformationCircleIcon } from "@heroicons/react/24/outline";
import { API_URL, getAccessToken } from "@/lib/api-client";
import { SectionHeader, SectionPanel } from "../field";

export function AboutSection() {
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    async function fetchVersion() {
      try {
        const res = await fetch(`${API_URL}/api/system/version`, {
          headers: { Authorization: `Bearer ${getAccessToken()}` },
          credentials: "include",
        });
        if (res.ok) {
          const data = await res.json();
          setVersion(data.version);
        }
      } catch {
        // ignore
      }
    }
    fetchVersion();
  }, []);

  return (
    <div className="space-y-6">
      <SectionHeader title="About" description="System information" icon={InformationCircleIcon} />

      <SectionPanel title="Version">
        <div>
          <label className="block text-xs font-medium text-text-tertiary mb-1">Server</label>
          <p className="text-sm text-text-primary">{version ?? "..."}</p>
        </div>
      </SectionPanel>
    </div>
  );
}
