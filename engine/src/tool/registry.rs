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
                .insert(def.name.clone(), owner_name.clone());

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

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

pub fn build_tool_registry(
    state: &AppState,
    allowed_tools: &[String],
    is_task: bool,
) -> AgentToolRegistry {
    use super::browser::tool::BrowserTool;
    use super::cli::CliTool;
    use super::delegate::DelegateTaskTool;
    use super::heartbeat::HeartbeatTool;
    use super::notify_human::NotifyHumanTool;
    use super::produce_file::ProduceFileTool;
    use super::remember::{RememberTool, RememberUserFactTool};
    use super::request_credentials::RequestCredentialsTool;
    use super::schedule::ScheduleTaskTool;
    use super::skill::SkillTool;
    use super::task_control::TaskControlTool;
    use super::time::TimeTool;
    use super::update_entity::UpdateEntityTool;
    use super::update_identity::UpdateIdentityTool;
    use super::web_fetch::WebFetchTool;
    use super::web_search::WebSearchTool;

    let mut registry = AgentToolRegistry::new();

    let prompts = state.prompts.clone();

    registry.register(Arc::new(TimeTool::new(prompts.clone())));
    registry.register(Arc::new(NotifyHumanTool::new(state.vault_service.clone(), prompts.clone())));

    let workspaces_path = std::path::PathBuf::from(&state.config.storage.workspaces_path);
    registry.register(Arc::new(ProduceFileTool::new(
        workspaces_path,
        prompts.clone(),
    )));

    registry.register(Arc::new(UpdateEntityTool::new(
        state.db.clone(),
        "agent",
        "update_agent",
    )));

    registry.register(Arc::new(UpdateIdentityTool::new(
        state.db.clone(),
        prompts.clone(),
    )));

    registry.register(Arc::new(RememberTool::new(
        state.memory_service.clone(),
        state.compaction_model_group(),
        prompts.clone(),
    )));

    registry.register(Arc::new(RememberUserFactTool::new(
        state.memory_service.clone(),
        state.compaction_model_group(),
        prompts.clone(),
    )));

    registry.register(Arc::new(SkillTool::new(
        state.skill_resolver.clone(),
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

    if allowed_tools.iter().any(|t| t == "web_search") {
        registry.register(Arc::new(WebSearchTool::new(state.search_provider.clone(), prompts.clone())));
    }

    if allowed_tools.iter().any(|t| t == "delegate")
        && let Some(executor) = state.task_executor()
    {
        registry.register(Arc::new(DelegateTaskTool::new(
            state.task_service.clone(),
            state.agent_service.clone(),
            executor,
            state.broadcast_service.clone(),
            prompts.clone(),
        )));
    }

    if allowed_tools.iter().any(|t| t == "schedule") {
        registry.register(Arc::new(ScheduleTaskTool::new(
            state.task_service.clone(),
            state.agent_service.clone(),
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

    if allowed_tools.iter().any(|t| t == "request_credentials") {
        registry.register(Arc::new(RequestCredentialsTool::new(
            state.vault_service.clone(),
            prompts.clone(),
        )));
    }

    if allowed_tools.iter().any(|t| t == "manage_service") {
        registry.register(Arc::new(crate::tool::manage_service::ManageServiceTool::new(
            state.app_service.clone(),
            prompts.clone(),
            state.notification_service.clone(),
            state.broadcast_service.clone(),
        )));
    }

    if allowed_tools.iter().any(|t| t == "make_voice_call") {
        registry.register(Arc::new(crate::tool::voice::VoiceCallTool {
            provider: state.voice_provider.clone(),
            prompts: prompts.clone(),
            contact_service: state.contact_service.clone(),
            call_service: state.call_service.clone(),
        }));
    }

    if allowed_tools.iter().any(|t| t == "send_dtmf") {
        registry.register(Arc::new(crate::tool::voice::SendDtmfTool {
            prompts: prompts.clone(),
        }));
    }

    if allowed_tools.iter().any(|t| t == "hangup_call") {
        registry.register(Arc::new(crate::tool::voice::HangupCallTool {
            prompts: prompts.clone(),
        }));
    }

    if is_task {
        registry.register(Arc::new(TaskControlTool::new(
            state.prompts.clone(),
        )));
    }

    tracing::info!(cli_tools_count = state.cli_tools_config.len(), ?allowed_tools, "Building tool registry");
    for tool_config in state.cli_tools_config.iter() {
        if allowed_tools.iter().any(|t| t == &tool_config.name) {
            tracing::info!(tool = %tool_config.name, "Registering CLI tool");
            registry.register(Arc::new(CliTool::new(
                tool_config.clone(),
                state.sandbox_manager.clone(),
                state.skill_resolver.clone(),
            )));
        }
    }

    let tool_names: Vec<&str> = registry.definitions.iter().map(|d| d.name.as_str()).collect();
    tracing::info!(
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
    if !tools.iter().any(|t| t == "delegate") {
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
                name: "mock_action".to_string(),
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
                sandbox_config: None,
                max_concurrent_tasks: None,
                avatar: None,
                identity: Default::default(),
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
        )
    }

    #[tokio::test]
    async fn test_registry_dispatch() {
        let mut registry = AgentToolRegistry::new();
        registry.register(Arc::new(MockTool));

        let defs = &registry.definitions;
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "mock_action");

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
