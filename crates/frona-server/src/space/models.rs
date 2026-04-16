use chrono::{DateTime, Utc};
use crate::Entity;
use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue, Entity)]
#[surreal(crate = "surrealdb::types")]
#[entity(table = "space")]
pub struct Space {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSpaceRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSpaceRequest {
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SpaceResponse {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Space> for SpaceResponse {
    fn from(space: Space) -> Self {
        Self {
            id: space.id,
            name: space.name,
            created_at: space.created_at,
            updated_at: space.updated_at,
        }
    }
}
