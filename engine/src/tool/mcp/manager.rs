use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::core::error::AppError;
use crate::tool::ToolDefinition;
use crate::tool::sandbox::{Sandbox, SandboxManager};

use super::client::{McpClient, default_client_info};
use super::models::{McpServer, McpServerStatus};

pub struct McpConnection {
    pub server_id: String,
    pub slug: String,
    pub user_id: String,
    pub client: McpClient,
    pub tools: Vec<ToolDefinition>,
    pub child: tokio::process::Child,
    pub restart_count: u32,
}

pub struct McpManager {
    connections: Arc<RwLock<HashMap<String, McpConnection>>>,
    sandbox_manager: Arc<SandboxManager>,
    workspaces_path: String,
    cache_path: String,
}

impl McpManager {
    pub fn new(
        sandbox_manager: Arc<SandboxManager>,
        workspaces_path: String,
        cache_path: String,
    ) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            sandbox_manager,
            workspaces_path,
            cache_path,
        }
    }

    pub fn workspaces_path(&self) -> &str {
        &self.workspaces_path
    }

    pub fn cache_path(&self) -> &str {
        &self.cache_path
    }

    /// Sandbox for install-phase package warm-up: network allowed, write access to the
    /// shared cache dir and the per-server workspace so `npx --yes …` / `uv pip install`
    /// can populate them.
    pub fn build_install_sandbox(&self, server: &McpServer) -> Sandbox {
        let sandbox_id = format!("mcp-{}", server.id);
        self.sandbox_manager
            .get_sandbox(&sandbox_id, true, Vec::new())
            .with_read_paths(vec![self.cache_path.clone()])
            .with_write_paths(vec![
                self.cache_path.clone(),
                server.workspace_dir.clone(),
            ])
    }

    /// Sandbox for the long-lived server process. Caches are read-only (they were
    /// populated at install time). Writes are scoped to the server's own workspace dir
    /// so OAuth tokens and local state survive restart.
    pub fn build_run_sandbox(
        &self,
        server: &McpServer,
        resolved_env: Vec<(String, String)>,
    ) -> Sandbox {
        let sandbox_id = format!("mcp-{}", server.id);
        let mut read_paths = vec![self.cache_path.clone()];
        read_paths.extend(server.extra_read_paths.iter().cloned());
        let mut write_paths = vec![server.workspace_dir.clone()];
        write_paths.extend(server.extra_write_paths.iter().cloned());
        self.sandbox_manager
            .get_sandbox(&sandbox_id, true, Vec::new())
            .with_read_paths(read_paths)
            .with_write_paths(write_paths)
            .with_extra_env_vars(resolved_env)
    }

    /// Spawn the server inside its run sandbox, perform the MCP `initialize` handshake
    /// over the child's piped stdio, build namespaced `ToolDefinition`s from the
    /// server's `tools/list`, and register the live connection keyed by `server.id`.
    pub async fn start(
        &self,
        server: &McpServer,
        resolved_env: BTreeMap<String, String>,
    ) -> Result<Vec<ToolDefinition>, AppError> {
        let env_pairs: Vec<(String, String)> = resolved_env.into_iter().collect();
        let sandbox = self.build_run_sandbox(server, env_pairs);
        sandbox.setup()?;

        let args_owned: Vec<String> = server.args.clone();
        let args_refs: Vec<&str> = args_owned.iter().map(|s| s.as_str()).collect();

        let mut child = sandbox.spawn(
            &server.command,
            &args_refs,
            Some(&server.workspace_dir),
            Vec::new(),
            Some(std::process::Stdio::piped()),
            std::process::Stdio::piped(),
            std::process::Stdio::inherit(),
        )?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AppError::Tool("MCP server child stdin missing".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::Tool("MCP server child stdout missing".into()))?;

        let client = McpClient::connect((stdout, stdin), default_client_info()).await?;

        let cached = client.cached_tools().await;
        let tools: Vec<ToolDefinition> = cached
            .into_iter()
            .map(|c| ToolDefinition {
                id: format!("mcp__{}__{}", server.slug, c.name),
                provider_id: format!("mcp:{}", server.slug),
                description: c.description,
                parameters: c.input_schema,
            })
            .collect();

        let connection = McpConnection {
            server_id: server.id.clone(),
            slug: server.slug.clone(),
            user_id: server.user_id.clone(),
            client,
            tools: tools.clone(),
            child,
            restart_count: 0,
        };

        self.connections
            .write()
            .await
            .insert(server.id.clone(), connection);

        Ok(tools)
    }

    /// Gracefully shut down a running connection. Drops the `McpClient` which tears
    /// down the transport (the child sees EOF on stdin) and then explicitly kills the
    /// child if it hasn't exited on its own.
    pub async fn stop(&self, server_id: &str) -> Result<(), AppError> {
        let Some(mut connection) = self.connections.write().await.remove(server_id) else {
            return Ok(());
        };
        connection.client.shutdown().await?;
        let _ = connection.child.kill().await;
        Ok(())
    }

    /// Invoke a remote tool. `tool_name` is the original un-namespaced name as the
    /// server knows it, not the `mcp__{slug}__{name}` form.
    pub async fn call(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<rmcp::model::CallToolResult, AppError> {
        let connections = self.connections.read().await;
        let connection = connections
            .get(server_id)
            .ok_or_else(|| AppError::Tool(format!("MCP server {server_id} not running")))?;
        connection.client.call_tool(tool_name, arguments).await
    }

    /// Flat list of every tool the user is allowed to see, filtered by `allowlist`.
    /// An entry in `allowlist` with an empty `HashSet` value means "every tool from
    /// that slug is allowed"; an absent slug means "none".
    pub async fn tools_for_user(
        &self,
        user_id: &str,
        allowlist: &HashMap<String, HashSet<String>>,
    ) -> Vec<ToolDefinition> {
        let connections = self.connections.read().await;
        let views: Vec<ConnectionView> = connections
            .values()
            .map(|c| ConnectionView {
                user_id: &c.user_id,
                slug: &c.slug,
                tools: &c.tools,
            })
            .collect();
        filter_tools_for_user(&views, user_id, allowlist)
    }

    /// Reverse lookup: given a namespaced `mcp__{slug}__{tool}` id, return the server
    /// id that owns it (if any running connection currently exposes that tool).
    pub async fn server_for_tool(&self, namespaced_name: &str) -> Option<String> {
        let connections = self.connections.read().await;
        for connection in connections.values() {
            if connection.tools.iter().any(|t| t.id == namespaced_name) {
                return Some(connection.server_id.clone());
            }
        }
        None
    }

    /// Return the ids of any connections whose child process has exited. The caller
    /// (supervisor) uses this to decide which connections to restart.
    pub async fn health_check(&self) -> Vec<String> {
        let mut dead = Vec::new();
        let mut connections = self.connections.write().await;
        for (id, connection) in connections.iter_mut() {
            match connection.child.try_wait() {
                Ok(Some(_)) => dead.push(id.clone()),
                Ok(None) => {}
                Err(_) => dead.push(id.clone()),
            }
        }
        dead
    }

    pub async fn is_running(&self, server_id: &str) -> bool {
        self.connections.read().await.contains_key(server_id)
    }
}

