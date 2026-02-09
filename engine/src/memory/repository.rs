use async_trait::async_trait;

use crate::core::error::AppError;
use crate::core::repository::Repository;

use super::models::{Memory, MemorySourceType};

#[async_trait]
pub trait MemoryRepository: Repository<Memory> {
    async fn find_latest(
        &self,
        source_type: MemorySourceType,
        source_id: &str,
    ) -> Result<Option<Memory>, AppError>;
}
