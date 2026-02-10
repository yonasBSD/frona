use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use crate::agent::models::{AgentResponse, CreateAgentRequest, UpdateAgentRequest};

use super::super::error::ApiError;
use super::super::middleware::auth::AuthUser;
use crate::core::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/agents", get(list_agents).post(create_agent))
        .route(
            "/api/agents/{id}",
            get(get_agent).put(update_agent).delete(delete_agent),
        )
}

async fn create_agent(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<AgentResponse>, ApiError> {
    let response = state.agent_service.create(&auth.user_id, req).await?;
    Ok(Json(response))
}

async fn list_agents(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<AgentResponse>>, ApiError> {
    let mut agents = state.agent_service.list(&auth.user_id).await?;

    let count_map: HashMap<String, u64> = state
        .db
        .query("SELECT agent_id, count() AS count FROM chat WHERE user_id = $user_id GROUP BY agent_id")
        .bind(("user_id", auth.user_id.clone()))
        .await
        .and_then(|mut r| r.take::<Vec<serde_json::Value>>(0))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| {
            let agent_id = v.get("agent_id")?.as_str()?.to_string();
            let count = v.get("count")?.as_u64()?;
            Some((agent_id, count))
        })
        .collect();

    for agent in &mut agents {
        if let Some(&count) = count_map.get(agent.id.as_str()) {
            agent.chat_count = count;
        }
    }

    Ok(Json(agents))
}

async fn get_agent(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AgentResponse>, ApiError> {
    let agent = state.agent_service.get(&auth.user_id, &id).await?;
    Ok(Json(agent))
}

async fn update_agent(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> Result<Json<AgentResponse>, ApiError> {
    let agent = state.agent_service.update(&auth.user_id, &id, req).await?;
    Ok(Json(agent))
}

async fn delete_agent(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(), ApiError> {
    state.agent_service.delete(&auth.user_id, &id).await?;
    Ok(())
}
