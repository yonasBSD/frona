use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::files::Attachment;
use super::models::{MessageRole, MessageTool};

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Deserialize)]
pub struct ResolveToolRequest {
    pub response: Option<String>,
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
    pub tool_calls: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<MessageTool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    pub created_at: DateTime<Utc>,
}

impl From<super::models::Message> for MessageResponse {
    fn from(msg: super::models::Message) -> Self {
        Self {
            id: msg.id,
            chat_id: msg.chat_id,
            role: msg.role,
            content: msg.content,
            agent_id: msg.agent_id,
            tool_calls: msg.tool_calls,
            tool_call_id: msg.tool_call_id,
            tool: msg.tool,
            attachments: msg.attachments,
            created_at: msg.created_at,
        }
    }
}
