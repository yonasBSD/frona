use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;

use crate::auth::jwt::JwtService;
use crate::chat::message::models::MessageResponse;
use crate::core::error::AppError;
use crate::credential::keypair::service::KeyPairService;

use crate::core::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub struct Attachment {
    pub filename: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct PresignClaims {
    pub sub: String,
    pub path: String,
    pub exp: usize,
}

pub fn resolve_virtual_path(virtual_path: &str, config: &Config) -> Result<PathBuf, AppError> {
    if let Some(rest) = virtual_path.strip_prefix("user://") {
        let resolved = Path::new(&config.files_base_path).join(rest);
        validate_no_traversal(&resolved, &config.files_base_path)?;
        Ok(resolved)
    } else if let Some(rest) = virtual_path.strip_prefix("agent://") {
        let resolved = Path::new(&config.workspaces_base_path).join(rest);
        validate_no_traversal(&resolved, &config.workspaces_base_path)?;
        Ok(resolved)
    } else {
        Err(AppError::Validation(format!(
            "Invalid virtual path scheme: {virtual_path}"
        )))
    }
}

pub fn make_user_path(user_id: &str, relative: &str) -> String {
    format!("user://{user_id}/{relative}")
}

pub fn make_agent_path(agent_id: &str, relative: &str) -> String {
    format!("agent://{agent_id}/{relative}")
}

pub fn virtual_path_to_url_segment(path: &str) -> Option<String> {
    // "user://uid/f.txt" → "user/uid/f.txt"
    // "agent://aid/f.txt" → "agent/aid/f.txt"
    let rest = path
        .strip_prefix("user://")
        .map(|r| format!("user/{r}"))
        .or_else(|| path.strip_prefix("agent://").map(|r| format!("agent/{r}")))?;
    Some(rest)
}

pub async fn presign_attachment(
    att: &mut Attachment,
    keypair_svc: &KeyPairService,
    jwt_svc: &JwtService,
    user_id: &str,
    issuer_url: &str,
    expiry_secs: u64,
) -> Result<(), AppError> {
    let segment = match virtual_path_to_url_segment(&att.path) {
        Some(s) => s,
        None => return Ok(()),
    };

    let owner = format!("user:{user_id}");
    let (encoding_key, kid) = keypair_svc.get_signing_key(&owner).await?;

    let exp = (chrono::Utc::now().timestamp() as u64 + expiry_secs) as usize;
    let claims = PresignClaims {
        sub: user_id.to_string(),
        path: att.path.clone(),
        exp,
    };

    let token = jwt_svc.sign(&claims, &encoding_key, &kid)?;
    att.url = Some(format!("{issuer_url}/api/files/{segment}?presign={token}"));
    Ok(())
}

pub async fn presign_message(
    msg: &mut MessageResponse,
    keypair_svc: &KeyPairService,
    jwt_svc: &JwtService,
    user_id: &str,
    issuer_url: &str,
    expiry_secs: u64,
) {
    for att in &mut msg.attachments {
        if let Err(e) = presign_attachment(att, keypair_svc, jwt_svc, user_id, issuer_url, expiry_secs).await {
            tracing::warn!(error = %e, path = %att.path, "Failed to presign attachment");
        }
    }
}

fn validate_no_traversal(resolved: &Path, base: &str) -> Result<(), AppError> {
    for component in resolved.components() {
        if let std::path::Component::ParentDir = component {
            return Err(AppError::Validation(
                "Path traversal not allowed".into(),
            ));
        }
    }

    let base_canonical = std::fs::canonicalize(base).unwrap_or_else(|_| PathBuf::from(base));
    let resolved_canonical =
        std::fs::canonicalize(resolved).unwrap_or_else(|_| resolved.to_path_buf());

    if !resolved_canonical.starts_with(&base_canonical)
        && !resolved.starts_with(base)
    {
        return Err(AppError::Validation(
            "Path escapes allowed directory".into(),
        ));
    }

    Ok(())
}

pub fn dedup_filename(dir: &Path, filename: &str) -> String {
    let target = dir.join(filename);
    if !target.exists() {
        return filename.to_string();
    }

    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);

    let ext_with_dots = &filename[stem.len()..];

    for i in 1..1000 {
        let candidate = format!("{stem}-{i}{ext_with_dots}");
        if !dir.join(&candidate).exists() {
            return candidate;
        }
    }

    format!("{stem}-{}{ext_with_dots}", uuid::Uuid::new_v4())
}

