use rig::completion::Message as RigMessage;
use tokio_util::sync::CancellationToken;

use crate::chat::session::ChatSessionContext;
use crate::core::state::AppState;
use crate::core::error::AppError;
use crate::inference::request::{InferenceRequest, InferenceResponse};
use crate::inference::tool_loop::InferenceEventKind;

pub struct AgentLoopOutcome {
    pub response: InferenceResponse,
    pub accumulated_text: String,
    pub last_segment: String,
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
        tool_registry, tool_ctx,
        mut tool_event_rx, ..
    } = ChatSessionContext::build_with_task(state, user_id, chat, cancel_token.clone(), is_task).await?;

    if let Some(prompt) = continuation_prompt {
        rig_history.push(RigMessage::user(prompt));
    }

    let inference_handle = tokio::spawn(async move {
        crate::inference::inference(InferenceRequest {
            registry,
            model_group,
            system_prompt,
            history: rig_history,
            tool_registry,
            ctx: tool_ctx,
            cancel_token,
        })
        .await
    });

    let mut accumulated = String::new();
    let mut last_segment = String::new();
    while let Some(event) = tool_event_rx.recv().await {
        match event.kind {
            InferenceEventKind::Text(text) => {
                accumulated.push_str(&text);
                last_segment.push_str(&text);
            }
            InferenceEventKind::ToolCall { .. } | InferenceEventKind::ToolResult { .. }  => {
                last_segment.clear();
            }
            InferenceEventKind::Done(_) | InferenceEventKind::Cancelled(_) => {}

            _ => {}
        }
    }

    let response = inference_handle.await.map_err(|e| {
        AppError::Internal(format!("Inference task panicked: {e}"))
    })??;

    Ok(AgentLoopOutcome {
        response,
        accumulated_text: accumulated,
        last_segment,
    })
}

/// Resume an interrupted chat (after external tool resolution, child task completion, etc.).
/// Runs the agent loop, saves the result, and broadcasts to the user.
/// This is the domain-layer equivalent of the API's SSE-based resume — no presigning.
pub async fn resume_agent_loop(
    state: &AppState,
    user_id: &str,
    chat_id: &str,
) -> Result<(), AppError> {
    let cancel_token = state.active_sessions.register(chat_id).await;

    let result = run_agent_loop(state, user_id, chat_id, cancel_token, false, None).await;

    match result {
        Ok(AgentLoopOutcome {
            response,
            accumulated_text,
            ..
        }) => match response {
            InferenceResponse::Completed { attachments, .. } => {
                if !accumulated_text.is_empty()
                    && let Ok(msg) = state
                        .chat_service
                        .save_assistant_message_with_tool_calls(
                            chat_id,
                            accumulated_text,
                            None,
                            attachments,
                        )
                        .await
                {
                    state
                        .broadcast_service
                        .broadcast_chat_message(user_id, chat_id, msg);
                }
            }
            InferenceResponse::Cancelled(_) => {
                if !accumulated_text.is_empty() {
                    let _ = state
                        .chat_service
                        .save_assistant_message(chat_id, accumulated_text)
                        .await;
                }
            }
            InferenceResponse::ExternalToolPending {
                accumulated_text: ext_text,
                tool_calls_json,
                tool_results,
                external_tool,
                system_prompt: _,
            } => {
                let text = if accumulated_text.is_empty() {
                    ext_text
                } else {
                    accumulated_text
                };
                if let Ok(msg) = state
                    .chat_service
                    .save_external_tool_pending(
                        chat_id,
                        text,
                        tool_calls_json,
                        &tool_results,
                        external_tool,
                    )
                    .await
                {
                    state
                        .broadcast_service
                        .broadcast_chat_message(user_id, chat_id, msg);
                }
            }
        },
        Err(e) => {
            tracing::error!(error = %e, chat_id = %chat_id, "Resume chat inference failed");
        }
    }

    state.active_sessions.remove(chat_id).await;
    Ok(())
}
