use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

use axum::response::sse::Event;
use tokio::sync::{RwLock, mpsc};

use crate::inference::tool_execution::ToolExecutionResponse;
use crate::inference::tool_loop::InferenceEventKind;
use crate::notification::models::Notification;

use super::message::models::MessageResponse;

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum BroadcastEventKind {
    // Inference streaming — wraps InferenceEventKind directly
    Inference(InferenceEventKind),

    // Inference lifecycle (sent after saving messages)
    InferenceDone { message: MessageResponse },
    InferenceCancelled { reason: String },
    InferenceError { error: String },
    ToolMessage { message: MessageResponse },
    ToolResolved { message: MessageResponse },
    ToolExecution { tool_execution: ToolExecutionResponse },
    ToolExecutionResolved { tool_execution: ToolExecutionResponse },
    Title { title: String },

    // Notifications (user-level, not chat-scoped)
    NewNotification { notification: Notification },

    // Existing broadcast events
    ChatMessage { message: MessageResponse },
    TaskUpdate {
        task_id: String,
        status: String,
        title: String,
        chat_id: Option<String>,
        source_chat_id: Option<String>,
        result_summary: Option<String>,
    },
    InferenceCount { count: usize },
}

#[derive(Debug, Clone)]
pub struct BroadcastEvent {
    pub user_id: String,
    pub chat_id: Option<String>,
    pub kind: BroadcastEventKind,
}

type SseSender = mpsc::UnboundedSender<Result<Event, Infallible>>;
type SessionRegistry = Arc<RwLock<HashMap<String, Vec<SseSender>>>>;

/// Pre-serialized event ready for the dispatcher to route.
struct DispatchEvent {
    user_id: String,
    is_global: bool,
    sse: Event,
}

#[derive(Clone)]
pub struct EventSender {
    tx: mpsc::UnboundedSender<DispatchEvent>,
    user_id: String,
    chat_id: String,
}

impl EventSender {
    pub fn send(&self, event: crate::inference::tool_loop::InferenceEvent) {
        if matches!(
            event.kind,
            InferenceEventKind::Done(_) | InferenceEventKind::Cancelled(_)
        ) {
            return;
        }
        let broadcast = BroadcastEvent {
            user_id: self.user_id.clone(),
            chat_id: Some(self.chat_id.clone()),
            kind: BroadcastEventKind::Inference(event.kind),
        };
        if let Some(sse) = map_event_to_sse(&broadcast) {
            let _ = self.tx.send(DispatchEvent {
                user_id: broadcast.user_id,
                is_global: false,
                sse,
            });
        }
    }

    pub fn send_kind(&self, kind: BroadcastEventKind) {
        let broadcast = BroadcastEvent {
            user_id: self.user_id.clone(),
            chat_id: Some(self.chat_id.clone()),
            kind,
        };
        if let Some(sse) = map_event_to_sse(&broadcast) {
            let _ = self.tx.send(DispatchEvent {
                user_id: broadcast.user_id,
                is_global: false,
                sse,
            });
        }
    }

    pub fn chat_id(&self) -> &str {
        &self.chat_id
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }
}

#[derive(Clone)]
pub struct BroadcastService {
    tx: mpsc::UnboundedSender<DispatchEvent>,
    sessions: SessionRegistry,
}

impl Default for BroadcastService {
    fn default() -> Self {
        Self::new()
    }
}

fn sse_event(name: &str, data: impl serde::Serialize) -> Event {
    Event::default().event(name).json_data(data).unwrap()
}

