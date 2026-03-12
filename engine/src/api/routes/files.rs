use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::extract::{FromRequestParts, Multipart, Path as AxumPath, Query, State};
use axum::http::request::Parts;
use axum::http::header;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio_util::io::ReaderStream;

use crate::api::files::{
    Attachment, dedup_filename, detect_content_type, resolve_virtual_path,
};

use super::super::error::ApiError;
use super::super::middleware::auth::AuthUser;
use crate::core::error::AppError;
use crate::core::state::AppState;

const MAX_FILE_SIZE: usize = 10 * 1024 * 1024; // 10MB

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/files", post(upload_file))
        .route("/api/files/presign", post(presign_file))
        .route(
            "/api/files/user/{username}/{*filename}",
            get(download_user_file).delete(delete_user_file),
        )
        .route(
            "/api/files/agent/{agent_id}/{*filepath}",
            get(download_agent_file),
        )
        .route("/api/files/browse/user", get(list_user_files))
        .route("/api/files/browse/user/{*dirpath}", get(list_user_files))
        .route(
            "/api/files/browse/agent/{agent_id}",
            get(list_agent_files_root),
        )
        .route(
            "/api/files/browse/agent/{agent_id}/{*dirpath}",
            get(list_agent_files_subdir),
        )
        .route("/api/files/search", get(search_files))
        .route("/api/files/rename", post(rename_user_file))
        .route("/api/files/copy", post(copy_files))
        .route("/api/files/move", post(move_files))
        .route("/api/files/mkdir", post(create_user_folder))
}

enum FileAuth {
    User(AuthUser),
    Presigned { owner: String, path: String },
}

#[derive(Deserialize)]
struct PresignQuery {
    presign: Option<String>,
}

impl FromRequestParts<AppState> for FileAuth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Try Bearer auth first
        if let Ok(auth) = AuthUser::from_request_parts(parts, state).await {
            return Ok(FileAuth::User(auth));
        }

        // Fall back to presign query param
        let query: Query<PresignQuery> =
            Query::try_from_uri(&parts.uri)
                .map_err(|_| ApiError(AppError::Auth("Missing authorization".into())))?;

        let token = query
            .presign
            .as_deref()
            .ok_or_else(|| ApiError(AppError::Auth("Missing authorization".into())))?;

        let claims = state.presign_service.verify(token).await?;

        Ok(FileAuth::Presigned {
            owner: claims.owner,
            path: claims.path,
        })
    }
}

async fn upload_file(
    auth: AuthUser,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<Attachment>, ApiError> {
    let mut file_data: Option<(String, Vec<u8>)> = None;
    let mut relative_path: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError(AppError::Validation(e.to_string())))?
    {
        match field.name() {
            Some("path") => {
                relative_path = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError(AppError::Validation(e.to_string())))?,
                );
            }
            Some("file") | Some("upload") => {
                let filename = field
                    .file_name()
                    .unwrap_or("upload")
                    .to_string();
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError(AppError::Validation(e.to_string())))?;

                if bytes.len() > MAX_FILE_SIZE {
                    return Err(ApiError(AppError::Validation(
                        format!("File too large (max {}MB)", MAX_FILE_SIZE / 1024 / 1024),
                    )));
                }

                file_data = Some((filename, bytes.to_vec()));
            }
            _ => {}
        }
    }

    let (original_filename, bytes) = file_data.ok_or_else(|| {
        ApiError(AppError::Validation(
            "Missing file field".into(),
        ))
    })?;

    let (dir, filename_for_dedup, virtual_relative) = if let Some(ref rel_path) = relative_path {
        validate_relative_path(rel_path)?;
        let parent = Path::new(rel_path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let leaf = Path::new(rel_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&original_filename)
            .to_string();
        let dir = Path::new(&state.config.storage.files_path)
            .join(&auth.username)
            .join(&parent);
        (dir, leaf, Some(rel_path.clone()))
    } else {
        let dir = Path::new(&state.config.storage.files_path).join(&auth.username);
        (dir, original_filename.clone(), None)
    };

    fs::create_dir_all(&dir)
        .await
        .map_err(|e| ApiError(AppError::Internal(e.to_string())))?;

    let final_filename = dedup_filename(&dir, &filename_for_dedup);
    let dest = dir.join(&final_filename);

    fs::write(&dest, &bytes)
        .await
        .map_err(|e| ApiError(AppError::Internal(e.to_string())))?;

    let relative = if let Some(mut rel) = virtual_relative {
        if let Some(parent) = Path::new(&rel).parent() {
            rel = parent.join(&final_filename).to_string_lossy().into_owned();
        } else {
            rel = final_filename.clone();
        }
        rel
    } else {
        final_filename.clone()
    };

    let content_type = detect_content_type(&final_filename).to_string();
    let size_bytes = bytes.len() as u64;

    Ok(Json(Attachment {
        filename: final_filename,
        content_type,
        size_bytes,
        owner: format!("user:{}", auth.user_id),
        path: relative,
        url: None,
    }))
}

