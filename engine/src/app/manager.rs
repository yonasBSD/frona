use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::Mutex;

use crate::core::error::AppError;
use crate::tool::sandbox::SandboxManager;

use super::models::{AppManifest, HealthCheck};

pub struct ProcessExit {
    pub status: Option<std::process::ExitStatus>,
    pub stderr_tail: String,
}

pub enum ProcessStatus {
    Alive,
    Dead(ProcessExit),
    NotManaged,
}

pub struct ManagedProcess {
    pub child: tokio::process::Child,
    pub port: u16,
    pub agent_id: String,
    pub manifest: AppManifest,
    pub credential_env_vars: Vec<(String, String)>,
    pub restart_count: u32,
    pub consecutive_failures: u32,
    pub log_path: Option<PathBuf>,
}

pub struct AppManager {
    processes: Arc<Mutex<HashMap<String, ManagedProcess>>>,
    allocated_ports: Arc<Mutex<HashSet<u16>>>,
    port_range: (u16, u16),
    sandbox_manager: Arc<SandboxManager>,
    last_accessed: Arc<Mutex<HashMap<String, DateTime<Utc>>>>,
}

impl AppManager {
    pub fn new(
        sandbox_manager: Arc<SandboxManager>,
        port_range_start: u16,
        port_range_end: u16,
    ) -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
            allocated_ports: Arc::new(Mutex::new(HashSet::new())),
            port_range: (port_range_start, port_range_end),
            sandbox_manager,
            last_accessed: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start_app(
        &self,
        app_id: &str,
        agent_id: &str,
        command: &str,
        manifest: &AppManifest,
        credential_env_vars: Vec<(String, String)>,
    ) -> Result<(u16, u32), AppError> {
        let port = self.allocate_port().await?;

        let (child, log_path) = self.spawn_process(
            agent_id,
            command,
            port,
            manifest,
            &credential_env_vars,
        )?;

        let pid = child.id().unwrap_or(0);

        let managed = ManagedProcess {
            child,
            port,
            agent_id: agent_id.to_string(),
            manifest: manifest.clone(),
            credential_env_vars,
            restart_count: 0,
            consecutive_failures: 0,
            log_path: Some(log_path),
        };

        self.processes
            .lock()
            .await
            .insert(app_id.to_string(), managed);

        Ok((port, pid))
    }

    pub async fn stop_app(&self, app_id: &str) -> Result<(), AppError> {
        let mut processes = self.processes.lock().await;
        if let Some(mut proc) = processes.remove(app_id) {
            let port = proc.port;

            let _ = proc.child.kill().await;
            let _ = proc.child.wait().await;

            self.allocated_ports.lock().await.remove(&port);
        }
        Ok(())
    }

    pub async fn health_check(&self, port: u16, health_check: &HealthCheck) -> bool {
        let url = format!("http://127.0.0.1:{}{}", port, health_check.path);
        let timeout = std::time::Duration::from_secs(health_check.effective_timeout());

        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build();

        let client = match client {
            Ok(c) => c,
            Err(_) => return false,
        };

        client.get(&url).send().await.is_ok_and(|r| r.status().is_success())
    }

    pub async fn check_process(&self, app_id: &str) -> ProcessStatus {
        let mut processes = self.processes.lock().await;
        let Some(proc) = processes.get_mut(app_id) else {
            return ProcessStatus::NotManaged;
        };
        match proc.child.try_wait() {
            Ok(None) => ProcessStatus::Alive,
            Ok(Some(status)) => {
                let stderr_tail = proc
                    .log_path
                    .as_ref()
                    .map(|p| read_tail(p, 4096))
                    .unwrap_or_default();
                ProcessStatus::Dead(ProcessExit {
                    status: Some(status),
                    stderr_tail,
                })
            }
            Err(_) => ProcessStatus::Dead(ProcessExit {
                status: None,
                stderr_tail: String::new(),
            }),
        }
    }

