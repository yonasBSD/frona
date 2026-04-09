use async_trait::async_trait;

use crate::core::error::AppError;
use crate::tool::mcp::models::{McpServer, McpServerStatus};
use crate::tool::mcp::repository::McpServerRepository;

use super::generic::SurrealRepo;

pub type SurrealMcpServerRepo = SurrealRepo<McpServer>;

const SELECT_CLAUSE: &str = "SELECT *, meta::id(id) as id";

#[async_trait]
impl McpServerRepository for SurrealRepo<McpServer> {
    async fn find_by_user(&self, user_id: &str) -> Result<Vec<McpServer>, AppError> {
        let query = format!(
            "{SELECT_CLAUSE} FROM mcp_server WHERE user_id = $user_id ORDER BY installed_at DESC"
        );
        let mut result = self
            .db()
            .query(&query)
            .bind(("user_id", user_id.to_string()))
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        result
            .take(0)
            .map_err(|e| AppError::Database(e.to_string()))
    }

    async fn find_running(&self) -> Result<Vec<McpServer>, AppError> {
        let query = format!("{SELECT_CLAUSE} FROM mcp_server WHERE status = $status");
        let mut result = self
            .db()
            .query(&query)
            .bind(("status", McpServerStatus::Running))
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        result
            .take(0)
            .map_err(|e| AppError::Database(e.to_string()))
    }
}
