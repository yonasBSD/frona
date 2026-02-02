use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::agent::repository::AgentRepository;
use crate::error::AppError;
use crate::schedule::service::ScheduleService;

use super::{AgentTool, ToolDefinition, ToolOutput};

pub struct UpdateRoutineTool {
    schedule_service: ScheduleService,
    agent_repo: Arc<dyn AgentRepository>,
    user_id: String,
    agent_id: String,
}

impl UpdateRoutineTool {
    pub fn new(
        schedule_service: ScheduleService,
        agent_repo: Arc<dyn AgentRepository>,
        user_id: String,
        agent_id: String,
    ) -> Self {
        Self {
            schedule_service,
            agent_repo,
            user_id,
            agent_id,
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
}

#[async_trait]
impl AgentTool for UpdateRoutineTool {
    fn name(&self) -> &str {
        "routine"
    }

    fn definitions(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            name: "update_routine".to_string(),
            description: "Manage an agent's routine — a recurring list of items the agent \
                processes on a schedule. Add/remove items and set the interval between runs.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "target_agent": {
                        "type": "string",
                        "description": "Optional: agent name to manage routine for (from <available_agents>). Omit for your own routine."
                    },
                    "items_to_add": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Descriptions of items to add to the routine"
                    },
                    "items_to_remove": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "IDs of items to remove from the routine"
                    },
                    "interval_minutes": {
                        "type": "integer",
                        "description": "Minutes between routine runs (e.g., 30 = every 30 min). Set to enable/update the schedule. Measured from completion of previous run."
                    }
                }
            }),
        }]
    }

    async fn execute(&self, _tool_name: &str, arguments: Value) -> Result<ToolOutput, AppError> {
        let target_agent = arguments.get("target_agent").and_then(|v| v.as_str());
        let agent_id = self.resolve_agent_id(target_agent).await?;

        let routine = self
            .schedule_service
            .get_or_create_routine(&self.user_id, &agent_id)
            .await?;

        let items_to_add: Vec<String> = arguments
            .get("items_to_add")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let items_to_remove: Vec<String> = arguments
            .get("items_to_remove")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let has_item_changes = !items_to_add.is_empty() || !items_to_remove.is_empty();
        let routine = if has_item_changes {
            self.schedule_service
                .update_routine_items(&routine.id, items_to_add, items_to_remove)
                .await?
        } else {
            routine
        };

        let interval_minutes = arguments
            .get("interval_minutes")
            .and_then(|v| v.as_u64());

        let routine = if let Some(mins) = interval_minutes {
            self.schedule_service
                .set_routine_interval(&routine.id, Some(mins))
                .await?
        } else {
            routine
        };

        let items_summary: Vec<Value> = routine
            .items
            .iter()
            .map(|item| {
                serde_json::json!({
                    "id": item.id,
                    "description": item.description,
                })
            })
            .collect();

        Ok(ToolOutput::text(serde_json::json!({
            "routine_id": routine.id,
            "agent_id": routine.agent_id,
            "items": items_summary,
            "item_count": routine.items.len(),
            "interval_minutes": routine.interval_mins,
            "next_run_at": routine.next_run_at.map(|t| t.to_rfc3339()),
            "status": routine.status,
        }).to_string()))
    }
}

pub struct UpdateRoutineFrequencyTool {
    schedule_service: ScheduleService,
    agent_repo: Arc<dyn AgentRepository>,
    user_id: String,
    agent_id: String,
}

impl UpdateRoutineFrequencyTool {
    pub fn new(
        schedule_service: ScheduleService,
        agent_repo: Arc<dyn AgentRepository>,
        user_id: String,
        agent_id: String,
    ) -> Self {
        Self {
            schedule_service,
            agent_repo,
            user_id,
            agent_id,
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
}

#[async_trait]
impl AgentTool for UpdateRoutineFrequencyTool {
    fn name(&self) -> &str {
        "routine"
    }

    fn definitions(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            name: "update_routine_frequency".to_string(),
            description: "Change how often an agent's routine runs. Use this to adjust the \
                interval between routine executions (e.g., from hourly to every 15 minutes).".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "target_agent": {
                        "type": "string",
                        "description": "Optional: agent name to update frequency for (from <available_agents>). Omit for your own routine."
                    },
                    "interval_minutes": {
                        "type": "integer",
                        "description": "Minutes between routine runs (e.g., 15 = every 15 min). The next run is rescheduled immediately."
                    }
                },
                "required": ["interval_minutes"]
            }),
        }]
    }

    async fn execute(&self, _tool_name: &str, arguments: Value) -> Result<ToolOutput, AppError> {
        let target_agent = arguments.get("target_agent").and_then(|v| v.as_str());
        let agent_id = self.resolve_agent_id(target_agent).await?;

        let interval_minutes = arguments
            .get("interval_minutes")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| AppError::Validation("interval_minutes is required".into()))?;

        let routine = self
            .schedule_service
            .get_or_create_routine(&self.user_id, &agent_id)
            .await?;

        let routine = self
            .schedule_service
            .set_routine_interval(&routine.id, Some(interval_minutes))
            .await?;

        Ok(ToolOutput::text(serde_json::json!({
            "routine_id": routine.id,
            "agent_id": routine.agent_id,
            "interval_minutes": routine.interval_mins,
            "next_run_at": routine.next_run_at.map(|t| t.to_rfc3339()),
            "status": routine.status,
        }).to_string()))
    }
}
