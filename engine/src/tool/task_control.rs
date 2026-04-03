use std::path::PathBuf;

use serde_json::Value;

use crate::agent::prompt::PromptLoader;
use crate::agent::task::models::TaskStatus;
use crate::inference::tool_execution::MessageTool;
use crate::storage::resolve_workspace_attachment;
use crate::core::error::AppError;
use frona_derive::agent_tool;

use super::{InferenceContext, ToolOutput};

pub struct TaskControlTool {
    workspaces_path: PathBuf,
    prompts: PromptLoader,
}

impl TaskControlTool {
    pub fn new(workspaces_path: PathBuf, prompts: PromptLoader) -> Self {
        Self { workspaces_path, prompts }
    }
}

#[agent_tool(name = "task_control", files("complete_task", "defer_task", "fail_task"))]
impl TaskControlTool {
    async fn execute(
        &self,
        tool_name: &str,
        arguments: Value,
        ctx: &InferenceContext,
    ) -> Result<ToolOutput, AppError> {
        let task = ctx
            .task
            .as_ref()
            .ok_or_else(|| AppError::Tool("task_control tools can only be used within a task context".into()))?;

        match tool_name {
            "complete_task" => {
                let result = arguments
                    .get("result")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let mut resolved_deliverables = Vec::new();
                if let Some(deliverables) = arguments.get("deliverables").and_then(|v| v.as_array()) {
                    for path_val in deliverables {
                        if let Some(path) = path_val.as_str() {
                            let attachment = resolve_workspace_attachment(
                                &self.workspaces_path,
                                &ctx.agent.id,
                                path,
                            ).await?;
                            resolved_deliverables.push(attachment);
                        }
                    }
                }

                let mut output = ToolOutput::text("Task marked as complete.").with_tool_data(
                    MessageTool::TaskCompletion {
                        task_id: task.id.clone(),
                        chat_id: Some(ctx.chat.id.clone()),
                        status: TaskStatus::Completed,
                        summary: result,
                        deliverables: resolved_deliverables.clone(),
                    },
                );

                for attachment in resolved_deliverables {
                    output = output.with_attachment(attachment);
                }

                Ok(output)
            }
            "fail_task" => {
                let reason = arguments
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AppError::Validation("Missing 'reason' parameter".into()))?;

                Ok(ToolOutput::text("Task marked as failed.").with_tool_data(
                    MessageTool::TaskCompletion {
                        task_id: task.id.clone(),
                        chat_id: Some(ctx.chat.id.clone()),
                        status: TaskStatus::Failed,
                        summary: Some(reason.to_string()),
                        deliverables: vec![],
                    },
                ))
            }
            "defer_task" => {
                let delay_minutes = arguments
                    .get("delay_minutes")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| {
                        AppError::Validation("Missing 'delay_minutes' parameter".into())
                    })? as u32;

                let reason = arguments
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AppError::Validation("Missing 'reason' parameter".into()))?;

                Ok(
                    ToolOutput::text(format!("Task deferred for {delay_minutes} minutes."))
                        .with_tool_data(MessageTool::TaskDeferred {
                            task_id: task.id.clone(),
                            delay_minutes,
                            reason: reason.to_string(),
                        }),
                )
            }
            _ => Err(AppError::Tool(format!("Unknown task_control tool: {tool_name}"))),
        }
    }
}