async fn download_user_file(
    file_auth: FileAuth,
    State(state): State<AppState>,
    AxumPath((username, filename)): AxumPath<(String, String)>,
) -> Result<Response, ApiError> {
    match file_auth {
        FileAuth::User(auth) => {
            if username != auth.username {
                return Err(ApiError(AppError::Forbidden(
                    "Cannot access another user's files".into(),
                )));
            }
        }
        FileAuth::Presigned { owner, path } => {
            if !owner.starts_with("user:") || path != filename {
                return Err(ApiError(AppError::Forbidden(
                    "Presigned URL does not match requested file".into(),
                )));
            }
        }
    }

    let virtual_path = format!("user://{username}/{filename}");
    serve_file(&virtual_path, &state).await
}

async fn download_agent_file(
    file_auth: FileAuth,
    State(state): State<AppState>,
    AxumPath((agent_id, filepath)): AxumPath<(String, String)>,
) -> Result<Response, ApiError> {
    match &file_auth {
        FileAuth::User(auth) => {
            // Verify agent ownership
            state
                .agent_service
                .get(&auth.user_id, &agent_id)
                .await?;
        }
        FileAuth::Presigned { owner, path } => {
            if owner != &format!("agent:{agent_id}") || *path != filepath {
                return Err(ApiError(AppError::Forbidden(
                    "Presigned URL does not match requested file".into(),
                )));
            }
        }
    }

    let virtual_path = format!("agent://{agent_id}/{filepath}");
    serve_file(&virtual_path, &state).await
}

async fn delete_user_file(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath((username, filename)): AxumPath<(String, String)>,
) -> Result<(), ApiError> {
    if username != auth.username {
        return Err(ApiError(AppError::Forbidden(
            "Cannot delete another user's files".into(),
        )));
    }

    let virtual_path = format!("user://{username}/{filename}");
    let resolved = resolve_virtual_path(&virtual_path, &state.config)?;

    if !resolved.exists() {
        return Err(ApiError(AppError::NotFound(
            "File not found".into(),
        )));
    }

    if resolved.is_dir() {
        fs::remove_dir_all(&resolved)
            .await
            .map_err(|e| ApiError(AppError::Internal(e.to_string())))?;
    } else {
        fs::remove_file(&resolved)
            .await
            .map_err(|e| ApiError(AppError::Internal(e.to_string())))?;
    }

    Ok(())
}

#[derive(Deserialize)]
struct PresignRequest {
    owner: String,
    path: String,
}

async fn presign_file(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<PresignRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Validate ownership: user files must belong to the requesting user
    if let Some(user_id) = req.owner.strip_prefix("user:")
        && user_id != auth.user_id
    {
        return Err(ApiError(AppError::Forbidden(
            "Cannot presign another user's files".into(),
        )));
    }

    // Resolve the virtual path to verify the file exists
    let virtual_path = if req.owner.starts_with("user:") {
        format!("user://{}/{}", auth.username, req.path)
    } else if let Some(agent_id) = req.owner.strip_prefix("agent:") {
        format!("agent://{}/{}", agent_id, req.path)
    } else {
        return Err(ApiError(AppError::Validation(
            "Invalid owner prefix".into(),
        )));
    };
    let _ = resolve_virtual_path(&virtual_path, &state.config)?;

    let url = state
        .presign_service
        .sign(&req.owner, &req.path, &auth.user_id, &auth.username)
        .await?;

    if url.is_empty() {
        return Err(ApiError(AppError::Internal(
            "Failed to generate presigned URL".into(),
        )));
    }

    Ok(Json(serde_json::json!({ "url": url })))
}