/// Transition hint for the service layer: which `McpServerStatus` a connection should
/// land in after a successful `start` call.
pub const STARTED_STATUS: McpServerStatus = McpServerStatus::Running;

struct ConnectionView<'a> {
    user_id: &'a str,
    slug: &'a str,
    tools: &'a [ToolDefinition],
}

fn filter_tools_for_user(
    conns: &[ConnectionView<'_>],
    user_id: &str,
    allowlist: &HashMap<String, HashSet<String>>,
) -> Vec<ToolDefinition> {
    let mut out = Vec::new();
    for conn in conns {
        if conn.user_id != user_id {
            continue;
        }
        let Some(allowed_names) = allowlist.get(conn.slug) else {
            continue;
        };
        for def in conn.tools {
            let name = def.id.rsplit("__").next().unwrap_or(&def.id);
            if allowed_names.is_empty() || allowed_names.contains(name) {
                out.push(def.clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn td(id: &str, provider_id: &str) -> ToolDefinition {
        ToolDefinition {
            id: id.to_string(),
            provider_id: provider_id.to_string(),
            description: String::new(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        }
    }

    fn gmail_tools() -> Vec<ToolDefinition> {
        vec![
            td("mcp__gmail__send", "mcp:gmail"),
            td("mcp__gmail__read", "mcp:gmail"),
            td("mcp__gmail__search", "mcp:gmail"),
        ]
    }

    fn github_tools() -> Vec<ToolDefinition> {
        vec![
            td("mcp__github__create_issue", "mcp:github"),
            td("mcp__github__list_repos", "mcp:github"),
        ]
    }

    #[test]
    fn filter_skips_other_users() {
        let gmail = gmail_tools();
        let github = github_tools();
        let conns = vec![
            ConnectionView {
                user_id: "user-1",
                slug: "gmail",
                tools: &gmail,
            },
            ConnectionView {
                user_id: "user-2",
                slug: "github",
                tools: &github,
            },
        ];

        let mut allowlist = HashMap::new();
        allowlist.insert("gmail".to_string(), HashSet::new());
        allowlist.insert("github".to_string(), HashSet::new());

        let out = filter_tools_for_user(&conns, "user-1", &allowlist);
        let ids: Vec<&str> = out.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids.len(), 3);
        assert!(ids.iter().all(|id| id.starts_with("mcp__gmail__")));
    }

    #[test]
    fn filter_empty_set_means_all_tools_in_that_slug() {
        let gmail = gmail_tools();
        let conns = vec![ConnectionView {
            user_id: "user-1",
            slug: "gmail",
            tools: &gmail,
        }];

        let mut allowlist = HashMap::new();
        allowlist.insert("gmail".to_string(), HashSet::new());

        let out = filter_tools_for_user(&conns, "user-1", &allowlist);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn filter_named_subset() {
        let gmail = gmail_tools();
        let conns = vec![ConnectionView {
            user_id: "user-1",
            slug: "gmail",
            tools: &gmail,
        }];

        let mut allowlist = HashMap::new();
        let mut allowed = HashSet::new();
        allowed.insert("send".to_string());
        allowed.insert("read".to_string());
        allowlist.insert("gmail".to_string(), allowed);

        let out = filter_tools_for_user(&conns, "user-1", &allowlist);
        let ids: Vec<&str> = out.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"mcp__gmail__send"));
        assert!(ids.contains(&"mcp__gmail__read"));
        assert!(!ids.contains(&"mcp__gmail__search"));
    }

    #[test]
    fn filter_absent_slug_yields_nothing() {
        let gmail = gmail_tools();
        let conns = vec![ConnectionView {
            user_id: "user-1",
            slug: "gmail",
            tools: &gmail,
        }];

        let allowlist: HashMap<String, HashSet<String>> = HashMap::new();

        let out = filter_tools_for_user(&conns, "user-1", &allowlist);
        assert!(out.is_empty());
    }

    #[test]
    fn filter_multiple_providers_same_user() {
        let gmail = gmail_tools();
        let github = github_tools();
        let conns = vec![
            ConnectionView {
                user_id: "user-1",
                slug: "gmail",
                tools: &gmail,
            },
            ConnectionView {
                user_id: "user-1",
                slug: "github",
                tools: &github,
            },
        ];

        let mut allowlist = HashMap::new();
        allowlist.insert("gmail".to_string(), HashSet::new());
        let mut gh_allowed = HashSet::new();
        gh_allowed.insert("create_issue".to_string());
        allowlist.insert("github".to_string(), gh_allowed);

        let out = filter_tools_for_user(&conns, "user-1", &allowlist);
        assert_eq!(out.len(), 4);
        let ids: Vec<&str> = out.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"mcp__github__create_issue"));
        assert!(!ids.contains(&"mcp__github__list_repos"));
    }
}
