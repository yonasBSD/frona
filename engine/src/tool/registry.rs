use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::core::error::AppError;
use crate::core::state::AppState;

use super::{AgentTool, InferenceContext, ToolDefinition, ToolOutput};

pub struct AgentToolRegistry {
    tools: HashMap<String, Arc<dyn AgentTool>>,
    tool_name_to_owner: HashMap<String, String>,
    pub definitions: Vec<ToolDefinition>,
}

impl Default for AgentToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            tool_name_to_owner: HashMap::new(),
            definitions: Vec::new(),
        }
    }

    pub fn register(&mut self, tool: Arc<dyn AgentTool>) {
        let owner_name = tool.name().to_string();
        for mut def in tool.definitions() {
            self.tool_name_to_owner
                .insert(def.id.clone(), owner_name.clone());

            if let Some(props) = def
                .parameters
                .as_object_mut()
                .and_then(|obj| obj.get_mut("properties"))
                .and_then(|p| p.as_object_mut())
            {
                props.insert(
                    "description".to_string(),
                    serde_json::json!({
                        "type": "string",
                        "description": "A short, specific description of what this tool call will accomplish (e.g. 'Checking the API status', 'Searching for weather data'). Shown to the user as a status indicator."
                    }),
                );
            }
            self.definitions.push(def);
        }
        self.tools.insert(owner_name, tool);
    }

    pub async fn execute(&self, tool_name: &str, arguments: Value, ctx: &InferenceContext) -> Result<ToolOutput, AppError> {
        let owner = self
            .tool_name_to_owner
            .get(tool_name)
            .ok_or_else(|| AppError::Tool(format!("Unknown tool: {tool_name}")))?;

        let tool = self
            .tools
            .get(owner)
            .ok_or_else(|| AppError::Tool(format!("Tool owner not found: {owner}")))?;

        tool.execute(tool_name, arguments, ctx).await
    }

    pub async fn cleanup(&self) -> Result<(), AppError> {
        for tool in self.tools.values() {
            tool.cleanup().await?;
        }
        Ok(())
    }

    pub fn owner_of(&self, tool_id: &str) -> Option<&str> {
        self.tool_name_to_owner.get(tool_id).map(|s| s.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

pub fn build_tool_registry(
    state: &AppState,
    agent_id: &str,
    allowed_tools: &[String],
    is_task: bool,
) -> AgentToolRegistry {
    use super::browser::tool::BrowserTool;
    use super::cli::CliTool;
    use super::heartbeat::HeartbeatTool;
    use super::notify_human::NotifyHumanTool;
    use super::produce_file::ProduceFileTool;
    use super::memory::{StoreAgentMemoryTool, StoreUserMemoryTool};
    use super::request_credentials::RequestCredentialsTool;
    use super::task::TaskTool;
    use super::task_control::TaskControlTool;
    use super::update_identity::UpdateIdentityTool;
    use super::web_fetch::WebFetchTool;
    use super::web_search::WebSearchTool;

    let mut registry = AgentToolRegistry::new();

    let prompts = state.prompts.clone();

    registry.register(Arc::new(NotifyHumanTool::new(state.vault_service.clone(), prompts.clone())));

    registry.register(Arc::new(super::send_message::SendMessageTool::new(
        state.chat_service.clone(),
        state.notification_service.clone(),
        state.broadcast_service.clone(),
        state.agent_service.clone(),
        state.task_service.clone(),
        prompts.clone(),
    )));

    let workspaces_path = std::path::PathBuf::from(&state.config.storage.workspaces_path);
    registry.register(Arc::new(ProduceFileTool::new(
        workspaces_path,
        prompts.clone(),
    )));

    registry.register(Arc::new(UpdateIdentityTool::new(
        state.db.clone(),
        prompts.clone(),
    )));

    registry.register(Arc::new(StoreAgentMemoryTool::new(
        state.memory_service.clone(),
        state.compaction_model_group(),
        prompts.clone(),
    )));

    registry.register(Arc::new(StoreUserMemoryTool::new(
        state.memory_service.clone(),
        state.compaction_model_group(),
        prompts.clone(),
    )));

    if allowed_tools.iter().any(|t| t == "browser") {
        registry.register(Arc::new(BrowserTool::new(
            state.browser_session_manager.clone(),
            state.vault_service.clone(),
        )));
    }

    if allowed_tools.iter().any(|t| t == "web_fetch") {
        registry.register(Arc::new(WebFetchTool::new(
            state.browser_session_manager.clone(),
            prompts.clone(),
        )));
    }

    if allowed_tools.iter().any(|t| t == "search") {
        registry.register(Arc::new(WebSearchTool::new(state.search_provider.clone(), prompts.clone())));
    }

    if allowed_tools.iter().any(|t| t == "task")
        && let Some(executor) = state.task_executor()
    {
        registry.register(Arc::new(TaskTool::new(
            state.task_service.clone(),
            state.agent_service.clone(),
            executor,
            state.broadcast_service.clone(),
            prompts.clone(),
        )));
    }

    if allowed_tools.iter().any(|t| t == "heartbeat") {
        registry.register(Arc::new(HeartbeatTool::new(
            state.agent_service.clone(),
            state.storage_service.clone(),
            prompts.clone(),
        )));
    }

    if allowed_tools.iter().any(|t| t == "credentials") {
        registry.register(Arc::new(RequestCredentialsTool::new(
            state.vault_service.clone(),
            prompts.clone(),
        )));
    }

    if allowed_tools.iter().any(|t| t == "app") {
        registry.register(Arc::new(crate::tool::manage_service::ManageServiceTool::new(
            state.app_service.clone(),
            prompts.clone(),
            state.notification_service.clone(),
            state.broadcast_service.clone(),
        )));
    }

    if allowed_tools.iter().any(|t| t == "voice_call") {
        registry.register(Arc::new(crate::tool::voice::VoiceCallTool {
            provider: state.voice_provider.clone(),
            prompts: prompts.clone(),
            contact_service: state.contact_service.clone(),
            call_service: state.call_service.clone(),
        }));
        registry.register(Arc::new(crate::tool::voice::SendDtmfTool {
            prompts: prompts.clone(),
        }));
        registry.register(Arc::new(crate::tool::voice::HangupCallTool {
            prompts: prompts.clone(),
        }));
    }

    if agent_id == "system" {
        registry.register(Arc::new(super::create_agent::CreateAgentTool::new(
            state.agent_service.clone(),
            state.storage_service.clone(),
            state.broadcast_service.clone(),
            prompts.clone(),
        )));
    }

    if is_task {
        registry.register(Arc::new(TaskControlTool::new(
            state.config.storage.workspaces_path.clone().into(),
            state.prompts.clone(),
        )));
    }

    tracing::debug!(cli_tools_count = state.cli_tools_config.len(), ?allowed_tools, "Building tool registry");
    for tool_config in state.cli_tools_config.iter() {
        if allowed_tools.iter().any(|t| t == &tool_config.name) {
            tracing::debug!(tool = %tool_config.name, "Registering CLI tool");
            registry.register(Arc::new(CliTool::new(
                tool_config.clone(),
                state.sandbox_manager.clone(),
                state.skill_service.clone(),
                state.storage_service.clone(),
            )));
        }
    }

    let tool_names: Vec<&str> = registry.definitions.iter().map(|d| d.id.as_str()).collect();
    tracing::debug!(
        ?tool_names,
        cli_configs = state.cli_tools_config.len(),
        ?allowed_tools,
        "Tool registry built"
    );

    registry
}

pub async fn build_agent_summaries(
    state: &AppState,
    user_id: &str,
    current_agent_id: &str,
    tools: &[String],
) -> Vec<(String, String)> {
    if !tools.iter().any(|t| t == "task") {
        return Vec::new();
    }

    let agents = match state.agent_service.list(user_id).await {
        Ok(agents) => agents,
        Err(_) => return Vec::new(),
    };

    agents
        .into_iter()
        .filter(|a| a.id != current_agent_id && a.enabled)
        .map(|a| (a.name, a.description))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct MockTool;

    #[async_trait]
    impl AgentTool for MockTool {
        fn name(&self) -> &str {
            "mock"
        }

        fn definitions(&self) -> Vec<ToolDefinition> {
            vec![ToolDefinition {
                id: "mock_action".to_string(),
                provider_id: String::new(),
                description: "A mock action".to_string(),
                parameters: serde_json::json!({"type": "object", "properties": {}}),
            }]
        }

        async fn execute(&self, tool_name: &str, _arguments: Value, _ctx: &InferenceContext) -> Result<ToolOutput, AppError> {
            Ok(ToolOutput::text(format!("executed {tool_name}")))
        }
    }

    fn mock_context() -> InferenceContext {
        let broadcast = crate::chat::broadcast::BroadcastService::new();
        let event_sender = broadcast.create_event_sender("test-user", "test-chat");
        InferenceContext::new(
            crate::auth::User {
                id: "test-user".into(),
                username: "testuser".into(),
                email: "test@test.com".into(),
                name: "Test".into(),
                password_hash: String::new(),
                timezone: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
            crate::agent::models::Agent {
                id: "test-agent".into(),
                user_id: Some("test-user".into()),
                name: "Test Agent".into(),
                description: String::new(),
                model_group: "primary".into(),
                enabled: true,
                tools: vec![],
                skills: None,
                sandbox_config: None,
                max_concurrent_tasks: None,
                avatar: None,
                identity: Default::default(),
                prompt: None,
                heartbeat_interval: None,
                next_heartbeat_at: None,
                heartbeat_chat_id: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
            crate::chat::models::Chat {
                id: "test-chat".into(),
                user_id: "test-user".into(),
                space_id: None,
                task_id: None,
                agent_id: "test-agent".into(),
                title: None,
                archived_at: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
            event_sender,
            tokio_util::sync::CancellationToken::new(),
            tokio_util::sync::CancellationToken::new(),
        )
    }

    #[tokio::test]
    async fn test_registry_dispatch() {
        let mut registry = AgentToolRegistry::new();
        registry.register(Arc::new(MockTool));

        let defs = &registry.definitions;
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].id, "mock_action");

        let ctx = mock_context();
        let output = registry
            .execute("mock_action", serde_json::json!({}), &ctx)
            .await
            .unwrap();
        assert_eq!(output.text_content(), "executed mock_action");
    }

    #[tokio::test]
    async fn test_registry_unknown_tool() {
        let registry = AgentToolRegistry::new();
        let ctx = mock_context();
        let result = registry.execute("nonexistent", serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
    }
}
