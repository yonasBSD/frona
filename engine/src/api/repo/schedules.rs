use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crate::error::AppError;
use crate::schedule::models::Routine;
use crate::schedule::repository::RoutineRepository;

use super::generic::SurrealRepo;

pub type SurrealRoutineRepo = SurrealRepo<Routine>;

const SELECT_CLAUSE: &str = "SELECT *, meta::id(id) as id";

#[async_trait]
impl RoutineRepository for SurrealRepo<Routine> {
    async fn find_by_agent_id(&self, user_id: &str, agent_id: &str) -> Result<Option<Routine>, AppError> {
        let query = format!(
            "{SELECT_CLAUSE} FROM routine WHERE user_id = $user_id AND agent_id = $agent_id LIMIT 1"
        );
        let mut result = self
            .db()
            .query(&query)
            .bind(("user_id", user_id.to_string()))
            .bind(("agent_id", agent_id.to_string()))
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        let routine: Option<Routine> = result
            .take(0)
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(routine)
    }

    async fn find_due_idle(&self, now: DateTime<Utc>) -> Result<Vec<Routine>, AppError> {
        let query = format!(
            "{SELECT_CLAUSE} FROM routine WHERE status.Idle IS NOT NONE AND interval_mins IS NOT NONE AND next_run_at IS NOT NONE AND next_run_at <= $now ORDER BY next_run_at ASC"
        );
        let mut result = self
            .db()
            .query(&query)
            .bind(("now", now))
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        let routines: Vec<Routine> = result
            .take(0)
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(routines)
    }
}
