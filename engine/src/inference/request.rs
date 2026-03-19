use std::sync::Arc;

use rig::completion::Message as RigMessage;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::agent::models::Agent;
use crate::agent::task::models::Task;
use crate::chat::broadcast::EventSender;
use crate::chat::models::Chat;
use crate::auth::User;
use crate::tool::registry::AgentToolRegistry;

use super::config::ModelGroup;
use super::registry::ModelProviderRegistry;
use crate::chat::message::models::MessageTool;

use crate::chat::message::models::Reasoning;

use super::tool_loop::ToolCallResult;

pub struct InferenceContext {
    pub user: User,
    pub agent: Agent,
    pub chat: Chat,
    pub task: Option<Task>,
    pub event_tx: EventSender,
    pub vault_env_vars: Arc<RwLock<Vec<(String, String)>>>,
}

impl InferenceContext {
    pub fn new(
        user: User,
        agent: Agent,
        chat: Chat,
        event_tx: EventSender,
    ) -> Self {
        Self {
            user,
            agent,
            chat,
            task: None,
            event_tx,
            vault_env_vars: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn with_task(mut self, task: Task) -> Self {
        self.task = Some(task);
        self
    }
}

pub struct InferenceRequest {
    pub registry: ModelProviderRegistry,
    pub model_group: ModelGroup,
    pub system_prompt: String,
    pub history: Vec<RigMessage>,
    pub tool_registry: AgentToolRegistry,
    pub ctx: InferenceContext,
    pub cancel_token: CancellationToken,
}

#[derive(Debug)]
pub enum InferenceResponse {
    Completed {
        text: String,
        attachments: Vec<crate::storage::Attachment>,
        lifecycle_event: Option<MessageTool>,
        reasoning: Option<Reasoning>,
    },
    Cancelled(String),
    ExternalToolPending {
        accumulated_text: String,
        tool_calls_json: serde_json::Value,
        tool_results: Vec<ToolCallResult>,
        external_tool: Box<ToolCallResult>,
        system_prompt: Option<String>,
    },
}