    pub async fn restart_app(
        &self,
        app_id: &str,
        agent_id: &str,
    ) -> Result<Option<(u16, u32)>, AppError> {
        let (command, manifest, creds) = {
            let processes = self.processes.lock().await;
            match processes.get(app_id) {
                Some(proc) => (
                    proc.manifest.command.clone(),
                    proc.manifest.clone(),
                    proc.credential_env_vars.clone(),
                ),
                None => return Ok(None),
            }
        };

        self.stop_app(app_id).await?;

        if let Some(cmd) = command {
            let (port, pid) = self.start_app(app_id, agent_id, &cmd, &manifest, creds).await?;
            Ok(Some((port, pid)))
        } else {
            Ok(None)
        }
    }

    pub async fn try_restart_crashed(
        &self,
        app_id: &str,
        max_restarts: u32,
    ) -> Result<Option<(u16, u32)>, AppError> {
        let info = {
            let mut processes = self.processes.lock().await;
            if let Some(proc) = processes.get_mut(app_id) {
                if proc.restart_count >= max_restarts {
                    return Ok(None);
                }

                let should_restart = match proc.manifest.effective_restart_policy() {
                    "always" => true,
                    "on_failure" => {
                        match proc.child.try_wait() {
                            Ok(Some(status)) => !status.success(),
                            _ => true,
                        }
                    }
                    _ => false,
                };

                if !should_restart {
                    return Ok(None);
                }

                proc.restart_count += 1;
                let restart_count = proc.restart_count;
                Some((
                    proc.agent_id.clone(),
                    proc.manifest.command.clone(),
                    proc.manifest.clone(),
                    proc.credential_env_vars.clone(),
                    restart_count,
                ))
            } else {
                None
            }
        };

        if let Some((agent_id, command, manifest, creds, restart_count)) = info {
            self.stop_app(app_id).await?;
            if let Some(cmd) = command {
                let (port, pid) =
                    self.start_app(app_id, &agent_id, &cmd, &manifest, creds).await?;
                self.processes
                    .lock()
                    .await
                    .get_mut(app_id)
                    .expect("process just inserted by start_app")
                    .restart_count = restart_count;
                return Ok(Some((port, pid)));
            }
        }

        Ok(None)
    }

    pub async fn remove_process(&self, app_id: &str) {
        let mut processes = self.processes.lock().await;
        if let Some(proc) = processes.remove(app_id) {
            self.allocated_ports.lock().await.remove(&proc.port);
        }
    }

    pub async fn record_access(&self, app_id: &str) {
        self.last_accessed
            .lock()
            .await
            .insert(app_id.to_string(), Utc::now());
    }

    pub async fn get_last_accessed(&self, app_id: &str) -> Option<DateTime<Utc>> {
        self.last_accessed.lock().await.get(app_id).copied()
    }

    pub async fn flush_access_times(&self) -> HashMap<String, DateTime<Utc>> {
        let mut map = self.last_accessed.lock().await;
        std::mem::take(&mut *map)
    }

    pub async fn get_process_port(&self, app_id: &str) -> Option<u16> {
        self.processes.lock().await.get(app_id).map(|p| p.port)
    }

    pub async fn has_process(&self, app_id: &str) -> bool {
        self.processes.lock().await.contains_key(app_id)
    }

    pub async fn get_managed_app_ids(&self) -> Vec<String> {
        self.processes.lock().await.keys().cloned().collect()
    }

    async fn allocate_port(&self) -> Result<u16, AppError> {
        let mut ports = self.allocated_ports.lock().await;
        for port in self.port_range.0..self.port_range.1 {
            if !ports.contains(&port) {
                ports.insert(port);
                return Ok(port);
            }
        }
        Err(AppError::Internal("No available ports in range".into()))
    }


