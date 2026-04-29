//! End-to-end tests for the Cedar policy → SandboxPolicy pipeline.
//!
//! Validates the full chain: policies created via PolicyService → evaluate_sandbox_policy →
//! SandboxPolicy with the correct read/write paths, network access, and deny rules.

use std::sync::Arc;

use frona::db::repo::generic::SurrealRepo;
use frona::policy::models::Policy;
use frona::policy::repository::PolicyRepository;
use frona::policy::schema::build_schema;
use frona::policy::service::PolicyService;
use frona::tool::{AgentTool, ToolDefinition, ToolOutput};

use surrealdb::Surreal;
use surrealdb::engine::local::{Db, Mem};

struct MockTool {
    name: &'static str,
    defs: Vec<ToolDefinition>,
}

#[async_trait::async_trait]
impl AgentTool for MockTool {
    fn name(&self) -> &str { self.name }
    fn definitions(&self) -> Vec<ToolDefinition> { self.defs.clone() }
    async fn execute(
        &self,
        _: &str,
        _: serde_json::Value,
        _: &frona::tool::InferenceContext,
    ) -> Result<ToolOutput, frona::core::error::AppError> {
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

async fn setup() -> (Surreal<Db>, PolicyService) {
    let db = Surreal::new::<Mem>(()).await.unwrap();
    frona::db::init::setup_schema(&db).await.unwrap();

    let schema = build_schema();
    let repo: Arc<dyn PolicyRepository> = Arc::new(SurrealRepo::<Policy>::new(db.clone()));
    let tool_manager = Arc::new(frona::tool::manager::ToolManager::new(false));

    tool_manager
        .register_user_tool(
            "user-1",
            Arc::new(MockTool {
                name: "browser",
                defs: vec![
                    mock_def("browser_navigate", "browser"),
                    mock_def("browser_screenshot", "browser"),
                ],
            }),
        )
        .await;
    tool_manager
        .register_user_tool(
            "user-1",
            Arc::new(MockTool {
                name: "search",
                defs: vec![mock_def("web_search", "search")],
            }),
        )
        .await;

    let service = PolicyService::new(repo, schema, tool_manager);
    service.sync_base_policies().await.unwrap();
    (db, service)
}

// --- Simple scenarios ---

#[tokio::test]
async fn e2e_no_policies_no_sandbox_permissions() {
    let (_db, service) = setup().await;
    let policy = service
        .evaluate_sandbox_policy("user-1", "agent-a")
        .await
        .unwrap();
    assert!(policy.read_paths.is_empty());
    assert!(policy.write_paths.is_empty());
    assert!(!policy.network_access);
}

#[tokio::test]
async fn e2e_simple_read_permit() {
    let (_db, service) = setup().await;
    service
        .create_policy(
            "user-1",
            r#"@id("e2e-read-data")
               permit(principal, action == Policy::Action::"read", resource == Policy::Directory::"/data");"#,
        )
        .await
        .unwrap();

    let policy = service.evaluate_sandbox_policy("user-1", "agent-a").await.unwrap();
    assert!(policy.read_paths.contains(&"/data".to_string()));
}

#[tokio::test]
async fn e2e_managed_default_network_grants_access() {
    let (_db, service) = setup().await;
    let managed = cedar_policy::Policy::from_json(
        Some(cedar_policy::PolicyId::new("default-network-access")),
        serde_json::json!({
            "effect": "permit",
            "principal": { "op": "All" },
            "action": { "op": "==", "entity": { "type": "Policy::Action", "id": "connect" } },
            "resource": { "op": "All" },
            "annotations": {
                "config": "sandbox.default_network_access",
                "readonly": "true"
            },
            "conditions": []
        }),
    ).unwrap();
    service.register_managed_policy(managed);

    let policy = service.evaluate_sandbox_policy("user-1", "any-agent").await.unwrap();
    assert!(policy.network_access);
}

#[tokio::test]
async fn e2e_agent_specific_policy_does_not_affect_other_agents() {
    let (_db, service) = setup().await;
    service
        .create_policy(
            "user-1",
            r#"@id("e2e-only-a")
               permit(principal == Policy::Agent::"agent-a", action == Policy::Action::"write", resource == Policy::Directory::"/output");"#,
        )
        .await
        .unwrap();

    let p_a = service.evaluate_sandbox_policy("user-1", "agent-a").await.unwrap();
    assert!(p_a.write_paths.contains(&"/output".to_string()));

    let p_b = service.evaluate_sandbox_policy("user-1", "agent-b").await.unwrap();
    assert!(p_b.write_paths.is_empty());
}

#[tokio::test]
async fn e2e_forbid_creates_denied_path() {
    let (_db, service) = setup().await;
    service
        .create_policy(
            "user-1",
            r#"@id("e2e-permit-workspace")
               permit(principal, action == Policy::Action::"write", resource == Policy::Directory::"/workspace");"#,
        )
        .await
        .unwrap();
    service
        .create_policy(
            "user-1",
            r#"@id("e2e-forbid-secrets")
               forbid(principal, action == Policy::Action::"write", resource == Policy::Directory::"/workspace/secrets");"#,
        )
        .await
        .unwrap();

    let policy = service.evaluate_sandbox_policy("user-1", "agent-a").await.unwrap();
    assert!(policy.write_paths.contains(&"/workspace".to_string()));
    assert!(policy.denied_paths.contains(&"/workspace/secrets".to_string()));
}

// --- Complex scenarios ---

#[tokio::test]
async fn e2e_tool_conditional_policy_applies_when_tool_permitted() {
    let (_db, service) = setup().await;

    // Forbid browser tools for agent-b (overrides base permit)
    service.forbid("user-1", "agent-b", &frona::policy::models::PolicyResource::ToolGroup {
        group: "browser".into(),
    }).await.unwrap();

    service
        .create_policy(
            "user-1",
            r#"@id("e2e-browser-data")
               permit(principal, action == Policy::Action::"read", resource == Policy::Directory::"/browser-data")
                   when { principal.tools.contains("browser_navigate") };"#,
        )
        .await
        .unwrap();

    let p_with_browser = service.evaluate_sandbox_policy("user-1", "agent-a").await.unwrap();
    assert!(
        p_with_browser.read_paths.contains(&"/browser-data".to_string()),
        "agent with browser tools (default) should get /browser-data: {p_with_browser:?}"
    );

    let p_without_browser = service.evaluate_sandbox_policy("user-1", "agent-b").await.unwrap();
    assert!(
        !p_without_browser.read_paths.contains(&"/browser-data".to_string()),
        "agent with browser forbidden should not get /browser-data: {p_without_browser:?}"
    );
}

#[tokio::test]
async fn e2e_forbid_unless_pattern_carves_exceptions() {
    let (_db, service) = setup().await;
    let managed = cedar_policy::Policy::from_json(
        Some(cedar_policy::PolicyId::new("default-network-access")),
        serde_json::json!({
            "effect": "permit",
            "principal": { "op": "All" },
            "action": { "op": "==", "entity": { "type": "Policy::Action", "id": "connect" } },
            "resource": { "op": "All" },
            "annotations": {},
            "conditions": []
        }),
    ).unwrap();
    service.register_managed_policy(managed);

    service
        .create_policy(
            "user-1",
            r#"@id("e2e-restrict-x")
               forbid(principal == Policy::Agent::"agent-x", action == Policy::Action::"connect", resource)
                   unless { resource == Policy::NetworkDestination::"gmail.com" };"#,
        )
        .await
        .unwrap();

    let p_x = service.evaluate_sandbox_policy("user-1", "agent-x").await.unwrap();
    assert!(p_x.network_destinations.contains(&"gmail.com".to_string()),
        "agent-x should have gmail.com as exception: {p_x:?}");

    let p_y = service.evaluate_sandbox_policy("user-1", "agent-y").await.unwrap();
    assert!(p_y.network_access, "agent-y should still have wildcard network access");
    assert!(!p_y.network_destinations.contains(&"gmail.com".to_string()),
        "agent-y should not have gmail.com specifically extracted");
}

#[tokio::test]
async fn e2e_multiple_actions_full_sandbox_policy() {
    let (_db, service) = setup().await;
    service
        .create_policy(
            "user-1",
            r#"@id("e2e-multi-read")
               permit(principal, action == Policy::Action::"read", resource == Policy::Directory::"/r");"#,
        ).await.unwrap();
    service
        .create_policy(
            "user-1",
            r#"@id("e2e-multi-write")
               permit(principal, action == Policy::Action::"write", resource == Policy::Directory::"/w");"#,
        ).await.unwrap();
    service
        .create_policy(
            "user-1",
            r#"@id("e2e-multi-connect")
               permit(principal, action == Policy::Action::"connect", resource == Policy::NetworkDestination::"api.com");"#,
        ).await.unwrap();
    service
        .create_policy(
            "user-1",
            r#"@id("e2e-multi-bind")
               permit(principal, action == Policy::Action::"bind", resource == Policy::NetworkDestination::"8080");"#,
        ).await.unwrap();
    service
        .create_policy(
            "user-1",
            r#"@id("e2e-multi-block")
               forbid(principal, action == Policy::Action::"connect", resource == Policy::NetworkDestination::"10.0.0.0/8");"#,
        ).await.unwrap();

    let policy = service.evaluate_sandbox_policy("user-1", "agent-a").await.unwrap();
    assert!(policy.read_paths.contains(&"/r".to_string()));
    assert!(policy.write_paths.contains(&"/w".to_string()));
    assert!(policy.network_access);
    assert!(policy.network_destinations.contains(&"api.com".to_string()));
    assert!(policy.bind_ports.contains(&8080));
    assert!(policy.blocked_networks.contains(&"10.0.0.0/8".to_string()));
}

#[tokio::test]
async fn e2e_policy_changes_invalidate_sandbox_cache() {
    let (_db, service) = setup().await;
    let p1 = service.evaluate_sandbox_policy("user-1", "agent-a").await.unwrap();
    assert!(p1.read_paths.is_empty());

    service
        .create_policy(
            "user-1",
            r#"@id("e2e-cache-invalidate")
               permit(principal, action == Policy::Action::"read", resource == Policy::Directory::"/added");"#,
        ).await.unwrap();

    let p2 = service.evaluate_sandbox_policy("user-1", "agent-a").await.unwrap();
    assert!(p2.read_paths.contains(&"/added".to_string()),
        "cache should be invalidated after policy creation");
}

#[tokio::test]
async fn e2e_apply_to_sandbox_config_merges_correctly() {
    use frona::tool::sandbox::driver::SandboxConfig;

    let (_db, service) = setup().await;
    service
        .create_policy(
            "user-1",
            r#"@id("e2e-apply-read")
               permit(principal, action == Policy::Action::"read", resource == Policy::Directory::"/data");"#,
        ).await.unwrap();
    service
        .create_policy(
            "user-1",
            r#"@id("e2e-apply-write")
               permit(principal, action == Policy::Action::"write", resource == Policy::Directory::"/output");"#,
        ).await.unwrap();
    service
        .create_policy(
            "user-1",
            r#"@id("e2e-apply-deny")
               forbid(principal, action == Policy::Action::"write", resource == Policy::Directory::"/output/secrets");"#,
        ).await.unwrap();
    service
        .create_policy(
            "user-1",
            r#"@id("e2e-apply-block")
               forbid(principal, action == Policy::Action::"connect", resource == Policy::NetworkDestination::"10.0.0.0/8");"#,
        ).await.unwrap();

    let policy = service.evaluate_sandbox_policy("user-1", "agent-a").await.unwrap();

    let mut config = SandboxConfig {
        workspace_dir: "/workspaces/agent-a".into(),
        additional_read_paths: vec!["/preexisting".into()],
        ..Default::default()
    };
    policy.apply(&mut config);

    assert!(config.additional_read_paths.contains(&"/preexisting".to_string()));
    assert!(config.additional_read_paths.contains(&"/data".to_string()));
    assert!(config.additional_write_paths.contains(&"/output".to_string()));
    assert!(config.denied_paths.contains(&"/output/secrets".to_string()));
    assert!(config.blocked_networks.contains(&"10.0.0.0/8".to_string()));
}

#[tokio::test]
async fn e2e_complex_real_world_browser_agent() {
    let (_db, service) = setup().await;

    // Default network access for everyone
    let managed = cedar_policy::Policy::from_json(
        Some(cedar_policy::PolicyId::new("default-network-access")),
        serde_json::json!({
            "effect": "permit",
            "principal": { "op": "All" },
            "action": { "op": "==", "entity": { "type": "Policy::Action", "id": "connect" } },
            "resource": { "op": "All" },
            "annotations": {},
            "conditions": []
        }),
    ).unwrap();
    service.register_managed_policy(managed);

    // Forbid browser tools for agent-other (agent-web keeps default permit)
    service.forbid("user-1", "agent-other", &frona::policy::models::PolicyResource::ToolGroup {
        group: "browser".into(),
    }).await.unwrap();

    // Browser agents get /browser-profiles read+write
    service
        .create_policy(
            "user-1",
            r#"@id("e2e-browser-profiles-read")
               permit(principal, action == Policy::Action::"read", resource == Policy::Directory::"/browser-profiles")
                   when { principal.tools.contains("browser_navigate") };"#,
        ).await.unwrap();
    service
        .create_policy(
            "user-1",
            r#"@id("e2e-browser-profiles-write")
               permit(principal, action == Policy::Action::"write", resource == Policy::Directory::"/browser-profiles")
                   when { principal.tools.contains("browser_navigate") };"#,
        ).await.unwrap();

    // Block internal network for everyone
    service
        .create_policy(
            "user-1",
            r#"@id("e2e-block-internal")
               forbid(principal, action == Policy::Action::"connect", resource == Policy::NetworkDestination::"10.0.0.0/8");"#,
        ).await.unwrap();

    // Web agent
    let web = service.evaluate_sandbox_policy("user-1", "agent-web").await.unwrap();
    assert!(web.read_paths.contains(&"/browser-profiles".to_string()));
    assert!(web.write_paths.contains(&"/browser-profiles".to_string()));
    assert!(web.network_access);
    assert!(web.blocked_networks.contains(&"10.0.0.0/8".to_string()));

    // Non-browser agent
    let other = service.evaluate_sandbox_policy("user-1", "agent-other").await.unwrap();
    assert!(!other.read_paths.contains(&"/browser-profiles".to_string()));
    assert!(!other.write_paths.contains(&"/browser-profiles".to_string()));
    assert!(other.network_access, "all agents have default network access");
    assert!(other.blocked_networks.contains(&"10.0.0.0/8".to_string()),
        "internal block applies to all agents");
}

#[tokio::test]
async fn e2e_managed_policy_combined_with_user_policies() {
    let (_db, service) = setup().await;

    // Dynamic: default network
    let managed = cedar_policy::Policy::from_json(
        Some(cedar_policy::PolicyId::new("default-network-access")),
        serde_json::json!({
            "effect": "permit",
            "principal": { "op": "All" },
            "action": { "op": "==", "entity": { "type": "Policy::Action", "id": "connect" } },
            "resource": { "op": "All" },
            "annotations": {},
            "conditions": []
        }),
    ).unwrap();
    service.register_managed_policy(managed);

    // User: read /shared
    service
        .create_policy(
            "user-1",
            r#"@id("e2e-combined-read")
               permit(principal, action == Policy::Action::"read", resource == Policy::Directory::"/shared");"#,
        ).await.unwrap();

    let policy = service.evaluate_sandbox_policy("user-1", "agent-a").await.unwrap();
    assert!(policy.network_access, "managed permit applies");
    assert!(policy.read_paths.contains(&"/shared".to_string()), "user permit applies");
}
