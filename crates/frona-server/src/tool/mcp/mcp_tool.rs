use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::core::error::AppError;
use crate::tool::{AgentTool, InferenceContext, ToolDefinition, ToolOutput};

use super::manager::McpManager;

pub struct McpTool {
    manager: Arc<McpManager>,
    cached_definitions: Vec<ToolDefinition>,
}

impl McpTool {
    pub fn new(manager: Arc<McpManager>, definitions: Vec<ToolDefinition>) -> Self {
        Self {
            manager,
            cached_definitions: definitions,
        }
    }
}

#[async_trait]
impl AgentTool for McpTool {
    fn name(&self) -> &str {
        "mcp"
    }

    fn definitions(&self) -> Vec<ToolDefinition> {
        self.cached_definitions.clone()
    }

    async fn execute(
        &self,
        tool_name: &str,
        arguments: Value,
        _ctx: &InferenceContext,
    ) -> Result<ToolOutput, AppError> {
        let server_id = self
            .manager
            .server_for_tool(tool_name)
            .await
            .ok_or_else(|| {
                AppError::Tool(format!(
                    "no running MCP server exposes tool {tool_name}"
                ))
            })?;

        let bare_name = tool_name
            .split("__")
            .nth(2)
            .unwrap_or(tool_name);

        let result = self.manager.call(&server_id, bare_name, arguments).await?;

        let is_error = result.is_error.unwrap_or(false);
        let text = result
            .content
            .iter()
            .filter_map(|c| match &c.raw {
                rmcp::model::RawContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        if is_error {
            Ok(ToolOutput::error(text))
        } else {
            Ok(ToolOutput::text(text))
        }
    }
}
