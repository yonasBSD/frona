use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use crate::Entity;
use serde::{Deserialize, Serialize};
use serde_aux::field_attributes::deserialize_bool_from_anything;
use surrealdb::types::SurrealValue;


#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub struct SandboxSettings {
    #[serde(default = "serde_aux::field_attributes::bool_true", deserialize_with = "deserialize_bool_from_anything")]
    pub network_access: bool,
    #[serde(default)]
    pub allowed_network_destinations: Vec<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub max_cpu_pct: Option<f64>,
    #[serde(default)]
    pub max_memory_pct: Option<f64>,
    #[serde(default)]
    pub shared_paths: Vec<String>,
}

impl Default for SandboxSettings {
    fn default() -> Self {
        Self {
            network_access: true,
            allowed_network_destinations: Vec::new(),
            timeout_secs: None,
            max_cpu_pct: None,
            max_memory_pct: None,
            shared_paths: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue, Entity)]
#[surreal(crate = "surrealdb::types")]
#[entity(table = "agent")]
pub struct Agent {
    pub id: String,
    #[serde(default)]
    pub user_id: Option<String>,
    pub name: String,
    pub description: String,
    pub model_group: String,
    pub enabled: bool,
    #[serde(default)]
    pub sandbox_config: Option<SandboxSettings>,
    #[serde(default)]
    pub max_concurrent_tasks: Option<u32>,
    #[serde(default)]
    pub skills: Option<Vec<String>>,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub identity: BTreeMap<String, String>,
    #[serde(default)]
    pub prompt: Option<String>,
    pub heartbeat_interval: Option<u64>,
    pub next_heartbeat_at: Option<DateTime<Utc>>,
    pub heartbeat_chat_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    pub model_group: Option<String>,
    pub tools: Option<Vec<String>>,
    pub skills: Option<Vec<String>>,
    pub sandbox_config: Option<SandboxSettings>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub model_group: Option<String>,
    pub enabled: Option<bool>,
    pub tools: Option<Vec<String>>,
    pub skills: Option<Vec<String>>,
    pub sandbox_config: Option<SandboxSettings>,
    pub prompt: Option<String>,
    pub identity: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Serialize)]
pub struct AgentResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub model_group: String,
    pub enabled: bool,
    pub tools: Vec<String>,
    pub skills: Option<Vec<String>>,
    pub sandbox_config: Option<SandboxSettings>,
    pub avatar: Option<String>,
    pub identity: BTreeMap<String, String>,
    pub prompt: Option<String>,
    pub default_prompt: String,
    pub is_shared: bool,
    pub chat_count: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AgentResponse {
    pub fn from_agent(agent: Agent, tools: Vec<String>) -> Self {
        let is_shared = agent.user_id.is_none();
        Self {
            id: agent.id,
            name: agent.name,
            description: agent.description,
            model_group: agent.model_group,
            enabled: agent.enabled,
            tools,
            skills: agent.skills,
            sandbox_config: agent.sandbox_config,
            avatar: agent.avatar,
            identity: agent.identity,
            prompt: agent.prompt,
            default_prompt: String::new(),
            is_shared,
            chat_count: 0,
            created_at: agent.created_at,
            updated_at: agent.updated_at,
        }
    }
}
