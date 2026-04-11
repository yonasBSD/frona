//! Data model + streaming reader for the prebuilt MCP registry dump.
//!
//! The dump is a top-level JSON array of [`RegistryServerEntry`], xz-compressed.
//! Queries walk the compressed file through a `serde::de::SeqAccess` visitor
//! one entry at a time instead of parsing the full catalog into memory.

use std::fmt;
use std::fs::File;
use std::io::{BufReader, Read};
use std::marker::PhantomData;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::de::{DeserializeOwned, Deserializer as _, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use xz2::read::XzDecoder;

use crate::core::error::AppError;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegistryServerEntry {
    pub name: String,
    pub description: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<Repository>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<RegistryPackage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remotes: Vec<RegistryTransport>,

    #[serde(default = "default_status")]
    pub status: RegistryStatus,
    #[serde(default)]
    pub is_latest: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_changed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enrichment: Option<Enrichment>,

    /// Quality score baked in by the build pipeline (see `scripts/ranking.py`
    /// in `fronalabs/mcp-registry-database`). `search_dump` sorts by this
    /// descending before applying the limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
}

fn default_status() -> RegistryStatus {
    RegistryStatus::Active
}

impl RegistryServerEntry {
    pub fn is_active(&self) -> bool {
        matches!(self.status, RegistryStatus::Active)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RegistryStatus {
    #[default]
    Active,
    Deprecated,
    Deleted,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Repository {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subfolder: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegistryPackage {
    pub registry_type: String,
    pub identifier: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_hint: Option<String>,
    pub transport: RegistryTransport,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_arguments: Vec<RegistryArgument>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub package_arguments: Vec<RegistryArgument>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environment_variables: Vec<RegistryEnvVar>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegistryTransport {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegistryArgument {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default)]
    pub is_required: bool,
    #[serde(default)]
    pub is_repeated: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegistryEnvVar {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub is_required: bool,
    #[serde(default)]
    pub is_secret: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Enrichment {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_stars: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_forks: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_watchers: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_open_issues: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_open_pull_requests: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_pushed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_is_fork: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_is_disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_archived: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_primary_language: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub github_topics: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_owner_avatar_url: Option<String>,
    pub enriched_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PrebuiltMetadata {
    #[serde(default)]
    pub schema_version: Option<String>,
    #[serde(default)]
    pub generated_at: Option<DateTime<Utc>>,
    pub content_sha256: String,
    #[serde(default)]
    pub counts: serde_json::Value,
}

/// Stream the compressed dump at `path`, collect every entry whose name,
/// description, or title contain `query` (case-insensitive substring), sort
/// the matches by `score` descending, and return the top `limit`. An empty
/// query matches every entry.
pub fn search_dump(
    path: &Path,
    query: &str,
    limit: usize,
) -> Result<Vec<RegistryServerEntry>, AppError> {
    let needle = query.to_lowercase();
    let mut hits: Vec<RegistryServerEntry> = Vec::new();
    stream_dump(path, |entry| {
        if matches_query(&entry, &needle) {
            hits.push(entry);
        }
        true
    })?;
    hits.sort_by(|a, b| {
        b.score
            .unwrap_or(0.0)
            .partial_cmp(&a.score.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });
    hits.truncate(limit);
    Ok(hits)
}

/// Stream the compressed dump at `path`, returning the first entry whose
/// `name` exactly equals `name`, or `None` if not present.
pub fn fetch_dump(path: &Path, name: &str) -> Result<Option<RegistryServerEntry>, AppError> {
    let mut found: Option<RegistryServerEntry> = None;
    stream_dump(path, |entry| {
        if entry.name == name {
            found = Some(entry);
            return false;
        }
        true
    })?;
    Ok(found)
}

fn stream_dump<F>(path: &Path, on_item: F) -> Result<(), AppError>
where
    F: FnMut(RegistryServerEntry) -> bool,
{
    let file = File::open(path).map_err(|e| {
        AppError::Tool(format!(
            "MCP registry cache not ready at {}: {e}",
            path.display()
        ))
    })?;
    let reader = XzDecoder::new(BufReader::new(file));
    stream_json_array::<_, RegistryServerEntry, _>(reader, on_item)
}

fn matches_query(entry: &RegistryServerEntry, needle_lc: &str) -> bool {
    if needle_lc.is_empty() {
        return true;
    }
    if entry.name.to_lowercase().contains(needle_lc) {
        return true;
    }
    if entry.description.to_lowercase().contains(needle_lc) {
        return true;
    }
    if let Some(t) = &entry.title
        && t.to_lowercase().contains(needle_lc)
    {
        return true;
    }
    false
}

/// Stream a top-level JSON array from `reader`, calling `on_item` for each
/// element. Return `false` from `on_item` to stop collecting; the remaining
/// array tail is drained as `IgnoredAny` so the deserializer finishes cleanly.
fn stream_json_array<R, T, F>(reader: R, on_item: F) -> Result<(), AppError>
where
    R: Read,
    T: DeserializeOwned,
    F: FnMut(T) -> bool,
{
    struct ArrVisitor<T, F>(F, PhantomData<T>);

    impl<'de, T, F> Visitor<'de> for ArrVisitor<T, F>
    where
        T: DeserializeOwned,
        F: FnMut(T) -> bool,
    {
        type Value = ();

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "a JSON array of registry servers")
        }

        fn visit_seq<A>(mut self, mut seq: A) -> Result<(), A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut stopped = false;
            loop {
                if stopped {
                    if seq.next_element::<serde::de::IgnoredAny>()?.is_none() {
                        break;
                    }
                } else {
                    match seq.next_element::<T>()? {
                        Some(item) => {
                            if !(self.0)(item) {
                                stopped = true;
                            }
                        }
                        None => break,
                    }
                }
            }
            Ok(())
        }
    }

    let mut de = serde_json::Deserializer::from_reader(reader);
    de.deserialize_seq(ArrVisitor(on_item, PhantomData))
        .map_err(|e| AppError::Tool(format!("streaming MCP registry dump failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn sample_entry(name: &str, description: &str) -> serde_json::Value {
        sample_entry_scored(name, description, 1.0)
    }

    fn sample_entry_scored(name: &str, description: &str, score: f64) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "description": description,
            "version": "1.0.0",
            "repository": { "url": format!("https://github.com/example/{name}") },
            "packages": [
                {
                    "registry_type": "npm",
                    "identifier": name,
                    "version": "1.0.0",
                    "transport": { "type": "stdio" }
                }
            ],
            "status": "active",
            "is_latest": true,
            "enrichment": {
                "github_stars": 42,
                "github_topics": ["mcp", "ai"],
                "enriched_at": "2026-04-01T00:00:00Z"
            },
            "score": score
        })
    }

    fn write_dump(dir: &Path, entries: &[serde_json::Value]) -> std::path::PathBuf {
        let path = dir.join("servers.json.xz");
        let json = serde_json::to_vec(entries).unwrap();
        let mut enc = xz2::write::XzEncoder::new(Vec::new(), 9);
        std::io::Write::write_all(&mut enc, &json).unwrap();
        let bytes = enc.finish().unwrap();
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn parses_snake_case_dump_entry() {
        let json = serde_json::to_string(&sample_entry("foo/bar", "A thing")).unwrap();
        let entry: RegistryServerEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry.name, "foo/bar");
        assert_eq!(entry.packages[0].registry_type, "npm");
        assert_eq!(entry.packages[0].transport.kind, "stdio");
        let enrichment = entry.enrichment.as_ref().unwrap();
        assert_eq!(enrichment.github_stars, Some(42));
        assert_eq!(enrichment.github_topics, vec!["mcp", "ai"]);
    }

    #[test]
    fn search_dump_matches_substring() {
        let dir = tempdir().unwrap();
        let path = write_dump(
            dir.path(),
            &[
                sample_entry("io.example.a/github-mcp", "GitHub integration"),
                sample_entry("io.example.b/gmail-mcp", "Gmail client"),
                sample_entry("io.example.c/slack", "Chat integration"),
            ],
        );

        let hits = search_dump(&path, "github", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].name.contains("github-mcp"));

        let match_all = search_dump(&path, "", 2).unwrap();
        assert_eq!(match_all.len(), 2, "limit caps the match-all case");
    }

    #[test]
    fn search_dump_stops_early_at_limit() {
        let dir = tempdir().unwrap();
        let path = write_dump(
            dir.path(),
            &[
                sample_entry("a/one", "shared"),
                sample_entry("b/two", "shared"),
                sample_entry("c/three", "shared"),
                sample_entry("d/four", "shared"),
            ],
        );
        let hits = search_dump(&path, "shared", 2).unwrap();
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn search_dump_sorts_by_score_desc_before_limit() {
        let dir = tempdir().unwrap();
        let path = write_dump(
            dir.path(),
            &[
                sample_entry_scored("a/low", "shared", 1.0),
                sample_entry_scored("b/high", "shared", 50.0),
                sample_entry_scored("c/mid", "shared", 20.0),
                sample_entry_scored("d/top", "shared", 99.0),
            ],
        );
        let hits = search_dump(&path, "shared", 2).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].name, "d/top");
        assert_eq!(hits[1].name, "b/high");
    }

    #[test]
    fn fetch_dump_finds_exact_name() {
        let dir = tempdir().unwrap();
        let path = write_dump(
            dir.path(),
            &[
                sample_entry("x/y", "first"),
                sample_entry("x/z", "second"),
            ],
        );
        let hit = fetch_dump(&path, "x/z").unwrap().unwrap();
        assert_eq!(hit.description, "second");
        assert!(fetch_dump(&path, "not/here").unwrap().is_none());
    }
}
