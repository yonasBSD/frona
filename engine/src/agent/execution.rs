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
    message_id: &str,
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
        chat_service: state.chat_service.clone(),
        message_id: message_id.to_string(),
    })
    .await?;

    Ok(AgentLoopOutcome { response })
}

/// Resume all interactive chats that were mid-inference when the server stopped.
/// Only resumes non-task chats — task chats are handled by the task executor.
pub async fn resume_all_chats(state: &AppState) {
    let executing = state.chat_service.find_executing_chat_messages().await;

    if executing.is_empty() {
        return;
    }

    tracing::info!(count = executing.len(), "Resuming interrupted chats from previous run");

    for msg in executing {
        let state = state.clone();
        let chat_id = msg.chat_id.clone();
        let msg_id = msg.id.clone();
        tokio::spawn(async move {
            let user_id = match state.chat_service.find_chat(&chat_id).await {
                Ok(Some(chat)) => chat.user_id,
                _ => {
                    tracing::error!(chat_id = %chat_id, "Failed to find chat for resume");
                    return;
                }
            };
            if let Err(e) = resume_agent_loop(&state, &user_id, &chat_id, &msg_id).await {
                tracing::error!(error = %e, chat_id = %chat_id, "Failed to resume chat");
            }
        });
    }
}

/// Resume an interrupted chat (after external tool resolution, child task completion, etc.).
/// Runs the agent loop, saves the result, and broadcasts to the user via the unified SSE stream.
pub async fn resume_agent_loop(
    state: &AppState,
    user_id: &str,
    chat_id: &str,
    message_id: &str,
) -> Result<(), AppError> {
    let cancel_token = state.active_sessions.register(chat_id).await;
    let event_sender = state.broadcast_service.create_event_sender(user_id, chat_id);

    let result = run_agent_loop(state, user_id, chat_id, message_id, cancel_token, false, None).await;

    match result {
        Ok(AgentLoopOutcome { response }) => match response {
            InferenceResponse::Completed { text, attachments, reasoning, .. } => {
                if let Ok(mut msg) = state
                    .chat_service
                    .complete_agent_message(message_id, text, attachments, reasoning)
                    .await
                {
                    if let Ok(tes) = state.chat_service.get_tool_executions_by_message(message_id).await {
                        msg.tool_executions = tes.into_iter().map(Into::into).collect();
                    }
                    presign_response_by_user_id(&state.presign_service, &mut msg, user_id).await;
                    event_sender.send_kind(BroadcastEventKind::InferenceDone { message: msg });
                }
            }
            InferenceResponse::Cancelled(text) => {
                let _ = state
                    .chat_service
                    .complete_agent_message(message_id, text, vec![], None)
                    .await;
                event_sender.send_kind(BroadcastEventKind::InferenceCancelled {
                    reason: "Cancelled".to_string(),
                });
            }
            InferenceResponse::ExternalToolPending {
                tool_executions, ..
            } => {
                for te in tool_executions {
                    event_sender.send_kind(BroadcastEventKind::ToolExecution { tool_execution: te });
                }
            }
        },
        Err(e) => {
            tracing::error!(error = %e, chat_id = %chat_id, "Resume chat inference failed");
            let _ = state.chat_service.fail_agent_message(message_id).await;
            event_sender.send_kind(BroadcastEventKind::InferenceError {
                error: e.to_string(),
            });
        }
    }

    state.active_sessions.remove(chat_id).await;
    Ok(())
}
