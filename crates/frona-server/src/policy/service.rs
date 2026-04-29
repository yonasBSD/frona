use std::str::FromStr;
use std::sync::{Arc, RwLock};

use cedar_policy::{
    Authorizer, Context, Decision, PolicySet, Request, Schema,
};
use moka::future::Cache;

use crate::agent::models::Agent;
use crate::core::error::AppError;

use super::models::{AuthorizationDecision, Policy, PolicyAction, PolicyResource};
use super::repository::PolicyRepository;
use super::schema::{
    agent_entity_uid, action_entity_uid, tool_entity_uid,
    build_tool_entities, build_agent_entities, references_agent,
    extract_annotations,
};

const BASE_POLICIES: &str = include_str!("../../../../resources/policy/frona.cedar");

struct CachedPolicySet {
    policy_set: PolicySet,
}

#[derive(Clone)]
pub struct PolicyService {
    repo: Arc<dyn PolicyRepository>,
    schema: Arc<Schema>,
    policy_cache: Cache<String, Arc<CachedPolicySet>>,
    sandbox_cache: Cache<String, Arc<super::sandbox::SandboxPolicy>>,
    tool_manager: Arc<crate::tool::manager::ToolManager>,
    managed_policies: Arc<RwLock<Vec<cedar_policy::Policy>>>,
}

