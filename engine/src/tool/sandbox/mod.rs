pub mod driver;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::core::error::AppError;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use self::driver::{SandboxConfig, SandboxOutput, create_driver, execute_sandboxed};
use self::driver::resource_monitor::ResourceUsage;

pub struct SandboxManager {
    base_path: PathBuf,
    driver: Arc<dyn driver::SandboxDriver>,
    shared_read_paths: Vec<String>,
    resource_usage: ResourceUsage,
    default_timeout_secs: u64,
}

impl SandboxManager {
    pub fn new(
        base_path: impl Into<PathBuf>,
        sandbox_disabled: bool,
        max_agent_cpu_pct: f64,
        max_agent_memory_pct: f64,
        max_total_cpu_pct: f64,
        max_total_memory_pct: f64,
    ) -> Self {
        Self {
            base_path: base_path.into(),
            driver: Arc::from(create_driver(sandbox_disabled)),
            shared_read_paths: Vec::new(),
            resource_usage: ResourceUsage::new(max_agent_cpu_pct, max_agent_memory_pct, max_total_cpu_pct, max_total_memory_pct),
            default_timeout_secs: 0,
        }
    }

    pub fn with_default_timeout(mut self, secs: u64) -> Self {
        self.default_timeout_secs = secs;
        self
    }

    pub fn default_timeout_secs(&self) -> u64 {
        self.default_timeout_secs
    }

    pub fn resource_usage(&self) -> &ResourceUsage {
        &self.resource_usage
    }

    pub fn with_shared_read_paths(mut self, paths: Vec<String>) -> Self {
        self.shared_read_paths = paths;
        self
    }

    pub fn get_sandbox(
        &self,
        agent_id: &str,
        network_access: bool,
        allowed_network_destinations: Vec<String>,
    ) -> Sandbox {
        let sanitized = agent_id.replace(['/', '\\', ':', '\0'], "_");
        let path = self.base_path.join(&sanitized);
        Sandbox {
            path,
            driver: Arc::clone(&self.driver),
            network_access,
            allowed_network_destinations,
            extra_env_vars: Vec::new(),
            shared_read_paths: self.shared_read_paths.clone(),
            shared_write_paths: Vec::new(),
            agent_id: agent_id.to_string(),
        }
    }
}

pub struct Sandbox {
    path: PathBuf,
    driver: Arc<dyn driver::SandboxDriver>,
    network_access: bool,
    allowed_network_destinations: Vec<String>,
    extra_env_vars: Vec<(String, String)>,
    shared_read_paths: Vec<String>,
    shared_write_paths: Vec<String>,
    agent_id: String,
}

impl Sandbox {
    pub fn with_extra_env_vars(mut self, vars: Vec<(String, String)>) -> Self {
        self.extra_env_vars = vars;
        self
    }

    pub fn with_read_paths(mut self, paths: Vec<String>) -> Self {
        self.shared_read_paths.extend(paths);
        self
    }

    pub fn with_write_paths(mut self, paths: Vec<String>) -> Self {
        self.shared_write_paths.extend(paths);
        self
    }
}

impl Sandbox {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn venv_path(&self) -> PathBuf {
        self.path.join(".venv")
    }

    pub fn setup(&self) -> Result<(), AppError> {
        if !self.path.exists() {
            std::fs::create_dir_all(&self.path).map_err(|e| {
                AppError::Tool(format!(
                    "Failed to create sandbox dir {}: {e}",
                    self.path.display()
                ))
            })?;
        }
        self.setup_venv();
        Ok(())
    }

    fn setup_venv(&self) {
        let venv = self.venv_path();
        if venv.exists() {
            return;
        }

        let result = std::process::Command::new("python3")
            .args(["-m", "venv", "--system-site-packages", ".venv"])
            .current_dir(&self.path)
            .output();

        match result {
            Ok(output) if output.status.success() => {
                tracing::info!(path = %venv.display(), "Created Python venv");
            }
            Ok(output) => {
                tracing::warn!(
                    stderr = String::from_utf8_lossy(&output.stderr).as_ref(),
                    "Failed to create Python venv"
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, "python3 not available, skipping venv creation");
            }
        }
    }