async fn serve_file(virtual_path: &str, state: &AppState) -> Result<Response, ApiError> {
    let resolved = resolve_virtual_path(virtual_path, &state.config)?;

    if !resolved.exists() {
        return Err(ApiError(AppError::NotFound(
            "File not found".into(),
        )));
    }

    let filename = resolved
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download");
    let content_type = detect_content_type(filename);

    let file = fs::File::open(&resolved)
        .await
        .map_err(|e| ApiError(AppError::Internal(e.to_string())))?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(
            header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{filename}\""),
        )
        .body(body)
        .unwrap())
}

#[derive(Serialize)]
struct FileEntry {
    id: String,
    size: u64,
    date: String,
    #[serde(rename = "type")]
    entry_type: String,
    parent: String,
}

async fn read_dir_entries(dir: &Path, parent_id: &str) -> Result<Vec<FileEntry>, ApiError> {
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut entries = Vec::new();
    let mut read_dir = fs::read_dir(dir)
        .await
        .map_err(|e| ApiError(AppError::Internal(e.to_string())))?;

    while let Some(entry) = read_dir
        .next_entry()
        .await
        .map_err(|e| ApiError(AppError::Internal(e.to_string())))?
    {
        let metadata = entry
            .metadata()
            .await
            .map_err(|e| ApiError(AppError::Internal(e.to_string())))?;

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

async fn list_user_files(
    auth: AuthUser,
    State(state): State<AppState>,
    dirpath: Option<AxumPath<String>>,
) -> Result<Json<Vec<FileEntry>>, ApiError> {
    let rel = dirpath.map(|p| p.0).unwrap_or_default();
    if !rel.is_empty() {
        validate_relative_path(&rel)?;
    }

    let base = PathBuf::from(&state.config.storage.files_path).join(&auth.username);
    let dir = if rel.is_empty() {
        base
    } else {
        base.join(&rel)
    };

    let parent_id = if rel.is_empty() {
        "/".to_string()
    } else {
        format!("/{rel}")
    };

    Ok(Json(read_dir_entries(&dir, &parent_id).await?))
}

async fn list_agent_dir(
    auth: &AuthUser,
    state: &AppState,
    agent_id: &str,
    rel: &str,
) -> Result<Json<Vec<FileEntry>>, ApiError> {
    // Verify agent ownership
    state.agent_service.get(&auth.user_id, agent_id).await?;

    if !rel.is_empty() {
        validate_relative_path(rel)?;
    }

    let dir = if rel.is_empty() {
        PathBuf::from(&state.config.storage.workspaces_path).join(agent_id)
    } else {
        PathBuf::from(&state.config.storage.workspaces_path)
            .join(agent_id)
            .join(rel)
    };

    let parent_id = if rel.is_empty() {
        "/".to_string()
    } else {
        format!("/{rel}")
    };

    Ok(Json(read_dir_entries(&dir, &parent_id).await?))
}

async fn list_agent_files_root(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(agent_id): AxumPath<String>,
) -> Result<Json<Vec<FileEntry>>, ApiError> {
    list_agent_dir(&auth, &state, &agent_id, "").await
}

async fn list_agent_files_subdir(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath((agent_id, dirpath)): AxumPath<(String, String)>,
) -> Result<Json<Vec<FileEntry>>, ApiError> {
    list_agent_dir(&auth, &state, &agent_id, &dirpath).await
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    /// Scope: "user", "user:/subdir", "agent:agent_id", or "agent:agent_id/subdir"
    scope: Option<String>,
}

async fn search_files(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<FileEntry>>, ApiError> {
    let q = query.q.to_lowercase();
    if q.is_empty() {
        return Ok(Json(vec![]));
    }

    // Collect (dir, root, source) tuples to search
    let mut search_targets: Vec<(PathBuf, PathBuf, String)> = Vec::new();

    match query.scope.as_deref() {
        Some(scope) if scope.starts_with("agent:") => {
            let rest = &scope["agent:".len()..];
            let (agent_id, subpath) = match rest.find('/') {
                Some(i) => (&rest[..i], Some(&rest[i + 1..])),
                None => (rest, None),
            };
            state.agent_service.get(&auth.user_id, agent_id).await?;
            let root =
                PathBuf::from(&state.config.storage.workspaces_path).join(agent_id);
            let dir = match subpath {
                Some(sub) => root.join(sub),
                None => root.clone(),
            };
            search_targets.push((dir, root, agent_id.to_string()));
        }
        Some(scope) if scope.starts_with("user") => {
            let subpath = scope.strip_prefix("user:").unwrap_or("");
            let base =
                PathBuf::from(&state.config.storage.files_path).join(&auth.username);
            let dir = if subpath.is_empty() {
                base.clone()
            } else {
                base.join(subpath)
            };
            search_targets.push((dir, base, "user".to_string()));
        }
        _ => {
            let user_dir =
                PathBuf::from(&state.config.storage.files_path).join(&auth.username);
            search_targets.push((user_dir.clone(), user_dir, "user".to_string()));

            let user_agents = state.agent_service.list(&auth.user_id).await?;
            for agent in &user_agents {
                let agent_dir =
                    PathBuf::from(&state.config.storage.workspaces_path).join(&agent.id);
                if agent_dir.is_dir() {
                    search_targets.push((agent_dir.clone(), agent_dir, agent.id.clone()));
                }
            }
        }
    }

    let results = tokio::task::spawn_blocking(move || {
        let mut results = Vec::new();
        for (dir, root, source) in &search_targets {
            results.extend(search_dir(dir, root, &q, source));
        }
        results
    })
    .await
    .map_err(|e| ApiError(AppError::Internal(e.to_string())))?;

    Ok(Json(results))
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
        // Skip the root directory itself
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

#[derive(Deserialize)]
struct RenameRequest {
    path: String,
    new_name: String,
}

async fn rename_user_file(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<RenameRequest>,
) -> Result<(), ApiError> {
    let virtual_path = format!("user://{}/{}", auth.username, req.path.trim_start_matches('/'));
    let resolved = resolve_virtual_path(&virtual_path, &state.config)?;

    if !resolved.exists() {
        return Err(ApiError(AppError::NotFound(
            "File not found".into(),
        )));
    }

    if req.new_name.contains('/') || req.new_name.contains("..") || req.new_name.contains('\0') {
        return Err(ApiError(AppError::Validation(
            "Invalid filename".into(),
        )));
    }

    let dest = resolved
        .parent()
        .ok_or_else(|| {
            ApiError(AppError::Internal("No parent dir".into()))
        })?
        .join(&req.new_name);

    if dest.exists() {
        return Err(ApiError(AppError::Validation(
            "A file with that name already exists".into(),
        )));
    }

    fs::rename(&resolved, &dest)
        .await
        .map_err(|e| ApiError(AppError::Internal(e.to_string())))?;

    Ok(())
}

#[derive(Deserialize)]
struct CopyMoveRequest {
    sources: Vec<String>,
    destination: String,
}

fn resolve_file_virtual_path(
    path: &str,
    auth: &AuthUser,
    config: &crate::core::config::Config,
) -> Result<PathBuf, ApiError> {
    if let Some(rest) = path.strip_prefix("user://") {
        // Ensure user can only access their own files
        let slash = rest.find('/').unwrap_or(rest.len());
        let path_username = &rest[..slash];
        if path_username != auth.username {
            return Err(ApiError(AppError::Forbidden(
                "Cannot access another user's files".into(),
            )));
        }
        resolve_virtual_path(path, config).map_err(ApiError)
    } else if path.starts_with("agent://") {
        resolve_virtual_path(path, config).map_err(ApiError)
    } else {
        let trimmed = path.trim_start_matches('/');
        let virtual_path = format!("user://{}/{trimmed}", auth.username);
        resolve_virtual_path(&virtual_path, config).map_err(ApiError)
    }
}

fn ensure_user_destination(path: &str) -> Result<(), ApiError> {
    if path.starts_with("agent://") {
        return Err(ApiError(AppError::Forbidden(
            "Cannot write to agent workspaces".into(),
        )));
    }
    Ok(())
}

async fn copy_files(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CopyMoveRequest>,
) -> Result<(), ApiError> {
    ensure_user_destination(&req.destination)?;

    let dest_dir =
        resolve_file_virtual_path(&req.destination, &auth, &state.config)?;

    fs::create_dir_all(&dest_dir)
        .await
        .map_err(|e| ApiError(AppError::Internal(e.to_string())))?;

    for source in &req.sources {
        let src = resolve_file_virtual_path(source, &auth, &state.config)?;
        if !src.exists() {
            continue;
        }
        let name = src
            .file_name()
            .ok_or_else(|| {
                ApiError(AppError::Internal("No filename".into()))
            })?
            .to_string_lossy()
            .into_owned();
        let target = dest_dir.join(&name);
        if src.is_dir() {
            copy_dir_recursive(&src, &target).await?;
        } else {
            fs::copy(&src, &target)
                .await
                .map_err(|e| ApiError(AppError::Internal(e.to_string())))?;
        }
    }

    Ok(())
}

async fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), ApiError> {
    fs::create_dir_all(dest)
        .await
        .map_err(|e| ApiError(AppError::Internal(e.to_string())))?;

    let mut read_dir = fs::read_dir(src)
        .await
        .map_err(|e| ApiError(AppError::Internal(e.to_string())))?;

    while let Some(entry) = read_dir
        .next_entry()
        .await
        .map_err(|e| ApiError(AppError::Internal(e.to_string())))?
    {
        let target = dest.join(entry.file_name());
        if entry
            .metadata()
            .await
            .map_err(|e| ApiError(AppError::Internal(e.to_string())))?
            .is_dir()
        {
            Box::pin(copy_dir_recursive(&entry.path(), &target)).await?;
        } else {
            fs::copy(entry.path(), &target)
                .await
                .map_err(|e| ApiError(AppError::Internal(e.to_string())))?;
        }
    }

    Ok(())
}

async fn move_files(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CopyMoveRequest>,
) -> Result<(), ApiError> {
    ensure_user_destination(&req.destination)?;

    let dest_dir =
        resolve_file_virtual_path(&req.destination, &auth, &state.config)?;

    fs::create_dir_all(&dest_dir)
        .await
        .map_err(|e| ApiError(AppError::Internal(e.to_string())))?;

    for source in &req.sources {
        if source.starts_with("agent://") {
            return Err(ApiError(AppError::Forbidden(
                "Cannot move from agent workspaces".into(),
            )));
        }
        let src = resolve_file_virtual_path(source, &auth, &state.config)?;
        if !src.exists() {
            continue;
        }
        let name = src
            .file_name()
            .ok_or_else(|| {
                ApiError(AppError::Internal("No filename".into()))
            })?
            .to_string_lossy()
            .into_owned();
        let target = dest_dir.join(&name);
        fs::rename(&src, &target)
            .await
            .map_err(|e| ApiError(AppError::Internal(e.to_string())))?;
    }

    Ok(())
}

#[derive(Deserialize)]
struct MkdirRequest {
    path: String,
}

async fn create_user_folder(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<MkdirRequest>,
) -> Result<(), ApiError> {
    let trimmed = req.path.trim_start_matches('/');
    validate_relative_path(trimmed)?;

    let virtual_path = format!("user://{}/{trimmed}", auth.username);
    let resolved = resolve_virtual_path(&virtual_path, &state.config)?;

    fs::create_dir_all(&resolved)
        .await
        .map_err(|e| ApiError(AppError::Internal(e.to_string())))?;

    Ok(())
}

fn validate_relative_path(path: &str) -> Result<(), ApiError> {
    if path.contains("..") {
        return Err(ApiError(AppError::Validation(
            "Path traversal not allowed".into(),
        )));
    }
    if path.starts_with('/') {
        return Err(ApiError(AppError::Validation(
            "Path must be relative".into(),
        )));
    }
    if path.contains('\0') {
        return Err(ApiError(AppError::Validation(
            "Path contains invalid characters".into(),
        )));
    }
    Ok(())
}
