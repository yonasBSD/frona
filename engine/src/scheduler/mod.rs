use std::sync::Arc;
use std::time::Duration;

use crate::agent::task::models::TaskKind;
use crate::agent::task::service::TaskService;
use crate::api::repo::chats::SurrealChatRepo;
use crate::api::repo::insights::SurrealInsightRepo;
use crate::api::repo::spaces::SurrealSpaceRepo;
use crate::api::state::AppState;
use crate::chat::dto::CreateChatRequest;
use crate::chat::repository::ChatRepository;
use crate::error::AppError;
use crate::llm::config::ModelGroup;
use crate::llm::convert::to_rig_messages;
use crate::llm::tool_loop::{self, ToolLoopEvent, ToolLoopEventKind, ToolLoopOutcome};
use crate::memory::insight::repository::InsightRepository;
use crate::memory::models::MemorySourceType;
use crate::memory::service::MemoryService;
use crate::repository::Repository;
use crate::schedule::models::Routine;
use crate::schedule::service::ScheduleService;
use crate::space::repository::SpaceRepository;

pub struct Scheduler {
    memory_service: MemoryService,
    space_repo: SurrealSpaceRepo,
    chat_repo: SurrealChatRepo,
    insight_repo: SurrealInsightRepo,
    compaction_model_group: ModelGroup,
    interval: Duration,
    task_service: TaskService,
    schedule_service: ScheduleService,
    app_state: AppState,
}

