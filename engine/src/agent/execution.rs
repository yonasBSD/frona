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
) -> Result<AgentLoopOutcome, AppError> {
    let chat = state
        .chat_service
        .find_chat(chat_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Chat not found".into()))?;

    let ChatSessionContext {
        system_prompt, model_group, rig_history, registry,
        tool_registry, tool_ctx,
        mut tool_event_rx, ..
    } = ChatSessionContext::build(state, user_id, chat, cancel_token.clone()).await?;

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
            InferenceEventKind::ToolCall { .. } | InferenceEventKind::ToolResult { .. } => {
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
