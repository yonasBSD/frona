"use client";

import { UserCircleIcon } from "@heroicons/react/24/outline";
import { useAuth } from "@/lib/auth";
import { SectionHeader, SectionPanel } from "../field";

export function ProfileSection() {
  const { user } = useAuth();

  return (
    <div className="space-y-6">
      <SectionHeader title="Profile" description="Your account information" icon={UserCircleIcon} />

      {user && (
        <SectionPanel title="Account">
          <div className="space-y-3">
            <div>
              <label className="block text-xs font-medium text-text-tertiary mb-1">Name</label>
              <p className="text-sm text-text-primary">{user.name}</p>
            </div>
            <div>
              <label className="block text-xs font-medium text-text-tertiary mb-1">Username</label>
              <p className="text-sm text-text-primary">@{user.username}</p>
            </div>
            <div>
              <label className="block text-xs font-medium text-text-tertiary mb-1">Email</label>
              <p className="text-sm text-text-primary">{user.email}</p>
            </div>
          </div>
        </SectionPanel>
      )}
    </div>
  );
}