impl Scheduler {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        memory_service: MemoryService,
        space_repo: SurrealSpaceRepo,
        chat_repo: SurrealChatRepo,
        insight_repo: SurrealInsightRepo,
        compaction_model_group: ModelGroup,
        interval: Duration,
        task_service: TaskService,
        schedule_service: ScheduleService,
        app_state: AppState,
    ) -> Self {
        Self {
            memory_service,
            space_repo,
            chat_repo,
            insight_repo,
            compaction_model_group,
            interval,
            task_service,
            schedule_service,
            app_state,
        }
    }

    pub fn start(self: Arc<Self>) {
        let scheduler = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(scheduler.interval).await;
                if let Err(e) = scheduler.run_space_compaction().await {
                    tracing::warn!(error = %e, "Scheduled space compaction failed");
                }
            }
        });

        let scheduler = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(7200)).await;
                if let Err(e) = scheduler.run_insight_compaction().await {
                    tracing::warn!(error = %e, "Scheduled insight compaction failed");
                }
            }
        });

        let scheduler = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(7200)).await;
                if let Err(e) = scheduler.run_user_insight_compaction().await {
                    tracing::warn!(error = %e, "Scheduled user insight compaction failed");
                }
            }
        });

        let scheduler = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                if let Err(e) = scheduler.run_cron_tasks().await {
                    tracing::warn!(error = %e, "Cron task check failed");
                }
                if let Err(e) = scheduler.run_routines().await {
                    tracing::warn!(error = %e, "Routine check failed");
                }
            }
        });
    }

    async fn run_cron_tasks(&self) -> Result<(), AppError> {
        let templates = self.task_service.find_due_cron_templates().await?;
        if templates.is_empty() {
            return Ok(());
        }

        let executor = match self.app_state.task_executor() {
            Some(e) => e,
            None => {
                tracing::warn!("Task executor not available, skipping cron tasks");
                return Ok(());
            }
        };

        for template in templates {
            let (cron_expression, _next_run_at) = match &template.kind {
                TaskKind::Cron { cron_expression, next_run_at, .. } => {
                    (cron_expression.clone(), *next_run_at)
                }
                _ => continue,
            };

            tracing::info!(
                task_id = %template.id,
                title = %template.title,
                "Firing cron task"
            );

            let child = match self.task_service.create_cron_run(&template).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(error = %e, task_id = %template.id, "Failed to create cron run child");
                    continue;
                }
            };

            if let Err(e) = executor.spawn_execution(child).await {
                tracing::warn!(error = %e, task_id = %template.id, "Failed to spawn cron run");
            }

            match ScheduleService::next_cron_occurrence(&cron_expression) {
                Ok(next) => {
                    if let Err(e) = self
                        .task_service
                        .advance_cron_template(&template.id, next, template.chat_id.as_deref())
                        .await
                    {
                        tracing::warn!(error = %e, task_id = %template.id, "Failed to advance cron template");
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, task_id = %template.id, "Failed to compute next cron occurrence");
                }
            }
        }

        Ok(())
    }

    async fn run_routines(&self) -> Result<(), AppError> {
        let routines = self.schedule_service.find_due_routines().await?;
        if routines.is_empty() {
            return Ok(());
        }

        for routine in routines {
            if routine.items.is_empty() {
                continue;
            }

            tracing::info!(
                routine_id = %routine.id,
                agent_id = %routine.agent_id,
                item_count = routine.items.len(),
                "Firing routine"
            );

            let schedule_service = self.schedule_service.clone();
            let app_state = self.app_state.clone();
            let routine_clone = routine.clone();

            if let Err(e) = self.schedule_service.mark_running(&routine.id).await {
                tracing::warn!(error = %e, routine_id = %routine.id, "Failed to mark routine running");
                continue;
            }

            tokio::spawn(async move {
                if let Err(e) = execute_routine(&app_state, &routine_clone).await {
                    tracing::error!(
                        error = %e,
                        routine_id = %routine_clone.id,
                        "Routine execution failed"
                    );
                }
                if let Err(e) = schedule_service.mark_idle_and_advance(&routine_clone.id).await {
                    tracing::error!(
                        error = %e,
                        routine_id = %routine_clone.id,
                        "Failed to mark routine idle"
                    );
                }
            });
        }

        Ok(())
    }

    async fn run_space_compaction(&self) -> Result<(), AppError> {
        let spaces = self.space_repo.find_all().await?;

        for space in spaces {
            let chats = self.chat_repo.find_by_space_id(&space.id).await?;
            if chats.is_empty() {
                continue;
            }

            let mut summaries = Vec::new();
            for chat in &chats {
                let title = chat.title.clone().unwrap_or_else(|| "Untitled".to_string());

                let memory = self
                    .memory_service
                    .get_memory(MemorySourceType::Chat, &chat.id)
                    .await?;

                let summary = if let Some(mem) = memory {
                    mem.content
                } else {
                    format!("(No summary available for chat: {title})")
                };

                summaries.push((title, summary));
            }

            if let Err(e) = self
                .memory_service
                .compact_space(&space.id, summaries, &self.compaction_model_group)
                .await
            {
                tracing::warn!(
                    space_id = %space.id,
                    error = %e,
                    "Failed to compact space"
                );
            }
        }

        Ok(())
    }

    async fn run_insight_compaction(&self) -> Result<(), AppError> {
        let agent_ids = self.insight_repo.find_distinct_agent_ids().await?;

        tracing::info!(
            agent_count = agent_ids.len(),
            "Starting scheduled insight compaction"
        );

        for agent_id in &agent_ids {
            tracing::info!(agent_id = %agent_id, "Running scheduled insight compaction for agent");
            if let Err(e) = self
                .memory_service
                .compact_insights_if_needed(agent_id, &self.compaction_model_group)
                .await
            {
                tracing::warn!(
                    agent_id = %agent_id,
                    error = %e,
                    "Failed to compact insights for agent"
                );
            }
        }

        Ok(())
    }

    async fn run_user_insight_compaction(&self) -> Result<(), AppError> {
        let user_ids = self.insight_repo.find_distinct_user_ids().await?;

        tracing::info!(
            user_count = user_ids.len(),
            "Starting scheduled user insight compaction"
        );

        for user_id in &user_ids {
            tracing::info!(user_id = %user_id, "Running scheduled user insight compaction");
            if let Err(e) = self
                .memory_service
                .compact_user_insights_if_needed(user_id, &self.compaction_model_group)
                .await
            {
                tracing::warn!(
                    user_id = %user_id,
                    error = %e,
                    "Failed to compact insights for user"
                );
            }
        }

        Ok(())
    }
}

