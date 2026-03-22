use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;

use crate::Entity;
use crate::chat::message::models::MessageTool;

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue, Entity)]
#[surreal(crate = "surrealdb::types")]
#[entity(table = "tool_execution")]
pub struct ToolExecution {
    pub id: String,
    pub chat_id: String,
    pub message_id: String,
    pub turn: u32,
    pub tool_call_id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    pub result: String,
    pub success: bool,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_data: Option<MessageTool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_text: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionResponse {
    pub id: String,
    pub chat_id: String,
    pub message_id: String,
    pub turn: u32,
    pub tool_call_id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    pub result: String,
    pub success: bool,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_data: Option<MessageTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_text: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl From<ToolExecution> for ToolExecutionResponse {
    fn from(te: ToolExecution) -> Self {
        Self {
            id: te.id,
            chat_id: te.chat_id,
            message_id: te.message_id,
            turn: te.turn,
            tool_call_id: te.tool_call_id,
            name: te.name,
            arguments: te.arguments,
            result: te.result,
            success: te.success,
            duration_ms: te.duration_ms,
            tool_data: te.tool_data,
            system_prompt: te.system_prompt,
            turn_text: te.turn_text,
            created_at: te.created_at,
        }
    }
}