impl PolicyService {
    pub fn new(
        repo: Arc<dyn PolicyRepository>,
        schema: Arc<Schema>,
        tool_manager: Arc<crate::tool::manager::ToolManager>,
    ) -> Self {
        let policy_cache = Cache::builder()
            .max_capacity(1000)
            .build();
        let sandbox_cache = Cache::builder()
            .max_capacity(1000)
            .build();

        Self {
            repo,
            schema,
            policy_cache,
            sandbox_cache,
            tool_manager,
            managed_policies: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn register_managed_policy(&self, policy: cedar_policy::Policy) {
        self.managed_policies.write().unwrap().push(policy);
        self.policy_cache.invalidate_all();
        self.sandbox_cache.invalidate_all();
    }

    pub fn managed_policies(&self) -> Vec<cedar_policy::Policy> {
        self.managed_policies.read().unwrap().clone()
    }

    pub async fn evaluate_sandbox_policy(
        &self,
        user_id: &str,
        agent_id: &str,
    ) -> Result<Arc<super::sandbox::SandboxPolicy>, AppError> {
        let key = format!("{user_id}:{agent_id}");
        if let Some(cached) = self.sandbox_cache.get(&key).await {
            return Ok(cached);
        }

        let cached = self.build_policy_set(user_id).await?;
        let agent_tools = self.resolve_agent_tools(user_id, agent_id, &cached.policy_set).await?;
        let policy = super::sandbox::evaluate_sandbox_policy(
            &cached.policy_set,
            agent_id,
            &agent_tools,
        );
        let arc = Arc::new(policy);
        self.sandbox_cache.insert(key, arc.clone()).await;
        Ok(arc)
    }

    async fn resolve_agent_tools(
        &self,
        user_id: &str,
        agent_id: &str,
        policy_set: &PolicySet,
    ) -> Result<Vec<String>, AppError> {
        let all_defs = self.tool_manager.definitions(user_id).await;
        let mut tools = Vec::new();
        for def in all_defs {
            let resource = PolicyResource::Tool {
                id: def.id.clone(),
                group: def.provider_id.clone(),
            };
            if self.is_permitted(agent_id, &resource, policy_set)? {
                tools.push(def.id);
            }
        }
        Ok(tools)
    }

    pub async fn authorize(
        &self,
        user_id: &str,
        agent: &Agent,
        action: PolicyAction,
    ) -> Result<AuthorizationDecision, AppError> {
        let start = std::time::Instant::now();
        let action_name = action.cedar_action_name();

        if agent.id == "system"
            && matches!(&action, PolicyAction::InvokeTool { tool_name, .. } if tool_name == "manage_policy")
        {
            crate::core::metrics::record_policy_evaluation(action_name, "allow", start.elapsed());
            return Ok(AuthorizationDecision::allow());
        }

        let cached = self.build_policy_set(user_id).await?;

        let principal = agent_entity_uid(&agent.id);
        let action_uid = action_entity_uid(action.cedar_action_name());
        let resource = match &action {
            PolicyAction::InvokeTool { tool_name, .. } => tool_entity_uid(tool_name),
            PolicyAction::DelegateTask { target_agent_id } => agent_entity_uid(target_agent_id),
            PolicyAction::SendMessage { target_agent_id } => agent_entity_uid(target_agent_id),
        };

        let context = Context::empty();

        let entities = match &action {
            PolicyAction::InvokeTool { tool_name, tool_group } => {
                build_tool_entities(tool_name, tool_group)
            }
            PolicyAction::DelegateTask { target_agent_id }
            | PolicyAction::SendMessage { target_agent_id } => {
                let all_defs = self.tool_manager.definitions(user_id).await;
                let mut principal_tools = Vec::new();
                let mut target_tools = Vec::new();
                for def in &all_defs {
                    let resource = PolicyResource::Tool {
                        id: def.id.clone(),
                        group: def.provider_id.clone(),
                    };
                    if self.is_permitted(&agent.id, &resource, &cached.policy_set)? {
                        principal_tools.push(def.id.clone());
                    }
                    if self.is_permitted(target_agent_id, &resource, &cached.policy_set)? {
                        target_tools.push(def.id.clone());
                    }
                }
                build_agent_entities(&agent.id, &principal_tools, target_agent_id, &target_tools)
            }
        };

        let request = Request::new(principal, action_uid, resource, context, Some(&self.schema))
            .map_err(|e| AppError::Internal(format!("Policy request error: {e}")))?;

        let authorizer = Authorizer::new();
        let response = authorizer.is_authorized(&request, &cached.policy_set, &entities);

        match response.decision() {
            Decision::Allow => {
                crate::core::metrics::record_policy_evaluation(action_name, "allow", start.elapsed());
                Ok(AuthorizationDecision::allow())
            }
            Decision::Deny => {
                let reasons: Vec<String> = response
                    .diagnostics()
                    .errors()
                    .map(|e| e.to_string())
                    .collect();
                let diag = if reasons.is_empty() {
                    "No matching permit policy".to_string()
                } else {
                    reasons.join("; ")
                };
                crate::core::metrics::record_policy_evaluation(action_name, "deny", start.elapsed());
                Ok(AuthorizationDecision::deny(diag))
            }
        }
    }

    pub async fn sync_agent_tools(
        &self,
        user_id: &str,
        agent_id: &str,
        selected_tools: &[String],
    ) -> Result<(), AppError> {
        use std::collections::{HashMap, HashSet};

        let selected: HashSet<&str> = selected_tools.iter().map(|s| s.as_str()).collect();
        let all_defs = self.tool_manager.definitions(user_id).await;

        let mut group_tools: HashMap<String, Vec<String>> = HashMap::new();
        for def in &all_defs {
            group_tools
                .entry(def.provider_id.clone())
                .or_default()
                .push(def.id.clone());
        }

        for (group, tools_in_group) in &group_tools {
            let selected_count = tools_in_group
                .iter()
                .filter(|t| selected.contains(t.as_str()))
                .count();
            let total = tools_in_group.len();

            if selected_count == total {
                let resource = PolicyResource::ToolGroup { group: group.clone() };
                self.permit(user_id, agent_id, &resource).await?;
                for tool_id in tools_in_group {
                    let tool_resource = PolicyResource::Tool {
                        id: tool_id.clone(),
                        group: group.clone(),
                    };
                    self.permit(user_id, agent_id, &tool_resource).await?;
                }
            } else if selected_count == 0 {
                let resource = PolicyResource::ToolGroup { group: group.clone() };
                self.forbid(user_id, agent_id, &resource).await?;
            } else {
                let resource = PolicyResource::ToolGroup { group: group.clone() };
                self.permit(user_id, agent_id, &resource).await?;
                for tool_id in tools_in_group {
                    let tool_resource = PolicyResource::Tool {
                        id: tool_id.clone(),
                        group: group.clone(),
                    };
                    if selected.contains(tool_id.as_str()) {
                        self.permit(user_id, agent_id, &tool_resource).await?;
                    } else {
                        self.forbid(user_id, agent_id, &tool_resource).await?;
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn permit(
        &self,
        user_id: &str,
        agent_id: &str,
        resource: &PolicyResource,
    ) -> Result<(), AppError> {
        self.set_access(user_id, agent_id, resource, true).await
    }

    pub async fn forbid(
        &self,
        user_id: &str,
        agent_id: &str,
        resource: &PolicyResource,
    ) -> Result<(), AppError> {
        self.set_access(user_id, agent_id, resource, false).await
    }

    async fn set_access(
        &self,
        user_id: &str,
        agent_id: &str,
        resource: &PolicyResource,
        allow: bool,
    ) -> Result<(), AppError> {
        let system_policies = self.repo.find_system_policies().await?;
        let user_policies = self.repo.find_by_user_id(user_id).await?;

        let resource_label = resource.label();
        let policy_name = format!("{agent_id}-{resource_label}");

        let mut existing_simple_id = None;
        let mut remaining_texts = Vec::new();

        for policy in system_policies.iter().chain(user_policies.iter()) {
            if !policy.enabled {
                continue;
            }
            if policy.user_id.is_some() && policy.name == policy_name {
                existing_simple_id = Some(policy.id.clone());
            } else {
                remaining_texts.push(policy.policy_text.clone());
            }
        }

        if let Some(id) = existing_simple_id {
            self.repo.delete_by_ids(&[id]).await?;
        }

        let combined_remaining = remaining_texts.join("\n");
        let temp_policy_set = if combined_remaining.trim().is_empty() {
            PolicySet::new()
        } else {
            PolicySet::from_str(&combined_remaining)
                .map_err(|e| AppError::Internal(format!("Failed to parse remaining policies: {e}")))?
        };

        let currently_allowed = self.is_permitted(agent_id, resource, &temp_policy_set)?;

        if currently_allowed == allow {
            self.invalidate_cache(user_id).await;
            return Ok(());
        }

        let effect = if allow { "permit" } else { "forbid" };
        let description = if allow {
            format!("Allow {agent_id} to use {resource_label}")
        } else {
            format!("Deny {agent_id} from using {resource_label}")
        };

        let policy_text = super::schema::build_tool_policy_text(
            agent_id, resource, effect, &policy_name, &description,
        );

        let now = chrono::Utc::now();
        let policy = Policy {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: Some(user_id.to_string()),
            name: policy_name.clone(),
            description,
            policy_text: policy_text.clone(),
            enabled: true,
            created_at: now,
            updated_at: now,
        };
        self.repo.create(&policy).await?;

        let mut verify_texts = remaining_texts;
        verify_texts.push(policy_text);
        let verify_combined = verify_texts.join("\n");
        let verify_set = PolicySet::from_str(&verify_combined)
            .map_err(|e| AppError::Internal(format!("Failed to parse policies: {e}")))?;

        let final_allowed = self.is_permitted(agent_id, resource, &verify_set)?;

        if final_allowed != allow {
            return Err(AppError::Validation(format!(
                "Cannot {effect} '{resource_label}' for agent '{agent_id}' — conflicting policies exist. Use the system agent to manage complex policy rules."
            )));
        }

        self.invalidate_cache(user_id).await;
        Ok(())
    }

    fn is_permitted(
        &self,
        agent_id: &str,
        resource: &PolicyResource,
        policy_set: &PolicySet,
    ) -> Result<bool, AppError> {
        let (entities, resource_uid) = match resource {
            PolicyResource::Tool { id, group } => {
                (build_tool_entities(id, group), tool_entity_uid(id))
            }
            PolicyResource::ToolGroup { group } => {
                (build_tool_entities(group, group), tool_entity_uid(group))
            }
        };
        let principal = agent_entity_uid(agent_id);
        let action_uid = action_entity_uid("invoke_tool");
        let context = Context::empty();

        let request = Request::new(principal, action_uid, resource_uid, context, Some(&self.schema))
            .map_err(|e| AppError::Internal(format!("Policy request error: {e}")))?;

        let authorizer = Authorizer::new();
        let response = authorizer.is_authorized(&request, policy_set, &entities);
        Ok(response.decision() == Decision::Allow)
    }

    pub async fn create_policy(
        &self,
        user_id: &str,
        policy_text: &str,
    ) -> Result<Policy, AppError> {
        self.validate_policy_text(policy_text)?;

        let (name, description) = extract_annotations(policy_text);
        let name = name.ok_or_else(|| {
            AppError::Validation("Policy must have an @id annotation".into())
        })?;

        if self.repo.find_by_name(user_id, &name).await?.is_some() {
            return Err(AppError::Validation(format!(
                "Policy with @id \"{name}\" already exists"
            )));
        }

        let now = chrono::Utc::now();
        let policy = Policy {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: Some(user_id.to_string()),
            name,
            description: description.unwrap_or_default(),
            policy_text: policy_text.to_string(),
            enabled: true,
            created_at: now,
            updated_at: now,
        };

        let created = self.repo.create(&policy).await?;
        self.invalidate_cache(user_id).await;
        Ok(created)
    }

    pub async fn update_policy(
        &self,
        user_id: &str,
        id: &str,
        policy_text: &str,
    ) -> Result<Policy, AppError> {
        self.validate_policy_text(policy_text)?;

        let mut policy = self
            .repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Policy not found".into()))?;

        if policy.user_id.as_deref() != Some(user_id) {
            return Err(AppError::Forbidden("Not your policy".into()));
        }

        let (name, description) = extract_annotations(policy_text);
        if let Some(ref new_name) = name
            && new_name != &policy.name
            && self.repo.find_by_name(user_id, new_name).await?.is_some()
        {
            return Err(AppError::Validation(format!(
                "Policy with @id \"{new_name}\" already exists"
            )));
        }

        policy.policy_text = policy_text.to_string();
        if let Some(name) = name {
            policy.name = name;
        }
        if let Some(desc) = description {
            policy.description = desc;
        }
        policy.updated_at = chrono::Utc::now();

        let updated = self.repo.update(&policy).await?;
        self.invalidate_cache(user_id).await;
        Ok(updated)
    }

    pub async fn delete_policy(&self, user_id: &str, id: &str) -> Result<(), AppError> {
        let policy = self
            .repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Policy not found".into()))?;

        if policy.user_id.as_deref() != Some(user_id) {
            return Err(AppError::Forbidden("Not your policy".into()));
        }

        self.repo.delete(id).await?;
        self.invalidate_cache(user_id).await;
        Ok(())
    }

    pub async fn find_by_name(
        &self,
        user_id: &str,
        name: &str,
    ) -> Result<Option<Policy>, AppError> {
        self.repo.find_by_name(user_id, name).await
    }

    pub async fn delete_policy_by_name(
        &self,
        user_id: &str,
        name: &str,
    ) -> Result<(), AppError> {
        let policy = self
            .repo
            .find_by_name(user_id, name)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Policy with @id \"{name}\" not found")))?;

        self.repo.delete(&policy.id).await?;
        self.invalidate_cache(user_id).await;
        Ok(())
    }

    pub async fn list_system_policies(&self) -> Result<Vec<Policy>, AppError> {
        self.repo.find_system_policies().await
    }

    pub async fn list_policies(&self, user_id: &str) -> Result<Vec<Policy>, AppError> {
        self.repo.find_by_user_id(user_id).await
    }

    pub async fn get_policy(&self, user_id: &str, id: &str) -> Result<Policy, AppError> {
        let policy = self
            .repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Policy not found".into()))?;

        if policy.user_id.as_deref() != Some(user_id) {
            return Err(AppError::Forbidden("Not your policy".into()));
        }

        Ok(policy)
    }

    pub fn validate_policy_text(&self, policy_text: &str) -> Result<(), AppError> {
        super::validation::validate_syntax(policy_text)
    }

    pub async fn validate_policy_entities(
        &self,
        user_id: &str,
        policy_text: &str,
    ) -> Result<Vec<String>, AppError> {
        let all_groups = self.tool_manager.tool_groups(user_id).await;
        let all_defs = self.tool_manager.definitions(user_id).await;
        super::validation::validate_entities(policy_text, &all_groups, &all_defs)
    }

    pub async fn delete_agent_policies(
        &self,
        user_id: &str,
        agent_id: &str,
    ) -> Result<(), AppError> {
        let all_policies = self.repo.find_by_user_id(user_id).await?;
        let mut ids_to_delete = Vec::new();

        for policy in &all_policies {
            if references_agent(&policy.policy_text, agent_id) {
                ids_to_delete.push(policy.id.clone());
            }
        }

        if !ids_to_delete.is_empty() {
            self.repo.delete_by_ids(&ids_to_delete).await?;
            self.invalidate_cache(user_id).await;
        }

        Ok(())
    }

    async fn build_policy_set(&self, user_id: &str) -> Result<Arc<CachedPolicySet>, AppError> {
        let key = user_id.to_string();

        if let Some(cached) = self.policy_cache.get(&key).await {
            return Ok(cached);
        }

        let system_policies = self.repo.find_system_policies().await?;
        let user_policies = self.repo.find_by_user_id(user_id).await?;

        let mut combined = String::new();
        for policy in system_policies.iter().chain(user_policies.iter()) {
            if policy.enabled {
                combined.push_str(&policy.policy_text);
                combined.push('\n');
            }
        }

        let mut policy_set = PolicySet::from_str(&combined)
            .map_err(|e| AppError::Internal(format!("Failed to parse policies: {e}")))?;

        for policy in self.managed_policies.read().unwrap().iter() {
            policy_set
                .add(policy.clone())
                .map_err(|e| AppError::Internal(format!("Failed to add managed policy: {e}")))?;
        }

        let cached = Arc::new(CachedPolicySet { policy_set });
        self.policy_cache.insert(key, cached.clone()).await;
        Ok(cached)
    }

    pub async fn sync_base_policies(&self) -> Result<(), AppError> {
        let base_policy_set = PolicySet::from_str(BASE_POLICIES)
            .map_err(|e| AppError::Internal(format!("Failed to parse base policies: {e}")))?;

        let existing = self.repo.find_system_policies().await?;
        let existing_ids: Vec<String> = existing.iter().map(|p| p.id.clone()).collect();
        if !existing_ids.is_empty() {
            self.repo.delete_by_ids(&existing_ids).await?;
        }

        for policy in base_policy_set.policies() {
            let Some(id) = policy.annotation("id") else {
                continue;
            };

            let description = policy.annotation("description").unwrap_or("").to_string();
            let policy_text = policy.to_string();
            let now = chrono::Utc::now();

            let record = Policy {
                id: uuid::Uuid::new_v4().to_string(),
                user_id: None,
                name: id.to_string(),
                description,
                policy_text,
                enabled: true,
                created_at: now,
                updated_at: now,
            };

            self.repo.create(&record).await?;
            tracing::info!(policy_id = id, "Synced base policy");
        }

        self.invalidate_all_caches().await;
        Ok(())
    }

    pub async fn invalidate_cache(&self, user_id: &str) {
        self.policy_cache.invalidate(user_id).await;
        let prefix = format!("{user_id}:");
        for entry in self.sandbox_cache.iter() {
            if entry.0.starts_with(&prefix) {
                self.sandbox_cache.invalidate(&*entry.0).await;
            }
        }
    }

    pub async fn invalidate_all_caches(&self) {
        self.policy_cache.invalidate_all();
        self.sandbox_cache.invalidate_all();
    }
}
