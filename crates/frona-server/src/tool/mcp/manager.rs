use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

use crate::core::error::AppError;
use crate::tool::ToolDefinition;
use crate::tool::sandbox::{Sandbox, SandboxManager};

use super::client::{McpClient, default_client_info};
use super::models::{McpServer, McpServerStatus, TransportConfig};

pub struct McpConnection {
    pub server_id: String,
    pub slug: String,
    pub user_id: String,
    pub client: McpClient,
    pub tools: Vec<ToolDefinition>,
    pub child: Option<tokio::process::Child>,
    pub port: Option<u16>,
    pub restart_count: u32,
    pub log_path: Option<std::path::PathBuf>,
}

pub struct McpManager {
    connections: Arc<RwLock<HashMap<String, McpConnection>>>,
    sandbox_manager: Arc<SandboxManager>,
    workspaces_path: String,
    allocated_ports: Arc<Mutex<HashSet<u16>>>,
    port_range: (u16, u16),
}

impl McpManager {
    pub fn new(
        sandbox_manager: Arc<SandboxManager>,
        workspaces_path: String,
        port_range_start: u16,
        port_range_end: u16,
    ) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            sandbox_manager,
            workspaces_path,
            allocated_ports: Arc::new(Mutex::new(HashSet::new())),
            port_range: (port_range_start, port_range_end),
        }
    }

    async fn allocate_port(&self) -> Result<u16, AppError> {
        let mut ports = self.allocated_ports.lock().await;
        for port in self.port_range.0..self.port_range.1 {
            if !ports.contains(&port) {
                ports.insert(port);
                return Ok(port);
            }
        }
        Err(AppError::Tool("No available ports for MCP HTTP server".into()))
    }

    async fn release_port(&self, port: u16) {
        self.allocated_ports.lock().await.remove(&port);
    }

    pub fn workspaces_path(&self) -> &str {
        &self.workspaces_path
    }

    /// Sandbox for install-phase package warm-up: network allowed, write access to the
    /// shared cache dir and the per-server workspace so `npx --yes …` / `uv pip install`
    /// can populate them.
    pub fn build_install_sandbox(&self, server: &McpServer) -> Sandbox {
        self.mcp_sandbox(server)
            .with_extra_env_vars(package_manager_env_vars(&server.workspace_dir))
    }

    pub fn build_run_sandbox(
        &self,
        server: &McpServer,
        resolved_env: Vec<(String, String)>,
    ) -> Sandbox {
        let mut env = package_manager_env_vars(&server.workspace_dir);
        env.extend(resolved_env);
        self.mcp_sandbox(server)
            .with_read_paths(server.extra_read_paths.to_vec())
            .with_write_paths(server.extra_write_paths.to_vec())
            .with_extra_env_vars(env)
    }

    fn mcp_sandbox(&self, server: &McpServer) -> Sandbox {
        let sandbox_id = format!("mcp-{}", server.id);
        self.sandbox_manager
            .sandbox_at(
                std::path::PathBuf::from(&server.workspace_dir),
                &sandbox_id,
                true,
                Vec::new(),
            )
            .without_venv()
            .without_node()
    }

    pub async fn start(
        &self,
        server: &McpServer,
        resolved_env: BTreeMap<String, String>,
    ) -> Result<Vec<ToolDefinition>, AppError> {
        let active = server.active_transport.as_str();
        tracing::info!(
            server_id = %server.id,
            active_transport = %active,
            transport_count = server.transports.len(),
            "starting MCP server"
        );
        let config = server.transports.iter().find(|t| match t {
            TransportConfig::Stdio { .. } => active == "stdio",
            TransportConfig::Http { .. } => active == "streamable-http" || active == "sse",
        });

        match config {
            Some(TransportConfig::Http { url, port_env_var, endpoint_path, args, env }) => {
                if let Some(url) = url.as_ref().filter(|u| !u.is_empty()) {
                    self.start_remote_http(server, url.clone()).await
                } else {
                    self.start_local_http(server, resolved_env, args, env, port_env_var.as_deref(), endpoint_path.as_deref()).await
                }
            }
            other => self.start_stdio(server, resolved_env, other).await,
        }
    }

    async fn start_stdio(
        &self,
        server: &McpServer,
        resolved_env: BTreeMap<String, String>,
        config: Option<&TransportConfig>,
    ) -> Result<Vec<ToolDefinition>, AppError> {
        let mut env_pairs: Vec<(String, String)> = resolved_env.into_iter().collect();
        if let Some(TransportConfig::Stdio { env, .. }) = config {
            env_pairs.extend(env.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        let sandbox = self.build_run_sandbox(server, env_pairs);
        sandbox.setup()?;

        let args_owned: Vec<String> = config
            .and_then(|c| if c.args().is_empty() { None } else { Some(c.args().to_vec()) })
            .unwrap_or_else(|| server.args.clone());
        let args_refs: Vec<&str> = args_owned.iter().map(|s| s.as_str()).collect();

        let log_dir = std::path::Path::new(&server.workspace_dir).join("logs");
        std::fs::create_dir_all(&log_dir).ok();
        let log_path = log_dir.join("server.log");
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| AppError::Tool(format!("opening {}: {e}", log_path.display())))?;

        let mut child = sandbox.spawn(
            &server.command,
            &args_refs,
            Some(&server.workspace_dir),
            Vec::new(),
            Some(std::process::Stdio::piped()),
            std::process::Stdio::piped(),
            std::process::Stdio::from(log_file),
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
        self.register_connection(server, client, Some(child), None, None).await
    }

    async fn start_local_http(
        &self,
        server: &McpServer,
        resolved_env: BTreeMap<String, String>,
        config_args: &[String],
        config_env: &BTreeMap<String, String>,
        port_env_var: Option<&str>,
        endpoint_path: Option<&str>,
    ) -> Result<Vec<ToolDefinition>, AppError> {
        let port = self.allocate_port().await?;
        let port_var = port_env_var.unwrap_or("PORT");
        let mut env_pairs: Vec<(String, String)> = resolved_env.into_iter().collect();
        env_pairs.push((port_var.to_string(), port.to_string()));
        env_pairs.extend(config_env.iter().map(|(k, v)| (k.clone(), v.clone())));

        let sandbox = self.build_run_sandbox(server, env_pairs)
            .with_bind_ports(vec![port]);
        sandbox.setup()?;

        let args_owned: Vec<String> = if config_args.is_empty() { server.args.clone() } else { config_args.to_vec() };
        tracing::info!(
            command = %server.command,
            args = ?args_owned,
            port = port,
            port_var = %port_var,
            endpoint_path = ?endpoint_path,
            "start_local_http spawning"
        );
        let args_refs: Vec<&str> = args_owned.iter().map(|s| s.as_str()).collect();

        let log_dir = std::path::Path::new(&server.workspace_dir).join("logs");
        std::fs::create_dir_all(&log_dir).ok();
        let log_path = log_dir.join("server.log");
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| AppError::Tool(format!("opening {}: {e}", log_path.display())))?;
        let log_file_clone = log_file.try_clone()
            .map_err(|e| AppError::Tool(format!("cloning log fd: {e}")))?;

        let child = sandbox.spawn(
            &server.command,
            &args_refs,
            Some(&server.workspace_dir),
            Vec::new(),
            None,
            std::process::Stdio::from(log_file),
            std::process::Stdio::from(log_file_clone),
        )?;

        let path = endpoint_path.unwrap_or("/mcp");
        let url = format!("http://127.0.0.1:{port}{path}");
        if let Err(e) = self.wait_for_ready(port, path, std::time::Duration::from_secs(30)).await {
            self.release_port(port).await;
            return Err(e);
        }

        let transport = rmcp::transport::streamable_http_client::StreamableHttpClientTransport::from_uri(url.as_str());
        let client = McpClient::connect(transport, default_client_info()).await
            .inspect_err(|_| {
                let allocated = self.allocated_ports.clone();
                tokio::spawn(async move { allocated.lock().await.remove(&port); });
            })?;

        self.register_connection(server, client, Some(child), Some(port), Some(log_path)).await
    }

    async fn start_remote_http(
        &self,
        server: &McpServer,
        url: String,
    ) -> Result<Vec<ToolDefinition>, AppError> {
        let transport = rmcp::transport::streamable_http_client::StreamableHttpClientTransport::from_uri(url.as_str());
        let client = McpClient::connect(transport, default_client_info()).await?;
        self.register_connection(server, client, None, None, None).await
    }

    async fn register_connection(
        &self,
        server: &McpServer,
        client: McpClient,
        child: Option<tokio::process::Child>,
        port: Option<u16>,
        log_path_override: Option<std::path::PathBuf>,
    ) -> Result<Vec<ToolDefinition>, AppError> {
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

        let log_path = log_path_override.unwrap_or_else(|| {
            std::path::PathBuf::from(&server.workspace_dir).join("logs").join("server.log")
        });

        let connection = McpConnection {
            server_id: server.id.clone(),
            slug: server.slug.clone(),
            user_id: server.user_id.clone(),
            client,
            tools: tools.clone(),
            child,
            port,
            restart_count: 0,
            log_path: Some(log_path),
        };

        self.connections
            .write()
            .await
            .insert(server.id.clone(), connection);

        Ok(tools)
    }

    async fn wait_for_ready(&self, port: u16, path: &str, timeout: std::time::Duration) -> Result<(), AppError> {
        let url = format!("http://127.0.0.1:{port}{path}");
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .map_err(|e| AppError::Tool(format!("http client: {e}")))?;
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if tokio::time::Instant::now() > deadline {
                return Err(AppError::Tool("MCP HTTP server did not become ready in time".into()));
            }
            if client.get(&url).send().await.is_ok() {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    }

    /// Gracefully shut down a running connection. Drops the `McpClient` which tears
    /// down the transport (the child sees EOF on stdin) and then explicitly kills the
    /// child if it hasn't exited on its own.
    pub async fn stop(&self, server_id: &str) -> Result<(), AppError> {
        let Some(mut connection) = self.connections.write().await.remove(server_id) else {
            return Ok(());
        };
        connection.client.shutdown().await?;
        if let Some(ref mut child) = connection.child {
            let _ = child.kill().await;
        }
        if let Some(port) = connection.port {
            self.release_port(port).await;
        }
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
            if let Some(ref mut child) = connection.child {
                match child.try_wait() {
                    Ok(Some(_)) => dead.push(id.clone()),
                    Ok(None) => {}
                    Err(_) => dead.push(id.clone()),
                }
            }
        }
        dead
    }

    pub async fn is_running(&self, server_id: &str) -> bool {
        self.connections.read().await.contains_key(server_id)
    }

    pub async fn read_logs(&self, server_id: &str, max_bytes: u64) -> String {
        let log_path = {
            let conns = self.connections.read().await;
            conns.get(server_id).and_then(|c| c.log_path.clone())
        };
        match log_path {
            Some(path) => read_log_file(&path, max_bytes),
            None => {
                let fallback = std::path::PathBuf::from(self.workspaces_path())
                    .join(server_id)
                    .join("logs")
                    .join("server.log");
                read_log_file(&fallback, max_bytes)
            }
        }
    }

    pub async fn connections_mut(
        &self,
    ) -> tokio::sync::RwLockWriteGuard<'_, std::collections::HashMap<String, McpConnection>> {
        self.connections.write().await
    }

    pub async fn restart_count(&self, server_id: &str) -> u32 {
        self.connections
            .read()
            .await
            .get(server_id)
            .map(|c| c.restart_count)
            .unwrap_or(0)
    }
}

