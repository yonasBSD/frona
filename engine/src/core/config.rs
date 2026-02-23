use std::collections::HashMap;

use serde::Deserialize;
use serde_aux::field_attributes::deserialize_bool_from_anything;

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub port: u16,
    pub static_dir: String,
    pub issuer_url: String,
    pub max_concurrent_tasks: usize,
    pub sandbox_disabled: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 3001,
            static_dir: "frontend/out".into(),
            issuer_url: "http://localhost:3001".into(),
            max_concurrent_tasks: 10,
            sandbox_disabled: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    pub encryption_secret: String,
    pub access_token_expiry_secs: u64,
    pub refresh_token_expiry_secs: u64,
    pub presign_expiry_secs: u64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            encryption_secret: "dev-secret-change-in-production".into(),
            access_token_expiry_secs: 900,
            refresh_token_expiry_secs: 604800,
            presign_expiry_secs: 86400,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct SsoConfig {
    pub enabled: bool,
    pub authority: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub scopes: String,
    pub allow_unknown_email_verification: bool,
    pub client_cache_expiration: u64,
    pub only: bool,
    pub signups_match_email: bool,
}

impl Default for SsoConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            authority: None,
            client_id: None,
            client_secret: None,
            scopes: "email profile offline_access".into(),
            allow_unknown_email_verification: false,
            client_cache_expiration: 0,
            only: false,
            signups_match_email: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub path: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: "data/db".into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct BrowserSettings {
    pub ws_url: String,
    pub profiles_path: String,
}

