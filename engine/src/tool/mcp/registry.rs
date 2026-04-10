//! MCP registry client backed by the `fronalabs/mcp-registry-database` release
//! artifact. Downloads the compressed dump into the MCP cache directory, refreshes
//! only when the remote `content_sha256` changes, and delegates all parsing and
//! searching to the [`metadata`](super::metadata) module (which streams the
//! on-disk file instead of caching it in memory).

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::task;

use super::metadata::{self, PrebuiltMetadata, RegistryServerEntry};
use crate::core::error::AppError;

pub const PREBUILT_METADATA_URL: &str =
    "https://github.com/fronalabs/mcp-registry-database/releases/latest/download/metadata.json";
pub const PREBUILT_SERVERS_URL: &str =
    "https://github.com/fronalabs/mcp-registry-database/releases/latest/download/servers.json.xz";

#[async_trait]
pub trait McpRegistryClient: Send + Sync {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<RegistryServerEntry>, AppError>;

    /// `name` is the reverse-DNS id like `io.github.foo/bar`.
    async fn fetch(&self, name: &str) -> Result<RegistryServerEntry, AppError>;

    /// The prebuilt dump is latest-only by design. Returns an error for any
    /// `version` that isn't the latest one currently in the dump.
    async fn fetch_version(
        &self,
        name: &str,
        version: &str,
    ) -> Result<RegistryServerEntry, AppError>;
}

pub struct PrebuiltMcpRegistryClient {
    metadata_url: String,
    servers_url: String,
    cache_dir: PathBuf,
    http: reqwest::Client,
}

impl PrebuiltMcpRegistryClient {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            metadata_url: PREBUILT_METADATA_URL.to_string(),
            servers_url: PREBUILT_SERVERS_URL.to_string(),
            cache_dir,
            http: reqwest::Client::new(),
        }
    }

    #[cfg(test)]
    fn with_urls(cache_dir: PathBuf, metadata_url: String, servers_url: String) -> Self {
        Self {
            metadata_url,
            servers_url,
            cache_dir,
            http: reqwest::Client::new(),
        }
    }

    fn metadata_path(&self) -> PathBuf {
        self.cache_dir.join("metadata.json")
    }

    fn servers_path(&self) -> PathBuf {
        self.cache_dir.join("servers.json.xz")
    }

    /// Downloads the dump if the remote `content_sha256` differs from the local
    /// one. No-op when they match. Must be called before any query method.
    pub async fn ensure_fresh(&self) -> Result<(), AppError> {
        fs::create_dir_all(&self.cache_dir).map_err(|e| {
            AppError::Tool(format!("creating MCP registry cache dir failed: {e}"))
        })?;

        let remote_meta_bytes = self
            .http
            .get(&self.metadata_url)
            .send()
            .await
            .map_err(|e| AppError::Tool(format!("fetching MCP registry metadata failed: {e}")))?
            .error_for_status()
            .map_err(|e| AppError::Tool(format!("MCP registry metadata HTTP error: {e}")))?
            .bytes()
            .await
            .map_err(|e| {
                AppError::Tool(format!("reading MCP registry metadata body failed: {e}"))
            })?;
        let remote_meta: PrebuiltMetadata = serde_json::from_slice(&remote_meta_bytes)
            .map_err(|e| AppError::Tool(format!("parsing MCP registry metadata failed: {e}")))?;

        let local_meta = self.read_local_metadata();
        let servers_path = self.servers_path();
        if let Some(local) = &local_meta
            && local.content_sha256 == remote_meta.content_sha256
            && servers_path.is_file()
        {
            return Ok(());
        }

        let servers_bytes = self
            .http
            .get(&self.servers_url)
            .send()
            .await
            .map_err(|e| AppError::Tool(format!("fetching MCP registry dump failed: {e}")))?
            .error_for_status()
            .map_err(|e| AppError::Tool(format!("MCP registry dump HTTP error: {e}")))?
            .bytes()
            .await
            .map_err(|e| AppError::Tool(format!("reading MCP registry dump body failed: {e}")))?;

        atomic_write(&servers_path, &servers_bytes)?;
        atomic_write(&self.metadata_path(), &remote_meta_bytes)?;
        Ok(())
    }

    fn read_local_metadata(&self) -> Option<PrebuiltMetadata> {
        let bytes = fs::read(self.metadata_path()).ok()?;
        serde_json::from_slice(&bytes).ok()
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppError::Tool(format!("creating {} failed: {e}", parent.display())))?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|s| s.to_str()).unwrap_or("")
    ));
    {
        let file = File::create(&tmp)
            .map_err(|e| AppError::Tool(format!("creating {} failed: {e}", tmp.display())))?;
        let mut w = BufWriter::new(file);
        w.write_all(bytes)
            .map_err(|e| AppError::Tool(format!("writing {} failed: {e}", tmp.display())))?;
        w.flush()
            .map_err(|e| AppError::Tool(format!("flushing {} failed: {e}", tmp.display())))?;
    }
    fs::rename(&tmp, path)
        .map_err(|e| AppError::Tool(format!("renaming into {} failed: {e}", path.display())))?;
    Ok(())
}

