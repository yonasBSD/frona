use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;

use super::super::error::ApiError;
use super::super::middleware::auth::AuthUser;
use crate::agent::skill::service::{
    RepoBrowseResult, SkillListItem, SkillPreview, SkillSearchResult, UpdateCheckResult,
};
use crate::core::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/skills", get(list_installed))
        .route("/api/skills/search", get(search_skills))
        .route("/api/skills/browse", get(browse_repo))
        .route("/api/skills/preview", get(preview_skill))
        .route("/api/skills/install", post(install_skill))
        .route("/api/skills/check", get(check_updates))
        .route("/api/skills/{name}", delete(uninstall_skill))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

#[derive(Deserialize)]
struct BrowseQuery {
    repo: String,
}

#[derive(Deserialize)]
struct PreviewQuery {
    repo: String,
    name: String,
}

#[derive(Deserialize)]
struct InstallRequest {
    repo: String,
    skill_name: String,
    agent_id: Option<String>,
}

async fn list_installed(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<SkillListItem>>, ApiError> {
    let items = state.skill_service.list_installed()?;
    Ok(Json(items))
}

async fn search_skills(
    _auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<SkillSearchResult>>, ApiError> {
    let results = state.skill_service.search(&params.q).await?;
    Ok(Json(results))
}

async fn browse_repo(
    _auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<BrowseQuery>,
) -> Result<Json<Vec<RepoBrowseResult>>, ApiError> {
    let results = state.skill_service.browse_repo(&params.repo).await?;
    Ok(Json(results))
}

async fn preview_skill(
    _auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<PreviewQuery>,
) -> Result<Json<SkillPreview>, ApiError> {
    let preview = state.skill_service.preview(&params.repo, &params.name).await?;
    Ok(Json(preview))
}

async fn install_skill(
    _auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<InstallRequest>,
) -> Result<Json<SkillListItem>, ApiError> {
    let item = state.skill_service.install(&req.repo, &req.skill_name, req.agent_id.as_deref()).await?;
    Ok(Json(item))
}

async fn uninstall_skill(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<()>, ApiError> {
    state.skill_service.uninstall(&name).await?;
    Ok(Json(()))
}

async fn check_updates(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<UpdateCheckResult>>, ApiError> {
    let results = state.skill_service.check_updates().await?;
    Ok(Json(results))
}
