use std::sync::Arc;

use crate::core::config::AppConfig;
use crate::core::error::AppError;

use super::manager::AppManager;
use super::models::{App, AppManifest, AppResponse, AppStatus};
use super::repository::AppRepository;

#[derive(Clone)]
pub struct AppService {
    repo: Arc<dyn AppRepository>,
    manager: Arc<AppManager>,
    config: AppConfig,
}

impl AppService {
    pub fn new(repo: impl AppRepository + 'static, manager: Arc<AppManager>, config: AppConfig) -> Self {
        Self {
            repo: Arc::new(repo),
            manager,
            config,
        }
    }

    pub fn manager(&self) -> &Arc<AppManager> {
        &self.manager
    }

    pub async fn deploy(
        &self,
        agent_id: &str,
        user_id: &str,
        manifest: &AppManifest,
        credential_env_vars: Vec<(String, String)>,
    ) -> Result<AppResponse, AppError> {
        let existing = self.find_by_manifest_id(agent_id, &manifest.id).await?;

        let app_id = if let Some(ref existing) = existing {
            existing.id.clone()
        } else {
            manifest.id.clone()
        };

        let kind = manifest.effective_kind().to_string();
        let now = chrono::Utc::now();

        let manifest_json = serde_json::to_value(manifest)
            .map_err(|e| AppError::Validation(format!("Invalid manifest: {e}")))?;

        match kind.as_str() {
            "static" => {
                let static_dir = manifest
                    .static_dir
                    .as_ref()
                    .ok_or_else(|| {
                        AppError::Validation("static_dir required for static apps".into())
                    })?
                    .clone();

                let app = App {
                    id: app_id,
                    agent_id: agent_id.to_string(),
                    user_id: user_id.to_string(),
                    name: manifest.name.clone(),
                    description: manifest.description.clone(),
                    kind,
                    command: None,
                    static_dir: Some(static_dir),
                    port: None,
                    status: AppStatus::Serving,
                    pid: None,
                    manifest: manifest_json,
                    last_accessed_at: None,
                    created_at: existing.as_ref().map(|e| e.created_at).unwrap_or(now),
                    updated_at: now,
                };

                let app = if existing.is_some() {
                    self.repo.update(&app).await?
                } else {
                    self.repo.create(&app).await?
                };
                Ok(app.into())
            }
            _ => {
                let command = manifest
                    .command
                    .as_ref()
                    .ok_or_else(|| {
                        AppError::Validation("command required for service apps".into())
                    })?
                    .clone();

                if let Some(ref ex) = existing
                    && matches!(ex.status, AppStatus::Running | AppStatus::Starting)
                {
                    self.manager.stop_app(&ex.id).await?;
                }

                let mut app = App {
                    id: app_id,
                    agent_id: agent_id.to_string(),
                    user_id: user_id.to_string(),
                    name: manifest.name.clone(),
                    description: manifest.description.clone(),
                    kind: "service".to_string(),
                    command: Some(command.clone()),
                    static_dir: None,
                    port: None,
                    status: AppStatus::Starting,
                    pid: None,
                    manifest: manifest_json,
                    last_accessed_at: None,
                    created_at: existing.as_ref().map(|e| e.created_at).unwrap_or(now),
                    updated_at: now,
                };

                app = if existing.is_some() {
                    self.repo.update(&app).await?
                } else {
                    self.repo.create(&app).await?
                };

                self.start_and_update(&mut app, &command, manifest, credential_env_vars)
                    .await
            }
        }
    }

    pub async fn stop(&self, agent_id: &str, app_id: &str) -> Result<AppResponse, AppError> {
        let mut app = self.get_owned_app(agent_id, app_id).await?;

        self.manager.stop_app(app_id).await?;

        app.status = AppStatus::Stopped;
        app.pid = None;
        app.port = None;
        app.updated_at = chrono::Utc::now();
        let app = self.repo.update(&app).await?;
        Ok(app.into())
    }

    pub async fn start(
        &self,
        agent_id: &str,
        app_id: &str,
        credential_env_vars: Vec<(String, String)>,
    ) -> Result<AppResponse, AppError> {
        let mut app = self.get_owned_app(agent_id, app_id).await?;

        if matches!(app.status, AppStatus::Running | AppStatus::Starting) {
            return Ok(app.into());
        }

        let command = app
            .command
            .as_ref()
            .ok_or_else(|| AppError::Validation("No command for this app".into()))?
            .clone();

        let manifest: AppManifest = serde_json::from_value(app.manifest.clone())
            .map_err(|e| AppError::Internal(format!("Invalid stored manifest: {e}")))?;

        self.start_and_update(&mut app, &command, &manifest, credential_env_vars)
            .await
    }

