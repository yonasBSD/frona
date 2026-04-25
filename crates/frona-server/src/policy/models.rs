use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;

use crate::Entity;

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue, Entity)]
#[surreal(crate = "surrealdb::types")]
#[entity(table = "policy")]
pub struct Policy {
    pub id: String,
    pub user_id: Option<String>,
    pub name: String,
    pub description: String,
    pub policy_text: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolStatus {
    pub id: String,
    pub group: String,
    pub enabled: bool,
    pub editable: bool,
}

#[derive(Debug, Clone)]
pub enum PolicyAction {
    InvokeTool { tool_name: String, tool_group: String },
    DelegateTask { target_agent_id: String },
    SendMessage { target_agent_id: String },
}

impl PolicyAction {
    pub fn cedar_action_name(&self) -> &'static str {
        match self {
            PolicyAction::InvokeTool { .. } => "invoke_tool",
            PolicyAction::DelegateTask { .. } => "delegate_task",
            PolicyAction::SendMessage { .. } => "send_message",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuthorizationDecision {
    pub allowed: bool,
    pub diagnostics: String,
}

impl AuthorizationDecision {
    pub fn allow() -> Self {
        Self {
            allowed: true,
            diagnostics: String::new(),
        }
    }

    pub fn deny(diagnostics: String) -> Self {
        Self {
            allowed: false,
            diagnostics,
        }
    }

    pub fn is_denied(&self) -> bool {
        !self.allowed
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreatePolicyRequest {
    pub policy_text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePolicyRequest {
    pub policy_text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PolicyResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub policy_text: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Policy> for PolicyResponse {
    fn from(p: Policy) -> Self {
        Self {
            id: p.id,
            name: p.name,
            description: p.description,
            policy_text: p.policy_text,
            enabled: p.enabled,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

#[derive(Debug, Clone)]
pub enum PolicyResource {
    Tool { id: String, group: String },
    ToolGroup { group: String },
}

impl PolicyResource {
    pub fn label(&self) -> &str {
        match self {
            PolicyResource::Tool { id, .. } => id,
            PolicyResource::ToolGroup { group } => group,
        }
    }
}

