"use client";

import type { SsoConfig } from "@/lib/config-types";
import { TextInput, NumberInput, Toggle, SensitiveInput, SectionHeader, SectionPanel } from "@/components/settings/field";
import { FingerPrintIcon } from "@heroicons/react/24/outline";

interface SsoSectionProps {
  sso: SsoConfig;
  onChange: (sso: SsoConfig) => void;
}

export function SsoSection({ sso, onChange }: SsoSectionProps) {
  return (
    <div>
      <SectionHeader title="Single Sign-On" description="OpenID Connect SSO configuration" icon={FingerPrintIcon} />
      <SectionPanel>

      <Toggle
        label="Enabled"
        description="Enable SSO authentication"
        value={sso.enabled}
        onChange={(enabled) => onChange({ ...sso, enabled })}
      />

      {sso.enabled && (
        <>
          <TextInput
            label="Authority URL"
            description="OpenID Connect discovery endpoint"
            value={sso.authority}
            onChange={(authority) => onChange({ ...sso, authority })}
            placeholder="https://accounts.google.com"
          />

          <TextInput
            label="Client ID"
            description="OAuth client identifier"
            value={sso.client_id}
            onChange={(client_id) => onChange({ ...sso, client_id })}
            placeholder="your-client-id"
          />

          <SensitiveInput
            label="Client Secret"
            description="OAuth client secret"
            value={sso.client_secret}
            onChange={(client_secret) => onChange({ ...sso, client_secret })}
            placeholder="Enter client secret"
          />

          <TextInput
            label="Scopes"
            description="Space-separated list of OAuth scopes to request"
            value={sso.scopes}
            onChange={(scopes) => onChange({ ...sso, scopes })}
            placeholder="email profile offline_access"
          />

          <Toggle
            label="SSO Only"
            description="Only allow SSO authentication, disabling native login"
            value={sso.only}
            onChange={(only) => onChange({ ...sso, only })}
            warning="Native email/password authentication will be disabled"
          />

          <Toggle
            label="Signups Match Email"
            description="Link SSO accounts to existing users by email address"
            value={sso.signups_match_email}
            onChange={(signups_match_email) => onChange({ ...sso, signups_match_email })}
          />

          <Toggle
            label="Allow Unknown Email Verification"
            description="Trust email verification claims from the SSO provider"
            value={sso.allow_unknown_email_verification}
            onChange={(allow_unknown_email_verification) => onChange({ ...sso, allow_unknown_email_verification })}
          />

          <NumberInput
            label="Client Cache Expiration (seconds)"
            description="How long to cache the OIDC client configuration"
            value={sso.client_cache_expiration}
            onChange={(client_cache_expiration) => onChange({ ...sso, client_cache_expiration })}
            min={0}
          />
        </>
      )}
      </SectionPanel>
    </div>
  );
}
