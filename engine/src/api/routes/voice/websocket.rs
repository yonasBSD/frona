use axum::extract::ws::{Message, WebSocket};
use axum::extract::{FromRequest, Query, Request, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::agent::execution::run_agent_loop;
use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::inference::InferenceResponse;
use crate::tool::voice::VoiceSessionClaims;

use super::models::TokenQuery;
use super::verify_jwt;

pub(crate) async fn twilio_ws_handler(
    State(state): State<AppState>,
    Query(q): Query<TokenQuery>,
    req: Request,
) -> Response {
    let claims: VoiceSessionClaims = match verify_jwt(&state, &q.token).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "Voice WS JWT verification failed");
            return (StatusCode::FORBIDDEN, "Invalid token").into_response();
        }
    };

    let chat_id = claims.chat_id.clone();
    let user_id = claims.sub.clone();
    let contact_id = claims.contact_id.clone();
    let call_id = claims.call_id.clone();

    let ws = match WebSocketUpgrade::from_request(req, &state).await {
        Ok(ws) => ws,
        Err(e) => return e.into_response(),
    };

    ws.on_upgrade(move |socket| handle_voice_socket(socket, state, chat_id, user_id, contact_id, call_id))
}

async fn handle_voice_socket(
    socket: WebSocket,
    state: AppState,
    chat_id: String,
    user_id: String,
    contact_id: Option<String>,
    call_id: Option<String>,
) {
    state.active_sessions.register(&chat_id).await;
    tracing::debug!(chat_id = %chat_id, "Voice WS session registered in active sessions");
    let (mut ws_send, mut ws_recv) = socket.split();
    let mut last_response = String::new();

    loop {
        let msg = match ws_recv.next().await {
            Some(Ok(Message::Text(raw))) => raw,
            Some(Ok(Message::Close(_))) | None => break,
            Some(Ok(_)) => continue,
            Some(Err(e)) => {
                tracing::warn!(error = %e, chat_id = %chat_id, "Voice WS receive error");
                break;
            }
        };

        let parsed: Value = match serde_json::from_str(&msg) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let msg_type = parsed["type"].as_str().unwrap_or("").to_string();
        tracing::debug!(chat_id = %chat_id, msg_type = %msg_type, "Voice WS message received");

        match msg_type.as_str() {
            "setup" => {
                tracing::info!(chat_id = %chat_id, user_id = %user_id, contact_id = ?contact_id, "ConversationRelay connected");
            }
            "interrupt" => {
                tracing::debug!(chat_id = %chat_id, "ConversationRelay interrupt — cancelling active turn");
                state.active_sessions.cancel(&chat_id).await;
            }
            "prompt" if parsed["last"].as_bool() == Some(true) => {
                let voice_prompt = match parsed["voicePrompt"].as_str() {
                    Some(s) if !s.is_empty() => s.to_string(),
                    _ => {
                        tracing::debug!(chat_id = %chat_id, "Ignoring prompt with empty voicePrompt");
                        continue;
                    }
                };

                tracing::info!(chat_id = %chat_id, prompt = %voice_prompt, "Voice turn starting");
                let cancel_token = state.active_sessions.register(&chat_id).await;
                let (response_text, should_hang_up) = match handle_voice_turn(
                    &state,
                    &user_id,
                    &chat_id,
                    &voice_prompt,
                    cancel_token,
                    &mut ws_send,
                    contact_id.as_deref(),
                    call_id.as_deref(),
                )
                .await
                {
                    Ok(result) => result,
                    Err(e) => {
                        tracing::error!(error = %e, chat_id = %chat_id, "Voice turn failed");
                        continue;
                    }
                };

                tracing::info!(chat_id = %chat_id, response_len = %response_text.len(), should_hang_up = %should_hang_up, "Voice turn complete");
                if !response_text.is_empty() {
                    last_response = response_text.clone();
                    tracing::debug!(chat_id = %chat_id, response = %response_text, "Sending TTS response");
                    let tts = serde_json::json!({
                        "type": "text",
                        "token": response_text,
                        "last": true
                    });
                    if ws_send
                        .send(Message::Text(tts.to_string().into()))
                        .await
                        .is_err()
                    {
                        tracing::warn!(chat_id = %chat_id, "Failed to send TTS response — closing");
                        break;
                    }
                }

                if should_hang_up {
                    let word_count = response_text.split_whitespace().count();
                    let tts_secs = ((word_count as f64 / 2.5).ceil() as u64 + 1).clamp(2, 30);
                    tracing::info!(chat_id = %chat_id, tts_secs, "Waiting for TTS before hangup");
                    tokio::time::sleep(std::time::Duration::from_secs(tts_secs)).await;
                    tracing::info!(chat_id = %chat_id, "Sending hangup signal to Twilio");
                    let end_msg = serde_json::json!({ "type": "end" });
                    ws_send.send(Message::Text(end_msg.to_string().into())).await.ok();
                    break;
                }
            }
            "prompt" => {
                tracing::debug!(chat_id = %chat_id, "Ignoring partial prompt (last=false)");
            }
            other => {
                tracing::debug!(chat_id = %other, msg_type = %other, "Unhandled ConversationRelay message type");
            }
        }
    }

    tracing::info!(chat_id = %chat_id, "Voice WS session ended");
    state.active_sessions.remove(&chat_id).await;

    if let Some(executor) = state.task_executor()
        && let Ok(Some(task)) = state.task_service.find_by_chat_id(&chat_id).await
        && matches!(task.status, crate::agent::task::models::TaskStatus::InProgress)
    {
        let summary = last_response;

        if let Ok(task) = state.task_service.mark_completed(&task.id, Some(summary.clone())).await {
            executor.deliver_to_source(&task, crate::agent::task::models::TaskStatus::Completed, summary).await;
            executor.broadcast_task_status(&task, "completed", None);
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_voice_turn(
    state: &AppState,
    user_id: &str,
    chat_id: &str,
    content: &str,
    cancel_token: CancellationToken,
    ws_send: &mut futures::stream::SplitSink<WebSocket, Message>,
    contact_id: Option<&str>,
    call_id: Option<&str>,
) -> Result<(String, bool), AppError> {
    state
        .chat_service
        .save_live_call_message(user_id, chat_id, content, contact_id)
        .await?;

    loop {
        let outcome = run_agent_loop(state, user_id, chat_id, cancel_token.clone(), false, None).await?;

        match outcome.response {
            InferenceResponse::ExternalToolPending {
                accumulated_text,
                tool_calls_json,
                tool_results,
                external_tool,
                system_prompt: _,
            } if external_tool.tool_name == "send_dtmf" => {
                tracing::debug!(chat_id = %chat_id, digits = %external_tool.result, "Sending DTMF digits");
                state
                    .chat_service
                    .save_assistant_message_with_tool_calls(
                        chat_id,
                        accumulated_text,
                        Some(tool_calls_json),
                        vec![],
                    )
                    .await
                    .ok();

                for tr in &tool_results {
                    state
                        .chat_service
                        .save_tool_result_message_with_tool(
                            chat_id,
                            &tr.tool_call_id,
                            tr.result.clone(),
                            tr.tool_data.clone(),
                            None,
                        )
                        .await
                        .ok();
                }

                let dtmf_msg = serde_json::json!({
                    "type": "sendDigits",
                    "digits": external_tool.result
                });
                ws_send
                    .send(Message::Text(dtmf_msg.to_string().into()))
                    .await
                    .ok();

                state
                    .chat_service
                    .save_tool_result_message_with_tool(
                        chat_id,
                        &external_tool.tool_call_id,
                        "DTMF sent".to_string(),
                        None,
                        None,
                    )
                    .await
                    .ok();
            }
            InferenceResponse::ExternalToolPending {
                accumulated_text,
                tool_calls_json,
                tool_results,
                external_tool,
                system_prompt: _,
            } if external_tool.tool_name == "hangup_call" => {
                tracing::debug!(chat_id = %chat_id, "Hangup requested by agent");
                state
                    .chat_service
                    .save_assistant_message_with_tool_calls(
                        chat_id,
                        accumulated_text.clone(),
                        Some(tool_calls_json),
                        vec![],
                    )
                    .await
                    .ok();

                for tr in &tool_results {
                    state
                        .chat_service
                        .save_tool_result_message_with_tool(
                            chat_id,
                            &tr.tool_call_id,
                            tr.result.clone(),
                            tr.tool_data.clone(),
                            None,
                        )
                        .await
                        .ok();
                }

                state
                    .chat_service
                    .save_tool_result_message_with_tool(
                        chat_id,
                        &external_tool.tool_call_id,
                        "Call ended".to_string(),
                        None,
                        None,
                    )
                    .await
                    .ok();

                if let Some(cid) = call_id
                    && let Err(e) = state.call_service.mark_completed(cid).await
                {
                    tracing::warn!(error = %e, call_id = %cid, "Failed to mark call completed");
                }

                return Ok((accumulated_text, true));
            }
            InferenceResponse::Completed { .. } => {
                let text = outcome.last_segment;
                if !text.is_empty() {
                    state
                        .chat_service
                        .save_assistant_message(chat_id, text.clone())
                        .await
                        .ok();
                }
                return Ok((text, false));
            }
            _ => {
                return Ok((outcome.last_segment, false));
            }
        }
    }
}