    fn base_config(&self) -> Result<SandboxConfig, AppError> {
        self.setup()?;

        let canonical_path = std::fs::canonicalize(&self.path).unwrap_or_else(|_| self.path.clone());

        let mut additional_path_dirs = Vec::new();
        let mut env_vars = Vec::new();

        let venv_bin = canonical_path.join(".venv").join("bin");
        if venv_bin.exists() {
            additional_path_dirs.push(venv_bin.to_string_lossy().into_owned());
            env_vars.push((
                "VIRTUAL_ENV".to_string(),
                canonical_path.join(".venv").to_string_lossy().into_owned(),
            ));
        }

        env_vars.push(("HOME".to_string(), canonical_path.to_string_lossy().into_owned()));
        env_vars.push(("XDG_CONFIG_HOME".to_string(), canonical_path.join(".config").to_string_lossy().into_owned()));
        env_vars.push(("XDG_CACHE_HOME".to_string(), canonical_path.join(".cache").to_string_lossy().into_owned()));

        env_vars.extend(self.extra_env_vars.clone());

        let config = SandboxConfig {
            workspace_dir: canonical_path.to_string_lossy().into_owned(),
            network_access: self.network_access,
            allowed_network_destinations: self.allowed_network_destinations.clone(),
            additional_read_paths: self.shared_read_paths.clone(),
            additional_write_paths: self.shared_write_paths.clone(),
            additional_path_dirs,
            env_vars,
            ..Default::default()
        };

        Ok(config)
    }

    pub fn spawn(
        &self,
        program: &str,
        args: &[&str],
        working_dir: Option<&str>,
        extra_path_dirs: Vec<String>,
        stdout: std::process::Stdio,
        stderr: std::process::Stdio,
    ) -> Result<tokio::process::Child, AppError> {
        let mut config = self.base_config()?;

        config.additional_path_dirs.extend(extra_path_dirs);

        if let Some(wd) = working_dir {
            let canonical_wd = std::fs::canonicalize(wd)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| wd.to_string());
            config.working_dir = Some(canonical_wd);
        }

        let mut cmd = self.driver.sandboxed_command(program, args, &config)?;

        cmd.env_clear();

        const PASSTHROUGH_VARS: &[&str] =
            &["TERM", "LANG", "LC_ALL", "LC_CTYPE", "TZ", "USER", "LOGNAME", "TMPDIR", "SHELL"];

        for key in PASSTHROUGH_VARS {
            if let Ok(val) = std::env::var(key) {
                cmd.env(key, val);
            }
        }

        {
            let existing = std::env::var("PATH").unwrap_or_default();
            if config.additional_path_dirs.is_empty() {
                cmd.env("PATH", existing);
            } else {
                let extra = config.additional_path_dirs.join(":");
                cmd.env("PATH", format!("{extra}:{existing}"));
            }
        }

        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        cmd.stdout(stdout);
        cmd.stderr(stderr);