pub fn detect_content_type(filename: &str) -> &'static str {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "xml" => "application/xml",
        "zip" => "application/zip",
        "gz" | "gzip" => "application/gzip",
        "tar" => "application/x-tar",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "ts" | "tsx" => "text/typescript",
        "rs" => "text/x-rust",
        "py" => "text/x-python",
        "rb" => "text/x-ruby",
        "go" => "text/x-go",
        "java" => "text/x-java",
        "c" | "h" => "text/x-c",
        "cpp" | "cc" | "cxx" | "hpp" => "text/x-c++",
        "md" | "markdown" => "text/markdown",
        "txt" | "log" => "text/plain",
        "csv" => "text/csv",
        "yaml" | "yml" => "text/yaml",
        "toml" => "text/toml",
        "sh" | "bash" | "zsh" => "text/x-shellscript",
        "sql" => "text/x-sql",
        "dockerfile" => "text/x-dockerfile",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

pub fn is_image_content_type(content_type: &str) -> bool {
    content_type.starts_with("image/")
}

pub fn is_text_content_type(content_type: &str) -> bool {
    content_type.starts_with("text/")
        || content_type == "application/json"
        || content_type == "application/xml"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config {
            port: 3001,
            jwt_secret: "test".into(),
            surreal_path: "data/db".into(),
            static_dir: "frontend/out".into(),
            models_config_path: "data/models.json".into(),
            browserless_ws_url: "ws://localhost:3333".into(),
            browser_profiles_path: "/profiles".into(),
            workspaces_base_path: "data/workspaces".into(),
            files_base_path: "data/files".into(),
            shared_config_dir: concat!(env!("CARGO_MANIFEST_DIR"), "/config").into(),
            sandbox_disabled: false,
            max_concurrent_tasks: 10,
            scheduler_space_compaction_secs: 3600,
            scheduler_insight_compaction_secs: 7200,
            scheduler_poll_secs: 60,
            issuer_url: "http://localhost:3001".into(),
            access_token_expiry_secs: 900,
            refresh_token_expiry_secs: 604800,
            sso_enabled: false,
            sso_authority: None,
            sso_client_id: None,
            sso_client_secret: None,
            sso_scopes: "email profile offline_access".into(),
            sso_allow_unknown_email_verification: false,
            sso_client_cache_expiration: 0,
            sso_only: false,
            sso_signups_match_email: true,
            presign_expiry_secs: 86400,
        }
    }

    #[test]
    fn resolve_user_path() {
        let config = test_config();
        let result = resolve_virtual_path("user://uid-123/report.pdf", &config).unwrap();
        assert_eq!(result, PathBuf::from("data/files/uid-123/report.pdf"));
    }

    #[test]
    fn resolve_agent_path() {
        let config = test_config();
        let result = resolve_virtual_path("agent://dev/output.csv", &config).unwrap();
        assert_eq!(
            result,
            PathBuf::from("data/workspaces/dev/output.csv")
        );
    }

    #[test]
    fn resolve_agent_nested_path() {
        let config = test_config();
        let result =
            resolve_virtual_path("agent://dev/subdir/nested/file.txt", &config).unwrap();
        assert_eq!(
            result,
            PathBuf::from("data/workspaces/dev/subdir/nested/file.txt")
        );
    }

    #[test]
    fn resolve_invalid_scheme() {
        let config = test_config();
        let result = resolve_virtual_path("invalid://x/y", &config);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_path_traversal_rejected() {
        let config = test_config();
        let result =
            resolve_virtual_path("user://uid/../../../etc/passwd", &config);
        assert!(result.is_err());
    }

    #[test]
    fn make_user_path_format() {
        assert_eq!(
            make_user_path("uid-123", "report.pdf"),
            "user://uid-123/report.pdf"
        );
    }

    #[test]
    fn make_agent_path_format() {
        assert_eq!(
            make_agent_path("developer", "output.csv"),
            "agent://developer/output.csv"
        );
    }

    #[test]
    fn make_agent_path_nested() {
        assert_eq!(
            make_agent_path("developer", "subdir/file.txt"),
            "agent://developer/subdir/file.txt"
        );
    }

    #[test]
    fn dedup_no_conflict() {
        let dir = std::env::temp_dir().join("frona_test_dedup_empty");
        let _ = std::fs::create_dir_all(&dir);
        assert_eq!(dedup_filename(&dir, "report.pdf"), "report.pdf");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dedup_with_conflict() {
        let dir = std::env::temp_dir().join("frona_test_dedup_conflict");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("report.pdf"), b"").unwrap();

        assert_eq!(dedup_filename(&dir, "report.pdf"), "report-1.pdf");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dedup_with_multiple_conflicts() {
        let dir = std::env::temp_dir().join("frona_test_dedup_multi");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("report.pdf"), b"").unwrap();
        std::fs::write(dir.join("report-1.pdf"), b"").unwrap();

        assert_eq!(dedup_filename(&dir, "report.pdf"), "report-2.pdf");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dedup_compound_extension() {
        let dir = std::env::temp_dir().join("frona_test_dedup_compound");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("archive.tar.gz"), b"").unwrap();

        assert_eq!(dedup_filename(&dir, "archive.tar.gz"), "archive.tar-1.gz");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dedup_no_extension() {
        let dir = std::env::temp_dir().join("frona_test_dedup_noext");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("Makefile"), b"").unwrap();

        assert_eq!(dedup_filename(&dir, "Makefile"), "Makefile-1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_common_types() {
        assert_eq!(detect_content_type("photo.png"), "image/png");
        assert_eq!(detect_content_type("photo.jpg"), "image/jpeg");
        assert_eq!(detect_content_type("doc.pdf"), "application/pdf");
        assert_eq!(detect_content_type("code.rs"), "text/x-rust");
        assert_eq!(detect_content_type("data.json"), "application/json");
        assert_eq!(detect_content_type("readme.md"), "text/markdown");
        assert_eq!(detect_content_type("unknown.xyz"), "application/octet-stream");
    }

    #[test]
    fn image_and_text_detection() {
        assert!(is_image_content_type("image/png"));
        assert!(is_image_content_type("image/jpeg"));
        assert!(!is_image_content_type("text/plain"));

        assert!(is_text_content_type("text/plain"));
        assert!(is_text_content_type("text/x-rust"));
        assert!(is_text_content_type("application/json"));
        assert!(!is_text_content_type("image/png"));
    }

    #[test]
    fn virtual_path_to_url_segment_user() {
        assert_eq!(
            virtual_path_to_url_segment("user://uid-123/report.pdf"),
            Some("user/uid-123/report.pdf".to_string()),
        );
    }

    #[test]
    fn virtual_path_to_url_segment_agent() {
        assert_eq!(
            virtual_path_to_url_segment("agent://dev/output.csv"),
            Some("agent/dev/output.csv".to_string()),
        );
    }

    #[test]
    fn virtual_path_to_url_segment_nested() {
        assert_eq!(
            virtual_path_to_url_segment("user://uid/sub/dir/file.txt"),
            Some("user/uid/sub/dir/file.txt".to_string()),
        );
    }

    #[test]
    fn virtual_path_to_url_segment_invalid_scheme() {
        assert_eq!(virtual_path_to_url_segment("invalid://x/y"), None);
    }

    #[test]
    fn attachment_url_defaults_to_none() {
        let json = r#"{"filename":"f.txt","content_type":"text/plain","size_bytes":10,"path":"user://uid/f.txt"}"#;
        let att: Attachment = serde_json::from_str(json).unwrap();
        assert!(att.url.is_none());
    }

    #[test]
    fn attachment_url_round_trips() {
        let att = Attachment {
            filename: "f.txt".into(),
            content_type: "text/plain".into(),
            size_bytes: 10,
            path: "user://uid/f.txt".into(),
            url: Some("http://localhost/presigned".into()),
        };
        let json = serde_json::to_string(&att).unwrap();
        assert!(json.contains("\"url\":"));

        let parsed: Attachment = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.url.as_deref(), Some("http://localhost/presigned"));
    }

    #[test]
    fn attachment_url_none_omitted_from_json() {
        let att = Attachment {
            filename: "f.txt".into(),
            content_type: "text/plain".into(),
            size_bytes: 10,
            path: "user://uid/f.txt".into(),
            url: None,
        };
        let json = serde_json::to_string(&att).unwrap();
        assert!(!json.contains("\"url\""));
    }

    #[test]
    fn presign_claims_round_trips() {
        let claims = PresignClaims {
            sub: "uid-123".into(),
            path: "user://uid-123/file.pdf".into(),
            exp: 9999999999,
        };
        let json = serde_json::to_string(&claims).unwrap();
        let parsed: PresignClaims = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.sub, "uid-123");
        assert_eq!(parsed.path, "user://uid-123/file.pdf");
        assert_eq!(parsed.exp, 9999999999);
    }
}
