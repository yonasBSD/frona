use axum::extract::{Path, State};
use axum::Json;
use rig::completion::Message as RigMessage;

use crate::chat::broadcast::{BroadcastEventKind, EventSender};
use crate::chat::message::models::SendMessageRequest;
use crate::chat::service::ChatService;
use crate::credential::presign::{PresignService, presign_response};
use crate::inference::conversation::{
    ConversationBuilder, ConversationContext, DefaultConversationBuilder, build_user_message,
};
use crate::inference::request::{InferenceRequest, InferenceResponse};
use crate::chat::message::models::MessageResponse;

use super::super::super::error::ApiError;
use super::super::super::middleware::auth::AuthUser;
use crate::core::state::AppState;

fn spawn_inference(
    req: InferenceRequest,
) -> tokio::task::JoinHandle<Result<InferenceResponse, crate::core::error::AppError>> {
    tokio::spawn(async move { crate::inference::inference(req).await })
}

#[allow(clippy::too_many_arguments)]
async fn handle_inference_result(
    result: Result<Result<InferenceResponse, crate::core::error::AppError>, tokio::task::JoinError>,
    accumulated: String,
    chat_service: &ChatService,
    chat_id: &str,
    presign_svc: &PresignService,
    user_id: &str,
    username: &str,
    event_sender: &EventSender,
) {
    match result {
        Ok(Ok(response)) => match response {
            InferenceResponse::Completed { text: _, attachments, reasoning, .. } => {
                if !accumulated.is_empty()
                    && let Ok(mut msg) = chat_service
                        .save_assistant_message_with_tool_calls(chat_id, accumulated, None, attachments, reasoning)
                        .await
                {
                    presign_response(presign_svc, &mut msg, user_id, username).await;
                    event_sender.send_kind(BroadcastEventKind::InferenceDone { message: msg });
                }
            }
            InferenceResponse::Cancelled(_) => {
                if !accumulated.is_empty() {
                    let _ = chat_service.save_assistant_message(chat_id, accumulated).await;
                }
                event_sender.send_kind(BroadcastEventKind::InferenceCancelled {
                    reason: "User cancelled generation".to_string(),
                });
            }
            InferenceResponse::ExternalToolPending {
                accumulated_text,
                tool_calls_json,
                tool_results,
                external_tool,
                system_prompt: _,
            } => {
                let text = if accumulated.is_empty() { accumulated_text } else { accumulated };
                if let Ok(mut tool_msg) = chat_service
                    .save_external_tool_pending(chat_id, text, tool_calls_json, &tool_results, external_tool)
                    .await
                {
                    presign_response(presign_svc, &mut tool_msg, user_id, username).await;
                    event_sender.send_kind(BroadcastEventKind::ToolMessage { message: tool_msg });
                }
            }
        },
        Ok(Err(e)) => {
            tracing::error!(error = %e, "Inference failed");
            event_sender.send_kind(BroadcastEventKind::InferenceError {
                error: e.to_string(),
            });
        }
        Err(e) => {
            tracing::error!(error = %e, "Inference task panicked");
            event_sender.send_kind(BroadcastEventKind::InferenceError {
                error: "Internal error".to_string(),
            });
        }
    }
}

