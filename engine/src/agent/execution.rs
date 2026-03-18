use rig::completion::Message as RigMessage;
use tokio_util::sync::CancellationToken;

use crate::chat::broadcast::BroadcastEventKind;
use crate::chat::session::ChatSessionContext;
use crate::credential::presign::presign_response_by_user_id;
use crate::core::state::AppState;
use crate::core::error::AppError;
use crate::inference::request::{InferenceRequest, InferenceResponse};

pub struct AgentLoopOutcome {
    pub response: InferenceResponse,
}

pub async fn run_agent_loop(
    state: &AppState,
    user_id: &str,
    chat_id: &str,
    cancel_token: CancellationToken,
    is_task: bool,
    continuation_prompt: Option<&str>,
) -> Result<AgentLoopOutcome, AppError> {
    let chat = state
        .chat_service
        .find_chat(chat_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Chat not found".into()))?;

    let ChatSessionContext {
        system_prompt, model_group, mut rig_history, registry,
        tool_registry, tool_ctx, ..
    } = ChatSessionContext::build_with_task(state, user_id, chat, cancel_token.clone(), is_task).await?;

    if let Some(prompt) = continuation_prompt {
        rig_history.push(RigMessage::user(prompt));
    }

    let response = crate::inference::inference(InferenceRequest {
        registry,
        model_group,
        system_prompt,
        history: rig_history,
        tool_registry,
        ctx: tool_ctx,
        cancel_token,
    })
    .await?;

    Ok(AgentLoopOutcome { response })
}

/// Resume an interrupted chat (after external tool resolution, child task completion, etc.).
/// Runs the agent loop, saves the result, and broadcasts to the user via the unified SSE stream.
pub async fn resume_agent_loop(
    state: &AppState,
    user_id: &str,
    chat_id: &str,
) -> Result<(), AppError> {
    let cancel_token = state.active_sessions.register(chat_id).await;
    let event_sender = state.broadcast_service.create_event_sender(user_id, chat_id);

    let result = run_agent_loop(state, user_id, chat_id, cancel_token, false, None).await;

    match result {
        Ok(AgentLoopOutcome { response }) => match response {
            InferenceResponse::Completed { text, attachments, .. } => {
                if !text.is_empty()
                    && let Ok(mut msg) = state
                        .chat_service
                        .save_assistant_message_with_tool_calls(
                            chat_id,
                            text,
                            None,
                            attachments,
                        )
                        .await
                {
                    presign_response_by_user_id(&state.presign_service, &mut msg, user_id).await;
                    event_sender.send_kind(BroadcastEventKind::InferenceDone { message: msg });
                }
            }
            InferenceResponse::Cancelled(text) => {
                if !text.is_empty() {
                    let _ = state
                        .chat_service
                        .save_assistant_message(chat_id, text)
                        .await;
                }
                event_sender.send_kind(BroadcastEventKind::InferenceCancelled {
                    reason: "Cancelled".to_string(),
                });
            }
            InferenceResponse::ExternalToolPending {
                accumulated_text,
                tool_calls_json,
                tool_results,
                external_tool,
                system_prompt: _,
            } => {
                if let Ok(mut msg) = state
                    .chat_service
                    .save_external_tool_pending(
                        chat_id,
                        accumulated_text,
                        tool_calls_json,
                        &tool_results,
                        external_tool,
                    )
                    .await
                {
                    presign_response_by_user_id(&state.presign_service, &mut msg, user_id).await;
                    event_sender.send_kind(BroadcastEventKind::ToolMessage { message: msg });
                }
            }
        },
        Err(e) => {
            tracing::error!(error = %e, chat_id = %chat_id, "Resume chat inference failed");
            event_sender.send_kind(BroadcastEventKind::InferenceError {
                error: e.to_string(),
            });
        }
    }

    state.active_sessions.remove(chat_id).await;
    Ok(())
}