    fn spawn_process(
        &self,
        agent_id: &str,
        command: &str,
        port: u16,
        manifest: &AppManifest,
        credential_env_vars: &[(String, String)],
    ) -> Result<(tokio::process::Child, PathBuf), AppError> {
        let mut network_dests = vec![format!("127.0.0.1:{port}")];
        if let Some(dests) = &manifest.network_destinations {
            for dest in dests {
                network_dests.push(format!("{}:{}", dest.host, dest.port));
            }
        }

        let mut env_vars = vec![("PORT".to_string(), port.to_string())];
        env_vars.extend(credential_env_vars.iter().cloned());

        let mut sandbox = self
            .sandbox_manager
            .get_sandbox(agent_id, true, network_dests)
            .with_bind_ports(vec![port])
            .with_extra_env_vars(env_vars);

        if let Some(paths) = &manifest.read_paths {
            sandbox = sandbox.with_read_paths(paths.clone());
        }

        let app_dir = sandbox.path().join("apps").join(&manifest.id);
        let app_log_dir = app_dir.join("logs");
        std::fs::create_dir_all(&app_log_dir)
            .map_err(|e| AppError::Tool(format!("Failed to create app directory: {e}")))?;

        let has_source_files = std::fs::read_dir(&app_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .any(|e| e.file_name() != "logs")
            })
            .unwrap_or(false);

        if !has_source_files {
            return Err(AppError::Tool(format!(
                "No source files found in apps/{}/ — write your app code there before deploying",
                manifest.id
            )));
        }

        let app_dir_str = app_dir.to_string_lossy().into_owned();

        let mut extra_path_dirs = Vec::new();
        let app_venv_bin = app_dir.join(".venv").join("bin");
        if app_venv_bin.exists() {
            extra_path_dirs.push(app_venv_bin.to_string_lossy().into_owned());
        }
        let app_bin = app_dir.join("bin");
        if app_bin.exists() {
            extra_path_dirs.push(app_bin.to_string_lossy().into_owned());
        }

        let log_path = app_log_dir.join("app.log");
        let log_file = std::fs::File::create(&log_path)
            .map_err(|e| AppError::Tool(format!("Failed to create app log: {e}")))?;
        let log_file_clone = log_file
            .try_clone()
            .map_err(|e| AppError::Tool(format!("Failed to clone log file handle: {e}")))?;

        let child = sandbox.spawn(
            "bash",
            &["-c", command],
            Some(&app_dir_str),
            extra_path_dirs,
            std::process::Stdio::from(log_file),
            std::process::Stdio::from(log_file_clone),
        )?;

        Ok((child, log_path))
    }
}

