use serde_json::Value;

use crate::agent::models::UpdateAgentRequest;
use crate::agent::service::AgentService;
use crate::agent::prompt::PromptLoader;
use crate::core::error::AppError;
use crate::inference::InferenceEventKind;
use frona_derive::agent_tool;

use super::{InferenceContext, ToolOutput};

pub struct UpdateIdentityTool {
    agent_service: AgentService,
    prompts: PromptLoader,
}

impl UpdateIdentityTool {
    pub fn new(agent_service: AgentService, prompts: PromptLoader) -> Self {
        Self { agent_service, prompts }
    }
}

#[agent_tool]
impl UpdateIdentityTool {
    async fn execute(&self, _tool_name: &str, arguments: Value, ctx: &InferenceContext) -> Result<ToolOutput, AppError> {
        let attrs = arguments
            .get("attributes")
            .and_then(|v| v.as_object().cloned())
            .ok_or_else(|| AppError::Tool("'attributes' must be a JSON object".into()))?;

        if attrs.is_empty() {
            return Err(AppError::Tool("No attributes provided".into()));
        }

        let agent_id = &ctx.agent.id;
        let user_id = &ctx.user.id;

        let mut identity = self
            .agent_service
            .get(user_id, agent_id)
            .await?
            .identity;

        for (key, value) in &attrs {
            let val_str = match value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            if val_str.is_empty() {
                identity.remove(key);
            } else {
                identity.insert(key.clone(), val_str);
            }
        }

        let updated = self
            .agent_service
            .update(
                user_id,
                agent_id,
                UpdateAgentRequest::builder().identity(identity).build(),
            )
            .await?;

        let mut entity_fields = serde_json::Map::new();
        entity_fields.insert("identity".to_string(), serde_json::to_value(&updated.identity).unwrap());
        entity_fields.insert("name".to_string(), serde_json::json!(updated.name));

        ctx.event_tx.send(crate::inference::tool_loop::InferenceEvent {
            kind: InferenceEventKind::EntityUpdated {
                table: "agent".to_string(),
                record_id: agent_id.clone(),
                fields: Value::Object(entity_fields),
            },
        });

        let updated_keys: Vec<&String> = attrs.keys().collect();
        Ok(ToolOutput::text(format!(
            "Updated identity attributes: {}",
            updated_keys
                .iter()
                .map(|k| k.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )))
    }
}
