use std::path::PathBuf;

use serde_json::Value;

use crate::agent::prompt::PromptLoader;
use crate::storage::detect_content_type;
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

        if relative_path.contains("..") {
            return Err(AppError::Validation(
                "Path traversal not allowed".into(),
            ));
        }

        let workspace_path = self.workspaces_path.join(&ctx.agent.id);
        let resolved = workspace_path.join(relative_path);

        if !resolved.exists() {
            return Err(AppError::NotFound(format!(
                "File not found in workspace: {relative_path}"
            )));
        }

        let metadata = tokio::fs::metadata(&resolved)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to read file metadata: {e}")))?;

        let filename = resolved
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(relative_path)
            .to_string();

        let content_type = detect_content_type(&filename).to_string();

        let attachment = crate::storage::Attachment {
            filename: filename.clone(),
            content_type: content_type.clone(),
            size_bytes: metadata.len(),
            owner: format!("agent:{}", ctx.agent.id),
            path: relative_path.to_string(),
            url: None,
        };

        Ok(ToolOutput::text(serde_json::to_string(&attachment).unwrap_or_default())
            .with_attachment(attachment))
    }
}
