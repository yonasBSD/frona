//! HITL types and resolve dispatcher. Channel-agnostic; rendering and
//! callback parsing live in `chat::channel::hitl`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;
use tokio_util::sync::CancellationToken;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::credential::vault::models::GrantDuration;
use crate::inference::request::InferenceContext;
use crate::inference::tool_call::{ToolCall, ToolStatus};

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub struct Hitl {
    pub prompt: String,
    /// Web frontend fallback URL — channels that can't render the affordance
    /// natively post this so the user can resolve via web.
    pub url: String,
    pub request: HitlRequest,
    pub status: ToolStatus,
    /// `None` iff `status == Pending`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<HitlResponse>,
    /// Delivery cursor uses this for retry idempotency (skips already-rendered
    /// HITLs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<HitlDelivery>,
}

/// Channels project this to `HitlKind` via `chat::channel::hitl::kind_for`
/// rather than matching directly, so new variants only need a `kind_for` arm.
#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
#[serde(tag = "type", content = "data")]
#[surreal(crate = "surrealdb::types", tag = "type", content = "data")]
pub enum HitlRequest {
    Question { options: Vec<String> },
    Takeover {
        reason: String,
        debugger_url: String,
    },
    App {
        action: String,
        manifest: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        previous_manifest: Option<serde_json::Value>,
    },
    Credential {
        query: String,
        reason: String,
    },
}

/// Channels can only emit `Approval` or `Choice` — the shapes a button tap
/// or text reply can carry. Variants beyond those are web-frontend-only.
#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
#[serde(tag = "type", content = "data")]
#[surreal(crate = "surrealdb::types", tag = "type", content = "data")]
pub enum HitlResponse {
    Approval(bool),
    Choice(String),
    Vault(VaultGrant),
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
#[serde(tag = "type", content = "data")]
#[surreal(crate = "surrealdb::types", tag = "type", content = "data")]
pub enum VaultGrant {
    Granted {
        connection_id: String,
        vault_item_id: String,
        grant_duration: GrantDuration,
        target: crate::credential::vault::models::CredentialTarget,
    },
    Denied,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub struct HitlDelivery {
    pub channel_id: String,
    /// Provider-specific (Telegram `message_id`, SMS `MessageSid`, etc.).
    /// Used for editing the original prompt on resolution.
    pub external_message_id: String,
    pub delivered_at: DateTime<Utc>,
}

/// Synthesized result text persisted as `te.result` — what the LLM sees on
/// resume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HitlOutcome {
    Resolved(String),
    Denied(String),
}

/// `AlreadyResolved` is the idempotent path — callers can render
/// "already resolved" UX without raising an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveOutcome {
    Resolved,
    AlreadyResolved,
}

/// Idempotent — returns `AlreadyResolved` if the HITL was already resolved
/// or denied. On the resolve path: invokes the tool's `on_resume` hook,
/// persists the outcome, broadcasts `Inference(Resume)`, and spawns
/// `resume_or_notify` once the per-message barrier clears.
pub async fn resolve_hitl(
    state: &AppState,
    tool_call_id: &str,
    response: HitlResponse,
) -> Result<ResolveOutcome, AppError> {
    let te = state
        .chat_service
        .get_tool_call(tool_call_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("tool_call {tool_call_id}")))?;

    let hitl = te
        .hitl
        .as_ref()
        .ok_or_else(|| AppError::Validation(format!("tool_call {tool_call_id} has no HITL")))?;

    if matches!(hitl.status, ToolStatus::Resolved | ToolStatus::Denied) {
        return Ok(ResolveOutcome::AlreadyResolved);
    }

    let tool = state
        .tool_manager
        .find_tool_for_resume(&te.name)
        .ok_or_else(|| {
            AppError::Validation(format!(
                "no tool registered to handle resume for '{}'",
                te.name
            ))
        })?;

    let ctx = build_inference_context_for_resume(state, &te).await?;
    let request = hitl.request.clone();
    let outcome = tool
        .on_resume(&te.name, &request, response.clone(), &ctx)
        .await?;

    let resolved_message = match outcome {
        HitlOutcome::Resolved(text) => {
            state
                .chat_service
                .resolve_tool_call_with_hitl_response(tool_call_id, Some(text), Some(response))
                .await?
        }
        HitlOutcome::Denied(text) => {
            state
                .chat_service
                .deny_tool_call_with_hitl_response(tool_call_id, Some(text), Some(response))
                .await?
        }
    };

    let message_response = match resolved_message {
        crate::chat::service::ToolResolveResult::Changed(m)
        | crate::chat::service::ToolResolveResult::AlreadyResolved(m) => m,
    };

    state.broadcast_service.send(crate::chat::broadcast::BroadcastEvent {
        user_id: ctx.user.id.clone(),
        chat_id: Some(te.chat_id.clone()),
        space_id: ctx.chat.space_id.clone(),
        kind: crate::chat::broadcast::BroadcastEventKind::Inference(
            crate::inference::tool_loop::InferenceEventKind::Resume {
                message: message_response,
            },
        ),
    });

    let message_id = te.message_id.clone();
    let did_flip = state
        .chat_service
        .mark_message_executing(&message_id)
        .await
        .unwrap_or(false);
    if did_flip {
        let state_clone = state.clone();
        let user_id = ctx.user.id.clone();
        let chat_id = te.chat_id.clone();
        tokio::spawn(async move {
            crate::agent::task::executor::resume_or_notify(
                &state_clone,
                &user_id,
                &chat_id,
                &message_id,
            )
            .await;
        });
    }

    Ok(ResolveOutcome::Resolved)
}

