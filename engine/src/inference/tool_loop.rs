use std::time::Instant;

use base64::Engine;
use rig::completion::message::{
    DocumentSourceKind, ImageMediaType, MimeType, ToolResult, ToolResultContent, UserContent,
};
use rig::completion::request::ToolDefinition as RigToolDefinition;
use rig::completion::{AssistantContent, Message as RigMessage};
use tokio_util::sync::CancellationToken;

use crate::chat::broadcast::EventSender;
use crate::chat::message::models::{MessageTool, Reasoning};
use crate::core::error::AppError;
use crate::core::metrics::{self, InferenceMetricsContext};
use crate::tool::registry::AgentToolRegistry;
use crate::tool::{InferenceContext, ToolDefinition};

use super::config::ModelGroup;
use super::registry::ModelProviderRegistry;
use super::retry::StreamResult;
use super::retry::stream_with_retry_and_fallback;

pub struct InferenceEvent {
    pub kind: InferenceEventKind,
}

#[derive(Debug, Clone)]
pub enum InferenceEventKind {
    Text(String),
    Reasoning(String),
    ToolCall {
        name: String,
        arguments: serde_json::Value,
        description: Option<String>,
    },
    ToolResult {
        name: String,
        result: String,
        success: bool,
    },
    EntityUpdated {
        table: String,
        record_id: String,
        fields: serde_json::Value,
    },
    Retry { retry_after_ms: u64, reason: &'static str },
    Done(String),
    Cancelled(String),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct ToolCallResult {
    pub tool_call_id: String,
    pub tool_name: String,
    pub result: String,
    pub tool_data: Option<MessageTool>,
    pub system_prompt: Option<String>,
}

#[derive(Debug)]
pub enum ToolLoopOutcome {
    Completed {
        text: String,
        attachments: Vec<crate::storage::Attachment>,
        lifecycle_event: Option<MessageTool>,
        reasoning: Option<Reasoning>,
    },
    Cancelled(String),
    ExternalToolPending {
        accumulated_text: String,
        tool_calls_json: serde_json::Value,
        tool_results: Vec<ToolCallResult>,
        external_tool: Box<ToolCallResult>,
        system_prompt: Option<String>,
    },
}

pub fn extract_reasoning(contents: &[AssistantContent]) -> Option<Reasoning> {
    contents.iter().find_map(|c| {
        if let AssistantContent::Reasoning(r) = c {
            Some(Reasoning {
                id: r.id.clone(),
                content: r.reasoning.join(""),
                signature: r.signature.clone(),
            })
        } else {
            None
        }
    })
}

fn to_rig_tool_definitions(defs: &[ToolDefinition]) -> Vec<RigToolDefinition> {
    defs.iter()
        .map(|d| RigToolDefinition {
            name: d.name.clone(),
            description: d.description.clone(),
            parameters: d.parameters.clone(),
        })
        .collect()
}

async fn check_cancellation(
    cancel_token: &CancellationToken,
    event_tx: &EventSender,
    accumulated_text: &str,
) -> Option<ToolLoopOutcome> {
    if cancel_token.is_cancelled() {
        event_tx.send(InferenceEvent {
            kind: InferenceEventKind::Cancelled(accumulated_text.to_string()),
        });
        Some(ToolLoopOutcome::Cancelled(accumulated_text.to_string()))
    } else {
        None
    }
}


async fn process_model_response(
    contents: &[AssistantContent],
    event_tx: &EventSender,
    chat_history: &mut Vec<RigMessage>,
) -> bool {
    let mut has_tool_calls = false;
    let mut assistant_content_items: Vec<AssistantContent> = Vec::new();

    for content in contents {
        if let AssistantContent::ToolCall(tool_call) = content {
            has_tool_calls = true;
            let mut args = tool_call.function.arguments.clone();
            let description = args
                .as_object_mut()
                .and_then(|obj| obj.remove("description"))
                .and_then(|v| v.as_str().map(String::from));
            event_tx.send(InferenceEvent {
                    kind: InferenceEventKind::ToolCall {
                        name: tool_call.function.name.clone(),
                        arguments: args,
                        description,
                    },
                });
        }
        assistant_content_items.push(content.clone());
    }

    let assistant_msg = RigMessage::Assistant {
        id: None,
        content: rig::OneOrMany::many(assistant_content_items)
            .unwrap_or_else(|_| rig::OneOrMany::one(AssistantContent::text(""))),
    };
    chat_history.push(assistant_msg);

    has_tool_calls
}

fn build_tool_result_message(
    tool_call_id: String,
    result: String,
    tool_output: &crate::tool::ToolOutput,
) -> RigMessage {
    let has_images = !tool_output.images().is_empty();
    if has_images {
        let tool_result_content = UserContent::ToolResult(ToolResult {
            id: tool_call_id,
            call_id: None,
            content: rig::OneOrMany::one(ToolResultContent::text(&result)),
        });
        let mut user_contents = vec![tool_result_content];
        for img in tool_output.images() {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&img.bytes);
            user_contents.push(UserContent::Image(
                rig::completion::message::Image {
                    data: DocumentSourceKind::Base64(b64),
                    media_type: ImageMediaType::from_mime_type(&img.media_type),
                    detail: None,
                    additional_params: None,
                },
            ));
        }
        RigMessage::User {
            content: rig::OneOrMany::many(user_contents).unwrap(),
        }
    } else {
        RigMessage::tool_result(tool_call_id, result)
    }
}

struct ToolExecutionResult {
    has_external: bool,
    external_tool_result: Option<ToolCallResult>,
    internal_tool_results: Vec<ToolCallResult>,
    accumulated_system_prompts: Vec<String>,
}

async fn execute_tool_calls(
    contents: &[AssistantContent],
    tool_registry: &AgentToolRegistry,
    ctx: &InferenceContext,
    event_tx: &EventSender,
    chat_history: &mut Vec<RigMessage>,
    all_attachments: &mut Vec<crate::storage::Attachment>,
    metrics_ctx: &InferenceMetricsContext,
) -> ToolExecutionResult {
    let mut result = ToolExecutionResult {
        has_external: false,
        external_tool_result: None,
        internal_tool_results: Vec::new(),
        accumulated_system_prompts: Vec::new(),
    };

    for content in contents {
        let AssistantContent::ToolCall(tool_call) = content else {
            continue;
        };

        let tool_name = &tool_call.function.name;
        let mut arguments = tool_call.function.arguments.clone();
        if let Some(obj) = arguments.as_object_mut() {
            obj.remove("description");
        }

        tracing::debug!(tool = %tool_name, args = %arguments, "Executing tool");

        let start = Instant::now();
        let (text, tool_output) = match tool_registry.execute(tool_name, arguments, ctx).await {
            Ok(output) => {
                let text = output.text_content().to_string();
                tracing::debug!(tool = %tool_name, result = %text, "Tool executed");
                let duration = start.elapsed();
                metrics::record_tool_execution(
                    tool_name,
                    &metrics_ctx.user_id,
                    &metrics_ctx.agent_id,
                    duration,
                    "success",
                );
                (text, Some(output))
            }
            Err(e) => {
                tracing::warn!(tool = %tool_name, error = %e, "Tool execution failed");
                let duration = start.elapsed();
                metrics::record_tool_execution(
                    tool_name,
                    &metrics_ctx.user_id,
                    &metrics_ctx.agent_id,
                    duration,
                    "error",
                );
                (format!("Error: {e}"), None)
            }
        };

        let success = tool_output.as_ref().is_some_and(|o| o.is_success());
        event_tx.send(InferenceEvent {
                kind: InferenceEventKind::ToolResult {
                    name: tool_name.clone(),
                    result: text.clone(),
                    success,
                },
            });

        let td = tool_output.as_ref().and_then(|o| o.tool_data().cloned());
        let sp = tool_output.as_ref().and_then(|o| o.system_prompt().map(str::to_string));

        if let Some(ref output) = tool_output {
            for attachment in output.attachments() {
                all_attachments.push(attachment.clone());
            }
        }

        let tool_call_result = ToolCallResult {
            tool_call_id: tool_call.id.clone(),
            tool_name: tool_name.clone(),
            result: text.clone(),
            tool_data: td,
            system_prompt: sp.clone(),
        };

        let is_pending_external = tool_output.as_ref().is_some_and(|o| o.is_pending_external());

        if is_pending_external {
            result.has_external = true;
            result.external_tool_result = Some(tool_call_result);
        } else {
            if let Some(sp_value) = sp {
                result.accumulated_system_prompts.push(sp_value);
            }
            result.internal_tool_results.push(tool_call_result);
            if let Some(output) = tool_output {
                let msg = build_tool_result_message(tool_call.id.clone(), text, &output);
                chat_history.push(msg);
            } else {
                chat_history.push(RigMessage::tool_result(tool_call.id.clone(), text));
            }
        }
    }

    result
}

fn build_tool_calls_json(contents: &[AssistantContent]) -> serde_json::Value {
    let calls: Vec<_> = contents
        .iter()
        .filter_map(|c| {
            if let AssistantContent::ToolCall(tc) = c {
                Some(serde_json::json!({
                    "id": tc.id,
                    "name": tc.function.name,
                    "arguments": tc.function.arguments,
                }))
            } else {
                None
            }
        })
        .collect();
    serde_json::json!(calls)
}

#[allow(clippy::too_many_arguments)]
pub async fn run_tool_loop(
    registry: &ModelProviderRegistry,
    model_group: &ModelGroup,
    system_prompt: &str,
    mut chat_history: Vec<RigMessage>,
    tool_registry: &AgentToolRegistry,
    event_tx: EventSender,
    cancel_token: CancellationToken,
    ctx: &InferenceContext,
    metrics_ctx: &InferenceMetricsContext,
) -> Result<ToolLoopOutcome, AppError> {
    let tool_defs = &tool_registry.definitions;
    let rig_tools = to_rig_tool_definitions(tool_defs);

    let mut accumulated_text = String::new();
    let mut all_attachments: Vec<crate::storage::Attachment> = Vec::new();
    let mut current_system_prompt = system_prompt.to_string();
    let mut last_reasoning: Option<Reasoning> = None;

    let max_tool_turns = model_group.inference.max_tool_turns;
    for turn in 0..max_tool_turns {
        if let Some(outcome) = check_cancellation(&cancel_token, &event_tx, &accumulated_text).await {
            return Ok(outcome);
        }

        tracing::debug!(turn, "Tool loop turn");

        let max_output = model_group.max_tokens.unwrap_or(model_group.inference.default_max_tokens) as usize;
        chat_history = crate::inference::context::truncate_history(
            chat_history,
            &current_system_prompt,
            &model_group.main.model_id,
            model_group.context_window,
            max_output,
            model_group.inference.history_truncation_pct,
        );

        // Drop leading orphaned tool_results whose tool_use was truncated away
        while let Some(RigMessage::User { content }) = chat_history.first() {
            if content.iter().any(|c| matches!(c, UserContent::ToolResult(_))) {
                chat_history.remove(0);
            } else {
                break;
            }
        }

        let contents = match stream_with_retry_and_fallback(
            registry,
            model_group,
            &current_system_prompt,
            &chat_history,
            &rig_tools,
            &event_tx,
            &cancel_token,
            &mut accumulated_text,
            metrics_ctx,
        )
        .await?
        {
            StreamResult::Contents(c) => c,
            StreamResult::Cancelled => {
                event_tx.send(InferenceEvent {
                        kind: InferenceEventKind::Cancelled(accumulated_text.clone()),
                    });
                return Ok(ToolLoopOutcome::Cancelled(accumulated_text));
            }
        };

        last_reasoning = extract_reasoning(&contents);

        let has_tool_calls =
            process_model_response(&contents, &event_tx, &mut chat_history).await;

        if !has_tool_calls {
            event_tx.send(InferenceEvent {
                    kind: InferenceEventKind::Done(accumulated_text.clone()),
                });
            break;
        }

        let exec_result = execute_tool_calls(
            &contents,
            tool_registry,
            ctx,
            &event_tx,
            &mut chat_history,
            &mut all_attachments,
            metrics_ctx,
        )
        .await;

        if let Some(outcome) = check_cancellation(&cancel_token, &event_tx, &accumulated_text).await {
            return Ok(outcome);
        }

        if exec_result.has_external {
            let external_tool = exec_result.external_tool_result.unwrap();
            let system_prompt_injection = external_tool.system_prompt.clone();
            event_tx.send(InferenceEvent {
                    kind: InferenceEventKind::Done(accumulated_text.clone()),
                });
            return Ok(ToolLoopOutcome::ExternalToolPending {
                accumulated_text,
                tool_calls_json: build_tool_calls_json(&contents),
                tool_results: exec_result.internal_tool_results,
                external_tool: Box::new(external_tool),
                system_prompt: system_prompt_injection,
            });
        }

        // Check for task lifecycle events (complete_task, fail_task, defer_task)
        // and break immediately — no need for another inference turn.
        let lifecycle_event = exec_result.internal_tool_results.iter().find_map(|r| {
            match &r.tool_data {
                Some(t @ (MessageTool::TaskCompletion { .. } | MessageTool::TaskDeferred { .. })) => {
                    Some(t.clone())
                }
                _ => None,
            }
        });
        if lifecycle_event.is_some() {
            event_tx.send(InferenceEvent {
                kind: InferenceEventKind::Done(accumulated_text.clone()),
            });
            return Ok(ToolLoopOutcome::Completed {
                text: accumulated_text,
                attachments: all_attachments,
                lifecycle_event,
                reasoning: last_reasoning,
            });
        }

        for sp in exec_result.accumulated_system_prompts {
            current_system_prompt.push_str("\n\n");
            current_system_prompt.push_str(&sp);
        }

        if turn == max_tool_turns - 1 {
            event_tx.send(InferenceEvent {
                    kind: InferenceEventKind::Error(
                        "Max tool turns reached".to_string(),
                    ),
                });
        }
    }

    Ok(ToolLoopOutcome::Completed {
        text: accumulated_text,
        attachments: all_attachments,
        lifecycle_event: None,
        reasoning: last_reasoning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::completion::AssistantContent;

    #[test]
    fn extract_reasoning_from_contents() {
        let contents = vec![
            AssistantContent::Reasoning(
                rig::completion::message::Reasoning::new("thinking hard")
                    .with_id("r-1".to_string())
                    .with_signature(Some("sig-123".to_string())),
            ),
            AssistantContent::text("final answer"),
        ];
        let r = extract_reasoning(&contents);
        assert!(r.is_some());
        let r = r.unwrap();
        assert_eq!(r.content, "thinking hard");
        assert_eq!(r.id, Some("r-1".to_string()));
        assert_eq!(r.signature, Some("sig-123".to_string()));
    }

    #[test]
    fn extract_reasoning_returns_none_when_absent() {
        let contents = vec![
            AssistantContent::text("just text"),
        ];
        assert!(extract_reasoning(&contents).is_none());
    }

    #[test]
    fn extract_reasoning_joins_multi_chunk() {
        let contents = vec![
            AssistantContent::Reasoning(
                rig::completion::message::Reasoning::multi(
                    vec!["chunk1 ".to_string(), "chunk2".to_string()],
                ),
            ),
        ];
        let r = extract_reasoning(&contents).unwrap();
        assert_eq!(r.content, "chunk1 chunk2");
    }
}
