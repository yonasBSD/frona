use rig::completion::{AssistantContent, Message as RigMessage};

use crate::chat::message::models::Message;
use crate::chat::message::models::MessageRole;

pub fn to_rig_messages(messages: &[Message], chat_agent_id: &str) -> Vec<RigMessage> {
    messages
        .iter()
        .filter_map(|msg| match msg.role {
            MessageRole::User => Some(RigMessage::user(&msg.content)),
            MessageRole::Agent => {
                let is_self = msg.agent_id.as_deref() == Some(chat_agent_id);
                if is_self {
                    if let Some(tool_calls_val) = &msg.tool_calls
                        && let Some(calls) = tool_calls_val.as_array()
                    {
                            let mut items: Vec<AssistantContent> = Vec::new();
                            if !msg.content.is_empty() {
                                items.push(AssistantContent::text(&msg.content));
                            }
                            for call in calls {
                                let id = call["id"].as_str().unwrap_or_default();
                                let name = call["name"].as_str().unwrap_or_default();
                                let arguments = call.get("arguments").cloned().unwrap_or_default();
                                items.push(AssistantContent::tool_call(id, name, arguments));
                            }
                            if items.is_empty() {
                                return None;
                            }
                            if let Ok(content) = rig::OneOrMany::many(items) {
                                return Some(RigMessage::Assistant { id: None, content });
                            }
                    }
                    Some(RigMessage::assistant(&msg.content))
                } else {
                    Some(RigMessage::user(&msg.content))
                }
            }
            MessageRole::ToolResult => {
                let tool_call_id = msg.tool_call_id.as_deref().unwrap_or_default();
                Some(RigMessage::tool_result(tool_call_id, &msg.content))
            }
            MessageRole::TaskCompletion => Some(RigMessage::user(&msg.content)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crate::agent::task::models::TaskStatus;
    use crate::chat::message::models::{MessageRole, MessageTool};

    fn make_message(role: MessageRole, content: &str) -> Message {
        Message {
            id: "m1".to_string(),
            chat_id: "c1".to_string(),
            role,
            content: content.to_string(),
            agent_id: None,
            tool_calls: None,
            tool_call_id: None,
            tool: None,
            created_at: Utc::now(),
        }
    }

    fn make_agent_message(content: &str, agent_id: &str) -> Message {
        Message {
            agent_id: Some(agent_id.to_string()),
            ..make_message(MessageRole::Agent, content)
        }
    }

    #[test]
    fn agent_same_id_converts_to_assistant() {
        let msg = make_agent_message("hello", "agent-1");
        let result = to_rig_messages(&[msg], "agent-1");
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], RigMessage::Assistant { .. }));
    }

    #[test]
    fn agent_different_id_converts_to_user() {
        let msg = make_agent_message("task instruction", "agent-2");
        let result = to_rig_messages(&[msg], "agent-1");
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], RigMessage::User { .. }));
    }

    #[test]
    fn agent_with_tool_calls_converts_to_assistant_with_tools() {
        let tool_calls = serde_json::json!([{
            "id": "tc-1",
            "name": "web_search",
            "arguments": {"query": "test"}
        }]);
        let msg = Message {
            agent_id: Some("agent-1".to_string()),
            tool_calls: Some(tool_calls),
            ..make_message(MessageRole::Agent, "searching...")
        };
        let result = to_rig_messages(&[msg], "agent-1");
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], RigMessage::Assistant { .. }));
    }

    #[test]
    fn task_completion_converts_to_user_message() {
        let msg = Message {
            tool: Some(MessageTool::TaskCompletion {
                task_id: "t1".to_string(),
                chat_id: Some("c2".to_string()),
                status: TaskStatus::Completed,
            }),
            ..make_message(MessageRole::TaskCompletion, "Task 'research' completed.")
        };

        let result = to_rig_messages(&[msg], "agent-1");
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], RigMessage::User { .. }));
    }

    #[test]
    fn task_completion_not_converted_to_tool_result() {
        let msg = Message {
            tool: Some(MessageTool::TaskCompletion {
                task_id: "t1".to_string(),
                chat_id: None,
                status: TaskStatus::Failed,
            }),
            ..make_message(MessageRole::TaskCompletion, "Task 'deploy' failed: timeout")
        };

        let result = to_rig_messages(&[msg], "agent-1");
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], RigMessage::User { .. }));
    }

    #[test]
    fn mixed_messages_preserve_order() {
        let messages = vec![
            make_message(MessageRole::User, "do something"),
            make_agent_message("delegating...", "agent-1"),
            Message {
                tool: Some(MessageTool::TaskCompletion {
                    task_id: "t1".to_string(),
                    chat_id: Some("c2".to_string()),
                    status: TaskStatus::Completed,
                }),
                ..make_message(MessageRole::TaskCompletion, "Task completed.")
            },
        ];

        let result = to_rig_messages(&messages, "agent-1");
        assert_eq!(result.len(), 3);
        assert!(matches!(result[0], RigMessage::User { .. }));
        assert!(matches!(result[1], RigMessage::Assistant { .. }));
        assert!(matches!(result[2], RigMessage::User { .. }));
    }
}