/// Builds a fresh `InferenceContext` for `on_resume` — never-fired cancel
/// tokens, no live event stream. `on_resume` MUST be pure side effects and
/// not depend on the original execute's context (which is gone by now).
pub async fn build_inference_context_for_resume(
    state: &AppState,
    te: &ToolCall,
) -> Result<InferenceContext, AppError> {
    let chat = state
        .chat_service
        .find_chat(&te.chat_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("chat {}", te.chat_id)))?;
    let user = state
        .user_service
        .find_by_id(&chat.user_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("user {}", chat.user_id)))?;
    let agent = state
        .agent_service
        .find_by_id(&chat.agent_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("agent {}", chat.agent_id)))?;

    let event_tx = state.broadcast_service.create_event_sender(
        &user.id,
        &te.chat_id,
        chat.space_id.clone(),
    );

    Ok(InferenceContext::new(
        user,
        agent,
        chat,
        event_tx,
        state.shutdown_token.clone(),
        CancellationToken::new(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credential::vault::models::GrantDuration;

    #[test]
    fn hitl_request_question_round_trip() {
        let req = HitlRequest::Question {
            options: vec!["a".into(), "b".into()],
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: HitlRequest = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, HitlRequest::Question { options } if options == vec!["a", "b"]));
    }

    #[test]
    fn hitl_request_takeover_round_trip() {
        let req = HitlRequest::Takeover {
            reason: "manual debug".into(),
            debugger_url: "https://example/d/1".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: HitlRequest = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            back,
            HitlRequest::Takeover { ref reason, ref debugger_url }
            if reason == "manual debug" && debugger_url == "https://example/d/1"
        ));
    }

    #[test]
    fn hitl_request_service_approval_round_trip() {
        let req = HitlRequest::App {
            action: "deploy".into(),
            manifest: serde_json::json!({"handle": "notes", "name": "Notes"}),
            previous_manifest: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: HitlRequest = serde_json::from_str(&json).unwrap();
        match back {
            HitlRequest::App { action, manifest, .. } => {
                assert_eq!(action, "deploy");
                assert_eq!(manifest["handle"], "notes");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn hitl_request_vault_pick_round_trip() {
        let req = HitlRequest::Credential {
            query: "postgres-prod".into(),
            reason: "ETL job".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: HitlRequest = serde_json::from_str(&json).unwrap();
        match back {
            HitlRequest::Credential { query, reason } => {
                assert_eq!(query, "postgres-prod");
                assert_eq!(reason, "ETL job");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn hitl_response_approval_round_trip() {
        let r = HitlResponse::Approval(true);
        let json = serde_json::to_string(&r).unwrap();
        let back: HitlResponse = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, HitlResponse::Approval(true)));
    }

    #[test]
    fn hitl_response_choice_round_trip() {
        let r = HitlResponse::Choice("staging".into());
        let json = serde_json::to_string(&r).unwrap();
        let back: HitlResponse = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, HitlResponse::Choice(s) if s == "staging"));
    }

    #[test]
    fn hitl_response_vault_granted_round_trip() {
        use crate::credential::vault::models::CredentialTarget;
        let r = HitlResponse::Vault(VaultGrant::Granted {
            connection_id: "conn-1".into(),
            vault_item_id: "item-1".into(),
            grant_duration: GrantDuration::Once,
            target: CredentialTarget::Prefix { env_var_prefix: "DB".into() },
        });
        let json = serde_json::to_string(&r).unwrap();
        let back: HitlResponse = serde_json::from_str(&json).unwrap();
        match back {
            HitlResponse::Vault(VaultGrant::Granted {
                connection_id,
                vault_item_id,
                target,
                ..
            }) => {
                use crate::credential::vault::models::CredentialTarget;
                assert_eq!(connection_id, "conn-1");
                assert_eq!(vault_item_id, "item-1");
                match target {
                    CredentialTarget::Prefix { env_var_prefix } => assert_eq!(env_var_prefix, "DB"),
                    _ => panic!("expected Prefix target"),
                }
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn hitl_response_vault_denied_round_trip() {
        let r = HitlResponse::Vault(VaultGrant::Denied);
        let json = serde_json::to_string(&r).unwrap();
        let back: HitlResponse = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, HitlResponse::Vault(VaultGrant::Denied)));
    }

    #[test]
    fn hitl_delivery_round_trip() {
        let d = HitlDelivery {
            channel_id: "ch-1".into(),
            external_message_id: "42".into(),
            delivered_at: Utc::now(),
        };
        let json = serde_json::to_string(&d).expect("serialize");
        let back: HitlDelivery = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.channel_id, "ch-1");
        assert_eq!(back.external_message_id, "42");
    }

    #[test]
    fn hitl_struct_with_response_and_delivery_round_trip() {
        let h = Hitl {
            prompt: "Pick a region?".into(),
            url: "https://app/chats/abc".into(),
            request: HitlRequest::Question {
                options: vec!["us".into(), "eu".into()],
            },
            status: ToolStatus::Resolved,
            response: Some(HitlResponse::Choice("us".into())),
            delivery: Some(HitlDelivery {
                channel_id: "ch-1".into(),
                external_message_id: "42".into(),
                delivered_at: Utc::now(),
            }),
        };
        let json = serde_json::to_string(&h).expect("serialize");
        let back: Hitl = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.prompt, "Pick a region?");
        assert!(matches!(back.status, ToolStatus::Resolved));
        assert!(back.response.is_some());
        assert!(back.delivery.is_some());
    }

}