/// Transition hint for the service layer: which `McpServerStatus` a connection should
/// land in after a successful `start` call.
pub const STARTED_STATUS: McpServerStatus = McpServerStatus::Running;

pub fn read_log_file(path: &std::path::Path, max_bytes: u64) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = std::fs::File::open(path) else {
        return String::new();
    };
    let Ok(metadata) = file.metadata() else {
        return String::new();
    };
    let len = metadata.len();
    if len > max_bytes {
        let _ = file.seek(SeekFrom::End(-(max_bytes as i64)));
    }
    let mut buf = String::new();
    let _ = file.read_to_string(&mut buf);
    buf
}

fn package_manager_env_vars(workspace_dir: &str) -> Vec<(String, String)> {
    let workspace = std::path::Path::new(workspace_dir);
    let mut env = vec![
        ("UV_CACHE_DIR".into(), format!("{workspace_dir}/.uv-cache")),
        ("UV_TOOL_DIR".into(), format!("{workspace_dir}/.uv-tools")),
        ("UV_LINK_MODE".into(), "copy".into()),
        ("NPM_CONFIG_CACHE".into(), format!("{workspace_dir}/.npm-cache")),
    ];
    let (_, node_env) = crate::tool::sandbox::node_env_vars(workspace);
    env.extend(node_env);
    env
}

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
