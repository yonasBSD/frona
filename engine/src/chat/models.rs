use chrono::{DateTime, Utc};
use crate::Entity;
use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue, Entity)]
#[surreal(crate = "surrealdb::types")]
#[entity(table = "chat")]
pub struct Chat {
    pub id: String,
    pub user_id: String,
    pub space_id: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    pub agent_id: String,
    pub title: Option<String>,
    #[serde(default)]
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_chat_without_task_id() {
        let json = serde_json::json!({
            "id": "c1",
            "user_id": "u1",
            "space_id": null,
            "agent_id": "system",
            "title": null,
            "created_at": "2025-01-01T00:00:00Z",
            "updated_at": "2025-01-01T00:00:00Z"
        });
        let chat: Chat = serde_json::from_value(json).unwrap();
        assert!(chat.task_id.is_none());
        assert!(chat.archived_at.is_none());
    }

    #[test]
    fn deserialize_chat_with_task_id() {
        let json = serde_json::json!({
            "id": "c1",
            "user_id": "u1",
            "space_id": null,
            "task_id": "t1",
            "agent_id": "system",
            "title": null,
            "created_at": "2025-01-01T00:00:00Z",
            "updated_at": "2025-01-01T00:00:00Z"
        });
        let chat: Chat = serde_json::from_value(json).unwrap();
        assert_eq!(chat.task_id.as_deref(), Some("t1"));
        assert!(chat.archived_at.is_none());
    }

    #[test]
    fn deserialize_chat_with_archived_at() {
        let json = serde_json::json!({
            "id": "c1",
            "user_id": "u1",
            "space_id": null,
            "agent_id": "system",
            "title": null,
            "archived_at": "2025-06-15T12:00:00Z",
            "created_at": "2025-01-01T00:00:00Z",
            "updated_at": "2025-01-01T00:00:00Z"
        });
        let chat: Chat = serde_json::from_value(json).unwrap();
        assert!(chat.archived_at.is_some());
        assert_eq!(
            chat.archived_at.unwrap().to_rfc3339(),
            "2025-06-15T12:00:00+00:00"
        );
    }

    #[test]
    fn deserialize_chat_without_archived_at_defaults_none() {
        let json = serde_json::json!({
            "id": "c1",
            "user_id": "u1",
            "space_id": null,
            "agent_id": "system",
            "title": null,
            "created_at": "2025-01-01T00:00:00Z",
            "updated_at": "2025-01-01T00:00:00Z"
        });
        let chat: Chat = serde_json::from_value(json).unwrap();
        assert!(chat.archived_at.is_none());
    }
}
