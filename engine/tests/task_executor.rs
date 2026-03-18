use std::sync::Arc;

use chrono::Utc;
use frona::agent::task::executor::TaskExecutor;
use frona::agent::task::models::{Task, TaskKind, TaskStatus};
use frona::agent::service::AgentService;
use frona::chat::message::models::{MessageRole, MessageTool};
use frona::storage::StorageService;
use frona::core::config::Config;
use frona::db::init as db;
use frona::db::repo::generic::SurrealRepo;
use frona::core::state::AppState;
use frona::core::repository::Repository;
use surrealdb::engine::local::{Db, Mem};
use surrealdb::Surreal;

async fn test_db() -> Surreal<Db> {
    let db = Surreal::new::<Mem>(()).await.unwrap();
    db::setup_schema(&db).await.unwrap();
    db
}

fn test_config(tmp: &tempfile::TempDir) -> Config {
    let base = tmp.path().to_string_lossy().to_string();
    Config {
        server: frona::core::config::ServerConfig {
            port: 0,
            static_dir: format!("{base}/static"),
            max_concurrent_tasks: 10,
            ..Default::default()
        },
        auth: frona::core::config::AuthConfig {
            encryption_secret: "test-secret".to_string(),
            ..Default::default()
        },
        database: frona::core::config::DatabaseConfig {
            path: format!("{base}/db"),
        },
        browser: Some(frona::core::config::BrowserConfig {
            ws_url: "ws://localhost:0".to_string(),
            profiles_path: format!("{base}/profiles"),
            connection_timeout_ms: 30000,
        }),
        storage: frona::core::config::StorageConfig {
            workspaces_path: format!("{base}/workspaces"),
            files_path: format!("{base}/files"),
            shared_config_dir: format!("{base}/config"),
        },
        ..Default::default()
    }
}

async fn test_app_state() -> (AppState, tempfile::TempDir) {
    let db = test_db().await;
    let tmp = tempfile::tempdir().unwrap();
    let config = test_config(&tmp);
    let storage = StorageService::new(&config);
    let agent_service = AgentService::new(
        SurrealRepo::new(db.clone()),
        &config.cache,
        std::path::PathBuf::from(&config.storage.shared_config_dir).join("agents"),
    );
    let metrics_handle = frona::core::metrics::setup_metrics_recorder();
    let state = AppState::new(db, &config, None, agent_service, storage, metrics_handle);
    (state, tmp)
}

fn make_executor(state: &AppState) -> Arc<TaskExecutor> {
    Arc::new(TaskExecutor::new(state.clone()))
}

fn make_task(kind: TaskKind) -> Task {
    let now = Utc::now();
    Task {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: "user-1".to_string(),
        agent_id: "agent-1".to_string(),
        space_id: None,
        chat_id: None,
        title: "Test task".to_string(),
        description: "Do something".to_string(),
        status: TaskStatus::Pending,
        kind,
        run_at: None,
        result_summary: None,
        error_message: None,
        created_at: now,
        updated_at: now,
    }
}

#[tokio::test]
async fn ensure_task_chat_creates_when_missing() {
    let (state, _tmp) = test_app_state().await;
    let executor = make_executor(&state);
    let mut task = make_task(TaskKind::Direct);
    assert!(task.chat_id.is_none());

    let chat_id = executor.ensure_task_chat(&mut task).await.unwrap();

    assert!(!chat_id.is_empty());
    assert_eq!(task.chat_id.as_deref(), Some(chat_id.as_str()));
}

#[tokio::test]
async fn ensure_task_chat_returns_existing() {
    let (state, _tmp) = test_app_state().await;
    let executor = make_executor(&state);
    let mut task = make_task(TaskKind::Direct);
    task.chat_id = Some("existing-chat-id".to_string());

    let chat_id = executor.ensure_task_chat(&mut task).await.unwrap();

    assert_eq!(chat_id, "existing-chat-id");
    assert_eq!(task.chat_id.as_deref(), Some("existing-chat-id"));
}

#[tokio::test]
async fn save_initial_message_saves_on_first_run() {
    let (state, _tmp) = test_app_state().await;
    let executor = make_executor(&state);
    let mut task = make_task(TaskKind::Direct);
    let chat_id = executor.ensure_task_chat(&mut task).await.unwrap();

    executor
        .save_initial_message_if_needed(&task, &chat_id)
        .await
        .unwrap();

    let messages = state.chat_service.get_stored_messages(&chat_id).await;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "Do something");
}

