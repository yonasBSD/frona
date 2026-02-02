use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::agent::repository::AgentRepository;
use crate::agent::task::dto::CreateTaskRequest;
use crate::agent::task::executor::TaskExecutor;
use crate::agent::task::service::TaskService;
use crate::error::AppError;

use super::{AgentTool, ToolDefinition, ToolOutput};

pub struct DelegateTaskTool {
    task_service: TaskService,
    agent_repo: Arc<dyn AgentRepository>,
    task_executor: Arc<TaskExecutor>,
    user_id: String,
    agent_id: String,
    chat_id: String,
    space_id: Option<String>,
}

impl DelegateTaskTool {
    pub fn new(
        task_service: TaskService,
        agent_repo: Arc<dyn AgentRepository>,
        task_executor: Arc<TaskExecutor>,
        user_id: String,
        agent_id: String,
        chat_id: String,
        space_id: Option<String>,
    ) -> Self {
        Self {
            task_service,
            agent_repo,
            task_executor,
            user_id,
            agent_id,
            chat_id,
            space_id,
        }
    }
}

#[async_trait]
impl AgentTool for DelegateTaskTool {
    fn name(&self) -> &str {
        "delegate"
    }

    fn definitions(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            name: "delegate_task".to_string(),
            description: "Delegate a task to another agent. The task will run in the background \
                and results will be posted back to this chat when complete. Returns immediately \
                with a task ID — does not block.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "target_agent": {
                        "type": "string",
                        "description": "The name of the agent to delegate the task to (from <available_agents>)"
                    },
                    "title": {
                        "type": "string",
                        "description": "A short title for the task"
                    },
                    "instruction": {
                        "type": "string",
                        "description": "Detailed instructions for the target agent"
                    },
                    "deliver_directly": {
                        "type": "boolean",
                        "description": "If true, the result is delivered to the source chat without resuming the parent agent's tool loop. Use when the delegated agent's output should be shown directly to the user."
                    }
                },
                "required": ["target_agent", "title", "instruction"]
            }),
        }]
    }

    async fn execute(&self, _tool_name: &str, arguments: Value) -> Result<ToolOutput, AppError> {
        let target_agent_name = arguments
            .get("target_agent")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("Missing 'target_agent' parameter".into()))?;

        let title = arguments
            .get("title")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("Missing 'title' parameter".into()))?;

        let instruction = arguments
            .get("instruction")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("Missing 'instruction' parameter".into()))?;

        let deliver_directly = arguments
            .get("deliver_directly")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let target_agent = self
            .agent_repo
            .find_by_name(&self.user_id, target_agent_name)
            .await?
            .ok_or_else(|| {
                AppError::Validation(format!(
                    "Agent '{}' not found. Check <available_agents> for valid agent names.",
                    target_agent_name
                ))
            })?;

        if !target_agent.enabled {
            return Err(AppError::Validation(format!(
                "Agent '{}' is disabled and cannot accept tasks.",
                target_agent.name
            )));
        }

        let req = CreateTaskRequest {
            agent_id: target_agent.id.clone(),
            space_id: self.space_id.clone(),
            chat_id: None,
            title: title.to_string(),
            description: Some(instruction.to_string()),
            source_agent_id: Some(self.agent_id.clone()),
            source_chat_id: Some(self.chat_id.clone()),
            deliver_directly: Some(deliver_directly),
        };

        let task_response = self.task_service.create(&self.user_id, req).await?;
        let task_id = task_response.id.clone();

        let task = self
            .task_service
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| AppError::Internal("Task just created but not found".into()))?;

        if let Err(e) = self.task_executor.spawn_execution(task).await {
            tracing::warn!(error = %e, task_id = %task_id, "Failed to spawn task execution immediately");
        }

        Ok(ToolOutput::text(serde_json::json!({
            "task_id": task_id,
            "target_agent": target_agent.name,
            "message": format!("Task '{}' delegated to {}. Results will be posted to this chat when complete.", title, target_agent.name)
        }).to_string()))
    }
}