        tokio::process::Command::from(cmd)
            .spawn()
            .map_err(|e| AppError::Tool(format!("Failed to spawn process: {e}")))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        program: &str,
        args: &[&str],
        timeout_secs: u64,
        on_stdout: Option<mpsc::Sender<String>>,
        stdin_rx: Option<mpsc::Receiver<String>>,
        cancel_token: Option<CancellationToken>,
        resource_usage: Option<&ResourceUsage>,
        agent_max_cpu_pct: Option<f64>,
        agent_max_memory_pct: Option<f64>,
    ) -> Result<SandboxOutput, AppError> {
        let mut config = self.base_config()?;
        config.timeout_secs = timeout_secs;

        execute_sandboxed(
            &*self.driver,
            program,
            args,
            &config,
            on_stdout,
            stdin_rx,
            cancel_token,
            resource_usage,
            Some(&self.agent_id),
            agent_max_cpu_pct,
            agent_max_memory_pct,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn python3_available() -> bool {
        std::process::Command::new("python3")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn temp_sandbox(name: &str) -> Sandbox {
        let dir = std::env::temp_dir().join("frona_test_workspace").join(name);
        Sandbox {
            path: dir,
            driver: Arc::from(create_driver(false)),
            network_access: false,
            allowed_network_destinations: Vec::new(),
            extra_env_vars: Vec::new(),
            shared_read_paths: Vec::new(),
            shared_write_paths: Vec::new(),
            agent_id: "test".to_string(),
        }
    }

    #[test]
    fn test_setup_creates_venv() {
        if !python3_available() {
            eprintln!("python3 not found, skipping");
            return;
        }
        let ws = temp_sandbox(&format!("setup_venv_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&ws.path);

        ws.setup().unwrap();

        assert!(ws.path.exists());
        assert!(ws.venv_path().exists());
        assert!(ws.venv_path().join("bin").join("python3").exists());

        let _ = std::fs::remove_dir_all(&ws.path);
    }

    #[test]
    fn test_setup_venv_with_relative_path() {
        if !python3_available() {
            eprintln!("python3 not found, skipping");
            return;
        }
        let name = format!("relative_venv_{}", uuid::Uuid::new_v4());
        let rel_base = PathBuf::from("target").join("test_workspaces");
        let rel_path = rel_base.join(&name);
        let _ = std::fs::remove_dir_all(&rel_path);

        let ws = Sandbox {
            path: rel_path.clone(),
            driver: Arc::from(create_driver(false)),
            network_access: false,
            allowed_network_destinations: Vec::new(),
            extra_env_vars: Vec::new(),
            shared_read_paths: Vec::new(),
            shared_write_paths: Vec::new(),
            agent_id: "test".to_string(),
        };
        ws.setup().unwrap();

        assert!(rel_path.join(".venv").exists(), "venv should exist at the workspace root");
        assert!(
            rel_path.join(".venv").join("bin").join("python3").exists(),
            "venv should contain bin/python3"
        );
        assert!(
            !rel_path.join("target").exists(),
            "no nested dirs should be created inside workspace"
        );

        let _ = std::fs::remove_dir_all(&rel_path);
    }

    #[tokio::test]
    async fn test_execute_sets_home_and_xdg_env_vars() {
        let ws = temp_sandbox(&format!("home_env_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&ws.path);

        let result = ws
            .execute(
                "bash",
                &["-c", "echo $HOME; echo $XDG_CONFIG_HOME; echo $XDG_CACHE_HOME"],
                10,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();

        let canonical = std::fs::canonicalize(&ws.path).unwrap();
        let lines: Vec<&str> = result.stdout.trim().lines().collect();
        assert_eq!(lines.len(), 3, "expected 3 lines, got: {:?}", lines);
        assert_eq!(lines[0], canonical.to_string_lossy());
        assert_eq!(lines[1], canonical.join(".config").to_string_lossy().as_ref());
        assert_eq!(lines[2], canonical.join(".cache").to_string_lossy().as_ref());

        let _ = std::fs::remove_dir_all(&ws.path);
    }

    #[tokio::test]
    async fn test_execute_does_not_leak_parent_env() {
        unsafe { std::env::set_var("FRONA_TEST_SECRET", "leaked") };
        let ws = temp_sandbox(&format!("env_leak_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&ws.path);

        let result = ws
            .execute("bash", &["-c", "echo $FRONA_TEST_SECRET"], 10, None, None, None, None, None, None)
            .await
            .unwrap();

        assert_eq!(result.stdout.trim(), "", "parent env vars must not leak into sandbox");

        unsafe { std::env::remove_var("FRONA_TEST_SECRET") };
        let _ = std::fs::remove_dir_all(&ws.path);
    }

    #[test]
    fn test_setup_idempotent() {
        if !python3_available() {
            eprintln!("python3 not found, skipping");
            return;
        }
        let ws = temp_sandbox(&format!("setup_idempotent_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&ws.path);

        ws.setup().unwrap();
        ws.setup().unwrap();

        assert!(ws.venv_path().exists());

        let _ = std::fs::remove_dir_all(&ws.path);
    }

    #[tokio::test]
    async fn test_execute_async_streaming() {
        let ws = temp_sandbox(&format!("streaming_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&ws.path);
        let (tx, mut rx) = mpsc::channel(16);

        let result = ws
            .execute(
                "bash",
                &["-c", "echo hello; echo world"],
                10,
                Some(tx),
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();

        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("hello"));
        assert!(result.stdout.contains("world"));

        let mut lines = Vec::new();
        while let Ok(line) = rx.try_recv() {
            lines.push(line);
        }
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "hello");
        assert_eq!(lines[1], "world");

        let _ = std::fs::remove_dir_all(&ws.path);
    }
}
