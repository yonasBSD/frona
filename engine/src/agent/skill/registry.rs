use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::core::error::AppError;

const SKILLS_SH_BASE: &str = "https://skills.sh";
const GITHUB_RAW_BASE: &str = "https://raw.githubusercontent.com";
const GITHUB_API_BASE: &str = "https://api.github.com";

#[derive(Debug, Clone, Serialize)]
pub struct RemoteSkillSummary {
    pub name: String,
    pub repo: String,
    pub avatar_url: String,
    pub installs: u64,
}

#[derive(Debug, Clone)]
pub struct FetchedSkillFile {
    pub path: String,
    pub content: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct FetchedSkill {
    pub name: String,
    pub description: String,
    pub content: String,
    pub repo: String,
    pub sha: String,
    /// Additional files in the skill directory (scripts, templates, etc.)
    pub files: Vec<FetchedSkillFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoSkillEntry {
    pub name: String,
    pub sha: String,
}

#[derive(Debug, Deserialize)]
struct SkillsShResponse {
    skills: Vec<SkillsShEntry>,
    #[allow(dead_code)]
    count: u64,
}

#[derive(Debug, Deserialize)]
struct SkillsShEntry {
    #[serde(rename = "skillId")]
    #[allow(dead_code)]
    skill_id: String,
    name: String,
    source: String,
    installs: u64,
}

#[derive(Debug, Deserialize)]
struct GitHubTreeResponse {
    tree: Vec<GitHubTreeEntry>,
}

#[derive(Debug, Deserialize)]
struct GitHubTreeEntry {
    path: String,
    sha: String,
    #[serde(rename = "type")]
    entry_type: String,
}

#[derive(Clone)]
pub struct SkillRegistryClient {
    client: reqwest::Client,
}

impl Default for SkillRegistryClient {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillRegistryClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("frona-skill-registry")
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<RemoteSkillSummary>, AppError> {
        let url = format!("{SKILLS_SH_BASE}/api/search");

        let resp = self.client.get(&url)
            .query(&[("q", query), ("limit", &limit.to_string())])
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("skills.sh search failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(AppError::Internal(format!("skills.sh returned status {}", resp.status())));
        }

        let data: SkillsShResponse = resp.json().await
            .map_err(|e| AppError::Internal(format!("Failed to parse skills.sh response: {e}")))?;

        Ok(data.skills.into_iter().map(|entry| {
            let avatar_url = derive_avatar_url(&entry.source);
            RemoteSkillSummary {
                name: entry.name,
                repo: entry.source,
                avatar_url,
                installs: entry.installs,
            }
        }).collect())
    }

    pub async fn browse_repo(&self, owner_repo: &str) -> Result<Vec<RepoSkillEntry>, AppError> {
        let shas = self.get_tree_shas(owner_repo).await?;
        Ok(shas.into_iter().map(|(name, sha)| RepoSkillEntry { name, sha }).collect())
    }

    pub async fn fetch_skill(&self, repo: &str, skill_name: &str) -> Result<FetchedSkill, AppError> {
        // Try direct path first, then with skills/ prefix (some repos nest under skills/)
        let prefixes = ["", "skills/"];

        let mut content = None;
        let mut skill_prefix = "";
        for prefix in &prefixes {
            let path = format!("{prefix}{skill_name}/SKILL.md");
            let url = format!("{GITHUB_RAW_BASE}/{repo}/main/{path}");
            let resp = self.client.get(&url)
                .send()
                .await
                .map_err(|e| AppError::Internal(format!("Failed to fetch SKILL.md: {e}")))?;

            if resp.status().is_success() {
                content = Some(resp.text().await
                    .map_err(|e| AppError::Internal(format!("Failed to read SKILL.md content: {e}")))?);
                skill_prefix = prefix;
                break;
            }
        }

        let content = content.ok_or_else(|| AppError::NotFound(format!("Skill '{skill_name}' not found in {repo}")))?;

        let parsed = agent_skills::Skill::parse(&content)
            .map_err(|e| AppError::Validation(format!("Invalid SKILL.md: {e}")))?;

        let sha = self.get_skill_sha(repo, skill_name).await.unwrap_or_default();

        // Fetch all additional files in the skill directory
        let files = self.fetch_skill_files(repo, skill_name, skill_prefix).await.unwrap_or_default();

        Ok(FetchedSkill {
            name: parsed.name().as_str().to_string(),
            description: parsed.description().as_str().to_string(),
            content,
            repo: repo.to_string(),
            sha,
            files,
        })
    }

    async fn fetch_skill_files(&self, repo: &str, skill_name: &str, prefix: &str) -> Result<Vec<FetchedSkillFile>, AppError> {
        // Get the skill directory's tree SHA
        let shas = self.get_tree_shas(repo).await?;
        let dir_sha = shas.get(skill_name)
            .ok_or_else(|| AppError::NotFound(format!("Skill dir '{skill_name}' not in tree")))?;

        // Fetch recursive tree for the skill directory
        let url = format!("{GITHUB_API_BASE}/repos/{repo}/git/trees/{dir_sha}?recursive=1");
        let tree = self.fetch_tree(&url).await?;

        let mut files = Vec::new();
        for entry in &tree.tree {
            if entry.entry_type != "blob" || entry.path == "SKILL.md" {
                continue;
            }

            let raw_url = format!("{GITHUB_RAW_BASE}/{repo}/main/{prefix}{skill_name}/{}", entry.path);
            let resp = self.client.get(&raw_url)
                .send()
                .await
                .map_err(|e| AppError::Internal(format!("Failed to fetch {}: {e}", entry.path)))?;

            if !resp.status().is_success() {
                continue;
            }

            let bytes = resp.bytes().await
                .map_err(|e| AppError::Internal(format!("Failed to read {}: {e}", entry.path)))?;

            files.push(FetchedSkillFile {
                path: entry.path.clone(),
                content: bytes.to_vec(),
            });
        }

        Ok(files)
    }

    pub async fn get_tree_shas(&self, owner_repo: &str) -> Result<HashMap<String, String>, AppError> {
        let data = self.fetch_tree(&format!("{GITHUB_API_BASE}/repos/{owner_repo}/git/trees/main")).await?;

        let dirs: HashMap<String, String> = data.tree.iter()
            .filter(|e| e.entry_type == "tree")
            .map(|e| (e.path.clone(), e.sha.clone()))
            .collect();

        // If the top-level tree has a "skills" subdirectory, recurse into it
        if let Some(skills_entry) = data.tree.iter().find(|e| e.entry_type == "tree" && e.path == "skills") {
            let nested_url = format!("{GITHUB_API_BASE}/repos/{owner_repo}/git/trees/{}", skills_entry.sha);
            if let Ok(nested) = self.fetch_tree(&nested_url).await {
                let mut result: HashMap<String, String> = nested.tree.into_iter()
                    .filter(|e| e.entry_type == "tree")
                    .map(|e| (e.path, e.sha))
                    .collect();
                // Top-level dirs (excluding "skills" itself) take precedence
                for (k, v) in dirs {
                    if k != "skills" {
                        result.insert(k, v);
                    }
                }
                return Ok(result);
            }
        }

        Ok(dirs)
    }

    async fn fetch_tree(&self, url: &str) -> Result<GitHubTreeResponse, AppError> {
        let resp = self.client.get(url)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("GitHub API request failed: {e}")))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(AppError::NotFound("Repository or tree not found".into()));
        }

        if !resp.status().is_success() {
            return Err(AppError::Internal(format!("GitHub API returned status {}", resp.status())));
        }

        resp.json().await
            .map_err(|e| AppError::Internal(format!("Failed to parse GitHub trees response: {e}")))
    }

    async fn get_skill_sha(&self, repo: &str, skill_name: &str) -> Result<String, AppError> {
        let shas = self.get_tree_shas(repo).await?;
        shas.get(skill_name)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("Skill '{skill_name}' SHA not found in tree")))
    }
}

fn derive_avatar_url(repo: &str) -> String {
    let owner = repo.split('/').next().unwrap_or(repo);
    format!("https://github.com/{owner}.png")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_avatar_url() {
        assert_eq!(
            derive_avatar_url("vercel-labs/agent-skills"),
            "https://github.com/vercel-labs.png"
        );
    }

    #[test]
    fn test_derive_avatar_url_no_slash() {
        assert_eq!(derive_avatar_url("owner"), "https://github.com/owner.png");
    }
}
