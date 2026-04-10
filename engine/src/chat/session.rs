use rig::completion::Message as RigMessage;
pub use tokio_util::sync::CancellationToken;

use crate::chat::broadcast::EventSender;
use crate::chat::models::Chat;
use crate::chat::service::AgentConfig;
use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::inference::config::ModelGroup;
use crate::inference::conversation::{ConversationBuilder, ConversationContext, DefaultConversationBuilder, TaskConversationBuilder, resolve_attachment_path};
use crate::inference::ModelProviderRegistry;
use crate::tool::registry::AgentToolRegistry;
use crate::tool::InferenceContext;

pub struct ChatSessionContext {
    pub chat: Chat,
    pub agent_config: AgentConfig,
    pub system_prompt: String,
    pub model_group: ModelGroup,
    pub rig_history: Vec<RigMessage>,
    pub registry: ModelProviderRegistry,
    pub tool_registry: AgentToolRegistry,
    pub tool_ctx: InferenceContext,
    pub cancel_token: CancellationToken,
}

impl ChatSessionContext {
    pub async fn build(
        state: &AppState,
        user_id: &str,
        chat: Chat,
        cancel_token: CancellationToken,
    ) -> Result<Self, AppError> {
        Self::build_with_task(state, user_id, chat, cancel_token, false).await
    }

    pub async fn build_with_task(
        state: &AppState,
        user_id: &str,
        chat: Chat,
        cancel_token: CancellationToken,
        is_task: bool,
    ) -> Result<Self, AppError> {
        let event_sender: EventSender =
            state.broadcast_service.create_event_sender(user_id, &chat.id);
        let agent_config = state
            .chat_service
            .resolve_agent_config(&chat.agent_id)
            .await?;

        let skills = state
            .skill_service
            .list(&chat.agent_id, agent_config.skills.as_deref())
            .await;

        let agent_summaries =
            crate::tool::registry::build_agent_summaries(
                state,
                user_id,
                &chat.agent_id,
                &agent_config.tools,
            )
            .await;

        let mut system_prompt = match state
            .memory_service
            .build_augmented_system_prompt(
                &agent_config.system_prompt,
                &chat.agent_id,
                user_id,
                chat.space_id.as_deref(),
                &skills,
                &agent_summaries,
                &agent_config.identity,
            )
            .await
        {
            Ok(prompt) => prompt,
            Err(e) => {
                tracing::warn!(error = %e, agent_id = %chat.agent_id, "Failed to build augmented system prompt, using base");
                agent_config.system_prompt.clone()
            }
        };

        let model_group = state
            .chat_service
            .provider_registry()
            .resolve_model_group(&agent_config.model_group)?;

        let stored_messages = state.chat_service.get_stored_messages(&chat.id).await;
        let tool_calls = state.chat_service
            .get_tool_calls(&chat.id)
            .await
            .unwrap_or_default();

        if is_task
            && let Some(task_prompt) = state.prompts.read("TASK.md")
        {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&task_prompt);
        }

        for te in &tool_calls {
            if let Some(sp) = &te.system_prompt {
                system_prompt.push_str("\n\n");
                system_prompt.push_str(sp);
            }
        }

        let model_ref = model_group.main.clone();
        let conv_ctx = ConversationContext {
            agent_id: chat.agent_id.clone(),
            model_ref,
            user_id: user_id.to_string(),
        };
        let task = if let Some(ref task_id) = chat.task_id {
            state.task_service.find_by_id(task_id).await.ok().flatten()
        } else {
            None
        };

        if let Some(ref task) = task {
            let local = chrono::Local::now();
            let fmt = "%Y-%m-%d %H:%M:%S %Z";
            let mut items = vec![
                ("created_at".into(), task.created_at.with_timezone(&local.timezone()).format(fmt).to_string()),
            ];
            if let Some(run_at) = task.run_at {
                items.push(("scheduled_at".into(), run_at.with_timezone(&local.timezone()).format(fmt).to_string()));
            }
            items.push(("now".into(), local.format(fmt).to_string()));
            crate::agent::prompt::append_tagged_section(
                &mut system_prompt,
                "task_time",
                None,
                &items,
            );
        }

        let task_in_progress = task.as_ref().is_some_and(|t| matches!(t.status,
            crate::agent::task::models::TaskStatus::Pending
            | crate::agent::task::models::TaskStatus::InProgress
        ));

        let rig_history = if task_in_progress {
            let builder = TaskConversationBuilder {
                user_service: state.user_service.clone(),
                storage_service: state.storage_service.clone(),
            };
            builder.build(&stored_messages, &tool_calls, &conv_ctx).await
        } else {
            let builder = DefaultConversationBuilder {
                user_service: state.user_service.clone(),
                storage_service: state.storage_service.clone(),
            };
            builder.build(&stored_messages, &tool_calls, &conv_ctx).await
        };

        let registry = state.chat_service.provider_registry().clone();

        let user = state
            .user_service
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".into()))?;

        let tool_registry = crate::tool::registry::build_tool_registry(
            state,
            &chat.agent_id,
            &agent_config.tools,
            is_task,
        );
        let agent = state
            .agent_service
            .find_by_id(&chat.agent_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Agent not found".into()))?;
        let mut file_paths = Vec::new();
        for msg in &stored_messages {
            for att in &msg.attachments {
                let resolved = resolve_attachment_path(att, &state.user_service, &state.storage_service).await;
                if !file_paths.contains(&resolved) {
                    file_paths.push(resolved);
                }
            }
        }

        let mut tool_ctx = InferenceContext::new(user, agent, chat.clone(), event_sender, state.shutdown_token.clone(), cancel_token.clone());
        tool_ctx.file_paths = file_paths;
        tool_ctx.task = task;

        let vault_env = state
            .vault_service
            .hydrate_chat_env_vars(user_id, &chat.id, &chat.agent_id)
            .await
            .unwrap_or_default();
        if !vault_env.is_empty() {
            let mut vault_vars = tool_ctx.vault_env_vars.write().await;
            vault_vars.extend(vault_env);
        }

        Ok(Self {
            chat,
            agent_config,
            system_prompt,
            model_group,
            rig_history,
            registry,
            tool_registry,
            tool_ctx,
            cancel_token,
        })
    }
}
