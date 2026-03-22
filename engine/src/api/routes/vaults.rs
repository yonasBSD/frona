use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::api::error::ApiError;
use crate::api::middleware::auth::AuthUser;
use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::credential::vault::models::*;
use crate::credential::vault::provider::create_vault_provider;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/vaults", post(create_connection).get(list_connections))
        .route("/api/vaults/approve", post(approve_request))
        .route("/api/vaults/deny", post(deny_request))
        .route("/api/vaults/grants", get(list_grants))
        .route("/api/vaults/grants/{id}", delete(revoke_grant))
        .route(
            "/api/vaults/local/items",
            get(list_local_items).post(create_local_item),
        )
        .route(
            "/api/vaults/local/items/{id}",
            axum::routing::put(update_local_item).delete(delete_local_item),
        )
        .route("/api/vaults/test", post(test_vault))
        .route("/api/vaults/{id}", delete(delete_connection))
        .route("/api/vaults/{id}/toggle", post(toggle_connection))
        .route("/api/vaults/{id}/test", post(test_connection))
        .route("/api/vaults/{id}/items", get(search_items).post(search_items_inline))
}

async fn create_connection(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateVaultConnectionRequest>,
) -> Result<Json<VaultConnectionResponse>, ApiError> {
    let response = state
        .vault_service
        .create_connection(&auth.user_id, req)
        .await?;
    Ok(Json(response))
}

