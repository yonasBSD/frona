use chrono::{DateTime, Utc};
use crate::Entity;
use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "lowercase")]
#[surreal(crate = "surrealdb::types", lowercase)]
pub enum RoutineStatus {
    Idle,
    Running,
}

impl Default for RoutineStatus {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub struct RoutineItem {
    pub id: String,
    pub description: String,
    pub added_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue, Entity)]
#[surreal(crate = "surrealdb::types")]
#[entity(table = "routine")]
pub struct Routine {
    pub id: String,
    pub user_id: String,
    pub agent_id: String,
    #[serde(default)]
    pub items: Vec<RoutineItem>,
    pub interval_mins: Option<u64>,
    pub chat_id: Option<String>,
    #[serde(default)]
    pub status: RoutineStatus,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routine_status_serialization() {
        for status in [RoutineStatus::Idle, RoutineStatus::Running] {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: RoutineStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, status);
        }
    }

    #[test]
    fn test_routine_item_serialization() {
        let item = RoutineItem {
            id: "item-1".to_string(),
            description: "Check emails".to_string(),
            added_at: Utc::now(),
        };
        let json = serde_json::to_string(&item).unwrap();
        let deserialized: RoutineItem = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "item-1");
        assert_eq!(deserialized.description, "Check emails");
    }
}
