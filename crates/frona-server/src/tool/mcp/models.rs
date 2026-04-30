use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use surrealdb::types::SurrealValue;

use crate::Entity;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "lowercase")]
#[surreal(crate = "surrealdb::types", lowercase)]
pub enum McpRuntime {
    Npm,
    Pypi,
    Binary,
}

impl std::fmt::Display for McpRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Npm => write!(f, "npm"),
            Self::Pypi => write!(f, "pypi"),
            Self::Binary => write!(f, "binary"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub struct McpPackage {
    pub runtime: McpRuntime,
    /// Package name, or an absolute path when `runtime == Binary`.
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "lowercase")]
#[surreal(crate = "surrealdb::types", lowercase)]
pub enum McpServerStatus {
    Installed,
    Starting,
    Running,
    Stopped,
    Failed,
}

impl std::fmt::Display for McpServerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Installed => write!(f, "installed"),
            Self::Starting => write!(f, "starting"),
            Self::Running => write!(f, "running"),
            Self::Stopped => write!(f, "stopped"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub struct McpServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub struct CachedMcpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub enum TransportConfig {
    Stdio {
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
    },
    Http {
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        port_env_var: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        endpoint_path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
}

impl TransportConfig {
    pub fn args(&self) -> &[String] {
        match self {
            Self::Stdio { args, .. } | Self::Http { args, .. } => args,
        }
    }

    pub fn env(&self) -> &BTreeMap<String, String> {
        match self {
            Self::Stdio { env, .. } | Self::Http { env, .. } => env,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue, Entity)]
#[surreal(crate = "surrealdb::types")]
#[entity(table = "mcp_server")]
pub struct McpServer {
    pub id: String,
    pub user_id: String,

    /// Immutable: changing it would orphan `mcp__{slug}__{tool}` ids stored in `Agent.tools`.
    pub slug: String,
    pub display_name: String,
    pub description: Option<String>,
    pub repository_url: Option<String>,
    pub registry_id: Option<String>,
    pub server_info: Option<McpServerInfo>,

    pub package: McpPackage,
    pub command: String,
    pub args: Vec<String>,
    /// Per-transport invocation configs (args, env, port, path).
    #[serde(default)]
    pub transports: Vec<TransportConfig>,
    /// Which transport the user has selected: "stdio", "streamable-http", "sse".
    pub active_transport: String,
    pub env: BTreeMap<String, String>,
    pub status: McpServerStatus,
    pub tool_cache: Vec<CachedMcpTool>,
    pub workspace_dir: String,

    pub installed_at: DateTime<Utc>,
    pub last_started_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

/// Wire-format binding the install API accepts. Translated server-side into
/// rows on `principal_credential_binding`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialBinding {
    pub connection_id: String,
    pub vault_item_id: String,
    pub env_var: String,
    pub field: crate::credential::vault::models::VaultField,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpServerInstall {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name_override: Option<String>,
    #[serde(default)]
    pub credentials: Vec<CredentialBinding>,
    #[serde(default)]
    pub extra_env: BTreeMap<String, String>,
    /// Reconciled into Cedar policies on install.
    #[serde(default)]
    pub sandbox_policy: Option<crate::policy::sandbox::SandboxPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpServerUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials: Option<Vec<CredentialBinding>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_env: Option<BTreeMap<String, String>>,
    /// When present, re-reconciles Cedar policies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_policy: Option<crate::policy::sandbox::SandboxPolicy>,
    pub active_transport: Option<String>,
}

/// Returns `"_"` when the input would otherwise produce an empty slug.
pub fn sanitize_slug(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_was_underscore = false;
    for c in input.chars() {
        let lowered = c.to_ascii_lowercase();
        if lowered.is_ascii_lowercase() || lowered.is_ascii_digit() {
            out.push(lowered);
            last_was_underscore = false;
        } else if !last_was_underscore {
            out.push('_');
            last_was_underscore = true;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "_".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_slug_basic() {
        assert_eq!(sanitize_slug("Google Workspace"), "google_workspace");
        assert_eq!(sanitize_slug("GitHub"), "github");
        assert_eq!(sanitize_slug("my-tool-2"), "my_tool_2");
    }

    #[test]
    fn sanitize_slug_collapses_runs() {
        assert_eq!(sanitize_slug("hello   world"), "hello_world");
        assert_eq!(sanitize_slug("foo--bar__baz"), "foo_bar_baz");
    }

    #[test]
    fn sanitize_slug_trims_edges() {
        assert_eq!(sanitize_slug("  spaced  "), "spaced");
        assert_eq!(sanitize_slug("___internal___"), "internal");
    }

    #[test]
    fn sanitize_slug_handles_registry_id() {
        assert_eq!(
            sanitize_slug("io.github.taylorwilsdon/google_workspace_mcp"),
            "io_github_taylorwilsdon_google_workspace_mcp"
        );
    }

    #[test]
    fn sanitize_slug_empty_maps_to_underscore() {
        assert_eq!(sanitize_slug(""), "_");
        assert_eq!(sanitize_slug("!@#$%"), "_");
    }

    #[test]
    fn sanitize_slug_unicode_stripped() {
        assert_eq!(sanitize_slug("héllo"), "h_llo");
        assert_eq!(sanitize_slug("日本語"), "_");
    }

    #[test]
    fn runtime_display_matches_serde() {
        assert_eq!(McpRuntime::Npm.to_string(), "npm");
        assert_eq!(McpRuntime::Pypi.to_string(), "pypi");
        assert_eq!(McpRuntime::Binary.to_string(), "binary");
    }

    #[test]
    fn runtime_serde_round_trip() {
        let npm = serde_json::to_string(&McpRuntime::Npm).unwrap();
        assert_eq!(npm, "\"npm\"");
        let parsed: McpRuntime = serde_json::from_str("\"pypi\"").unwrap();
        assert_eq!(parsed, McpRuntime::Pypi);
    }

    #[test]
    fn status_display_matches_serde() {
        assert_eq!(McpServerStatus::Installed.to_string(), "installed");
        assert_eq!(McpServerStatus::Running.to_string(), "running");
        assert_eq!(McpServerStatus::Failed.to_string(), "failed");
    }

    #[test]
    fn status_serde_round_trip() {
        let json = serde_json::to_string(&McpServerStatus::Running).unwrap();
        assert_eq!(json, "\"running\"");
        let parsed: McpServerStatus = serde_json::from_str("\"installed\"").unwrap();
        assert_eq!(parsed, McpServerStatus::Installed);
    }

    #[test]
    fn install_dto_accepts_registry_id_only() {
        let json = serde_json::json!({ "registry_id": "io.github.foo/bar" });
        let parsed: McpServerInstall = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.registry_id.as_deref(), Some("io.github.foo/bar"));
        assert!(parsed.manifest.is_none());
        assert!(parsed.display_name_override.is_none());
    }

    #[test]
    fn install_dto_accepts_manifest_only() {
        let json = serde_json::json!({
            "manifest": { "name": "foo", "packages": [] }
        });
        let parsed: McpServerInstall = serde_json::from_value(json).unwrap();
        assert!(parsed.manifest.is_some());
    }
}
