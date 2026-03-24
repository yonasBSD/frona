use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::agent::config::parse_frontmatter;
use crate::storage::StorageService;

#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub path: String,
}

#[derive(Clone)]
pub struct SkillResolver {
    config_dir: PathBuf,
    installed_dir: Option<PathBuf>,
    storage: StorageService,
}

impl SkillResolver {
    pub fn new(config_dir: impl Into<PathBuf>, storage: StorageService) -> Self {
        Self {
            config_dir: config_dir.into(),
            installed_dir: None,
            storage,
        }
    }

    pub fn with_installed_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.installed_dir = Some(dir.into());
        self
    }

    pub fn installed_dir(&self) -> Option<&Path> {
        self.installed_dir.as_deref()
    }

    pub fn builtin_skills_dir(&self) -> PathBuf {
        self.config_dir.join("skills")
    }

    /// Resolution order:
    /// 1. Agent Workspace FS
    /// 2. Installed skills dir (data/skills/) — filtered by agent_skills when non-empty
    /// 3. Built-in FS (resources/skills/) — filtered by agent_skills when non-empty
    pub fn list(&self, agent_id: &str, agent_skills: &[String]) -> Vec<Skill> {
        let mut seen = HashMap::new();

        // Tier 1: Agent workspace FS (always included)
        let ws = self.storage.agent_workspace(agent_id);
        for name in ws.read_dir("skills") {
            if seen.contains_key(&name) {
                continue;
            }
            let skill_path = format!("skills/{name}/SKILL.md");
            if let Some(content) = ws.read(&skill_path) {
                let parsed = parse_frontmatter(&content);
                let description = parsed
                    .metadata
                    .get("description")
                    .cloned()
                    .unwrap_or_default();

                let dir_path = ws
                    .resolve_path(&format!("skills/{name}"))
                    .and_then(|p| std::fs::canonicalize(&p).ok())
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();

                seen.insert(name.clone(), Skill {
                    name,
                    description,
                    path: dir_path,
                });
            }
        }

        // Tier 2: Installed skills (filtered when agent_skills is non-empty)
        if let Some(dir) = &self.installed_dir {
            for skill in self.scan_fs_skills(dir) {
                if !agent_skills.is_empty() && !agent_skills.contains(&skill.name) {
                    continue;
                }
                seen.entry(skill.name.clone()).or_insert(skill);
            }
        }

        // Tier 3: Built-in FS (filtered when agent_skills is non-empty)
        for skill in self.scan_fs_skills(&self.config_dir.join("skills")) {
            if !agent_skills.is_empty() && !agent_skills.contains(&skill.name) {
                continue;
            }
            seen.entry(skill.name.clone()).or_insert(skill);
        }

        seen.into_values().collect()
    }

    fn scan_fs_skills(&self, dir: &Path) -> Vec<Skill> {
        let mut results = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return results;
        };

        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }

            let skill_name = entry.file_name().to_string_lossy().to_string();
            let skill_md = entry.path().join("SKILL.md");

            if let Ok(content) = std::fs::read_to_string(&skill_md) {
                let parsed = parse_frontmatter(&content);
                let description = parsed
                    .metadata
                    .get("description")
                    .cloned()
                    .unwrap_or_default();

                let abs_dir = std::fs::canonicalize(entry.path())
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| entry.path().to_string_lossy().into_owned());

                results.push(Skill {
                    name: skill_name,
                    description,
                    path: abs_dir,
                });
            }
        }

        results
    }
}
