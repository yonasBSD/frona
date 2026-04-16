use std::collections::HashMap;
use std::path::Path as StdPath;

use axum::extract::{Multipart, Path, State};
use axum::routing::{get, put};
use axum::{Json, Router};
use crate::agent::config::parse_frontmatter;
use crate::agent::models::{AgentResponse, CreateAgentRequest, UpdateAgentRequest};
use crate::chat::broadcast::{BroadcastEvent, BroadcastEventKind};
use crate::inference::tool_loop::InferenceEventKind;

use super::super::error::ApiError;
use super::super::middleware::auth::AuthUser;
use crate::core::error::AppError;
use crate::core::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/agents", get(list_agents).post(create_agent))
        .route(
            "/api/agents/{id}",
            get(get_agent).put(update_agent).delete(delete_agent),
        )
        .route("/api/agents/{id}/skills", get(list_agent_skills))
        .route("/api/agents/{id}/avatar", put(upload_avatar))
}

fn resolve_default_prompt(state: &AppState, agent_id: &str) -> String {
    state
        .storage_service
        .agent_workspace(agent_id)
        .read("AGENT.md")
        .map(|c| parse_frontmatter(&c).template)
        .unwrap_or_default()
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
    agents.retain(|agent| {
        !agent.is_shared || agent.tools.iter().all(|t| crate::tool::is_tool_available(&state, t))
    });

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
        agent.default_prompt = resolve_default_prompt(&state, &agent.id);
    }

    Ok(Json(agents))
}

async fn get_agent(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AgentResponse>, ApiError> {
    let mut agent = state.agent_service.get(&auth.user_id, &id).await?;
    agent.default_prompt = resolve_default_prompt(&state, &id);
    Ok(Json(agent))
}

async fn update_agent(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> Result<Json<AgentResponse>, ApiError> {
    let mut agent = state.agent_service.update(&auth.user_id, &id, req).await?;
    agent.default_prompt = resolve_default_prompt(&state, &id);

    state.broadcast_service.send(BroadcastEvent {
        user_id: auth.user_id,
        chat_id: None,
        kind: BroadcastEventKind::Inference(InferenceEventKind::EntityUpdated {
            table: "agent".to_string(),
            record_id: id,
            fields: serde_json::to_value(&agent).unwrap_or_default(),
        }),
    });

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

async fn list_agent_skills(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<crate::agent::skill::service::SkillListItem>>, ApiError> {
    state.agent_service.get(&auth.user_id, &id).await?;
    let skills = state.skill_service.list(&id, None).await;
    let items = skills.into_iter().map(|s| crate::agent::skill::service::SkillListItem {
        name: s.name,
        description: s.description,
        source: None,
        installed_at: None,
        scope: s.scope,
    }).collect();
    Ok(Json(items))
}

const MAX_AVATAR_SIZE: usize = 2 * 1024 * 1024; // 2MB

async fn upload_avatar(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.agent_service.get(&auth.user_id, &id).await?;

    let mut file_data: Option<(String, Vec<u8>)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError(AppError::Validation(e.to_string())))?
    {
        if field.name() == Some("file") {
            let filename = field.file_name().unwrap_or("avatar").to_string();
            let bytes = field
                .bytes()
                .await
                .map_err(|e| ApiError(AppError::Validation(e.to_string())))?;
            if bytes.len() > MAX_AVATAR_SIZE {
                return Err(ApiError(AppError::Validation(
                    "Avatar too large (max 2MB)".into(),
                )));
            }
            file_data = Some((filename, bytes.to_vec()));
        }
    }

    let (filename, bytes) = file_data
        .ok_or_else(|| ApiError(AppError::Validation("Missing file field".into())))?;

    let ext = StdPath::new(&filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg");
    let avatar_filename = format!("avatar.{ext}");

    let workspace = state.storage_service.agent_workspace(&id);
    workspace
        .write_bytes(&avatar_filename, &bytes)
        .map_err(|e| ApiError(AppError::Internal(e.to_string())))?;

    let url = format!("/api/files/agent/{id}/{avatar_filename}");
    Ok(Json(serde_json::json!({ "url": url })))
}

