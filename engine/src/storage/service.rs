use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::core::config::Config;
use crate::core::error::AppError;

use super::models::{FileEntry, SearchTarget};
use super::path::{Namespace, VirtualPath, validate_no_traversal};
use super::workspace::Workspace;

#[derive(Clone)]
pub struct StorageService {
    workspaces_path: PathBuf,
    files_path: PathBuf,
    shared_agents_dir: PathBuf,
}

impl StorageService {
    pub fn new(config: &Config) -> Self {
        let shared_config_dir = PathBuf::from(&config.storage.shared_config_dir);
        Self {
            workspaces_path: PathBuf::from(&config.storage.workspaces_path),
            files_path: PathBuf::from(&config.storage.files_path),
            shared_agents_dir: shared_config_dir.join("agents"),
        }
    }

    pub fn agent_workspace(&self, agent_id: &str) -> Workspace {
        let sanitized = agent_id.replace(['/', '\\', ':', '\0'], "_");
        let workspace_path = self.workspaces_path.join(&sanitized);
        let shared_path = self.shared_agents_dir.join(&sanitized);

        Workspace::new(vec![workspace_path], Some(shared_path))
    }

    pub fn user_workspace(&self, username: &str) -> Workspace {
        let user_path = self.files_path.join(username);
        Workspace::new(vec![user_path], None)
    }

    pub fn resolve(&self, path: &VirtualPath) -> Result<PathBuf, AppError> {
        let resolved = match &path.namespace {
            Namespace::User(name) => {
                let resolved = self.files_path.join(name).join(&path.relative);
                validate_no_traversal(&resolved, self.files_path.to_str().unwrap_or(""))?;
                resolved
            }
            Namespace::Agent(name) => {
                let resolved = self.workspaces_path.join(name).join(&path.relative);
                validate_no_traversal(&resolved, self.workspaces_path.to_str().unwrap_or(""))?;
                resolved
            }
        };

        // Return absolute path so agents can access files regardless of working directory
        if resolved.is_absolute() {
            Ok(resolved)
        } else {
            Ok(std::env::current_dir()
                .map(|cwd| cwd.join(&resolved))
                .unwrap_or(resolved))
        }
    }

    pub async fn list_dir(&self, dir: &Path, parent_id: &str) -> Result<Vec<FileEntry>, AppError> {
        if !dir.exists() {
            return Ok(vec![]);
        }

        let mut entries = Vec::new();
        let mut read_dir = tokio::fs::read_dir(dir)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
        {
            let metadata = entry
                .metadata()
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

            let name = entry.file_name().to_string_lossy().into_owned();
            let id = if parent_id.is_empty() || parent_id == "/" {
                format!("/{name}")
            } else {
                format!("{parent_id}/{name}")
            };

            let modified: DateTime<Utc> = metadata
                .modified()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                .into();

            entries.push(FileEntry {
                id,
                size: if metadata.is_dir() {
                    0
                } else {
                    metadata.len()
                },
                date: modified.to_rfc3339(),
                entry_type: if metadata.is_dir() {
                    "folder".into()
                } else {
                    "file".into()
                },
                parent: if parent_id.is_empty() {
                    "/".into()
                } else {
                    parent_id.into()
                },
            });
        }

        Ok(entries)
    }

    pub async fn search(
        &self,
        targets: Vec<SearchTarget>,
        query: &str,
    ) -> Result<Vec<FileEntry>, AppError> {
        let q = query.to_lowercase();

        tokio::task::spawn_blocking(move || {
            let mut results = Vec::new();
            for target in &targets {
                results.extend(search_dir(&target.dir, &target.root, &q, &target.source));
            }
            results
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))
    }
}

