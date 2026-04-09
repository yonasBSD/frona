use std::collections::BTreeMap;

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::core::state::AppState;
use crate::tool::provider::{
    BUILTIN_PROVIDERS, ToolProvider, ToolProviderKind, ToolProviderStatus, builtin_providers,
    is_configurable_builtin,
};
use crate::tool::registry::build_tool_registry;

use super::super::error::ApiError;
use super::super::middleware::auth::AuthUser;

#[derive(Serialize, Clone)]
pub struct ToolInfo {
    pub id: String,
    pub description: String,
    pub configurable: bool,
}

#[derive(Serialize, Clone)]
pub struct ToolProviderWithTools {
    #[serde(flatten)]
    pub provider: ToolProvider,
    pub tools: Vec<ToolInfo>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/tools", get(list_tools))
        .route("/api/agents/{id}/tools", get(agent_tools))
}

/// Build the full provider+tool catalog for the user. Built-in providers come from the static
/// catalog; tools are sourced from a synthetic registry built with every configurable
/// provider id allowed, so each tool definition is observed at least once.
fn build_catalog(state: &AppState) -> Vec<ToolProviderWithTools> {
    let mut all_allowed: Vec<String> = BUILTIN_PROVIDERS
        .iter()
        .map(|s| s.id.to_string())
        .collect();
    for cli in state.cli_tools_config.iter() {
        all_allowed.push(cli.name.clone());
    }
    let registry = build_tool_registry(state, "", &all_allowed, false);

    let mut by_provider: BTreeMap<String, Vec<ToolInfo>> = BTreeMap::new();
    for def in &registry.definitions {
        let configurable = is_configurable_builtin(&def.provider_id)
            || state
                .cli_tools_config
                .iter()
                .any(|c| c.name == def.provider_id);
        by_provider
            .entry(def.provider_id.clone())
            .or_default()
            .push(ToolInfo {
                id: def.id.clone(),
                description: def.description.clone(),
                configurable,
            });
    }

    let mut providers: Vec<ToolProviderWithTools> = builtin_providers()
        .into_iter()
        .map(|p| {
            let tools = by_provider.remove(&p.id).unwrap_or_default();
            ToolProviderWithTools { provider: p, tools }
        })
        .collect();

    // Append CLI tool providers (one per CLI tool config) — synthesized at request time
    // since they're configured dynamically rather than baked into BUILTIN_PROVIDERS.
    for cli in state.cli_tools_config.iter() {
        let tools = by_provider.remove(&cli.name).unwrap_or_default();
        providers.push(ToolProviderWithTools {
            provider: ToolProvider {
                id: cli.name.clone(),
                display_name: cli.name.clone(),
                description: Some(cli.description.clone()),
                icon: None,
                kind: ToolProviderKind::Builtin,
                status: ToolProviderStatus::Available,
            },
            tools,
        });
    }

    providers
}

/// All providers and their tools available in the system, for the agent settings UI.
async fn list_tools(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> Json<Vec<ToolProviderWithTools>> {
    Json(build_catalog(&state))
}

/// Tools currently assigned to a specific agent (flat list of selected tool ids).
async fn agent_tools(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ToolInfo>>, ApiError> {
    let agent = state.agent_service.get(&auth.user_id, &id).await?;
    let registry = build_tool_registry(&state, &id, &agent.tools, false);
    let infos: Vec<ToolInfo> = registry
        .definitions
        .iter()
        .map(|d| ToolInfo {
            id: d.id.clone(),
            description: d.description.clone(),
            configurable: is_configurable_builtin(&d.provider_id)
                || state
                    .cli_tools_config
                    .iter()
                    .any(|c| c.name == d.provider_id),
        })
        .collect();
    Ok(Json(infos))
}