fn read_tail(path: &PathBuf, max_bytes: u64) -> String {
    use std::io::{Read, Seek, SeekFrom};

    let Ok(mut file) = std::fs::File::open(path) else {
        return String::new();
    };
    let Ok(metadata) = file.metadata() else {
        return String::new();
    };
    let len = metadata.len();
    if len > max_bytes {
        let _ = file.seek(SeekFrom::End(-(max_bytes as i64)));
    }
    let mut buf = String::new();
    let _ = file.read_to_string(&mut buf);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manager(port_start: u16, port_end: u16) -> AppManager {
        AppManager::new(
            Arc::new(SandboxManager::new("/tmp/test_workspaces", true, Arc::new(crate::tool::sandbox::driver::resource_monitor::SystemResourceManager::new(60.0, 60.0, 60.0, 60.0)))),
            port_start,
            port_end,
        )
    }

    #[tokio::test]
    async fn test_allocate_port_returns_sequential() {
        let manager = test_manager(5000, 5003);

        let p1 = manager.allocate_port().await.unwrap();
        let p2 = manager.allocate_port().await.unwrap();
        let p3 = manager.allocate_port().await.unwrap();

        assert_eq!(p1, 5000);
        assert_eq!(p2, 5001);
        assert_eq!(p3, 5002);

        let p4 = manager.allocate_port().await;
        assert!(p4.is_err());
    }

    #[tokio::test]
    async fn test_allocate_port_reuses_after_free() {
        let manager = test_manager(5000, 5002);

        let p1 = manager.allocate_port().await.unwrap();
        assert_eq!(p1, 5000);

        manager.allocated_ports.lock().await.remove(&5000);

        let p2 = manager.allocate_port().await.unwrap();
        assert_eq!(p2, 5000);
    }

    #[tokio::test]
    async fn test_record_and_get_access() {
        let manager = test_manager(5000, 5100);

        assert!(manager.get_last_accessed("app1").await.is_none());

        manager.record_access("app1").await;
        assert!(manager.get_last_accessed("app1").await.is_some());
    }

    #[tokio::test]
    async fn test_flush_access_times() {
        let manager = test_manager(5000, 5100);

        manager.record_access("app1").await;
        manager.record_access("app2").await;

        let flushed = manager.flush_access_times().await;
        assert_eq!(flushed.len(), 2);
        assert!(flushed.contains_key("app1"));

        assert!(manager.get_last_accessed("app1").await.is_none());
    }

    #[tokio::test]
    async fn test_restart_count_preserved_after_crash_restart() {
        let tmp = tempfile::tempdir().expect("failed to create temp dir");
        let workspaces = tmp.path();

        let app_dir = workspaces.join("agent-1").join("apps").join("test-app");
        std::fs::create_dir_all(&app_dir).unwrap();
        std::fs::write(app_dir.join("run.sh"), "#!/bin/sh\ntrue").unwrap();

        let manager = AppManager::new(
            Arc::new(SandboxManager::new(workspaces, true, Arc::new(crate::tool::sandbox::driver::resource_monitor::SystemResourceManager::new(60.0, 60.0, 60.0, 60.0)))),
            6000,
            6010,
        );

        let child = tokio::process::Command::new("true")
            .spawn()
            .expect("failed to spawn dummy process");

        let manifest = crate::app::models::AppManifest {
            id: "test-app".to_string(),
            name: "Test".to_string(),
            description: None,
            icon: None,
            kind: None,
            command: Some("true".to_string()),
            restart_policy: Some("always".to_string()),
            health_check: None,
            resources: None,
            static_dir: None,
            expose: None,
            network_destinations: None,
            read_paths: None,
            write_paths: None,
            credentials: None,
            hibernate: None,
        };

        let port = manager.allocate_port().await.unwrap();
        manager
            .processes
            .lock()
            .await
            .insert(
                "test-app".to_string(),
                ManagedProcess {
                    child,
                    port,
                    agent_id: "agent-1".to_string(),
                    manifest,
                    credential_env_vars: Vec::new(),
                    restart_count: 0,
                    consecutive_failures: 0,
                    log_path: None,
                },
            );

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let result = manager.try_restart_crashed("test-app", 3).await.unwrap();
        assert!(result.is_some(), "restart should succeed");

        let processes = manager.processes.lock().await;
        let proc = processes.get("test-app").expect("process should exist");
        assert_eq!(proc.restart_count, 1, "restart_count should be preserved as 1");
    }

    #[test]
    fn test_app_dir_structure() {
        let workspace = PathBuf::from("/tmp/workspaces/agent_123");
        let manifest_id = "my-dashboard";

        let app_dir = workspace.join("apps").join(manifest_id);
        let log_dir = app_dir.join("logs");
        let log_path = log_dir.join("app.log");

        assert_eq!(
            app_dir,
            PathBuf::from("/tmp/workspaces/agent_123/apps/my-dashboard")
        );
        assert_eq!(
            log_dir,
            PathBuf::from("/tmp/workspaces/agent_123/apps/my-dashboard/logs")
        );
        assert_eq!(
            log_path,
            PathBuf::from("/tmp/workspaces/agent_123/apps/my-dashboard/logs/app.log")
        );
    }
}
