use std::sync::Arc;

use serde_json::Value;

use crate::agent::prompt::PromptLoader;
use crate::core::error::AppError;
use frona_derive::agent_tool;

use super::{InferenceContext, ToolOutput};
use super::browser::session::BrowserSessionManager;

pub struct WebFetchTool {
    session_manager: Arc<BrowserSessionManager>,
    prompts: PromptLoader,
}

impl WebFetchTool {
    pub fn new(session_manager: Arc<BrowserSessionManager>, prompts: PromptLoader) -> Self {
        Self {
            session_manager,
            prompts,
        }
    }

    fn provider(&self) -> &str {
        "default"
    }
}

#[agent_tool]
impl WebFetchTool {
    async fn execute(&self, _tool_name: &str, arguments: Value, ctx: &InferenceContext) -> Result<ToolOutput, AppError> {
        let url = arguments
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("Missing required parameter: url".into()))?;

        let session_key = &ctx.user.username;

        let navigate_params = serde_json::json!({
            "url": url,
            "wait_for_load": true,
        });

        self.session_manager
            .execute_tool(session_key, self.provider(), "navigate", navigate_params)
            .await?;

        let markdown = self
            .session_manager
            .execute_tool(session_key, self.provider(), "get_markdown", serde_json::json!({}))
            .await?;

        Ok(ToolOutput::text(markdown))
    }

    async fn cleanup(&self) -> Result<(), AppError> {
        Ok(())
    }
}