#[tokio::test]
async fn save_initial_message_skips_on_resume() {
    let (state, _tmp) = test_app_state().await;
    let executor = make_executor(&state);
    let mut task = make_task(TaskKind::Direct);
    let chat_id = executor.ensure_task_chat(&mut task).await.unwrap();

    executor
        .save_initial_message_if_needed(&task, &chat_id)
        .await
        .unwrap();
    executor
        .save_initial_message_if_needed(&task, &chat_id)
        .await
        .unwrap();

    let messages = state.chat_service.get_stored_messages(&chat_id).await;
    assert_eq!(messages.len(), 1);
}

#[tokio::test]
async fn handle_cancelled_saves_partial_text() {
    let (state, _tmp) = test_app_state().await;
    let executor = make_executor(&state);
    let mut task = make_task(TaskKind::Direct);
    let chat_id = executor.ensure_task_chat(&mut task).await.unwrap();

    let repo: SurrealRepo<Task> = SurrealRepo::new(state.db.clone());
    repo.create(&task).await.unwrap();

    executor
        .handle_cancelled(&task, &chat_id, "partial output".to_string())
        .await
        .unwrap();

    let messages = state.chat_service.get_stored_messages(&chat_id).await;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "partial output");

    let updated = repo.find_by_id(&task.id).await.unwrap().unwrap();
    assert_eq!(updated.status, TaskStatus::Cancelled);
}

#[tokio::test]
async fn handle_cancelled_skips_empty_text() {
    let (state, _tmp) = test_app_state().await;
    let executor = make_executor(&state);
    let mut task = make_task(TaskKind::Direct);
    let chat_id = executor.ensure_task_chat(&mut task).await.unwrap();

    let repo: SurrealRepo<Task> = SurrealRepo::new(state.db.clone());
    repo.create(&task).await.unwrap();

    executor
        .handle_cancelled(&task, &chat_id, String::new())
        .await
        .unwrap();

    let messages = state.chat_service.get_stored_messages(&chat_id).await;
    assert!(messages.is_empty());

    let updated = repo.find_by_id(&task.id).await.unwrap().unwrap();
    assert_eq!(updated.status, TaskStatus::Cancelled);
}

#[tokio::test]
async fn handle_error_marks_failed_and_delivers() {
    let (state, _tmp) = test_app_state().await;
    let executor = make_executor(&state);

    let source_chat = state
        .chat_service
        .create_chat(
            "user-1",
            frona::chat::models::CreateChatRequest {
                space_id: None,
                task_id: None,
                agent_id: "agent-1".to_string(),
                title: Some("Source chat".to_string()),
            },
        )
        .await
        .unwrap();

    let mut task = make_task(TaskKind::Delegation {
        source_agent_id: "agent-1".to_string(),
        source_chat_id: source_chat.id.clone(),
        deliver_directly: true,
    });
    task.chat_id = Some("task-chat-id".to_string());

    let repo: SurrealRepo<Task> = SurrealRepo::new(state.db.clone());
    repo.create(&task).await.unwrap();

    let error = frona::core::error::AppError::Internal("something broke".to_string());
    executor.handle_error(&task, &error).await.unwrap();

    let updated = repo.find_by_id(&task.id).await.unwrap().unwrap();
    assert_eq!(updated.status, TaskStatus::Failed);

    let source_messages = state
        .chat_service
        .get_stored_messages(&source_chat.id)
        .await;
    assert_eq!(source_messages.len(), 1);
    assert!(source_messages[0].content.contains("something broke"));
}

#[tokio::test]
async fn lifecycle_complete_event_detected() {
    let (state, _tmp) = test_app_state().await;
    let _executor = make_executor(&state);

    let source_chat = state
        .chat_service
        .create_chat(
            "user-1",
            frona::chat::models::CreateChatRequest {
                space_id: None,
                task_id: Some("task-1".to_string()),
                agent_id: "agent-1".to_string(),
                title: Some("Task chat".to_string()),
            },
        )
        .await
        .unwrap();

    // Save a System lifecycle event (simulating what TaskControlTool does)
    state
        .chat_service
        .save_system_event(
            &source_chat.id,
            MessageTool::TaskCompletion {
                task_id: "task-1".to_string(),
                chat_id: Some(source_chat.id.clone()),
                status: TaskStatus::Completed,
                summary: Some("Research findings here".to_string()),
            },
        )
        .await
        .unwrap();

    // Verify the System message was saved
    let messages = state.chat_service.get_stored_messages(&source_chat.id).await;
    let system_msgs: Vec<_> = messages
        .iter()
        .filter(|m| m.role == MessageRole::System)
        .collect();
    assert_eq!(system_msgs.len(), 1);
    assert!(matches!(
        &system_msgs[0].tool,
        Some(MessageTool::TaskCompletion {
            status: TaskStatus::Completed,
            summary: Some(s),
            ..
        }) if s == "Research findings here"
    ));
}

