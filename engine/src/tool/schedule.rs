use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::agent::repository::AgentRepository;
use crate::agent::task::service::TaskService;
use crate::error::AppError;
use crate::schedule::service::ScheduleService;

use super::{AgentTool, ToolDefinition, ToolOutput};

pub struct ScheduleTaskTool {
    task_service: TaskService,
    agent_repo: Arc<dyn AgentRepository>,
    user_id: String,
    agent_id: String,
    chat_id: String,
}

impl ScheduleTaskTool {
    pub fn new(
        task_service: TaskService,
        agent_repo: Arc<dyn AgentRepository>,
        user_id: String,
        agent_id: String,
        chat_id: String,
    ) -> Self {
        Self {
            task_service,
            agent_repo,
            user_id,
            agent_id,
            chat_id,
        }
    }

    async fn resolve_agent_id(&self, target_agent: Option<&str>) -> Result<String, AppError> {
        match target_agent {
            Some(name) => {
                let agent = self
                    .agent_repo
                    .find_by_name(&self.user_id, name)
                    .await?
                    .ok_or_else(|| {
                        AppError::Validation(format!(
                            "Agent '{}' not found. Check <available_agents> for valid agent names.",
                            name
                        ))
                    })?;
                if !agent.enabled {
                    return Err(AppError::Validation(format!(
                        "Agent '{}' is disabled.",
                        agent.name
                    )));
                }
                Ok(agent.id)
            }
            None => Ok(self.agent_id.clone()),
        }
    }

    async fn handle_create(&self, arguments: &Value) -> Result<ToolOutput, AppError> {
        let cron_expression = arguments
            .get("cron_expression")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("Missing 'cron_expression' parameter".into()))?;

        let instruction = arguments
            .get("instruction")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("Missing 'instruction' parameter".into()))?;

        let title = arguments
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or(instruction);

        let target_agent = arguments.get("target_agent").and_then(|v| v.as_str());
        let agent_id = self.resolve_agent_id(target_agent).await?;

        ScheduleService::parse_cron(cron_expression)?;
        let next_run_at = ScheduleService::next_cron_occurrence(cron_expression)?;

        let (source_agent_id, source_chat_id) = if target_agent.is_some() {
            (Some(self.agent_id.clone()), Some(self.chat_id.clone()))
        } else {
            (None, None)
        };

        let task = self
            .task_service
            .create_cron_template(
                &self.user_id,
                &agent_id,
                title,
                instruction,
                cron_expression,
                next_run_at,
                source_agent_id,
                source_chat_id,
            )
            .await?;

        Ok(ToolOutput::text(serde_json::json!({
            "task_id": task.id,
            "cron_expression": cron_expression,
            "next_run_at": next_run_at.to_rfc3339(),
            "message": format!("Scheduled task '{}' created. Next run at {}.", title, next_run_at.format("%Y-%m-%d %H:%M UTC"))
        }).to_string()))
    }

    async fn handle_delete(&self, arguments: &Value) -> Result<ToolOutput, AppError> {
        let task_id = arguments
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("Missing 'task_id' parameter".into()))?;

        let task = self
            .task_service
            .find_by_id(task_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Scheduled task not found".into()))?;

        if task.user_id != self.user_id {
            return Err(AppError::Forbidden("Not your task".into()));
        }

        self.task_service.mark_cancelled(task_id).await?;

        Ok(ToolOutput::text(serde_json::json!({
            "message": format!("Scheduled task '{}' cancelled.", task.title)
        }).to_string()))
    }

    async fn handle_list(&self, arguments: &Value) -> Result<ToolOutput, AppError> {
        let target_agent = arguments.get("target_agent").and_then(|v| v.as_str());
        let _ = self.resolve_agent_id(target_agent).await?;

        let tasks = self.task_service.find_due_cron_templates().await.unwrap_or_default();
        let all_tasks = self.task_service.list_active(&self.user_id).await.unwrap_or_default();

        let cron_tasks: Vec<_> = all_tasks
            .into_iter()
            .filter(|t| matches!(t.kind, crate::agent::task::models::TaskKind::Cron { .. }))
            .map(|t| {
                serde_json::json!({
                    "id": t.id,
                    "title": t.title,
                    "description": t.description,
                    "agent_id": t.agent_id,
                    "kind": t.kind,
                    "status": t.status,
                })
            })
            .collect();

        let _ = tasks;

        Ok(ToolOutput::text(serde_json::json!({
            "scheduled_tasks": cron_tasks,
            "count": cron_tasks.len(),
        }).to_string()))
    }
}

#[async_trait]
impl AgentTool for ScheduleTaskTool {
    fn name(&self) -> &str {
        "schedule"
    }

    fn definitions(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            name: "schedule_task".to_string(),
            description: "Create, delete, or list scheduled (cron) tasks. Scheduled tasks run \
                automatically at the specified cron schedule.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["create", "delete", "list"],
                        "description": "The action to perform"
                    },
                    "target_agent": {
                        "type": "string",
                        "description": "Optional: agent name to schedule for (from <available_agents>). Omit to schedule for yourself."
                    },
                    "cron_expression": {
                        "type": "string",
                        "description": "5-field cron expression (minute hour day-of-month month day-of-week). Required for 'create'."
                    },
                    "title": {
                        "type": "string",
                        "description": "Short title for the scheduled task. Optional for 'create'."
                    },
                    "instruction": {
                        "type": "string",
                        "description": "Detailed instructions for the agent when the task fires. Required for 'create'."
                    },
                    "task_id": {
                        "type": "string",
                        "description": "The scheduled task ID to cancel. Required for 'delete'."
                    }
                },
                "required": ["action"]
            }),
        }]
    }

    async fn execute(&self, _tool_name: &str, arguments: Value) -> Result<ToolOutput, AppError> {
        let action = arguments
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("Missing 'action' parameter".into()))?;

        match action {
            "create" => self.handle_create(&arguments).await,
            "delete" => self.handle_delete(&arguments).await,
            "list" => self.handle_list(&arguments).await,
            _ => Err(AppError::Validation(format!("Unknown action: {}", action))),
        }
    }
}
