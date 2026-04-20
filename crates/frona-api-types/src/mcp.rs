use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeServerInfo {
    pub slug: String,
    pub display_name: String,
    pub description: Option<String>,
    pub tool_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeServerDetail {
    pub slug: String,
    pub display_name: String,
    pub description: Option<String>,
    pub tools: Vec<BridgeToolInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeCallRequest {
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeCallResponse {
    pub content: String,
    pub is_error: bool,
}
