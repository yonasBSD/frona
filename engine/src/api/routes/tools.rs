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

/// Returns the effective provider id for a CLI tool: its `provider` override if set,
/// otherwise falls back to the tool's own name (one-provider-per-tool default).
fn cli_provider_id(cli: &crate::tool::cli::CliToolConfig) -> String {
    cli.provider.clone().unwrap_or_else(|| cli.name.clone())
}

/// Metadata for synthetic providers that group multiple CLI tools. If a CLI tool declares
/// `provider: code` in its frontmatter but no entry exists here, the id is used as-is and
/// the description is borrowed from the first tool in the group.
fn synthetic_provider_metadata(id: &str) -> Option<(&'static str, &'static str)> {
    match id {
        "code" => Some(("code", "Execute arbitrary code the agent writes in a supported language.")),
        _ => None,
    }
}

/// Build the full provider+tool catalog for the user. Built-in providers come from the static
/// catalog; tools are sourced from a synthetic registry built with every configurable
/// provider id allowed, so each tool definition is observed at least once.
async fn build_catalog(state: &AppState) -> Vec<ToolProviderWithTools> {
    let mut all_allowed: Vec<String> = BUILTIN_PROVIDERS
        .iter()
        .map(|s| s.id.to_string())
        .collect();
    for cli in state.cli_tools_config.iter() {
        all_allowed.push(cli_provider_id(cli));
        all_allowed.push(cli.name.clone());
    }
    let registry = build_tool_registry(state, "", "", &all_allowed, false).await;

    let mut by_provider: BTreeMap<String, Vec<ToolInfo>> = BTreeMap::new();
    for def in &registry.definitions {
        let configurable = is_configurable_builtin(&def.provider_id)
            || state
                .cli_tools_config
                .iter()
                .any(|c| cli_provider_id(c) == def.provider_id);
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

    // Synthesize CLI providers, grouping configs that share the same effective provider id.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for cli in state.cli_tools_config.iter() {
        let provider_id = cli_provider_id(cli);
        if !seen.insert(provider_id.clone()) {
            continue;
        }
        let tools = by_provider.remove(&provider_id).unwrap_or_default();
        let (display_name, description) = match synthetic_provider_metadata(&provider_id) {
            Some((name, desc)) => (name.to_string(), desc.to_string()),
            None => (cli.name.clone(), cli.description.clone()),
        };
        providers.push(ToolProviderWithTools {
            provider: ToolProvider {
                id: provider_id,
                display_name,
                description: Some(description),
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
    Json(build_catalog(&state).await)
}

/// Tools currently assigned to a specific agent (flat list of selected tool ids).
async fn agent_tools(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ToolInfo>>, ApiError> {
    let agent = state.agent_service.get(&auth.user_id, &id).await?;
    let registry = build_tool_registry(&state, &id, &auth.user_id, &agent.tools, false).await;
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
                    .any(|c| cli_provider_id(c) == d.provider_id),
        })
        .collect();
    Ok(Json(infos))
}
