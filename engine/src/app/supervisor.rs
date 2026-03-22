use tracing::info;

use crate::core::state::AppState;
use crate::notification::models::NotificationLevel;

use super::manager::{ProcessExit, ProcessStatus};
use super::models::AppStatus;

pub async fn restore_and_supervise_apps(
    state: AppState,
) -> Result<(), Box<dyn std::error::Error>> {
    let apps = state.app_service.find_running().await?;
    info!(count = apps.len(), "Found apps to restore");
    for app in &apps {
        info!(
            app_id = %app.id,
            name = %app.name,
            kind = %app.kind,
            status = ?app.status,
            agent_id = %app.agent_id,
            "Restoring app"
        );
        if app.kind == "static" {
            info!(app_id = %app.id, "Skipping static app (no process needed)");
            continue;
        }
        if let Some(ref command) = app.command {
            let manifest: super::models::AppManifest =
                serde_json::from_value(app.manifest.clone())?;
            match state
                .app_service
                .manager()
                .start_app(&app.id, &app.agent_id, command, &manifest, Vec::new())
                .await
            {
                Ok((port, pid)) => {
                    let _ = state
                        .app_service
                        .update_status(&app.id, AppStatus::Running, Some(port), Some(pid))
                        .await;
                    info!(app_id = %app.id, port, "Restored app");
                }
                Err(e) => {
                    tracing::warn!(app_id = %app.id, error = %e, "Failed to restore app");
                    let _ = state
                        .app_service
                        .update_status(&app.id, AppStatus::Failed, None, None)
                        .await;
                    send_app_notification(
                        &state, &app.user_id, &app.id, "restore",
                        NotificationLevel::Error,
                        &format!("App '{}' failed to start", app.name),
                        &e.to_string(),
                    ).await;
                }
            }
        }
    }

    info!(count = apps.len(), "App restoration complete, starting supervision");

    let max_restarts = state.app_service.max_restart_attempts();
    let hibernate_secs = state.app_service.hibernate_after_secs();

    loop {
        tokio::select! {
            () = tokio::time::sleep(std::time::Duration::from_secs(10)) => {}
            () = state.shutdown_token.cancelled() => {
                info!("App supervisor stopping for shutdown");
                let app_ids = state.app_service.manager().get_managed_app_ids().await;
                for app_id in &app_ids {
                    let _ = state.app_service.manager().stop_app(app_id).await;
                }
                return Ok(());
            }
        }

        let access_times = state.app_service.manager().flush_access_times().await;
        for (app_id, at) in &access_times {
            let _ = state.app_service.update_last_accessed(app_id, *at).await;
        }

        let app_ids = state.app_service.manager().get_managed_app_ids().await;
        for app_id in &app_ids {
            if let ProcessStatus::Dead(ProcessExit { status, stderr_tail }) =
                state.app_service.manager().check_process(app_id).await
            {
                let exit_display = status
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                let stderr_summary = if stderr_tail.is_empty() {
                    String::new()
                } else {
                    let last_lines: Vec<&str> = stderr_tail.lines().rev().take(10).collect();
                    last_lines.into_iter().rev().collect::<Vec<_>>().join("\n")
                };
                tracing::warn!(
                    app_id = %app_id,
                    exit_status = %exit_display,
                    stderr = %stderr_summary,
                    "App process died, attempting restart"
                );
                match state
                    .app_service
                    .manager()
                    .try_restart_crashed(app_id, max_restarts)
                    .await
                {
                    Ok(Some((port, pid))) => {
                        let _ = state
                            .app_service
                            .update_status(app_id, AppStatus::Running, Some(port), Some(pid))
                            .await;
                        tracing::info!(app_id = %app_id, "App restarted after crash");
                    }
                    Ok(None) => {
                        let _ = state
                            .app_service
                            .update_status(app_id, AppStatus::Failed, None, None)
                            .await;
                        state.app_service.manager().remove_process(app_id).await;
                        tracing::warn!(app_id = %app_id, "App exceeded max restarts, removed from supervision");
                        if let Ok(Some(app)) = state.app_service.get(app_id).await {
                            send_app_notification(
                                &state, &app.user_id, app_id, "crash",
                                NotificationLevel::Error,
                                &format!("App '{}' crashed", app.name),
                                &format!("Exceeded max restarts.\n{stderr_summary}"),
                            ).await;
                            attempt_fix_app_on_crash(&state, &app).await;
                        }
                    }
                    Err(e) => {
                        tracing::error!(app_id = %app_id, error = %e, "Failed to restart app");
                    }
                }
            }
        }

        if hibernate_secs > 0 {
            let now = chrono::Utc::now();
            if let Ok(running_apps) = state.app_service.find_running().await {
                for app in running_apps {
                    if app.kind == "static" || app.status != AppStatus::Running {
                        continue;
                    }
                    let manifest: Result<super::models::AppManifest, _> =
                        serde_json::from_value(app.manifest.clone());
                    if let Ok(m) = manifest
                        && !m.effective_hibernate()
                    {
                        continue;
                    }

                    let last = state
                        .app_service
                        .manager()
                        .get_last_accessed(&app.id)
                        .await
                        .or(app.last_accessed_at)
                        .unwrap_or(app.updated_at);

                    let idle = (now - last).num_seconds() as u64;
                    if idle >= hibernate_secs {
                        tracing::info!(app_id = %app.id, idle_secs = idle, "Hibernating idle app");
                        let _ = state.app_service.manager().stop_app(&app.id).await;
                        let _ = state
                            .app_service
                            .update_status(&app.id, AppStatus::Hibernated, None, None)
                            .await;
                    }
                }
            }
        }
    }
}

async fn attempt_fix_app_on_crash(state: &AppState, app: &super::models::App) {
    if app.crash_fix_attempts > 0 {
        return;
    }

    if let Some(mut app) = state.app_service.get(&app.id).await.ok().flatten() {
        app.crash_fix_attempts += 1;
        let _ = state.app_service.update_crash_fix_attempts(&app.id, app.crash_fix_attempts).await;
    }

    let Some(crash_msg) = state.prompts.read_with_vars(
        "APP_CRASH.md",
        &[("app_name", &app.name), ("app_id", &app.id)],
    ) else {
        return;
    };

    let _ = state
        .chat_service
        .save_system_message(&app.chat_id, crash_msg, false)
        .await;

    let state = state.clone();
    let user_id = app.user_id.clone();
    let chat_id = app.chat_id.clone();
    tokio::spawn(async move {
        crate::agent::task::executor::resume_or_notify(&state, &user_id, &chat_id).await;
    });
}

async fn send_app_notification(
    state: &AppState,
    user_id: &str,
    app_id: &str,
    action: &str,
    level: NotificationLevel,
    title: &str,
    body: &str,
) {
    if let Ok(notification) = state
        .notification_service
        .create(
            user_id,
            crate::notification::models::NotificationData::App {
                app_id: app_id.to_string(),
                action: action.to_string(),
            },
            level,
            title.to_string(),
            body.to_string(),
        )
        .await
    {
        state.broadcast_service.send_notification(user_id, notification);
    }
}
