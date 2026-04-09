use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::core::state::AppState;
use crate::tool::configurable_tools;
use crate::tool::registry::build_tool_registry;

use super::super::error::ApiError;
use super::super::middleware::auth::AuthUser;

#[derive(Serialize, Clone)]
pub struct ToolInfo {
    pub id: String,
    pub group: String,
    pub description: String,
    pub configurable: bool,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/tools", get(list_tools))
        .route("/api/agents/{id}/tools", get(agent_tools))
}

fn registry_to_tool_infos(state: &AppState, agent_id: &str, allowed: &[String]) -> Vec<ToolInfo> {
    let configurable: Vec<&str> = configurable_tools().iter().map(|s| s.as_str()).collect();
    let registry = build_tool_registry(state, agent_id, allowed, false);
    registry
        .definitions
        .iter()
        .map(|d| {
            ToolInfo {
                id: d.id.clone(),
                group: d.provider_id.clone(),
                description: d.description.clone(),
                configurable: configurable.contains(&d.provider_id.as_str()),
            }
        })
        .collect()
}

/// All configurable tools available in the system.
async fn list_tools(_auth: AuthUser, State(state): State<AppState>) -> Json<Vec<ToolInfo>> {
    let all = configurable_tools().to_vec();
    Json(registry_to_tool_infos(&state, "", &all))
}

/// Tools currently assigned to a specific agent.
async fn agent_tools(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ToolInfo>>, ApiError> {
    let agent = state.agent_service.get(&auth.user_id, &id).await?;
    Ok(Json(registry_to_tool_infos(&state, &id, &agent.tools)))
}
