use std::sync::Arc;

use frona::agent::models::Agent;
use frona::db::repo::generic::SurrealRepo;
use frona::policy::models::{Policy, PolicyAction, PolicyResource};
use frona::policy::repository::PolicyRepository;
use frona::policy::schema::build_schema;
use frona::policy::schema::extract_annotations;
use frona::policy::service::PolicyService;

use surrealdb::Surreal;
use surrealdb::engine::local::{Db, Mem};

async fn setup() -> (Surreal<Db>, PolicyService) {
    let db = Surreal::new::<Mem>(()).await.unwrap();
    frona::db::init::setup_schema(&db).await.unwrap();

    let schema = build_schema();
    let repo: Arc<dyn PolicyRepository> =
        Arc::new(SurrealRepo::<Policy>::new(db.clone()));
    let tool_manager = std::sync::Arc::new(frona::tool::manager::ToolManager::new(false));

    // Register a mock tool so delegation checks have tools to compare
    use frona::tool::{AgentTool, ToolDefinition, ToolOutput};
    struct MockTool {
        name: &'static str,
        defs: Vec<ToolDefinition>,
    }
    #[async_trait::async_trait]
    impl AgentTool for MockTool {
        fn name(&self) -> &str { self.name }
        fn definitions(&self) -> Vec<ToolDefinition> { self.defs.clone() }
        async fn execute(&self, _: &str, _: serde_json::Value, _: &frona::tool::InferenceContext) -> Result<ToolOutput, frona::core::error::AppError> {
            Ok(ToolOutput::text("ok"))
        }
    }
    fn mock_def(id: &str, group: &str) -> ToolDefinition {
        ToolDefinition {
            id: id.into(),
            provider_id: group.into(),
            description: id.into(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        }
    }
    tool_manager.register_user_tool("user-1", std::sync::Arc::new(MockTool {
        name: "browser",
        defs: vec![mock_def("browser_navigate", "browser")],
    })).await;
    tool_manager.register_user_tool("user-1", std::sync::Arc::new(MockTool {
        name: "voice",
        defs: vec![mock_def("make_voice_call", "voice"), mock_def("hangup_call", "voice")],
    })).await;
    tool_manager.register_user_tool("user-1", std::sync::Arc::new(MockTool {
        name: "search",
        defs: vec![mock_def("web_search", "search")],
    })).await;

    let service = PolicyService::new(repo, schema, tool_manager);
    service.sync_base_policies().await.unwrap();
    (db, service)
}

fn test_agent(id: &str) -> Agent {
    Agent {
        id: id.to_string(),
        user_id: Some("user-1".into()),
        name: format!("Agent {id}"),
        description: String::new(),
        model_group: "primary".into(),
        enabled: true,
        skills: None,
        sandbox_config: None,
        max_concurrent_tasks: None,
        avatar: None,
        identity: Default::default(),
        prompt: None,
        heartbeat_interval: None,
        next_heartbeat_at: None,
        heartbeat_chat_id: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn authorize_allows_by_default_with_base_policies() {
    let (_db, service) = setup().await;
    let agent = test_agent("agent-1");

    let decision = service
        .authorize(
            "user-1",
            &agent,
            PolicyAction::InvokeTool { tool_name: "browser_navigate".into(), tool_group: "browser".into() },
        )
        .await
        .unwrap();

    assert!(decision.allowed);
}

#[tokio::test]
async fn authorize_allows_with_permit_policy() {
    let (_db, service) = setup().await;
    let agent = test_agent("agent-1");

    service
        .create_policy(
            "user-1",
            "@id(\"test-permit\")\npermit(principal, action, resource);",
        )
        .await
        .unwrap();

    let decision = service
        .authorize(
            "user-1",
            &agent,
            PolicyAction::InvokeTool { tool_name: "browser_navigate".into(), tool_group: "browser".into() },
        )
        .await
        .unwrap();

    assert!(decision.allowed);
}

#[tokio::test]
async fn forbid_overrides_permit() {
    let (_db, service) = setup().await;
    let agent = test_agent("agent-1");

    service
        .create_policy(
            "user-1",
            "@id(\"allow-all\")\npermit(principal, action, resource);",
        )
        .await
        .unwrap();

    service
        .create_policy(
            "user-1",
            "@id(\"deny-browser\")\nforbid(\n  principal == Policy::Agent::\"agent-1\",\n  action == Policy::Action::\"invoke_tool\",\n  resource in Policy::ToolGroup::\"browser\"\n);",
        )
        .await
        .unwrap();

    let browser = service
        .authorize("user-1", &agent, PolicyAction::InvokeTool { tool_name: "browser_navigate".into(), tool_group: "browser".into() })
        .await
        .unwrap();
    assert!(browser.is_denied());

    let search = service
        .authorize("user-1", &agent, PolicyAction::InvokeTool { tool_name: "web_search".into(), tool_group: "search".into() })
        .await
        .unwrap();
    assert!(search.allowed);
}

#[tokio::test]
async fn forbid_denies_tool_that_base_permits() {
    let (_db, service) = setup().await;
    let agent = test_agent("agent-1");

    let before = service
        .authorize("user-1", &agent, PolicyAction::InvokeTool { tool_name: "browser_navigate".into(), tool_group: "browser".into() })
        .await
        .unwrap();
    assert!(before.allowed);

    service
        .forbid("user-1", "agent-1", &PolicyResource::ToolGroup { group: "browser".into() })
        .await
        .unwrap();

    let after = service
        .authorize("user-1", &agent, PolicyAction::InvokeTool { tool_name: "browser_navigate".into(), tool_group: "browser".into() })
        .await
        .unwrap();
    assert!(after.is_denied());
}

#[tokio::test]
async fn permit_restores_tool_after_forbid() {
    let (_db, service) = setup().await;
    let agent = test_agent("agent-1");

    service
        .forbid("user-1", "agent-1", &PolicyResource::ToolGroup { group: "browser".into() })
        .await
        .unwrap();

    service
        .permit("user-1", "agent-1", &PolicyResource::ToolGroup { group: "browser".into() })
        .await
        .unwrap();

    let decision = service
        .authorize("user-1", &agent, PolicyAction::InvokeTool { tool_name: "browser_navigate".into(), tool_group: "browser".into() })
        .await
        .unwrap();
    assert!(decision.allowed);
}

#[tokio::test]
async fn permit_forbid_preserves_complex_policies() {
    let (_db, service) = setup().await;

    service
        .create_policy(
            "user-1",
            "@id(\"custom-forbid\")\nforbid(\n  principal == Policy::Agent::\"agent-1\",\n  action == Policy::Action::\"invoke_tool\",\n  resource in Policy::ToolGroup::\"browser\"\n) when { context.is_task };",
        )
        .await
        .unwrap();

    service
        .forbid("user-1", "agent-1", &PolicyResource::ToolGroup { group: "search".into() })
        .await
        .unwrap();

    let policies = service.list_policies("user-1").await.unwrap();
    assert!(policies.iter().any(|p| p.name == "custom-forbid"));
}

#[tokio::test]
async fn base_policies_include_delegation_and_communication() {
    let (_db, service) = setup().await;
    let agent = test_agent("agent-1");

    let delegate = service
        .authorize("user-1", &agent, PolicyAction::DelegateTask { target_agent_id: "agent-2".into() })
        .await
        .unwrap();
    assert!(delegate.allowed);

    let send = service
        .authorize("user-1", &agent, PolicyAction::SendMessage { target_agent_id: "agent-2".into() })
        .await
        .unwrap();
    assert!(send.allowed);
}

#[tokio::test]
async fn permit_is_noop_when_already_allowed() {
    let (_db, service) = setup().await;

    service
        .permit("user-1", "agent-1", &PolicyResource::ToolGroup { group: "browser".into() })
        .await
        .unwrap();

    let policies = service.list_policies("user-1").await.unwrap();
    assert!(!policies.iter().any(|p| p.name == "agent-1-browser"));
}

#[tokio::test]
async fn policy_crud_lifecycle() {
    let (_db, service) = setup().await;

    let policy = service
        .create_policy("user-1", "@id(\"test\")\n@description(\"A test\")\npermit(principal, action, resource);")
        .await
        .unwrap();
    assert_eq!(policy.name, "test");
    assert_eq!(policy.description, "A test");

    let updated = service
        .update_policy("user-1", &policy.id, "@id(\"test\")\n@description(\"Updated\")\npermit(principal, action, resource);")
        .await
        .unwrap();
    assert_eq!(updated.description, "Updated");

    let fetched = service.get_policy("user-1", &policy.id).await.unwrap();
    assert_eq!(fetched.description, "Updated");

    service.delete_policy("user-1", &policy.id).await.unwrap();
    assert!(service.get_policy("user-1", &policy.id).await.is_err());
}

#[tokio::test]
async fn duplicate_policy_name_rejected() {
    let (_db, service) = setup().await;

    service
        .create_policy("user-1", "@id(\"unique\")\npermit(principal, action, resource);")
        .await
        .unwrap();

    let result = service
        .create_policy("user-1", "@id(\"unique\")\nforbid(principal, action, resource);")
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn delete_agent_policies_cleans_up() {
    let (_db, service) = setup().await;

    service
        .forbid("user-1", "agent-1", &PolicyResource::ToolGroup { group: "browser".into() })
        .await
        .unwrap();

    service
        .create_policy(
            "user-1",
            "@id(\"agent-1-custom\")\npermit(\n  principal == Policy::Agent::\"agent-1\",\n  action,\n  resource\n);",
        )
        .await
        .unwrap();

    service.delete_agent_policies("user-1", "agent-1").await.unwrap();

    let policies = service.list_policies("user-1").await.unwrap();
    assert!(policies.is_empty());
}

#[tokio::test]
async fn system_agent_always_allowed_manage_policy() {
    let (_db, service) = setup().await;
    let system_agent = test_agent("system");

    let decision = service
        .authorize(
            "user-1",
            &system_agent,
            PolicyAction::InvokeTool { tool_name: "manage_policy".into(), tool_group: "policy".into() },
        )
        .await
        .unwrap();

    assert!(decision.allowed);
}

#[tokio::test]
async fn cache_invalidation_reflects_new_policy() {
    let (_db, service) = setup().await;
    let agent = test_agent("agent-1");

    let allowed = service
        .authorize("user-1", &agent, PolicyAction::InvokeTool { tool_name: "browser_navigate".into(), tool_group: "browser".into() })
        .await
        .unwrap();
    assert!(allowed.allowed);

    service
        .create_policy("user-1", "@id(\"deny-browser\")\nforbid(\n  principal == Policy::Agent::\"agent-1\",\n  action == Policy::Action::\"invoke_tool\",\n  resource in Policy::ToolGroup::\"browser\"\n);")
        .await
        .unwrap();

    let denied = service
        .authorize("user-1", &agent, PolicyAction::InvokeTool { tool_name: "browser_navigate".into(), tool_group: "browser".into() })
        .await
        .unwrap();
    assert!(denied.is_denied());
}

#[tokio::test]
async fn base_policy_restricts_agent_tools_to_system() {
    let (_db, service) = setup().await;
    let regular_agent = test_agent("agent-1");
    let system_agent = test_agent("system");

    let denied = service
        .authorize("user-1", &regular_agent, PolicyAction::InvokeTool { tool_name: "create_agent".into(), tool_group: "agent".into() })
        .await
        .unwrap();
    assert!(denied.is_denied());

    let allowed = service
        .authorize("user-1", &system_agent, PolicyAction::InvokeTool { tool_name: "create_agent".into(), tool_group: "agent".into() })
        .await
        .unwrap();
    assert!(allowed.allowed);
}

#[tokio::test]
async fn validate_policy_text_catches_syntax_errors() {
    let (_db, service) = setup().await;

    assert!(service.validate_policy_text("permit(principal, action, resource);").is_ok());
    assert!(service.validate_policy_text("invalid syntax here").is_err());
}

#[tokio::test]
async fn extract_annotations_from_parsed_policy() {
    let (id, desc) = extract_annotations(
        "@id(\"my-policy\")\n@description(\"Does something\")\npermit(principal, action, resource);"
    );
    assert_eq!(id.as_deref(), Some("my-policy"));
    assert_eq!(desc.as_deref(), Some("Does something"));
}

#[tokio::test]
async fn delegation_allowed_when_principal_has_superset() {
    let (_db, service) = setup().await;
    let agent_a = test_agent("agent-a");

    let decision = service
        .authorize("user-1", &agent_a, PolicyAction::DelegateTask { target_agent_id: "agent-b".into() })
        .await
        .unwrap();
    assert!(decision.allowed);
}

#[tokio::test]
async fn delegation_denied_when_target_has_more_tools() {
    let (_db, service) = setup().await;
    let agent_a = test_agent("agent-a");

    service
        .forbid("user-1", "agent-a", &PolicyResource::ToolGroup { group: "browser".into() })
        .await
        .unwrap();

    let decision = service
        .authorize("user-1", &agent_a, PolicyAction::DelegateTask { target_agent_id: "agent-b".into() })
        .await
        .unwrap();
    assert!(decision.is_denied());
}

#[tokio::test]
async fn delegation_denied_researcher_to_receptionist() {
    let (_db, service) = setup().await;
    let researcher = test_agent("researcher");

    // Receptionist can only use voice tools
    service
        .create_policy(
            "user-1",
            "@id(\"receptionist-voice-only\")\nforbid(\n  principal == Policy::Agent::\"receptionist\",\n  action == Policy::Action::\"invoke_tool\",\n  resource\n) unless { resource in Policy::ToolGroup::\"voice\" };",
        )
        .await
        .unwrap();

    // Voice tools restricted to receptionist only
    service
        .create_policy(
            "user-1",
            "@id(\"voice-receptionist-only\")\nforbid(\n  principal,\n  action == Policy::Action::\"invoke_tool\",\n  resource in Policy::ToolGroup::\"voice\"\n) unless { principal == Policy::Agent::\"receptionist\" };",
        )
        .await
        .unwrap();

    // Researcher doesn't have voice tools, receptionist does
    // containsAll should fail — researcher can't delegate to receptionist
    let decision = service
        .authorize("user-1", &researcher, PolicyAction::DelegateTask { target_agent_id: "receptionist".into() })
        .await
        .unwrap();
    assert!(decision.is_denied(), "researcher should not be able to delegate to receptionist (missing voice tools)");
}

#[tokio::test]
async fn delegation_allowed_researcher_to_developer() {
    let (_db, service) = setup().await;
    let researcher = test_agent("researcher");

    // Voice tools restricted to receptionist only
    service
        .create_policy(
            "user-1",
            "@id(\"voice-receptionist-only\")\nforbid(\n  principal,\n  action == Policy::Action::\"invoke_tool\",\n  resource in Policy::ToolGroup::\"voice\"\n) unless { principal == Policy::Agent::\"receptionist\" };",
        )
        .await
        .unwrap();

    // Both researcher and developer have the same tools (everything except voice)
    // containsAll should pass
    let decision = service
        .authorize("user-1", &researcher, PolicyAction::DelegateTask { target_agent_id: "developer".into() })
        .await
        .unwrap();
    assert!(decision.allowed, "researcher should be able to delegate to developer (same tool set)");
}

#[tokio::test]
async fn sync_base_policies_no_duplicates_on_restart() {
    let (_db, service) = setup().await;

    // setup() already calls sync_base_policies once
    let first = service.list_system_policies().await.unwrap();
    let first_count = first.len();
    assert!(first_count > 0, "base policies should be seeded");

    // Simulate restarts
    service.sync_base_policies().await.unwrap();
    service.sync_base_policies().await.unwrap();
    service.sync_base_policies().await.unwrap();

    let after = service.list_system_policies().await.unwrap();
    assert_eq!(after.len(), first_count, "base policies should not duplicate on restart");

    let names: Vec<&str> = after.iter().map(|p| p.name.as_str()).collect();
    let mut unique_names = names.clone();
    unique_names.sort();
    unique_names.dedup();
    assert_eq!(names.len(), unique_names.len(), "all policy names should be unique");
}

// --- sync_agent_tools tests ---

#[tokio::test]
async fn sync_all_tools_selected_no_policies_created() {
    let (_db, service) = setup().await;
    let agent = test_agent("agent-1");

    // Select all tools — base policy already permits everything
    let all_tools: Vec<String> = vec![
        "browser_navigate".into(), "make_voice_call".into(), "hangup_call".into(), "web_search".into(),
    ];
    service.sync_agent_tools("user-1", "agent-1", &all_tools).await.unwrap();

    let policies = service.list_policies("user-1").await.unwrap();
    assert!(policies.is_empty(), "no user policies needed when all tools selected (base permits all)");

    // Verify all tools are still accessible
    let decision = service
        .authorize("user-1", &agent, PolicyAction::InvokeTool { tool_name: "browser_navigate".into(), tool_group: "browser".into() })
        .await.unwrap();
    assert!(decision.allowed);
}

#[tokio::test]
async fn sync_no_tools_selected_forbids_all_groups() {
    let (_db, service) = setup().await;
    let agent = test_agent("agent-1");

    service.sync_agent_tools("user-1", "agent-1", &[]).await.unwrap();

    let decision = service
        .authorize("user-1", &agent, PolicyAction::InvokeTool { tool_name: "browser_navigate".into(), tool_group: "browser".into() })
        .await.unwrap();
    assert!(decision.is_denied(), "browser should be denied after deselecting all");

    let decision = service
        .authorize("user-1", &agent, PolicyAction::InvokeTool { tool_name: "web_search".into(), tool_group: "search".into() })
        .await.unwrap();
    assert!(decision.is_denied(), "search should be denied after deselecting all");
}

#[tokio::test]
async fn sync_partial_group_selection() {
    let (_db, service) = setup().await;
    let agent = test_agent("agent-1");

    // Select only make_voice_call but not hangup_call (both in voice group)
    let selected: Vec<String> = vec![
        "browser_navigate".into(), "web_search".into(), "make_voice_call".into(),
    ];
    service.sync_agent_tools("user-1", "agent-1", &selected).await.unwrap();

    // make_voice_call should be permitted
    let decision = service
        .authorize("user-1", &agent, PolicyAction::InvokeTool { tool_name: "make_voice_call".into(), tool_group: "voice".into() })
        .await.unwrap();
    assert!(decision.allowed, "make_voice_call should be permitted");

    // hangup_call should be denied (partial group — individual forbid)
    let decision = service
        .authorize("user-1", &agent, PolicyAction::InvokeTool { tool_name: "hangup_call".into(), tool_group: "voice".into() })
        .await.unwrap();
    assert!(decision.is_denied(), "hangup_call should be denied in partial selection");

    // browser should be permitted (full group selected)
    let decision = service
        .authorize("user-1", &agent, PolicyAction::InvokeTool { tool_name: "browser_navigate".into(), tool_group: "browser".into() })
        .await.unwrap();
    assert!(decision.allowed, "browser should be permitted");
}

#[tokio::test]
async fn sync_then_reselect_all_removes_forbids() {
    let (_db, service) = setup().await;
    let agent = test_agent("agent-1");

    // First deselect everything
    service.sync_agent_tools("user-1", "agent-1", &[]).await.unwrap();

    let decision = service
        .authorize("user-1", &agent, PolicyAction::InvokeTool { tool_name: "browser_navigate".into(), tool_group: "browser".into() })
        .await.unwrap();
    assert!(decision.is_denied());

    // Now reselect all
    let all_tools: Vec<String> = vec![
        "browser_navigate".into(), "make_voice_call".into(), "hangup_call".into(), "web_search".into(),
    ];
    service.sync_agent_tools("user-1", "agent-1", &all_tools).await.unwrap();

    let decision = service
        .authorize("user-1", &agent, PolicyAction::InvokeTool { tool_name: "browser_navigate".into(), tool_group: "browser".into() })
        .await.unwrap();
    assert!(decision.allowed, "browser should be permitted again after reselecting all");

    let policies = service.list_policies("user-1").await.unwrap();
    assert!(policies.is_empty(), "forbid policies should be cleaned up after reselecting all");
}

#[tokio::test]
async fn sync_preserves_other_agent_policies() {
    let (_db, service) = setup().await;
    let agent_a = test_agent("agent-a");
    let agent_b = test_agent("agent-b");

    // Forbid browser for agent-b
    service.sync_agent_tools("user-1", "agent-b", &["web_search".into()]).await.unwrap();

    // Now sync agent-a — should not affect agent-b
    service.sync_agent_tools("user-1", "agent-a", &["browser_navigate".into()]).await.unwrap();

    // agent-b should still have browser denied
    let decision = service
        .authorize("user-1", &agent_b, PolicyAction::InvokeTool { tool_name: "browser_navigate".into(), tool_group: "browser".into() })
        .await.unwrap();
    assert!(decision.is_denied(), "agent-b browser should still be denied");

    // agent-a should have browser permitted
    let decision = service
        .authorize("user-1", &agent_a, PolicyAction::InvokeTool { tool_name: "browser_navigate".into(), tool_group: "browser".into() })
        .await.unwrap();
    assert!(decision.allowed, "agent-a browser should be permitted");
}

#[tokio::test]
async fn forbid_then_permit_conflict_detected() {
    let (_db, service) = setup().await;

    service
        .create_policy(
            "user-1",
            "@id(\"hard-deny\")\nforbid(\n  principal == Policy::Agent::\"agent-1\",\n  action == Policy::Action::\"invoke_tool\",\n  resource in Policy::ToolGroup::\"browser\"\n);",
        )
        .await
        .unwrap();

    let result = service
        .permit("user-1", "agent-1", &PolicyResource::ToolGroup { group: "browser".into() })
        .await;

    assert!(result.is_err());
}
