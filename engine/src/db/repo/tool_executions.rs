use async_trait::async_trait;
use crate::core::error::AppError;
use crate::core::repository::Repository;
use crate::inference::tool_execution::ToolExecution;

use super::generic::SurrealRepo;

pub type SurrealToolExecutionRepo = SurrealRepo<ToolExecution>;

const SELECT_CLAUSE: &str = "SELECT *, meta::id(id) as id";

#[async_trait]
pub trait ToolExecutionRepository: Repository<ToolExecution> {
    async fn find_by_chat_id(&self, chat_id: &str) -> Result<Vec<ToolExecution>, AppError>;
    async fn find_by_message_id(&self, message_id: &str) -> Result<Vec<ToolExecution>, AppError>;
    async fn find_pending_by_chat_id(&self, chat_id: &str) -> Result<Option<ToolExecution>, AppError>;
}

#[async_trait]
impl ToolExecutionRepository for SurrealRepo<ToolExecution> {
    async fn find_by_chat_id(&self, chat_id: &str) -> Result<Vec<ToolExecution>, AppError> {
        let query = format!(
            "{SELECT_CLAUSE} FROM tool_execution WHERE chat_id = $chat_id ORDER BY created_at ASC"
        );
        let mut result = self
            .db()
            .query(&query)
            .bind(("chat_id", chat_id.to_string()))
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        let executions: Vec<ToolExecution> = result
            .take(0)
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(executions)
    }

    async fn find_by_message_id(&self, message_id: &str) -> Result<Vec<ToolExecution>, AppError> {
        let query = format!(
            "{SELECT_CLAUSE} FROM tool_execution WHERE message_id = $message_id ORDER BY created_at ASC"
        );
        let mut result = self
            .db()
            .query(&query)
            .bind(("message_id", message_id.to_string()))
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        let executions: Vec<ToolExecution> = result
            .take(0)
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(executions)
    }

    async fn find_pending_by_chat_id(&self, chat_id: &str) -> Result<Option<ToolExecution>, AppError> {
        let all = self.find_by_chat_id(chat_id).await?;
        Ok(all
            .into_iter()
            .rev()
            .find(|te| {
                te.tool_data
                    .as_ref()
                    .and_then(|t| t.tool_status())
                    .is_some_and(|s| matches!(s, crate::chat::message::models::ToolStatus::Pending))
            }))
    }
}
