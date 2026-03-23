use serde_json::Value;

use crate::agent::prompt::PromptLoader;
use crate::agent::skill::service::SkillService;
use crate::core::error::AppError;
use frona_derive::agent_tool;

use super::{InferenceContext, ToolOutput};

pub struct SkillTool {
    skill_service: SkillService,
    prompts: PromptLoader,
}

impl SkillTool {
    pub fn new(skill_service: SkillService, prompts: PromptLoader) -> Self {
        Self {
            skill_service,
            prompts,
        }
    }
}

#[agent_tool(files("read_skill"))]
impl SkillTool {
    async fn execute(&self, _tool_name: &str, arguments: Value, ctx: &InferenceContext) -> Result<ToolOutput, AppError> {
        let name = arguments
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("Missing 'name' parameter".into()))?;

        match self.skill_service.resolve(&ctx.agent.id, &ctx.agent.skills, name).await {
            Some(skill) => Ok(ToolOutput::text(skill.content)),
            None => Ok(ToolOutput::text(format!(
                "Skill '{name}' not found. Check the available skills in your system prompt."
            ))),
        }
    }
}
