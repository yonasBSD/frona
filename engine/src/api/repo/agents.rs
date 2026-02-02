use async_trait::async_trait;
use crate::agent::models::Agent;
use crate::agent::repository::AgentRepository;
use crate::error::AppError;

use super::generic::SurrealRepo;

pub type SurrealAgentRepo = SurrealRepo<Agent>;

const SELECT_CLAUSE: &str = "SELECT *, meta::id(id) as id";

#[async_trait]
impl AgentRepository for SurrealRepo<Agent> {
    async fn find_by_user_id(&self, user_id: &str) -> Result<Vec<Agent>, AppError> {
        let query = format!("{SELECT_CLAUSE} FROM agent WHERE user_id = $user_id OR user_id IS NONE ORDER BY created_at DESC");
        let mut result = self
            .db()
            .query(&query)
            .bind(("user_id", user_id.to_string()))
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        let agents: Vec<Agent> = result
            .take(0)
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(agents)
    }

    async fn find_by_name(&self, user_id: &str, name: &str) -> Result<Option<Agent>, AppError> {
        let query = format!(
            "{SELECT_CLAUSE} FROM agent WHERE (user_id = $user_id OR user_id IS NONE) AND string::lowercase(name) = string::lowercase($name) LIMIT 1"
        );
        let mut result = self
            .db()
            .query(&query)
            .bind(("user_id", user_id.to_string()))
            .bind(("name", name.to_string()))
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        let agent: Option<Agent> = result
            .take(0)
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(agent)
    }
}