impl Default for BrowserSettings {
    fn default() -> Self {
        Self {
            ws_url: "ws://localhost:3333".into(),
            profiles_path: "/profiles".into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    pub workspaces_path: String,
    pub files_path: String,
    pub shared_config_dir: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            workspaces_path: "data/workspaces".into(),
            files_path: "data/files".into(),
            shared_config_dir: concat!(env!("CARGO_MANIFEST_DIR"), "/config").into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct SchedulerConfig {
    pub space_compaction_secs: u64,
    pub insight_compaction_secs: u64,
    pub poll_secs: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            space_compaction_secs: 3600,
            insight_compaction_secs: 7200,
            poll_secs: 60,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub initial_backoff_ms: u64,
    pub backoff_multiplier: f64,
    pub max_backoff_ms: u64,
}

impl RetryConfig {
    pub fn to_backoff(&self) -> backon::ExponentialBuilder {
        backon::ExponentialBuilder::default()
            .with_max_times(self.max_retries as usize)
            .with_min_delay(std::time::Duration::from_millis(self.initial_backoff_ms))
            .with_factor(self.backoff_multiplier as f32)
            .with_max_delay(std::time::Duration::from_millis(self.max_backoff_ms))
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_ms: 500,
            backoff_multiplier: 2.0,
            max_backoff_ms: 30_000,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelGroupConfig {
    pub main: String,
    #[serde(default)]
    pub fallbacks: Vec<String>,
    #[serde(default)]
    pub max_tokens: Option<u64>,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub context_window: Option<usize>,
    #[serde(default)]
    pub retry: RetryConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelProviderConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    #[serde(
        default = "serde_aux::prelude::bool_true",
        deserialize_with = "deserialize_bool_from_anything"
    )]
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub sso: SsoConfig,
    pub database: DatabaseConfig,
    pub browser: BrowserSettings,
    pub storage: StorageConfig,
    pub scheduler: SchedulerConfig,
    #[serde(default)]
    pub models: HashMap<String, ModelGroupConfig>,
    #[serde(default)]
    pub providers: HashMap<String, ModelProviderConfig>,
}

pub struct LoadedConfig {
    pub config: Config,
    pub models: Option<crate::inference::config::ModelRegistryConfig>,
}

macro_rules! env_override {
    ($builder:ident { $($env:expr => $key:expr),* $(,)? }) => {
        $( if let Ok(v) = std::env::var($env) {
            $builder = $builder.set_override($key, v).expect(concat!("override ", $key));
        } )*
    };
}

impl Config {
    pub fn load() -> LoadedConfig {
        let config_path = std::env::var("FRONA_CONFIG")
            .unwrap_or_else(|_| "data/config.yaml".into());

        let yaml_content = std::fs::read_to_string(&config_path).ok();

        let mut builder = config::Config::builder();

        if let Some(ref content) = yaml_content {
            let expanded = expand_env_vars(content);
            builder = builder.add_source(
                config::File::from_str(&expanded, config::FileFormat::Yaml),
            );
        }

        env_override!(builder {
            "PORT" => "server.port",
            "STATIC_DIR" => "server.static_dir",
            "ISSUER_URL" => "server.issuer_url",
            "MAX_CONCURRENT_TASKS" => "server.max_concurrent_tasks",
            "SANDBOX_DISABLED" => "server.sandbox_disabled",
            "JWT_SECRET" => "auth.encryption_secret",
            "ACCESS_TOKEN_EXPIRY_SECS" => "auth.access_token_expiry_secs",
            "REFRESH_TOKEN_EXPIRY_SECS" => "auth.refresh_token_expiry_secs",
            "PRESIGN_EXPIRY_SECS" => "auth.presign_expiry_secs",
            "SSO_ENABLED" => "sso.enabled",
            "SSO_AUTHORITY" => "sso.authority",
            "SSO_CLIENT_ID" => "sso.client_id",
            "SSO_CLIENT_SECRET" => "sso.client_secret",
            "SSO_SCOPES" => "sso.scopes",
            "SSO_ALLOW_UNKNOWN_EMAIL_VERIFICATION" => "sso.allow_unknown_email_verification",
            "SSO_CLIENT_CACHE_EXPIRATION" => "sso.client_cache_expiration",
            "SSO_ONLY" => "sso.only",
            "SSO_SIGNUPS_MATCH_EMAIL" => "sso.signups_match_email",
            "SURREAL_PATH" => "database.path",
            "BROWSERLESS_WS_URL" => "browser.ws_url",
            "BROWSER_PROFILES_PATH" => "browser.profiles_path",
            "WORKSPACES_BASE_PATH" => "storage.workspaces_path",
            "FILES_BASE_PATH" => "storage.files_path",
            "FRONA_SHARED_CONFIG" => "storage.shared_config_dir",
            "SCHEDULER_SPACE_COMPACTION_SECS" => "scheduler.space_compaction_secs",
            "SCHEDULER_INSIGHT_COMPACTION_SECS" => "scheduler.insight_compaction_secs",
            "SCHEDULER_POLL_SECS" => "scheduler.poll_secs"
        });

        let built = builder.build().expect("Failed to build config");

        let config: Config = built
            .try_deserialize()
            .expect("Failed to deserialize config");

        let models = if !config.models.is_empty() || !config.providers.is_empty() {
            Some(crate::inference::config::ModelRegistryConfig {
                providers: config.providers.clone().into_iter().collect(),
                models: config.models.clone().into_iter().collect(),
            })
        } else {
            None
        };

        if yaml_content.is_some() {
            tracing::info!(path = %config_path, "Loaded config from YAML");
        } else {
            tracing::info!("No config file found, using defaults and env vars");
        }

        LoadedConfig { config, models }
    }
}

pub fn expand_env_vars(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next();
            let mut var_name = String::new();
            for c in chars.by_ref() {
                if c == '}' {
                    break;
                }
                var_name.push(c);
            }
            if let Ok(val) = std::env::var(&var_name) {
                result.push_str(&val);
            }
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_env_vars() {
        unsafe { std::env::set_var("TEST_KEY_123", "my-secret") };
        let result = expand_env_vars("key=${TEST_KEY_123}");
        assert_eq!(result, "key=my-secret");
        unsafe { std::env::remove_var("TEST_KEY_123") };
    }

    #[test]
    fn test_expand_env_vars_missing() {
        let result = expand_env_vars("key=${NONEXISTENT_VAR_XYZ}");
        assert_eq!(result, "key=");
    }

    #[test]
    fn defaults_are_sensible() {
        let config = Config::default();
        assert_eq!(config.server.port, 3001);
        assert_eq!(config.auth.encryption_secret, "dev-secret-change-in-production");
        assert_eq!(config.database.path, "data/db");
        assert_eq!(config.storage.workspaces_path, "data/workspaces");
        assert_eq!(config.scheduler.space_compaction_secs, 3600);
        assert!(!config.sso.enabled);
        assert!(config.sso.signups_match_email);
    }
}