    pub async fn restart(
        &self,
        agent_id: &str,
        app_id: &str,
    ) -> Result<AppResponse, AppError> {
        let mut app = self.get_owned_app(agent_id, app_id).await?;

        let command = app
            .command
            .as_ref()
            .ok_or_else(|| AppError::Validation("No command for this app".into()))?
            .clone();

        let manifest: AppManifest = serde_json::from_value(app.manifest.clone())
            .map_err(|e| AppError::Internal(format!("Invalid stored manifest: {e}")))?;

        self.manager.stop_app(app_id).await?;

        self.start_and_update(&mut app, &command, &manifest, Vec::new())
            .await
    }

    pub async fn destroy(&self, agent_id: &str, app_id: &str) -> Result<(), AppError> {
        let app = self.get_owned_app(agent_id, app_id).await?;

        if matches!(
            app.status,
            AppStatus::Running | AppStatus::Starting | AppStatus::Hibernated
        ) {
            self.manager.stop_app(app_id).await?;
        }

        self.repo.delete(app_id).await
    }

    pub async fn list(&self, agent_id: &str) -> Result<Vec<AppResponse>, AppError> {
        let apps = self.repo.find_by_agent_id(agent_id).await?;
        Ok(apps.into_iter().map(Into::into).collect())
    }

    pub async fn list_by_user(&self, user_id: &str) -> Result<Vec<AppResponse>, AppError> {
        let apps = self.repo.find_by_user_id(user_id).await?;
        Ok(apps.into_iter().map(Into::into).collect())
    }

    pub async fn get(&self, app_id: &str) -> Result<Option<App>, AppError> {
        self.repo.find_by_id(app_id).await
    }

    pub async fn get_by_user(
        &self,
        user_id: &str,
        app_id: &str,
    ) -> Result<AppResponse, AppError> {
        let app = self
            .repo
            .find_by_id(app_id)
            .await?
            .ok_or_else(|| AppError::NotFound("App not found".into()))?;
        if app.user_id != user_id {
            return Err(AppError::Forbidden("Not your app".into()));
        }
        Ok(app.into())
    }

    pub async fn update_status(
        &self,
        app_id: &str,
        status: AppStatus,
        port: Option<u16>,
        pid: Option<u32>,
    ) -> Result<(), AppError> {
        if let Some(mut app) = self.repo.find_by_id(app_id).await? {
            app.status = status;
            app.port = port;
            app.pid = pid;
            app.updated_at = chrono::Utc::now();
            self.repo.update(&app).await?;
        }
        Ok(())
    }

    pub async fn update_last_accessed(
        &self,
        app_id: &str,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), AppError> {
        if let Some(mut app) = self.repo.find_by_id(app_id).await? {
            app.last_accessed_at = Some(at);
            self.repo.update(&app).await?;
        }
        Ok(())
    }

    pub async fn find_running(&self) -> Result<Vec<App>, AppError> {
        self.repo.find_running().await
    }

    pub fn max_restart_attempts(&self) -> u32 {
        self.config.max_restart_attempts
    }

    pub fn hibernate_after_secs(&self) -> u64 {
        self.config.hibernate_after_secs
    }

    async fn get_owned_app(&self, agent_id: &str, app_id: &str) -> Result<App, AppError> {
        let app = self
            .repo
            .find_by_id(app_id)
            .await?
            .ok_or_else(|| AppError::NotFound("App not found".into()))?;
        if app.agent_id != agent_id {
            return Err(AppError::Forbidden("App not owned by this agent".into()));
        }
        Ok(app)
    }

    pub async fn find_by_manifest_id(
        &self,
        agent_id: &str,
        manifest_id: &str,
    ) -> Result<Option<App>, AppError> {
        let apps = self.repo.find_by_agent_id(agent_id).await?;
        let manifest_json_id = serde_json::Value::String(manifest_id.to_string());
        Ok(apps.into_iter().find(|a| {
            a.manifest
                .get("id")
                .is_some_and(|id| *id == manifest_json_id)
        }))
    }

    async fn start_and_update(
        &self,
        app: &mut App,
        command: &str,
        manifest: &AppManifest,
        credential_env_vars: Vec<(String, String)>,
    ) -> Result<AppResponse, AppError> {
        match self
            .manager
            .start_app(&app.id, &app.agent_id, command, manifest, credential_env_vars)
            .await
        {
            Ok((port, pid)) => {
                app.port = Some(port);
                app.pid = Some(pid);
                app.status = AppStatus::Running;
                app.updated_at = chrono::Utc::now();
                let app = self.repo.update(app).await?;
                Ok(app.into())
            }
            Err(e) => {
                app.status = AppStatus::Failed;
                app.updated_at = chrono::Utc::now();
                let _ = self.repo.update(app).await;
                Err(e)
            }
        }
    }
}
