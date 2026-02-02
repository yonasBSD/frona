use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct CreateChatRequest {
    pub space_id: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    pub agent_id: String,
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateChatRequest {
    pub title: Option<String>,
    pub space_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub id: String,
    pub space_id: Option<String>,
    pub task_id: Option<String>,
    pub agent_id: String,
    pub title: Option<String>,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<super::models::Chat> for ChatResponse {
    fn from(chat: super::models::Chat) -> Self {
        Self {
            id: chat.id,
            space_id: chat.space_id,
            task_id: chat.task_id,
            agent_id: chat.agent_id,
            title: chat.title,
            archived_at: chat.archived_at,
            created_at: chat.created_at,
            updated_at: chat.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::models::Chat;

    fn make_chat(archived_at: Option<DateTime<Utc>>) -> Chat {
        let now = chrono::Utc::now();
        Chat {
            id: "c1".to_string(),
            user_id: "u1".to_string(),
            space_id: Some("s1".to_string()),
            task_id: Some("t1".to_string()),
            agent_id: "system".to_string(),
            title: Some("Test".to_string()),
            archived_at,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn chat_response_from_chat_maps_archived_at_none() {
        let chat = make_chat(None);
        let resp = ChatResponse::from(chat);
        assert!(resp.archived_at.is_none());
    }

    #[test]
    fn chat_response_from_chat_maps_archived_at_some() {
        let ts = chrono::Utc::now();
        let chat = make_chat(Some(ts));
        let resp = ChatResponse::from(chat);
        assert_eq!(resp.archived_at, Some(ts));
    }

    #[test]
    fn chat_response_from_chat_maps_all_fields() {
        let chat = make_chat(None);
        let resp = ChatResponse::from(chat.clone());
        assert_eq!(resp.id, chat.id);
        assert_eq!(resp.space_id, chat.space_id);
        assert_eq!(resp.task_id, chat.task_id);
        assert_eq!(resp.agent_id, chat.agent_id);
        assert_eq!(resp.title, chat.title);
        assert_eq!(resp.created_at, chat.created_at);
        assert_eq!(resp.updated_at, chat.updated_at);
    }
}