pub(crate) async fn stream_message(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<MessageResponse>, ApiError> {
    use crate::chat::message::models::{MessageTool, ToolStatus};

    let chat = state
        .chat_service
        .get_chat(&auth.user_id, &chat_id)
        .await
        .map_err(ApiError::from)?;

    let stored_messages = state.chat_service.get_stored_messages(&chat_id).await;
    let pending_tool_id = stored_messages.iter().rev().find_map(|m| match &m.tool {
        Some(MessageTool::Question { status: ToolStatus::Pending, .. })
        | Some(MessageTool::HumanInTheLoop { status: ToolStatus::Pending, .. })
        | Some(MessageTool::VaultApproval { status: ToolStatus::Pending, .. })
        | Some(MessageTool::ServiceApproval { status: ToolStatus::Pending, .. }) => {
            Some(m.id.clone())
        }
        _ => None,
    });

    let cancel_token = state.active_sessions.register(&chat_id).await;
    let mut ctx = crate::chat::session::ChatSessionContext::build(
        &state, &auth.user_id, chat, cancel_token,
    )
    .await
    .map_err(ApiError::from)?;

    if let Some(compaction_group) = state.compaction_model_group() {
        let max_output = ctx.model_group.max_tokens.unwrap_or(8192) as usize;
        if let Err(e) = state.memory_service.compact_chat_if_needed(
            &chat_id,
            &ctx.chat.agent_id,
            &ctx.system_prompt,
            &ctx.model_group.main.model_id,
            ctx.model_group.context_window,
            max_output,
            &compaction_group,
        ).await {
            tracing::warn!(error = %e, "Chat compaction failed, continuing without compaction");
        }
    }

    let (chat_summary, context_messages) = state
        .memory_service
        .get_conversation_context(&chat_id)
        .await
        .unwrap_or((None, stored_messages));

    let mut rig_history = Vec::new();
    if let Some(summary) = &chat_summary {
        rig_history.push(RigMessage::user(format!(
            "[Previous conversation summary]\n{summary}"
        )));
        rig_history.push(RigMessage::assistant(
            "Understood. I have context from our previous conversation. How can I help?",
        ));
    }
    let conv_builder = DefaultConversationBuilder {
        user_service: state.user_service.clone(),
        storage_service: state.storage_service.clone(),
    };
    let conv_ctx = ConversationContext {
        agent_id: ctx.chat.agent_id.clone(),
        model_ref: ctx.model_group.main.clone(),
        user_id: auth.user_id.clone(),
    };
    rig_history.extend(conv_builder.build(&context_messages, &conv_ctx).await);
    ctx.rig_history = rig_history;

    let user_content = req.content;
    let agent_id = ctx.chat.agent_id.clone();
    let needs_title = ctx.chat.title.is_none();

    let crate::chat::session::ChatSessionContext {
        system_prompt, model_group, rig_history, registry, tool_registry,
        tool_ctx, cancel_token, ..
    } = ctx;

    let event_sender = tool_ctx.event_tx.clone();

    if let Some(pending_id) = pending_tool_id {
        let mut user_response = state
            .chat_service
            .create_stream_user_message(&auth.user_id, &chat_id, &user_content, vec![])
            .await
            .map_err(ApiError::from)?;

        presign_response(&state.presign_service, &mut user_response, &auth.user_id, &auth.username).await;

        let resolved = state
            .chat_service
            .resolve_tool_message(&pending_id, Some(user_content))
            .await
            .map_err(ApiError::from)?
            .into_message();

        let pending_conv_builder = DefaultConversationBuilder {
            user_service: state.user_service.clone(),
            storage_service: state.storage_service.clone(),
        };
        let pending_conv_ctx = ConversationContext {
            agent_id: agent_id.clone(),
            model_ref: model_group.main.clone(),
            user_id: auth.user_id.clone(),
        };

        let chat_service = state.chat_service.clone();
        let presign_svc = state.presign_service.clone();
        let active_sessions = state.active_sessions.clone();
        let user_id = auth.user_id.clone();
        let username = auth.username.clone();
        let chat_id_clone = chat_id.clone();

        event_sender.send_kind(BroadcastEventKind::ToolResolved { message: resolved });

        tokio::spawn(async move {
            let stored_messages = chat_service.get_stored_messages(&chat_id_clone).await;
            let rig_history = pending_conv_builder.build(&stored_messages, &pending_conv_ctx).await;

            let handle = spawn_inference(InferenceRequest {
                registry, model_group, system_prompt,
                history: rig_history, tool_registry,
                ctx: tool_ctx, cancel_token,
            });

            let result = handle.await;
            let mut accumulated = String::new();
            if let Ok(Ok(ref resp)) = result {
                match resp {
                    InferenceResponse::Completed { text, .. } => accumulated = text.clone(),
                    InferenceResponse::Cancelled(text) => accumulated = text.clone(),
                    InferenceResponse::ExternalToolPending { accumulated_text, .. } => {
                        accumulated = accumulated_text.clone()
                    }
                }
            }

            handle_inference_result(
                result, accumulated, &chat_service, &chat_id_clone,
                &presign_svc, &user_id, &username, &event_sender,
            ).await;
            active_sessions.remove(&chat_id_clone).await;
        });

        Ok(Json(user_response))
    } else {
        let attachments = req.attachments.clone();

        let mut user_response = state
            .chat_service
            .create_stream_user_message(&auth.user_id, &chat_id, &user_content, req.attachments)
            .await
            .map_err(ApiError::from)?;

        presign_response(&state.presign_service, &mut user_response, &auth.user_id, &auth.username).await;

        let chat_service = state.chat_service.clone();
        let presign_svc = state.presign_service.clone();
        let active_sessions = state.active_sessions.clone();
        let user_id = auth.user_id.clone();
        let username = auth.username.clone();
        let msg_user_service = state.user_service.clone();
        let msg_storage_service = state.storage_service.clone();
        let chat_id_clone = chat_id.clone();

        tokio::spawn(async move {
            if needs_title {
                let svc = chat_service.clone();
                let cid = chat_id_clone.clone();
                let aid = agent_id.clone();
                let content = user_content.clone();
                let es = event_sender.clone();
                tokio::spawn(async move {
                    match svc.generate_title(&cid, &aid, &content).await {
                        Ok(title) => {
                            es.send_kind(BroadcastEventKind::Title { title });
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Title generation failed");
                        }
                    }
                });
            }

            let user_rig_msg = build_user_message(
                &user_content,
                &attachments,
                &msg_user_service,
                &msg_storage_service,
            ).await;
            let mut full_history = rig_history;
            full_history.push(user_rig_msg);

            let handle = spawn_inference(InferenceRequest {
                registry, model_group, system_prompt,
                history: full_history, tool_registry,
                ctx: tool_ctx, cancel_token,
            });

            let result = handle.await;
            let mut accumulated = String::new();
            if let Ok(Ok(ref resp)) = result {
                match resp {
                    InferenceResponse::Completed { text, .. } => accumulated = text.clone(),
                    InferenceResponse::Cancelled(text) => accumulated = text.clone(),
                    InferenceResponse::ExternalToolPending { accumulated_text, .. } => {
                        accumulated = accumulated_text.clone()
                    }
                }
            }

            handle_inference_result(
                result, accumulated, &chat_service, &chat_id_clone,
                &presign_svc, &user_id, &username, &event_sender,
            ).await;
            active_sessions.remove(&chat_id_clone).await;
        });

        Ok(Json(user_response))
    }
}