async fn list_connections(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<VaultConnectionResponse>>, ApiError> {
    let connections = state
        .vault_service
        .list_connections(&auth.user_id)
        .await?;
    Ok(Json(connections))
}

async fn delete_connection(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .vault_service
        .delete_connection(&auth.user_id, &id)
        .await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

async fn toggle_connection(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ToggleVaultConnectionRequest>,
) -> Result<Json<VaultConnectionResponse>, ApiError> {
    let response = state
        .vault_service
        .toggle_connection(&auth.user_id, &id, req.enabled)
        .await?;
    Ok(Json(response))
}

async fn test_connection(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .vault_service
        .test_connection(&auth.user_id, &id)
        .await?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

#[derive(Deserialize)]
struct SearchQuery {
    #[serde(default)]
    q: String,
    #[serde(default = "default_max_results")]
    max_results: usize,
}

fn default_max_results() -> usize {
    10
}

async fn search_items(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<VaultItem>>, ApiError> {
    let items = state
        .vault_service
        .search_items(&auth.user_id, &id, &query.q, query.max_results)
        .await?;
    Ok(Json(items))
}

#[derive(Deserialize)]
struct InlineSearchRequest {
    provider: VaultProviderType,
    config: VaultConnectionConfig,
    #[serde(default)]
    q: String,
    #[serde(default = "default_max_results")]
    max_results: usize,
}

async fn search_items_inline(
    _auth: AuthUser,
    Path(_id): Path<String>,
    Json(req): Json<InlineSearchRequest>,
) -> Result<Json<Vec<VaultItem>>, ApiError> {
    let tmp = tempfile::tempdir()
        .map_err(|e| ApiError::from(AppError::Tool(format!("Failed to create temp dir: {e}"))))?;
    let provider = create_vault_provider(req.provider, req.config, tmp.path().to_path_buf())?;
    let items = provider.search(&req.q, req.max_results).await?;
    Ok(Json(items))
}


async fn approve_request(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<ApproveVaultRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let chat = state
        .chat_service
        .get_chat(&auth.user_id, &req.chat_id)
        .await
        .map_err(ApiError::from)?;

    let secret = state
        .vault_service
        .get_secret(&auth.user_id, &req.connection_id, &req.vault_item_id)
        .await?;

    let pending_te = state.chat_service
        .find_pending_tool_execution(&req.chat_id)
        .await
        .map_err(ApiError::from)?;

    let (original_query, original_reason) = pending_te.as_ref()
        .and_then(|te| te.tool_data.as_ref())
        .map(|td| match td {
            crate::inference::tool_execution::MessageTool::VaultApproval { query, reason, .. } => {
                (query.clone(), reason.clone())
            }
            _ => (String::new(), String::new()),
        })
        .unwrap_or_default();

    {
        let agent_id = &chat.agent_id;

        if !matches!(req.grant_duration, GrantDuration::Once) {
            state
                .vault_service
                .create_grant(
                    &auth.user_id,
                    agent_id,
                    &req.connection_id,
                    &req.vault_item_id,
                    &original_query,
                    req.env_var_prefix.as_deref(),
                    &req.grant_duration,
                )
                .await?;
        }

        state
            .vault_service
            .log_access(
                &auth.user_id,
                agent_id,
                &req.chat_id,
                &req.connection_id,
                &req.vault_item_id,
                req.env_var_prefix.as_deref(),
                &original_query,
                &original_reason,
            )
            .await?;
    }

    let result_text = if let Some(ref prefix) = req.env_var_prefix {
        let env_vars = secret.to_env_vars(prefix);
        let var_names: Vec<String> = env_vars.iter().map(|(k, _)| k.clone()).collect();
        format!(
            "Credentials loaded into environment variables: {}. Use these in CLI commands.",
            var_names.join(", ")
        )
    } else {
        let mut parts = Vec::new();
        parts.push(format!("Credentials for: {}", secret.name));
        if let Some(ref u) = secret.username {
            parts.push(format!("Username: {u}"));
        }
        if let Some(ref p) = secret.password {
            parts.push(format!("Password: {p}"));
        }
        for (k, v) in &secret.fields {
            parts.push(format!("{k}: {v}"));
        }
        parts.join("\n")
    };

    if let Some(te) = pending_te {
        let message_id = te.message_id.clone();
        let resolved = state
            .chat_service
            .resolve_tool_execution(&te.id, Some(result_text.clone()))
            .await
            .map_err(ApiError::from)?
            .into_message();

        state.broadcast_service.send(crate::chat::broadcast::BroadcastEvent {
            user_id: auth.user_id.clone(),
            chat_id: Some(req.chat_id.clone()),
            kind: crate::chat::broadcast::BroadcastEventKind::ToolResolved { message: resolved },
        });

        let user_id = auth.user_id.clone();
        let chat_id = req.chat_id.clone();
        let state_clone = state.clone();
        tokio::spawn(async move {
            crate::agent::task::executor::resume_or_notify(&state_clone, &user_id, &chat_id, &message_id).await;
        });
    }

    Ok(Json(serde_json::json!({ "approved": true })))
}

async fn deny_request(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<DenyVaultRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .chat_service
        .get_chat(&auth.user_id, &req.chat_id)
        .await
        .map_err(ApiError::from)?;

    let pending_te = state.chat_service
        .find_pending_tool_execution(&req.chat_id)
        .await
        .map_err(ApiError::from)?;

    if let Some(te) = pending_te {
        let message_id = te.message_id.clone();
        let denied = state
            .chat_service
            .deny_tool_execution(
                &te.id,
                Some("User denied the credential request.".to_string()),
            )
            .await
            .map_err(ApiError::from)?
            .into_message();

        state.broadcast_service.send(crate::chat::broadcast::BroadcastEvent {
            user_id: auth.user_id.clone(),
            chat_id: Some(req.chat_id.clone()),
            kind: crate::chat::broadcast::BroadcastEventKind::ToolResolved { message: denied },
        });

        let user_id = auth.user_id.clone();
        let chat_id = req.chat_id.clone();
        let state_clone = state.clone();
        tokio::spawn(async move {
            crate::agent::task::executor::resume_or_notify(&state_clone, &user_id, &chat_id, &message_id).await;
        });
    }

    Ok(Json(serde_json::json!({ "denied": true })))
}

async fn list_grants(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<VaultGrantResponse>>, ApiError> {
    let grants = state.vault_service.list_grants(&auth.user_id).await?;
    Ok(Json(grants))
}

async fn revoke_grant(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .vault_service
        .revoke_grant(&auth.user_id, &id)
        .await?;
    Ok(Json(serde_json::json!({ "revoked": true })))
}

// --- Inline test route ---

#[derive(Deserialize)]
struct TestVaultRequest {
    provider: VaultProviderType,
    config: VaultConnectionConfig,
}

async fn test_vault(
    _auth: AuthUser,
    Json(req): Json<TestVaultRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let tmp = tempfile::tempdir()
        .map_err(|e| ApiError::from(AppError::Tool(format!("Failed to create temp dir: {e}"))))?;
    let provider = create_vault_provider(req.provider, req.config, tmp.path().to_path_buf())?;
    provider.test_connection().await?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

// --- Local item routes ---

async fn create_local_item(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateLocalItemRequest>,
) -> Result<Json<CredentialResponse>, ApiError> {
    let response = state
        .vault_service
        .create_credential(&auth.user_id, req)
        .await?;
    Ok(Json(response))
}

async fn list_local_items(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<CredentialResponse>>, ApiError> {
    let credentials = state.vault_service.list_credentials(&auth.user_id).await?;
    Ok(Json(credentials))
}

async fn update_local_item(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateLocalItemRequest>,
) -> Result<Json<CredentialResponse>, ApiError> {
    let response = state
        .vault_service
        .update_credential(&auth.user_id, &id, req)
        .await?;
    Ok(Json(response))
}

async fn delete_local_item(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .vault_service
        .delete_credential(&auth.user_id, &id)
        .await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}
