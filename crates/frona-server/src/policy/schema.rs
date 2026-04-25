use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;

use cedar_policy::{
    Entities, EntityId, EntityTypeName, EntityUid, PolicySet, Schema,
};

pub const NAMESPACE: &str = "Policy";

pub fn build_schema() -> Arc<Schema> {
    let (schema, warnings) = Schema::from_cedarschema_str(
        include_str!("../../../../resources/policy/frona.cedarschema"),
    )
        .expect("Failed to parse built-in policy schema");

    for warning in warnings {
        tracing::warn!(%warning, "Policy schema warning");
    }

    Arc::new(schema)
}

pub fn entity_type_name(type_name: &str) -> EntityTypeName {
    format!("{NAMESPACE}::{type_name}")
        .parse()
        .expect("valid entity type name")
}

pub fn entity_uid(type_name: &str, id: &str) -> EntityUid {
    EntityUid::from_type_name_and_id(entity_type_name(type_name), EntityId::new(id))
}

pub fn agent_entity_uid(agent_id: &str) -> EntityUid {
    entity_uid("Agent", agent_id)
}

pub fn tool_entity_uid(tool_name: &str) -> EntityUid {
    entity_uid("Tool", tool_name)
}

pub fn action_entity_uid(action_name: &str) -> EntityUid {
    entity_uid("Action", action_name)
}

fn tool_group_entity_uid(group: &str) -> EntityUid {
    entity_uid("ToolGroup", group)
}

pub fn build_tool_entities(tool_name: &str, tool_group: &str) -> Entities {
    let tool_uid = tool_entity_uid(tool_name);
    let group_uid = tool_group_entity_uid(tool_group);

    let tool_entity = cedar_policy::Entity::new_no_attrs(
        tool_uid,
        HashSet::from([group_uid.clone()]),
    );
    let group_entity = cedar_policy::Entity::new_no_attrs(
        group_uid,
        HashSet::new(),
    );

    Entities::from_entities([tool_entity, group_entity], None)
        .unwrap_or_else(|_| Entities::empty())
}

pub fn build_agent_entities(
    principal_id: &str,
    principal_tools: &[String],
    target_id: &str,
    target_tools: &[String],
) -> Entities {
    let principal_uid = agent_entity_uid(principal_id);
    let target_uid = agent_entity_uid(target_id);

    fn tools_to_set(tools: &[String]) -> cedar_policy::RestrictedExpression {
        let elements: Vec<cedar_policy::RestrictedExpression> = tools
            .iter()
            .map(|t| cedar_policy::RestrictedExpression::new_string(t.clone()))
            .collect();
        cedar_policy::RestrictedExpression::new_set(elements)
    }

    let agent_attrs = [
        ("enabled".into(), cedar_policy::RestrictedExpression::new_bool(true)),
        ("model_group".into(), cedar_policy::RestrictedExpression::new_string("primary".into())),
        ("tools".into(), tools_to_set(principal_tools)),
    ];
    let principal_entity = cedar_policy::Entity::new(
        principal_uid,
        agent_attrs.into_iter().collect(),
        HashSet::new(),
    ).expect("valid principal entity");

    let target_attrs = [
        ("enabled".into(), cedar_policy::RestrictedExpression::new_bool(true)),
        ("model_group".into(), cedar_policy::RestrictedExpression::new_string("primary".into())),
        ("tools".into(), tools_to_set(target_tools)),
    ];
    let target_entity = cedar_policy::Entity::new(
        target_uid,
        target_attrs.into_iter().collect(),
        HashSet::new(),
    ).expect("valid target entity");

    Entities::from_entities([principal_entity, target_entity], None)
        .unwrap_or_else(|_| Entities::empty())
}

pub fn prepend_annotations(id: &str, description: &str, policy_text: &str) -> String {
    format!("@id(\"{id}\")\n@description(\"{description}\")\n{policy_text}")
}

fn resource_to_cedar_clause(resource: &super::models::PolicyResource) -> String {
    match resource {
        super::models::PolicyResource::Tool { id, .. } => {
            format!("resource == {NAMESPACE}::Tool::\"{id}\"")
        }
        super::models::PolicyResource::ToolGroup { group } => {
            format!("resource in {NAMESPACE}::ToolGroup::\"{group}\"")
        }
    }
}

pub fn build_tool_policy_text(
    agent_id: &str,
    resource: &super::models::PolicyResource,
    effect: &str,
    policy_name: &str,
    description: &str,
) -> String {
    let resource_cedar = resource_to_cedar_clause(resource);
    format!(
        "@id(\"{policy_name}\")\n@description(\"{description}\")\n{effect}(\n  principal == {NAMESPACE}::Agent::\"{agent_id}\",\n  action == {NAMESPACE}::Action::\"invoke_tool\",\n  {resource_cedar}\n);"
    )
}

pub fn references_agent(policy_text: &str, agent_id: &str) -> bool {
    let Ok(policy_set) = PolicySet::from_str(policy_text) else {
        return false;
    };
    let target = agent_entity_uid(agent_id);

    policy_set.policies().any(|p| {
        matches!(
            p.principal_constraint(),
            cedar_policy::PrincipalConstraint::Eq(ref uid) if *uid == target
        ) || matches!(
            p.resource_constraint(),
            cedar_policy::ResourceConstraint::Eq(ref uid) if *uid == target
        )
    })
}

pub fn extract_annotations(policy_text: &str) -> (Option<String>, Option<String>) {
    let Ok(policy_set) = PolicySet::from_str(policy_text) else {
        return (None, None);
    };

    let first = policy_set.policies().next();
    let Some(policy) = first else {
        return (None, None);
    };

    let id = policy.annotation("id").map(|s| s.to_string());
    let description = policy.annotation("description").map(|s| s.to_string());

    (id, description)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_parses_without_error() {
        let schema = build_schema();
        assert!(Arc::strong_count(&schema) == 1);
    }

    #[test]
    fn test_references_agent() {
        let text = "permit(principal == Policy::Agent::\"my-agent\", action, resource);";
        assert!(references_agent(text, "my-agent"));
        assert!(!references_agent(text, "other-agent"));
    }

    #[test]
    fn test_extract_annotations() {
        let text = "@id(\"my-policy\")\n@description(\"A test policy\")\npermit(principal, action, resource);";
        let (id, desc) = extract_annotations(text);
        assert_eq!(id.as_deref(), Some("my-policy"));
        assert_eq!(desc.as_deref(), Some("A test policy"));
    }

    #[test]
    fn test_extract_annotations_none() {
        let text = "permit(principal, action, resource);";
        let (id, desc) = extract_annotations(text);
        assert!(id.is_none());
        assert!(desc.is_none());
    }
}