fn map_event_to_sse(event: &BroadcastEvent) -> Option<Event> {
    match &event.kind {
        BroadcastEventKind::Inference(kind) => {
            let chat_id = event.chat_id.as_deref().unwrap_or("");
            match kind {
                InferenceEventKind::Text(text) => Some(sse_event(
                    "token",
                    serde_json::json!({ "chat_id": chat_id, "content": text }),
                )),
                InferenceEventKind::ToolCall { id, name, arguments, description } => Some(sse_event(
                    "tool_call",
                    serde_json::json!({
                        "chat_id": chat_id,
                        "id": id,
                        "name": name,
                        "arguments": arguments,
                        "description": description,
                    }),
                )),
                InferenceEventKind::Reasoning(text) => Some(sse_event(
                    "reasoning",
                    serde_json::json!({ "chat_id": chat_id, "content": text }),
                )),
                InferenceEventKind::ToolResult { name, result, success } => {
                    let summary: String = result.chars().take(200).collect();
                    Some(sse_event(
                        "tool_result",
                        serde_json::json!({ "chat_id": chat_id, "name": name, "success": success, "summary": summary }),
                    ))
                }
                InferenceEventKind::EntityUpdated { table, record_id, fields } => Some(sse_event(
                    "entity_updated",
                    serde_json::json!({
                        "chat_id": chat_id,
                        "table": table,
                        "record_id": record_id,
                        "fields": fields,
                    }),
                )),
                InferenceEventKind::Retry { retry_after_ms, reason } => Some(sse_event(
                    "retry",
                    serde_json::json!({
                        "chat_id": chat_id,
                        "retry_after_secs": retry_after_ms / 1000,
                        "reason": reason,
                    }),
                )),
                InferenceEventKind::Error(err) => Some(sse_event(
                    "inference_error",
                    serde_json::json!({ "chat_id": chat_id, "error": err }),
                )),
                InferenceEventKind::Done(_) | InferenceEventKind::Cancelled(_) => None,
            }
        }
        BroadcastEventKind::InferenceDone { message } => {
            let chat_id = event.chat_id.as_deref().unwrap_or("");
            Some(sse_event(
                "inference_done",
                serde_json::json!({ "chat_id": chat_id, "message": message }),
            ))
        }
        BroadcastEventKind::InferenceCancelled { reason } => {
            let chat_id = event.chat_id.as_deref().unwrap_or("");
            Some(sse_event(
                "inference_cancelled",
                serde_json::json!({ "chat_id": chat_id, "reason": reason }),
            ))
        }
        BroadcastEventKind::InferenceError { error } => {
            let chat_id = event.chat_id.as_deref().unwrap_or("");
            Some(sse_event(
                "inference_error",
                serde_json::json!({ "chat_id": chat_id, "error": error }),
            ))
        }
        BroadcastEventKind::ToolMessage { message } => {
            let chat_id = event.chat_id.as_deref().unwrap_or("");
            Some(sse_event(
                "tool_message",
                serde_json::json!({ "chat_id": chat_id, "message": message }),
            ))
        }
        BroadcastEventKind::ToolResolved { message } => {
            let chat_id = event.chat_id.as_deref().unwrap_or("");
            Some(sse_event(
                "tool_resolved",
                serde_json::json!({ "chat_id": chat_id, "message": message }),
            ))
        }
        BroadcastEventKind::ToolExecution { tool_execution } => {
            let chat_id = event.chat_id.as_deref().unwrap_or("");
            Some(sse_event(
                "tool_message",
                serde_json::json!({ "chat_id": chat_id, "tool_execution": tool_execution }),
            ))
        }
        BroadcastEventKind::ToolExecutionResolved { tool_execution } => {
            let chat_id = event.chat_id.as_deref().unwrap_or("");
            Some(sse_event(
                "tool_resolved",
                serde_json::json!({ "chat_id": chat_id, "tool_execution": tool_execution }),
            ))
        }
        BroadcastEventKind::Title { title } => {
            let chat_id = event.chat_id.as_deref().unwrap_or("");
            Some(sse_event(
                "title",
                serde_json::json!({ "chat_id": chat_id, "title": title }),
            ))
        }
        BroadcastEventKind::ChatMessage { message } => {
            let chat_id = event.chat_id.as_deref().unwrap_or("");
            Some(sse_event(
                "chat_message",
                serde_json::json!({ "chat_id": chat_id, "message": message }),
            ))
        }
        BroadcastEventKind::TaskUpdate {
            task_id,
            status,
            title,
            chat_id,
            source_chat_id,
            result_summary,
        } => Some(sse_event(
            "task_update",
            serde_json::json!({
                "task_id": task_id,
                "status": status,
                "title": title,
                "chat_id": chat_id,
                "source_chat_id": source_chat_id,
                "result_summary": result_summary,
            }),
        )),
        BroadcastEventKind::InferenceCount { count } => Some(sse_event(
            "inference_count",
            serde_json::json!({ "count": count }),
        )),
        BroadcastEventKind::NewNotification { notification } => Some(sse_event(
            "notification",
            serde_json::json!({ "notification": notification }),
        )),
    }
}

