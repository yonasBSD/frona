use chrono::{Duration, Utc};
use serde_json::Value;

use crate::agent::prompt::PromptLoader;
use crate::agent::service::AgentService;
use crate::storage::StorageService;
use crate::core::error::AppError;
use frona_derive::agent_tool;

use super::{InferenceContext, ToolOutput};

pub struct HeartbeatTool {
    agent_service: AgentService,
    storage: StorageService,
    prompts: PromptLoader,
}

impl HeartbeatTool {
    pub fn new(
        agent_service: AgentService,
        storage: StorageService,
        prompts: PromptLoader,
    ) -> Self {
        Self {
            agent_service,
            storage,
            prompts,
        }
    }
}

#[agent_tool(files("set_heartbeat"))]
impl HeartbeatTool {
    async fn execute(&self, _tool_name: &str, arguments: Value, ctx: &InferenceContext) -> Result<ToolOutput, AppError> {
        let interval_minutes = arguments
            .get("interval_minutes")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| AppError::Validation("interval_minutes is required".into()))?;

        let agent_id = &ctx.agent.id;

        if interval_minutes > 0 {
            let ws = self.storage.agent_workspace(agent_id);
            match ws.read("HEARTBEAT.md") {
                Some(content) if !content.trim().is_empty() => {}
                _ => {
                    return Err(AppError::Validation(
                        "HEARTBEAT.md is missing or empty. Write your heartbeat checklist to HEARTBEAT.md first.".into(),
                    ));
                }
            }
        }

        if interval_minutes == 0 {
            self.agent_service
                .set_heartbeat(agent_id, None)
                .await?;

            return Ok(ToolOutput::text(serde_json::json!({
                "message": "Heartbeat disabled.",
                "heartbeat_interval": null,
                "next_heartbeat_at": null,
            }).to_string()));
        }

        let next = Utc::now() + Duration::minutes(interval_minutes as i64);
        self.agent_service
            .set_heartbeat(agent_id, Some(interval_minutes))
            .await?;

        Ok(ToolOutput::text(serde_json::json!({
            "message": format!("Heartbeat set to every {} minutes. Next heartbeat at {}.", interval_minutes, next.format("%Y-%m-%d %H:%M UTC")),
            "heartbeat_interval": interval_minutes,
            "next_heartbeat_at": next.to_rfc3339(),
        }).to_string()))
    }
}