#[async_trait]
impl McpRegistryClient for PrebuiltMcpRegistryClient {
    async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<RegistryServerEntry>, AppError> {
        self.ensure_fresh().await?;
        let path = self.servers_path();
        let query = query.to_string();
        task::spawn_blocking(move || metadata::search_dump(&path, &query, limit))
            .await
            .map_err(|e| AppError::Tool(format!("MCP registry search task failed: {e}")))?
    }

    async fn fetch(&self, name: &str) -> Result<RegistryServerEntry, AppError> {
        self.ensure_fresh().await?;
        let path = self.servers_path();
        let name_owned = name.to_string();
        let found = task::spawn_blocking(move || metadata::fetch_dump(&path, &name_owned))
            .await
            .map_err(|e| AppError::Tool(format!("MCP registry fetch task failed: {e}")))??;
        found.ok_or_else(|| AppError::Tool(format!("MCP registry has no server named {name}")))
    }

    async fn fetch_version(
        &self,
        name: &str,
        version: &str,
    ) -> Result<RegistryServerEntry, AppError> {
        let entry = self.fetch(name).await?;
        if entry.version == version {
            Ok(entry)
        } else {
            Err(AppError::Tool(format!(
                "MCP registry dump only has {name}@{latest}, not {version}",
                latest = entry.version
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn atomic_write_overwrites_existing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("foo.json");
        atomic_write(&path, b"first").unwrap();
        atomic_write(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
    }

    #[test]
    fn local_metadata_round_trips() {
        let dir = tempdir().unwrap();
        let client = PrebuiltMcpRegistryClient::with_urls(
            dir.path().to_path_buf(),
            String::new(),
            String::new(),
        );
        assert!(client.read_local_metadata().is_none());
        fs::write(
            dir.path().join("metadata.json"),
            serde_json::to_vec(&serde_json::json!({
                "content_sha256": "deadbeef",
                "counts": {}
            }))
            .unwrap(),
        )
        .unwrap();
        let meta = client.read_local_metadata().unwrap();
        assert_eq!(meta.content_sha256, "deadbeef");
    }

    #[derive(Clone)]
    #[allow(dead_code)]
    pub struct FakeMcpRegistryClient {
        pub entries: Vec<RegistryServerEntry>,
    }

    #[async_trait]
    impl McpRegistryClient for FakeMcpRegistryClient {
        async fn search(
            &self,
            query: &str,
            limit: usize,
        ) -> Result<Vec<RegistryServerEntry>, AppError> {
            let needle = query.to_lowercase();
            Ok(self
                .entries
                .iter()
                .filter(|e| {
                    needle.is_empty()
                        || e.name.to_lowercase().contains(&needle)
                        || e.description.to_lowercase().contains(&needle)
                })
                .take(limit)
                .cloned()
                .collect())
        }

        async fn fetch(&self, name: &str) -> Result<RegistryServerEntry, AppError> {
            self.entries
                .iter()
                .find(|e| e.name == name)
                .cloned()
                .ok_or_else(|| AppError::Tool(format!("fake registry has no {name}")))
        }

        async fn fetch_version(
            &self,
            name: &str,
            version: &str,
        ) -> Result<RegistryServerEntry, AppError> {
            let entry = self.fetch(name).await?;
            if entry.version == version {
                Ok(entry)
            } else {
                Err(AppError::Tool(format!(
                    "fake registry only has {name}@{} not {version}",
                    entry.version
                )))
            }
        }
    }
}
