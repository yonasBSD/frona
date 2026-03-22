use serde_json::Value;

use crate::agent::prompt::PromptLoader;
use crate::app::models::{App, AppManifest, AppResponse};
use crate::app::service::AppService;
use crate::chat::broadcast::BroadcastService;
use crate::inference::tool_execution::{MessageTool, ToolStatus};
use crate::core::error::AppError;
use crate::notification::models::{NotificationData, NotificationLevel};
use crate::notification::service::NotificationService;

use frona_derive::agent_tool;

use super::{InferenceContext, ToolOutput};

pub struct ManageServiceTool {
    app_service: AppService,
    prompts: PromptLoader,
    notification_service: NotificationService,
    broadcast_service: BroadcastService,
}

impl ManageServiceTool {
    pub fn new(
        app_service: AppService,
        prompts: PromptLoader,
        notification_service: NotificationService,
        broadcast_service: BroadcastService,
    ) -> Self {
        Self {
            app_service,
            prompts,
            notification_service,
            broadcast_service,
        }
    }
}

#[agent_tool]
impl ManageServiceTool {
    async fn execute(
        &self,
        _tool_name: &str,
        arguments: Value,
        ctx: &InferenceContext,
    ) -> Result<ToolOutput, AppError> {
        let action = arguments
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("Missing required parameter: action".into()))?
            .to_string();

        let manifest_value = arguments.get("manifest").cloned();

        match action.as_str() {
            "status" => self.handle_status(ctx, manifest_value).await,
            "deploy" => self.handle_deploy(ctx, manifest_value).await,
            "stop" => self.handle_stop(ctx, manifest_value).await,
            "start" => self.handle_start(ctx, manifest_value).await,
            "restart" => self.handle_restart(ctx, manifest_value).await,
            "destroy" => self.handle_destroy(ctx, manifest_value).await,
            _ => Err(AppError::Validation(format!(
                "Unknown action: {action}. Valid actions: deploy, stop, start, restart, destroy, status"
            ))),
        }
    }
}

impl ManageServiceTool {
    async fn handle_status(
        &self,
        ctx: &InferenceContext,
        manifest_value: Option<Value>,
    ) -> Result<ToolOutput, AppError> {
        let agent_id = &ctx.agent.id;
        let apps = self.app_service.list(agent_id).await?;

        if let Some(ref mv) = manifest_value
            && let Some(manifest_id) = mv.get("id").and_then(|v| v.as_str())
        {
            if let Some(app) = apps.iter().find(|a| {
                a.manifest
                    .get("id")
                    .and_then(|v| v.as_str())
                    .is_some_and(|id| id == manifest_id)
            }) {
                return Ok(ToolOutput::text(serde_json::to_string_pretty(app).unwrap_or_default()));
            }
            return Ok(ToolOutput::text(format!(
                "No app found with id '{manifest_id}'"
            )));
        }

        if apps.is_empty() {
            return Ok(ToolOutput::text("No apps deployed for this agent."));
        }
        Ok(ToolOutput::text(
            serde_json::to_string_pretty(&apps).unwrap_or_default(),
        ))
    }

    async fn handle_deploy(
        &self,
        ctx: &InferenceContext,
        manifest_value: Option<Value>,
    ) -> Result<ToolOutput, AppError> {
        let manifest_value = manifest_value
            .ok_or_else(|| AppError::Validation("manifest is required for deploy".into()))?;

        let manifest: AppManifest = serde_json::from_value(manifest_value.clone())
            .map_err(|e| AppError::Validation(format!("Invalid manifest: {e}")))?;

        let existing = self.app_service.find_by_manifest_id(&ctx.agent.id, &manifest.id).await?;

        let needs_approval = check_needs_approval(&existing, &manifest_value);

        if needs_approval {
            let previous = existing.map(|a| a.manifest);

            return Ok(ToolOutput::text(serde_json::json!({
                "tool_type": "ServiceApproval",
                "action": "deploy",
                "manifest": manifest_value,
            }).to_string())
            .with_tool_data(MessageTool::ServiceApproval {
                action: "deploy".to_string(),
                manifest: manifest_value,
                previous_manifest: previous,
                status: ToolStatus::Pending,
                response: None,
            })
            .as_pending_external());
        }

        if let Some(ref existing) = existing {
            let app = self
                .app_service
                .restart(&ctx.agent.id, &existing.id, &ctx.chat.id)
                .await?;
            return Ok(ToolOutput::text(format_app_result("restarted", &app)));
        }

        let app = self
            .app_service
            .deploy_and_await(&ctx.agent.id, &ctx.user.id, &ctx.chat.id, &manifest, Vec::new())
            .await?;

        Ok(ToolOutput::text(format_app_result("deployed successfully", &app)))
    }