impl BroadcastService {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let sessions: SessionRegistry = Arc::new(RwLock::new(HashMap::new()));

        let sessions_clone = sessions.clone();
        tokio::spawn(async move {
            Self::run_dispatcher(rx, sessions_clone).await;
        });

        Self { tx, sessions }
    }

    async fn run_dispatcher(
        mut rx: mpsc::UnboundedReceiver<DispatchEvent>,
        sessions: SessionRegistry,
    ) {
        while let Some(event) = rx.recv().await {
            let registry = sessions.read().await;
            if event.is_global {
                for senders in registry.values() {
                    for sender in senders {
                        let _ = sender.send(Ok(event.sse.clone()));
                    }
                }
            } else if let Some(senders) = registry.get(&event.user_id) {
                let mut dead_indices = Vec::new();
                for (i, sender) in senders.iter().enumerate() {
                    if sender.send(Ok(event.sse.clone())).is_err() {
                        dead_indices.push(i);
                    }
                }
                if !dead_indices.is_empty() {
                    drop(registry);
                    let mut registry = sessions.write().await;
                    if let Some(senders) = registry.get_mut(&event.user_id) {
                        for i in dead_indices.into_iter().rev() {
                            if i < senders.len() {
                                senders.swap_remove(i);
                            }
                        }
                        if senders.is_empty() {
                            registry.remove(&event.user_id);
                        }
                    }
                }
            }
        }
    }

    fn dispatch(&self, event: BroadcastEvent) {
        let is_global = matches!(event.kind, BroadcastEventKind::InferenceCount { .. });
        if let Some(sse) = map_event_to_sse(&event) {
            let _ = self.tx.send(DispatchEvent {
                user_id: event.user_id,
                is_global,
                sse,
            });
        }
    }

    pub fn create_event_sender(&self, user_id: &str, chat_id: &str) -> EventSender {
        EventSender {
            tx: self.tx.clone(),
            user_id: user_id.to_string(),
            chat_id: chat_id.to_string(),
        }
    }

    pub fn register_session(
        &self,
        user_id: &str,
        sender: SseSender,
    ) {
        let sessions = self.sessions.clone();
        let user_id = user_id.to_string();
        tokio::spawn(async move {
            let mut registry = sessions.write().await;
            registry.entry(user_id).or_default().push(sender);
        });
    }

    pub fn send(&self, event: BroadcastEvent) {
        self.dispatch(event);
    }

    pub fn broadcast_chat_message(
        &self,
        user_id: &str,
        chat_id: &str,
        message: MessageResponse,
    ) {
        self.dispatch(BroadcastEvent {
            user_id: user_id.to_string(),
            chat_id: Some(chat_id.to_string()),
            kind: BroadcastEventKind::ChatMessage { message },
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn broadcast_task_update(
        &self,
        user_id: &str,
        task_id: &str,
        status: &str,
        title: &str,
        chat_id: Option<&str>,
        source_chat_id: Option<&str>,
        result_summary: Option<&str>,
    ) {
        self.dispatch(BroadcastEvent {
            user_id: user_id.to_string(),
            chat_id: None,
            kind: BroadcastEventKind::TaskUpdate {
                task_id: task_id.to_string(),
                status: status.to_string(),
                title: title.to_string(),
                chat_id: chat_id.map(|s| s.to_string()),
                source_chat_id: source_chat_id.map(|s| s.to_string()),
                result_summary: result_summary.map(|s| s.to_string()),
            },
        });
    }

    pub fn send_notification(&self, user_id: &str, notification: Notification) {
        self.dispatch(BroadcastEvent {
            user_id: user_id.to_string(),
            chat_id: None,
            kind: BroadcastEventKind::NewNotification { notification },
        });
    }

    pub fn broadcast_inference_count(&self, count: usize) {
        self.dispatch(BroadcastEvent {
            user_id: String::new(),
            chat_id: None,
            kind: BroadcastEventKind::InferenceCount { count },
        });
    }
}
