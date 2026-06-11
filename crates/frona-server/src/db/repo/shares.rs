use async_trait::async_trait;
use chrono::Utc;

use crate::core::error::AppError;
use crate::credential::share::models::Share;
use crate::credential::share::repository::ShareRepository;

use super::generic::SurrealRepo;

pub type SurrealShareRepo = SurrealRepo<Share>;

const SELECT_CLAUSE: &str = "SELECT *, meta::id(id) as id";

#[async_trait]
impl ShareRepository for SurrealRepo<Share> {
    async fn find_active_by_id(&self, id: &str) -> Result<Option<Share>, AppError> {
        let query = format!(
            "{SELECT_CLAUSE} FROM share WHERE id = $id AND expires_at > $now LIMIT 1"
        );
        let thing = surrealdb::types::RecordId::new("share", id);
        let mut result = self
            .db()
            .query(&query)
            .bind(("id", thing))
            .bind(("now", Utc::now()))
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        let row: Option<Share> = result
            .take(0)
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(row)
    }

    async fn delete_expired(&self) -> Result<u64, AppError> {
        let mut result = self
            .db()
            .query("DELETE FROM share WHERE expires_at <= $now RETURN BEFORE")
            .bind(("now", Utc::now()))
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        let deleted: Vec<surrealdb::types::Value> = result
            .take(0)
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(deleted.len() as u64)
    }
}