    async fn handle_stop(
        &self,
        ctx: &InferenceContext,
        manifest_value: Option<Value>,
    ) -> Result<ToolOutput, AppError> {
        let app_id = self.resolve_app_id(ctx, manifest_value.as_ref()).await?;

        let app = self.app_service.stop(&ctx.agent.id, &app_id, &ctx.chat.id).await?;
        self.emit_notification(ctx, &app_id, "stop", NotificationLevel::Info, &format!("App '{}' stopped", app.name)).await;
        Ok(ToolOutput::text(format!(
            "App '{}' stopped. Status: {}",
            app.name, app.status
        )))
    }

    async fn handle_start(
        &self,
        ctx: &InferenceContext,
        manifest_value: Option<Value>,
    ) -> Result<ToolOutput, AppError> {
        let app_id = self.resolve_app_id(ctx, manifest_value.as_ref()).await?;

        let app = self
            .app_service
            .start(&ctx.agent.id, &app_id, &ctx.chat.id, Vec::new())
            .await?;

        self.emit_notification(ctx, &app_id, "start", NotificationLevel::Success, &format!("App '{}' started", app.name)).await;
        Ok(ToolOutput::text(format_app_result("started", &app)))
    }

    async fn handle_restart(
        &self,
        ctx: &InferenceContext,
        manifest_value: Option<Value>,
    ) -> Result<ToolOutput, AppError> {
        let app_id = self.resolve_app_id(ctx, manifest_value.as_ref()).await?;

        let app = self.app_service.restart(&ctx.agent.id, &app_id, &ctx.chat.id).await?;

        self.emit_notification(ctx, &app_id, "restart", NotificationLevel::Info, &format!("App '{}' restarted", app.name)).await;
        Ok(ToolOutput::text(format_app_result("restarted", &app)))
    }

    async fn handle_destroy(
        &self,
        ctx: &InferenceContext,
        manifest_value: Option<Value>,
    ) -> Result<ToolOutput, AppError> {
        let app_id = self.resolve_app_id(ctx, manifest_value.as_ref()).await?;

        let app_name = self
            .app_service
            .get(&app_id)
            .await?
            .map(|a| a.name)
            .unwrap_or_default();

        self.app_service.destroy(&ctx.agent.id, &app_id).await?;

        Ok(ToolOutput::text(format!("App '{app_name}' destroyed.")))
    }

    async fn emit_notification(
        &self,
        ctx: &InferenceContext,
        app_id: &str,
        action: &str,
        level: NotificationLevel,
        title: &str,
    ) {
        if let Ok(notification) = self
            .notification_service
            .create(
                &ctx.user.id,
                NotificationData::App {
                    app_id: app_id.to_string(),
                    action: action.to_string(),
                },
                level,
                title.to_string(),
                String::new(),
            )
            .await
        {
            self.broadcast_service.send_notification(&ctx.user.id, notification);
        }
    }

    async fn resolve_app_id(
        &self,
        ctx: &InferenceContext,
        manifest_value: Option<&Value>,
    ) -> Result<String, AppError> {
        let manifest_id = manifest_value
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AppError::Validation("manifest.id is required to identify the app".into())
            })?;

        self.app_service
            .find_by_manifest_id(&ctx.agent.id, manifest_id)
            .await?
            .map(|a| a.id)
            .ok_or_else(|| {
                AppError::NotFound(format!("No app found with manifest id '{manifest_id}'"))
            })
    }
}

pub fn format_app_result(action: &str, app: &AppResponse) -> String {
    format!("App '{}' {action}. Status: {}", app.name, app.status)
}