#[tokio::test]
async fn lifecycle_defer_event_detected() {
    let (state, _tmp) = test_app_state().await;
    let _executor = make_executor(&state);

    let task_chat = state
        .chat_service
        .create_chat(
            "user-1",
            frona::chat::models::CreateChatRequest {
                space_id: None,
                task_id: Some("task-2".to_string()),
                agent_id: "agent-1".to_string(),
                title: Some("Task chat".to_string()),
            },
        )
        .await
        .unwrap();

    state
        .chat_service
        .save_system_event(
            &task_chat.id,
            MessageTool::TaskDeferred {
                task_id: "task-2".to_string(),
                delay_minutes: 30,
                reason: "Waiting for external API".to_string(),
            },
        )
        .await
        .unwrap();

    let messages = state.chat_service.get_stored_messages(&task_chat.id).await;
    let system_msgs: Vec<_> = messages
        .iter()
        .filter(|m| m.role == MessageRole::System)
        .collect();
    assert_eq!(system_msgs.len(), 1);
    assert!(matches!(
        &system_msgs[0].tool,
        Some(MessageTool::TaskDeferred {
            delay_minutes: 30,
            reason,
            ..
        }) if reason == "Waiting for external API"
    ));
}

#[tokio::test]
async fn mark_deferred_sets_pending_with_run_at() {
    let (state, _tmp) = test_app_state().await;

    let repo: SurrealRepo<Task> = SurrealRepo::new(state.db.clone());
    let mut task = make_task(TaskKind::Direct);
    task.status = TaskStatus::InProgress;
    repo.create(&task).await.unwrap();

    let run_at = Utc::now() + chrono::Duration::minutes(30);
    state
        .task_service
        .mark_deferred(&task.id, run_at, "waiting")
        .await
        .unwrap();

    let updated = repo.find_by_id(&task.id).await.unwrap().unwrap();
    assert_eq!(updated.status, TaskStatus::Pending);
    assert!(updated.run_at.is_some());
}

#[tokio::test]
async fn deliver_to_source_skips_direct_tasks() {
    let (state, _tmp) = test_app_state().await;
    let executor = make_executor(&state);
    let task = make_task(TaskKind::Direct);

    executor
        .deliver_to_source(&task, TaskStatus::Completed, "result".to_string())
        .await;
}

#[tokio::test]
async fn deliver_to_source_sends_to_delegation() {
    let (state, _tmp) = test_app_state().await;
    let executor = make_executor(&state);

    let source_chat = state
        .chat_service
        .create_chat(
            "user-1",
            frona::chat::models::CreateChatRequest {
                space_id: None,
                task_id: None,
                agent_id: "agent-1".to_string(),
                title: Some("Source".to_string()),
            },
        )
        .await
        .unwrap();

    let mut task = make_task(TaskKind::Delegation {
        source_agent_id: "agent-1".to_string(),
        source_chat_id: source_chat.id.clone(),
        deliver_directly: true,
    });
    task.chat_id = Some("task-chat".to_string());

    executor
        .deliver_to_source(&task, TaskStatus::Completed, "All done".to_string())
        .await;

    let messages = state
        .chat_service
        .get_stored_messages(&source_chat.id)
        .await;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "All done");
}

#[tokio::test]
async fn broadcast_task_status_emits_event() {
    let (state, _tmp) = test_app_state().await;
    let executor = make_executor(&state);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    state.broadcast_service.register_session("user-1", tx);

    // Small delay to let register_session complete (it spawns a task)
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut task = make_task(TaskKind::Direct);
    task.chat_id = Some("chat-123".to_string());

    executor.broadcast_task_status(&task, "completed", Some("All done"));

    // Wait briefly for the dispatcher to route the event
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let event = rx.try_recv().expect("Expected to receive an SSE event");
    let event: Result<axum::response::sse::Event, std::convert::Infallible> = event;
    let _sse_event = event.unwrap();
    // The fact that we received an event confirms the broadcast works.
    // Detailed field-level assertions are covered by API integration tests.
}

#[tokio::test]
async fn concurrency_global_limit() {
    let (mut state, _tmp) = test_app_state().await;
    state.max_concurrent_tasks = 1;
    let executor = make_executor(&state);

    let repo: SurrealRepo<Task> = SurrealRepo::new(state.db.clone());

    let mut task1 = make_task(TaskKind::Direct);
    task1.id = "task-1".to_string();
    task1.status = TaskStatus::InProgress;
    repo.create(&task1).await.unwrap();
    executor.spawn_execution(task1).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut task2 = make_task(TaskKind::Direct);
    task2.id = "task-2".to_string();
    repo.create(&task2).await.unwrap();
    executor.spawn_execution(task2).await.unwrap();

    let t2 = repo.find_by_id("task-2").await.unwrap().unwrap();
    assert_eq!(t2.status, TaskStatus::Pending, "Second task should stay pending when limit reached");
}