fn search_dir(dir: &Path, root: &Path, query: &str, source: &str) -> Vec<FileEntry> {
    let mut results = Vec::new();

    let walker = ignore::WalkBuilder::new(dir)
        .hidden(true)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .build();

    for entry in walker.flatten() {
        if entry.path() == dir {
            continue;
        }

        let name = entry.file_name().to_string_lossy();
        if !name.to_lowercase().contains(query) {
            continue;
        }

        let path = entry.path();
        let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        let id = format!("/{rel}");
        let parent_path = path.parent().unwrap_or(root);
        let parent_rel = parent_path
            .strip_prefix(root)
            .unwrap_or(parent_path)
            .to_string_lossy()
            .into_owned();
        let parent = if parent_rel.is_empty() {
            "/".to_string()
        } else {
            format!("/{parent_rel}")
        };

        let metadata = entry.metadata().ok();
        let modified: DateTime<Utc> = metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            .into();

        results.push(FileEntry {
            id: format!("{source}:{id}"),
            size: if is_dir {
                0
            } else {
                metadata.as_ref().map(|m| m.len()).unwrap_or(0)
            },
            date: modified.to_rfc3339(),
            entry_type: if is_dir { "folder" } else { "file" }.into(),
            parent: format!("{source}:{parent}"),
        });
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config::default()
    }

    fn test_service() -> StorageService {
        StorageService::new(&test_config())
    }

    #[test]
    fn resolve_user_path() {
        let svc = test_service();
        let vp = VirtualPath::user("uid-123", "report.pdf");
        let result = svc.resolve(&vp).unwrap();
        assert!(result.is_absolute());
        assert!(result.ends_with("data/files/uid-123/report.pdf"));
    }

    #[test]
    fn resolve_agent_path() {
        let svc = test_service();
        let vp = VirtualPath::agent("dev", "output.csv");
        let result = svc.resolve(&vp).unwrap();
        assert!(result.is_absolute());
        assert!(result.ends_with("data/workspaces/dev/output.csv"));
    }

    #[test]
    fn resolve_agent_nested_path() {
        let svc = test_service();
        let vp = VirtualPath::agent("dev", "subdir/nested/file.txt");
        let result = svc.resolve(&vp).unwrap();
        assert!(result.is_absolute());
        assert!(result.ends_with("data/workspaces/dev/subdir/nested/file.txt"));
    }

    #[test]
    fn resolve_path_traversal_rejected() {
        let svc = test_service();
        let vp = VirtualPath::user("uid", "../../../etc/passwd");
        let result = svc.resolve(&vp);
        assert!(result.is_err());
    }

    fn shared_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("resources")
    }

    #[test]
    fn workspace_read_builtin() {
        let tmp = std::env::temp_dir().join("frona_storage_test_read_builtin");
        let mut config = Config::default();
        config.storage.workspaces_path = tmp.join("workspaces").to_string_lossy().into_owned();
        config.storage.shared_config_dir = shared_dir().to_string_lossy().into_owned();
        let svc = StorageService::new(&config);
        let ws = svc.agent_workspace("system");
        let content = ws.read("AGENT.md");
        assert!(content.is_some(), "Should read AGENT.md from shared layer");
        assert!(content.unwrap().contains("You're not a chatbot"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn workspace_read_not_found() {
        let tmp = std::env::temp_dir().join("frona_storage_test_read_not_found");
        let mut config = Config::default();
        config.storage.workspaces_path = tmp.join("workspaces").to_string_lossy().into_owned();
        config.storage.shared_config_dir = shared_dir().to_string_lossy().into_owned();
        let svc = StorageService::new(&config);
        let ws = svc.agent_workspace("system");
        assert!(ws.read("nonexistent.md").is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn workspace_data_layer_shadows_builtin() {
        let tmp = std::env::temp_dir().join("frona_storage_test_shadow");
        let _ = std::fs::remove_dir_all(&tmp);
        let mut config = Config::default();
        config.storage.workspaces_path = tmp.join("workspaces").to_string_lossy().into_owned();
        config.storage.shared_config_dir = shared_dir().to_string_lossy().into_owned();
        let svc = StorageService::new(&config);
        let ws = svc.agent_workspace("system");

        ws.write("AGENT.md", "Custom prompt").unwrap();
        let content = ws.read("AGENT.md").unwrap();
        assert_eq!(content, "Custom prompt");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn workspace_write_creates_file() {
        let tmp = std::env::temp_dir().join("frona_storage_test_write");
        let _ = std::fs::remove_dir_all(&tmp);
        let mut config = Config::default();
        config.storage.workspaces_path = tmp.join("workspaces").to_string_lossy().into_owned();
        config.storage.shared_config_dir = shared_dir().to_string_lossy().into_owned();
        let svc = StorageService::new(&config);
        let ws = svc.agent_workspace("test_agent");

        ws.write("test.md", "hello").unwrap();
        assert_eq!(ws.read("test.md").unwrap(), "hello");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn workspace_exists_builtin() {
        let tmp = std::env::temp_dir().join("frona_storage_test_exists_builtin");
        let mut config = Config::default();
        config.storage.workspaces_path = tmp.join("workspaces").to_string_lossy().into_owned();
        config.storage.shared_config_dir = shared_dir().to_string_lossy().into_owned();
        let svc = StorageService::new(&config);
        let ws = svc.agent_workspace("system");
        assert!(ws.exists("AGENT.md"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn workspace_read_dir_builtin() {
        let tmp = std::env::temp_dir().join("frona_storage_test_read_dir_builtin");
        let mut config = Config::default();
        config.storage.workspaces_path = tmp.join("workspaces").to_string_lossy().into_owned();
        config.storage.shared_config_dir = shared_dir().to_string_lossy().into_owned();
        let svc = StorageService::new(&config);
        let ws = svc.agent_workspace("system");
        let entries = ws.read_dir("");
        assert!(entries.contains(&"AGENT.md".to_string()));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn user_workspace_no_shared_dir() {
        let svc = test_service();
        let ws = svc.user_workspace("testuser");
        assert_eq!(ws.base_path(), Path::new("data/files/testuser"));
    }
}