async fn execute_routine(
    state: &AppState,
    routine: &Routine,
) -> Result<(), AppError> {
    let user_id = &routine.user_id;
    let agent_id = &routine.agent_id;

    let chat_id = if let Some(ref cid) = routine.chat_id {
        cid.clone()
    } else {
        let chat = state
            .chat_service
            .create_chat(
                user_id,
                CreateChatRequest {
                    space_id: None,
                    task_id: None,
                    agent_id: agent_id.clone(),
                    title: Some("Routine".to_string()),
                },
            )
            .await?;

        if let Some(r) = state.schedule_service.find_by_id(&routine.id).await? {
            let mut updated = r;
            updated.chat_id = Some(chat.id.clone());
            updated.updated_at = chrono::Utc::now();
            state.schedule_service.repo().update(&updated).await?;
        }

        chat.id
    };

    let items_text: Vec<String> = routine
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| format!("{}. {}", i + 1, item.description))
        .collect();
    let message_content = format!(
        "Time to run your routine. Process each item:\n\n{}",
        items_text.join("\n")
    );

    state
        .chat_service
        .create_stream_user_message(user_id, &chat_id, &message_content, vec![])
        .await?;

    let agent_config = state
        .chat_service
        .resolve_agent_config(agent_id)
        .await?;

    let skill_summaries: Vec<(String, String)> = state
        .skill_resolver
        .list(agent_id)
        .await
        .into_iter()
        .map(|s| (s.name, s.description))
        .collect();

    let agent_summaries = crate::api::routes::messages::build_agent_summaries_from_state(
        state, user_id, agent_id, &agent_config.tools,
    )
    .await;

    let system_prompt = state
        .memory_service
        .build_augmented_system_prompt(
            &agent_config.system_prompt,
            agent_id,
            user_id,
            None,
            &skill_summaries,
            &agent_summaries,
            &agent_config.identity,
        )
        .await
        .unwrap_or(agent_config.system_prompt.clone());

    let model_group = state
        .chat_service
        .provider_registry()
        .resolve_model_group(&agent_config.model_group)
        .map_err(|e| AppError::Llm(e.to_string()))?;

    let registry = state.chat_service.provider_registry().clone();

    let stored_messages = state.chat_service.get_stored_messages(&chat_id).await;
    let rig_history = to_rig_messages(&stored_messages, agent_id);

    let (tool_event_tx, mut tool_event_rx) = tokio::sync::mpsc::channel::<ToolLoopEvent>(32);

    let tool_registry = crate::api::routes::messages::build_tool_registry(
        state,
        agent_id,
        user_id,
        &chat_id,
        &agent_config.tools,
        agent_config.sandbox_config.as_ref(),
        tool_event_tx.clone(),
    )
    .await;

    let cancel_token = state.active_sessions.register(&chat_id).await;

    let tool_handle = {
        let cancel_token = cancel_token.clone();
        tokio::spawn(async move {
            tool_loop::run_tool_loop(
                &registry,
                &model_group,
                &system_prompt,
                rig_history,
                &tool_registry,
                tool_event_tx,
                cancel_token,
            )
            .await
        })
    };

    let mut accumulated = String::new();
    while let Some(event) = tool_event_rx.recv().await {
        if let ToolLoopEventKind::Text(text) = event.kind {
            accumulated.push_str(&text);
        }
    }

    match tool_handle.await {
        Ok(Ok(outcome)) => {
            if let ToolLoopOutcome::Completed(_) = outcome
                && !accumulated.is_empty()
            {
                let _ = state
                    .chat_service
                    .save_assistant_message(&chat_id, accumulated)
                    .await;
            }
        }
        Ok(Err(e)) => {
            tracing::error!(error = %e, routine_id = %routine.id, "Routine tool loop failed");
        }
        Err(e) => {
            tracing::error!(error = %e, routine_id = %routine.id, "Routine tool loop panicked");
        }
    }

    state.active_sessions.remove(&chat_id).await;
    Ok(())
}
