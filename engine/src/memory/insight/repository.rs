use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::core::error::AppError;
use crate::core::repository::Repository;

use super::models::Insight;

#[async_trait]
pub trait InsightRepository: Repository<Insight> {
    async fn find_by_agent_id(&self, agent_id: &str) -> Result<Vec<Insight>, AppError>;
    async fn find_by_agent_id_after(
        &self,
        agent_id: &str,
        after: DateTime<Utc>,
    ) -> Result<Vec<Insight>, AppError>;
    async fn delete_by_agent_id_before(
        &self,
        agent_id: &str,
        before: DateTime<Utc>,
    ) -> Result<(), AppError>;
    async fn find_distinct_agent_ids(&self) -> Result<Vec<String>, AppError>;
    async fn find_by_user_id(&self, user_id: &str) -> Result<Vec<Insight>, AppError>;
    async fn find_by_user_id_after(
        &self,
        user_id: &str,
        after: DateTime<Utc>,
    ) -> Result<Vec<Insight>, AppError>;
    async fn delete_by_user_id_before(
        &self,
        user_id: &str,
        before: DateTime<Utc>,
    ) -> Result<(), AppError>;
    async fn find_distinct_user_ids(&self) -> Result<Vec<String>, AppError>;
}
