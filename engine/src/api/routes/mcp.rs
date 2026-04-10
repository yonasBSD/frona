use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
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
            delete(uninstall_server).patch(update_server),
        )
        .route("/api/mcp/servers/{id}/start", post(start_server))
        .route("/api/mcp/servers/{id}/stop", post(stop_server))
        .route("/api/mcp/registry/search", get(search_registry))
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
