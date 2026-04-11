use std::convert::Infallible;

use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};

use crate::api::error::ApiError;
use crate::api::middleware::auth::AuthUser;
use crate::core::state::AppState;
use crate::tool::mcp::models::{McpServerInstall, McpServerUpdate};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/mcp/servers", get(list_servers).post(install_server))
        .route(
            "/api/mcp/servers/{id}",
            get(get_server).delete(uninstall_server).patch(update_server),
        )
        .route("/api/mcp/servers/{id}/start", post(start_server))
        .route("/api/mcp/servers/{id}/stop", post(stop_server))
        .route("/api/mcp/servers/{id}/logs", get(get_logs))
        .route("/api/mcp/servers/{id}/logs/stream", get(stream_logs))
        .route("/api/mcp/registry/search", get(search_registry))
        .route("/api/mcp/registry/{name}", get(fetch_registry_entry))
}

#[derive(Serialize)]
struct McpServerResponse {
    id: String,
    slug: String,
    display_name: String,
    description: Option<String>,
    repository_url: Option<String>,
    registry_id: Option<String>,
    status: String,
    command: String,
    args: Vec<String>,
    tool_count: usize,
    env: std::collections::BTreeMap<String, String>,
    extra_read_paths: Vec<String>,
    extra_write_paths: Vec<String>,
    installed_at: String,
    last_started_at: Option<String>,
}

impl From<crate::tool::mcp::McpServer> for McpServerResponse {
    fn from(s: crate::tool::mcp::McpServer) -> Self {
        Self {
            id: s.id,
            slug: s.slug,
            display_name: s.display_name,
            description: s.description,
            repository_url: s.repository_url,
            registry_id: s.registry_id,
            status: s.status.to_string(),
            command: s.command,
            args: s.args,
            tool_count: s.tool_cache.len(),
            env: s.env,
            extra_read_paths: s.extra_read_paths,
            extra_write_paths: s.extra_write_paths,
            installed_at: s.installed_at.to_rfc3339(),
            last_started_at: s.last_started_at.map(|t| t.to_rfc3339()),
        }
    }
}

async fn list_servers(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<McpServerResponse>>, ApiError> {
    let servers = state.mcp_service.list_for_user(&auth.user_id).await?;
    Ok(Json(servers.into_iter().map(Into::into).collect()))
}

async fn get_server(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<McpServerResponse>, ApiError> {
    let servers = state.mcp_service.list_for_user(&auth.user_id).await?;
    let server = servers
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| ApiError::from(crate::core::error::AppError::NotFound(format!("mcp server {id}"))))?;
    Ok(Json(server.into()))
}

async fn install_server(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<McpServerInstall>,
) -> Result<Json<McpServerResponse>, ApiError> {
    let server = state.mcp_service.install(&auth.user_id, req).await?;
    Ok(Json(server.into()))
}

async fn update_server(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<McpServerUpdate>,
) -> Result<Json<UpdateResponse>, ApiError> {
    let result = state.mcp_service.update(&auth.user_id, &id, req).await?;
    Ok(Json(UpdateResponse {
        server: result.server.into(),
        restart_required: result.restart_required,
    }))
}

#[derive(Serialize)]
struct UpdateResponse {
    server: McpServerResponse,
    restart_required: bool,
}

async fn uninstall_server(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.mcp_service.uninstall(&auth.user_id, &id).await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

async fn start_server(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<StartResponse>, ApiError> {
    let result = state.mcp_service.start(&auth.user_id, &id).await?;
    Ok(Json(StartResponse {
        tool_count: result.tools.len(),
        tools: result
            .tools
            .into_iter()
            .map(|t| ToolInfo {
                id: t.id,
                description: t.description,
            })
            .collect(),
    }))
}

#[derive(Serialize)]
struct StartResponse {
    tool_count: usize,
    tools: Vec<ToolInfo>,
}

#[derive(Serialize)]
struct ToolInfo {
    id: String,
    description: String,
}

async fn stop_server(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.mcp_service.stop(&auth.user_id, &id).await?;
    Ok(Json(serde_json::json!({ "stopped": true })))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    20
}

async fn resolve_log_path(state: &AppState, server_id: &str) -> std::path::PathBuf {
    if let Ok(server) = state.mcp_service.find_by_id(server_id).await {
        return std::path::PathBuf::from(&server.workspace_dir)
            .join("logs")
            .join("server.log");
    }
    std::path::PathBuf::from(state.mcp_manager.workspaces_path())
        .join(server_id)
        .join("logs")
        .join("server.log")
}

async fn get_logs(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let log_path = resolve_log_path(&state, &id).await;
    let logs = crate::tool::mcp::manager::read_log_file(&log_path, 64 * 1024);
    Ok(Json(serde_json::json!({ "logs": logs })))
}

async fn stream_logs(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let log_path = resolve_log_path(&state, &id).await;

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<Event, Infallible>>();

    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};

        // Wait for the file to exist
        let file = loop {
            match tokio::fs::File::open(&log_path).await {
                Ok(f) => break f,
                Err(_) => {
                    if tx.is_closed() { return; }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        };

        let mut reader = BufReader::new(file);

        // Seek to end minus 8KB to send recent context on connect
        if let Ok(metadata) = tokio::fs::metadata(&log_path).await {
            let len = metadata.len();
            if len > 8192 {
                let _ = reader.seek(std::io::SeekFrom::End(-8192)).await;
                // Skip partial line
                let mut partial = String::new();
                let _ = reader.read_line(&mut partial).await;
            }
        }

        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    // EOF — wait for more data
                    if tx.is_closed() { return; }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
                Ok(_) => {
                    let trimmed = line.trim_end();
                    if !trimmed.is_empty()
                        && tx.send(Ok(Event::default().data(trimmed))).is_err()
                    {
                        return;
                    }
                }
                Err(_) => {
                    return;
                }
            }
        }
    });

    let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn fetch_registry_entry(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<crate::tool::mcp::RegistryServerEntry>, ApiError> {
    let entry = state.mcp_service.fetch_registry(&name).await?;
    Ok(Json(entry))
}

async fn search_registry(
    _auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<crate::tool::mcp::RegistryServerEntry>>, ApiError> {
    let results = state
        .mcp_service
        .search_registry(&query.q, query.limit)
        .await?;
    Ok(Json(results))
}
