use std::path::PathBuf;

use serde_json::Value;

use crate::agent::prompt::PromptLoader;
use crate::storage::resolve_workspace_attachment;
use crate::core::error::AppError;
use frona_derive::agent_tool;

use super::{InferenceContext, ToolOutput};

pub struct ProduceFileTool {
    workspaces_path: PathBuf,
    prompts: PromptLoader,
}

impl ProduceFileTool {
    pub fn new(workspaces_path: PathBuf, prompts: PromptLoader) -> Self {
        Self {
            workspaces_path,
            prompts,
        }
    }
}

#[agent_tool]
impl ProduceFileTool {
    async fn execute(&self, _tool_name: &str, arguments: Value, ctx: &InferenceContext) -> Result<ToolOutput, AppError> {
        let relative_path = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("Missing 'path' parameter".into()))?;

        let attachment = resolve_workspace_attachment(
            &self.workspaces_path,
            &ctx.agent.id,
            relative_path,
        ).await?;

        Ok(ToolOutput::text(serde_json::to_string(&attachment).unwrap_or_default())
            .with_attachment(attachment))
    }
}