fn check_needs_approval(existing: &Option<App>, manifest_value: &Value) -> bool {
    let Some(app) = existing else {
        return true;
    };

    let (Ok(old), Ok(new)) = (
        serde_json::from_value::<AppManifest>(app.manifest.clone()),
        serde_json::from_value::<AppManifest>(manifest_value.clone()),
    ) else {
        return true;
    };

    old.command != new.command
        || old.effective_kind() != new.effective_kind()
        || old.static_dir != new.static_dir
        || old.effective_expose() != new.effective_expose()
        || old.network_destinations != new.network_destinations
        || old.credentials != new.credentials
        || old.read_paths != new.read_paths
        || old.write_paths != new.write_paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_app(manifest: serde_json::Value) -> App {
        let now = Utc::now();
        App {
            id: "app-1".to_string(),
            agent_id: "agent-1".to_string(),
            user_id: "user-1".to_string(),
            name: "Test".to_string(),
            description: None,
            kind: "service".to_string(),
            command: Some("python app.py".to_string()),
            static_dir: None,
            port: Some(4000),
            status: crate::app::models::AppStatus::Running,
            pid: Some(1234),
            manifest,
            chat_id: "test-chat".to_string(),
            crash_fix_attempts: 0,
            last_accessed_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn approval_required_for_new_app() {
        let manifest = serde_json::json!({"id": "test", "name": "Test", "command": "python app.py"});
        assert!(check_needs_approval(&None, &manifest));
    }

    #[test]
    fn no_approval_when_manifest_identical() {
        let manifest = serde_json::json!({"id": "test", "name": "Test", "command": "python app.py"});
        let app = make_app(manifest.clone());
        assert!(!check_needs_approval(&Some(app), &manifest));
    }

    #[test]
    fn no_approval_when_only_name_changes() {
        let old = serde_json::json!({"id": "test", "name": "Old Name", "command": "python app.py"});
        let new = serde_json::json!({"id": "test", "name": "New Name", "command": "python app.py"});
        let app = make_app(old);
        assert!(!check_needs_approval(&Some(app), &new));
    }

    #[test]
    fn no_approval_when_only_description_changes() {
        let old = serde_json::json!({"id": "test", "name": "Test", "command": "python app.py"});
        let new = serde_json::json!({"id": "test", "name": "Test", "command": "python app.py", "description": "new desc"});
        let app = make_app(old);
        assert!(!check_needs_approval(&Some(app), &new));
    }

    #[test]
    fn no_approval_when_only_health_check_changes() {
        let old = serde_json::json!({"id": "test", "name": "Test", "command": "python app.py"});
        let new = serde_json::json!({"id": "test", "name": "Test", "command": "python app.py", "health_check": {"path": "/healthz"}});
        let app = make_app(old);
        assert!(!check_needs_approval(&Some(app), &new));
    }

    #[test]
    fn approval_required_when_command_changes() {
        let old = serde_json::json!({"id": "test", "name": "Test", "command": "python app.py"});
        let new = serde_json::json!({"id": "test", "name": "Test", "command": "node server.js"});
        let app = make_app(old);
        assert!(check_needs_approval(&Some(app), &new));
    }

    #[test]
    fn approval_required_when_network_destinations_change() {
        let old = serde_json::json!({"id": "test", "name": "Test", "command": "python app.py"});
        let new = serde_json::json!({"id": "test", "name": "Test", "command": "python app.py", "network_destinations": [{"host": "evil.com", "port": 443}]});
        let app = make_app(old);
        assert!(check_needs_approval(&Some(app), &new));
    }

    #[test]
    fn approval_required_when_credentials_change() {
        let old = serde_json::json!({"id": "test", "name": "Test", "command": "python app.py"});
        let new = serde_json::json!({"id": "test", "name": "Test", "command": "python app.py", "credentials": [{"query": "api-key", "reason": "need it", "env_var_prefix": "API"}]});
        let app = make_app(old);
        assert!(check_needs_approval(&Some(app), &new));
    }

    #[test]
    fn approval_required_when_expose_changes() {
        let old = serde_json::json!({"id": "test", "name": "Test", "command": "python app.py", "expose": false});
        let new = serde_json::json!({"id": "test", "name": "Test", "command": "python app.py", "expose": true});
        let app = make_app(old);
        assert!(check_needs_approval(&Some(app), &new));
    }

    #[test]
    fn approval_required_when_kind_changes() {
        let old = serde_json::json!({"id": "test", "name": "Test", "command": "python app.py"});
        let new = serde_json::json!({"id": "test", "name": "Test", "kind": "static", "static_dir": "dist/"});
        let app = make_app(old);
        assert!(check_needs_approval(&Some(app), &new));
    }

    #[test]
    fn approval_required_when_stored_manifest_unparseable() {
        let old = serde_json::json!("not a valid manifest");
        let new = serde_json::json!({"id": "test", "name": "Test", "command": "python app.py"});
        let app = make_app(old);
        assert!(check_needs_approval(&Some(app), &new));
    }
}
