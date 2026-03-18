mod stream;

use std::convert::Infallible;

use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream::Stream;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::chat::message::models::{MessageResponse, ResolveToolRequest, SendMessageRequest};
use crate::credential::presign::presign_response;

use super::super::error::ApiError;
use super::super::middleware::auth::AuthUser;
use crate::core::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/chats/{chat_id}/messages",
            get(list_messages).post(send_message),
        )
        .route(
            "/api/chats/{chat_id}/messages/stream",
            post(stream::stream_message),
        )
        .route(
            "/api/chats/{chat_id}/messages/{message_id}/resolve",
            post(resolve_tool_message),
        )
        .route(
            "/api/chats/{chat_id}/cancel",
            post(cancel_generation),
        )
        .route("/api/stream", get(event_stream))
}

async fn send_message(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<Vec<MessageResponse>>, ApiError> {
    let response = state
        .chat_service
        .send_message(&auth.user_id, &chat_id, req)
        .await?;
    Ok(Json(response))
}

async fn list_messages(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Result<Json<Vec<MessageResponse>>, ApiError> {
    let mut messages = state
        .chat_service
        .list_messages(&auth.user_id, &chat_id)
        .await?;

    for msg in &mut messages {
        presign_response(&state.presign_service, msg, &auth.user_id, &auth.username).await;
    }

    Ok(Json(messages))
}

async fn cancel_generation(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .chat_service
        .get_chat(&auth.user_id, &chat_id)
        .await
        .map_err(ApiError::from)?;

    let cancelled = state.active_sessions.cancel(&chat_id).await;
    Ok(Json(serde_json::json!({ "cancelled": cancelled })))
}

async fn resolve_tool_message(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((chat_id, message_id)): Path<(String, String)>,
    Json(req): Json<ResolveToolRequest>,
) -> Result<Json<MessageResponse>, ApiError> {
    state
        .chat_service
        .get_chat(&auth.user_id, &chat_id)
        .await
        .map_err(ApiError::from)?;

    let updated = state
        .chat_service
        .resolve_tool_message(&message_id, req.response.clone())
        .await
        .map_err(ApiError::from)?;

    let user_id = auth.user_id.clone();
    let state = state.clone();

    tokio::spawn(async move {
        crate::agent::task::executor::resume_or_notify(&state, &user_id, &chat_id).await;
    });

    Ok(Json(updated))
}

async fn event_stream(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<Event, Infallible>>();

    state.broadcast_service.register_session(&auth.user_id, tx);

    let stream = UnboundedReceiverStream::new(rx);
    Sse::new(stream).keep_alive(KeepAlive::default())
}