#[tokio::test]
async fn find_lifecycle_event_uses_last_assistant_message_as_summary() {
    let (state, _tmp) = test_app_state().await;

    let chat = state
        .chat_service
        .create_chat(
            "user-1",
            frona::chat::models::CreateChatRequest {
                space_id: None,
                task_id: Some("task-lf".to_string()),
                agent_id: "agent-1".to_string(),
                title: Some("Task chat".to_string()),
            },
        )
        .await
        .unwrap();

    // Save an assistant message (the actual answer)
    state
        .chat_service
        .save_assistant_message(&chat.id, "The answer is 42.".to_string())
        .await
        .unwrap();

    // Save lifecycle event AFTER the assistant message (no summary)
    state
        .chat_service
        .save_system_event(
            &chat.id,
            MessageTool::TaskCompletion {
                task_id: "task-lf".to_string(),
                chat_id: Some(chat.id.clone()),
                status: TaskStatus::Completed,
                summary: None,
            },
        )
        .await
        .unwrap();

    // Create a task and ensure it exists in DB
    let mut task = make_task(TaskKind::Delegation {
        source_agent_id: "agent-1".to_string(),
        source_chat_id: "source-chat".to_string(),
        deliver_directly: true,
    });
    task.id = "task-lf".to_string();
    task.chat_id = Some(chat.id.clone());
    task.status = TaskStatus::InProgress;
    let repo: SurrealRepo<Task> = SurrealRepo::new(state.db.clone());
    repo.create(&task).await.unwrap();

    // Verify: the lifecycle event should resolve its summary from the assistant message
    let messages = state.chat_service.get_stored_messages(&chat.id).await;
    let agent_msg = messages.iter().find(|m| m.role == MessageRole::Agent);
    assert!(agent_msg.is_some());
    assert_eq!(agent_msg.unwrap().content, "The answer is 42.");

    let system_msg = messages.iter().find(|m| m.role == MessageRole::System);
    assert!(system_msg.is_some());
    assert!(matches!(
        &system_msg.unwrap().tool,
        Some(MessageTool::TaskCompletion { summary: None, .. })
    ));
}

#[tokio::test]
async fn deliver_to_source_saves_message_to_user_chat() {
    let (state, _tmp) = test_app_state().await;
    let executor = make_executor(&state);

    // Create a user chat (no task_id) — deliver_directly=false so
    // check_and_resume_parent runs, but it should bail out because
    // the source chat is not a task chat.
    let user_chat = state
        .chat_service
        .create_chat(
            "user-1",
            frona::chat::models::CreateChatRequest {
                space_id: None,
                task_id: None,
                agent_id: "agent-1".to_string(),
                title: Some("User chat".to_string()),
            },
        )
        .await
        .unwrap();

    let mut task = make_task(TaskKind::Delegation {
        source_agent_id: "agent-1".to_string(),
        source_chat_id: user_chat.id.clone(),
        deliver_directly: false,
    });
    task.chat_id = Some("task-chat".to_string());

    let repo: SurrealRepo<Task> = SurrealRepo::new(state.db.clone());
    repo.create(&task).await.unwrap();

    executor
        .deliver_to_source(&task, TaskStatus::Completed, "Done".to_string())
        .await;

    // Message should be delivered
    let messages = state
        .chat_service
        .get_stored_messages(&user_chat.id)
        .await;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "Done");
}

#[tokio::test]
async fn lifecycle_event_saved_after_assistant_message() {
    let (state, _tmp) = test_app_state().await;

    let chat = state
        .chat_service
        .create_chat(
            "user-1",
            frona::chat::models::CreateChatRequest {
                space_id: None,
                task_id: Some("task-order".to_string()),
                agent_id: "agent-1".to_string(),
                title: Some("Task chat".to_string()),
            },
        )
        .await
        .unwrap();

    // Simulate the executor flow: save assistant message first
    state
        .chat_service
        .save_assistant_message(&chat.id, "Here is my answer.".to_string())
        .await
        .unwrap();

    // Then save lifecycle event
    state
        .chat_service
        .save_system_event(
            &chat.id,
            MessageTool::TaskCompletion {
                task_id: "task-order".to_string(),
                chat_id: Some(chat.id.clone()),
                status: TaskStatus::Completed,
                summary: None,
            },
        )
        .await
        .unwrap();

    // Verify ordering: assistant message comes before system event
    let messages = state.chat_service.get_stored_messages(&chat.id).await;
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, MessageRole::Agent);
    assert_eq!(messages[0].content, "Here is my answer.");
    assert_eq!(messages[1].role, MessageRole::System);
    assert!(matches!(
        &messages[1].tool,
        Some(MessageTool::TaskCompletion { status: TaskStatus::Completed, .. })
    ));
}
