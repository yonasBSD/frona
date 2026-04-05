use chrono::{DateTime, Utc};
use crate::Entity;
use crate::storage::Attachment;
use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub struct Reasoning {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "lowercase")]
#[surreal(crate = "surrealdb::types", lowercase)]
pub enum MessageStatus {
    Executing,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "lowercase")]
#[surreal(crate = "surrealdb::types", lowercase)]
pub enum MessageRole {
    User,
    Agent,
    TaskCompletion,
    Contact,
    LiveCall,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
#[serde(tag = "type", content = "data")]
#[surreal(crate = "surrealdb::types", tag = "type", content = "data")]
pub enum MessageEvent {
    TaskCompletion {
        task_id: String,
        chat_id: Option<String>,
        status: crate::agent::task::models::TaskStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    TaskDeferred {
        task_id: String,
        delay_minutes: u32,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue, Entity)]
#[surreal(crate = "surrealdb::types")]
#[entity(table = "message")]
pub struct Message {
    pub id: String,
    pub chat_id: String,
    pub role: MessageRole,
    pub content: String,
    pub agent_id: Option<String>,
    pub event: Option<MessageEvent>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<MessageStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,
    pub created_at: DateTime<Utc>,
}

impl Message {
    pub fn builder(chat_id: &str, role: MessageRole, content: String) -> MessageBuilder {
        MessageBuilder {
            chat_id: chat_id.to_string(),
            role,
            content,
            agent_id: None,
            event: None,
            attachments: vec![],
            contact_id: None,
            status: None,
            reasoning: None,
        }
    }
}

pub struct MessageBuilder {
    chat_id: String,
    role: MessageRole,
    content: String,
    agent_id: Option<String>,
    event: Option<MessageEvent>,
    attachments: Vec<Attachment>,
    contact_id: Option<String>,
    status: Option<MessageStatus>,
    reasoning: Option<Reasoning>,
}

impl MessageBuilder {
    pub fn agent_id(mut self, id: String) -> Self {
        self.agent_id = Some(id);
        self
    }

    pub fn event(mut self, e: MessageEvent) -> Self {
        self.event = Some(e);
        self
    }

    pub fn attachments(mut self, a: Vec<Attachment>) -> Self {
        self.attachments = a;
        self
    }

    pub fn contact_id(mut self, id: impl Into<String>) -> Self {
        self.contact_id = Some(id.into());
        self
    }

    pub fn status(mut self, s: MessageStatus) -> Self {
        self.status = Some(s);
        self
    }

    pub fn reasoning(mut self, r: Reasoning) -> Self {
        self.reasoning = Some(r);
        self
    }

    pub fn build(self) -> Message {
        Message {
            id: uuid::Uuid::new_v4().to_string(),
            chat_id: self.chat_id,
            role: self.role,
            content: self.content,
            agent_id: self.agent_id,
            event: self.event,
            attachments: self.attachments,
            contact_id: self.contact_id,
            status: self.status,
            reasoning: self.reasoning,
            created_at: chrono::Utc::now(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct MessageQuery {
    pub before: Option<DateTime<Utc>>,
    pub after: Option<DateTime<Utc>>,
    #[serde(default = "default_message_limit")]
    pub limit: u32,
}

fn default_message_limit() -> u32 {
    50
}

#[derive(Debug, Clone, Serialize)]
pub struct PaginatedMessagesResponse {
    pub messages: Vec<MessageResponse>,
    pub has_more: bool,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ToolResolutionAction {
    #[default]
    Success,
    Fail,
}

#[derive(Debug, Deserialize)]
pub struct ToolResolution {
    pub tool_call_id: String,
    pub response: Option<String>,
    #[serde(default)]
    pub action: ToolResolutionAction,
}

#[derive(Debug, Deserialize)]
pub struct ResolveToolRequest {
    pub resolutions: Vec<ToolResolution>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageResponse {
    pub id: String,
    pub chat_id: String,
    pub role: MessageRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<MessageEvent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<MessageStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<crate::inference::tool_call::ToolCallResponse>,
    pub created_at: DateTime<Utc>,
}

impl From<Message> for MessageResponse {
    fn from(msg: Message) -> Self {
        Self {
            id: msg.id,
            chat_id: msg.chat_id,
            role: msg.role,
            content: msg.content,
            agent_id: msg.agent_id,
            event: msg.event,
            attachments: msg.attachments,
            contact_id: msg.contact_id,
            status: msg.status,
            reasoning: msg.reasoning.map(|r| r.content),
            tool_calls: vec![],
            created_at: msg.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasoning_serialization_round_trip() {
        let reasoning = Reasoning {
            id: Some("r-1".to_string()),
            content: "thinking about the problem".to_string(),
            signature: Some("sig-abc".to_string()),
        };

        let json = serde_json::to_string(&reasoning).unwrap();
        let deserialized: Reasoning = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, Some("r-1".to_string()));
        assert_eq!(deserialized.content, "thinking about the problem");
        assert_eq!(deserialized.signature, Some("sig-abc".to_string()));
    }

    #[test]
    fn reasoning_skip_serializing_none_fields() {
        let reasoning = Reasoning {
            id: None,
            content: "just text".to_string(),
            signature: None,
        };

        let json = serde_json::to_string(&reasoning).unwrap();
        assert!(!json.contains("\"id\""));
        assert!(!json.contains("\"signature\""));
        assert!(json.contains("\"content\""));
    }

    #[test]
    fn message_with_reasoning_serialization() {
        let msg = Message::builder("chat-1", MessageRole::Agent, "answer".to_string())
            .reasoning(Reasoning {
                id: Some("r-1".to_string()),
                content: "I need to think".to_string(),
                signature: None,
            })
            .build();

        let json = serde_json::to_value(&msg).unwrap();
        let reasoning = json.get("reasoning").unwrap();
        assert_eq!(reasoning["content"], "I need to think");
        assert_eq!(reasoning["id"], "r-1");
    }

    #[test]
    fn message_without_reasoning_omits_field() {
        let msg = Message::builder("chat-1", MessageRole::Agent, "answer".to_string())
            .build();

        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("\"reasoning\""));
    }

    #[test]
    fn message_response_maps_reasoning_content() {
        let msg = Message::builder("chat-1", MessageRole::Agent, "answer".to_string())
            .reasoning(Reasoning {
                id: Some("r-1".to_string()),
                content: "deep thinking".to_string(),
                signature: Some("sig".to_string()),
            })
            .build();

        let response: MessageResponse = msg.into();
        assert_eq!(response.reasoning, Some("deep thinking".to_string()));
    }

    #[test]
    fn message_response_none_reasoning_when_absent() {
        let msg = Message::builder("chat-1", MessageRole::Agent, "answer".to_string())
            .build();

        let response: MessageResponse = msg.into();
        assert!(response.reasoning.is_none());
    }

    #[test]
    fn message_deserialize_without_reasoning_field() {
        let json = serde_json::json!({
            "id": "m-1",
            "chat_id": "c-1",
            "role": "agent",
            "content": "hello",
            "attachments": [],
            "created_at": "2025-01-01T00:00:00Z"
        });

        let msg: Message = serde_json::from_value(json).unwrap();
        assert!(msg.reasoning.is_none());
    }
}
