use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;

use frona_derive::Entity;

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue, Entity)]
#[surreal(crate = "surrealdb::types")]
#[entity(table = "insight")]
pub struct Insight {
    pub id: String,
    pub agent_id: String,
    pub user_id: Option<String>,
    pub content: String,
    pub source_chat_id: Option<String>,
    pub created_at: DateTime<Utc>,
}
