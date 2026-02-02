use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crate::error::AppError;
use crate::repository::Repository;

use super::models::Routine;

#[async_trait]
pub trait RoutineRepository: Repository<Routine> {
    async fn find_by_agent_id(&self, user_id: &str, agent_id: &str) -> Result<Option<Routine>, AppError>;
    async fn find_due_idle(&self, now: DateTime<Utc>) -> Result<Vec<Routine>, AppError>;
}
