use std::path::Path;

use axum::body::Body;
use axum::extract::{Multipart, Path as AxumPath, State};
use axum::http::header;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use tokio::fs;
use tokio_util::io::ReaderStream;

use crate::api::files::{
    Attachment, dedup_filename, detect_content_type, make_user_path, resolve_virtual_path,
};

use super::super::error::ApiError;
use super::super::middleware::auth::AuthUser;
use crate::core::state::AppState;

const MAX_FILE_SIZE: usize = 10 * 1024 * 1024; // 10MB

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/files", post(upload_file))
        .route(
            "/api/files/user/{user_id}/{*filename}",
            get(download_user_file).delete(delete_user_file),
        )
        .route(
            "/api/files/agent/{agent_id}/{*filepath}",
            get(download_agent_file),
        )
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
        .map_err(|e| ApiError(crate::core::error::AppError::Validation(e.to_string())))?
    {
        match field.name() {
            Some("path") => {
                relative_path = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError(crate::core::error::AppError::Validation(e.to_string())))?,
                );
            }
            Some("file") => {
                let filename = field
                    .file_name()
                    .unwrap_or("upload")
                    .to_string();
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError(crate::core::error::AppError::Validation(e.to_string())))?;

                if bytes.len() > MAX_FILE_SIZE {
                    return Err(ApiError(crate::core::error::AppError::Validation(
                        format!("File too large (max {}MB)", MAX_FILE_SIZE / 1024 / 1024),
                    )));
                }

                file_data = Some((filename, bytes.to_vec()));
            }
            _ => {}
        }
    }

    let (original_filename, bytes) = file_data.ok_or_else(|| {
        ApiError(crate::core::error::AppError::Validation(
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
        let dir = Path::new(&state.config.files_base_path)
            .join(&auth.user_id)
            .join(&parent);
        (dir, leaf, Some(rel_path.clone()))
    } else {
        let dir = Path::new(&state.config.files_base_path).join(&auth.user_id);
        (dir, original_filename.clone(), None)
    };

    fs::create_dir_all(&dir)
        .await
        .map_err(|e| ApiError(crate::core::error::AppError::Internal(e.to_string())))?;

    let final_filename = dedup_filename(&dir, &filename_for_dedup);
    let dest = dir.join(&final_filename);

    fs::write(&dest, &bytes)
        .await
        .map_err(|e| ApiError(crate::core::error::AppError::Internal(e.to_string())))?;

    let virtual_path = if let Some(mut rel) = virtual_relative {
        if let Some(parent) = Path::new(&rel).parent() {
            rel = parent.join(&final_filename).to_string_lossy().into_owned();
        } else {
            rel = final_filename.clone();
        }
        make_user_path(&auth.user_id, &rel)
    } else {
        make_user_path(&auth.user_id, &final_filename)
    };

    let content_type = detect_content_type(&final_filename).to_string();
    let size_bytes = bytes.len() as u64;

    Ok(Json(Attachment {
        filename: final_filename,
        content_type,
        size_bytes,
        path: virtual_path,
    }))
}

async fn download_user_file(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath((user_id, filename)): AxumPath<(String, String)>,
) -> Result<Response, ApiError> {
    if user_id != auth.user_id {
        return Err(ApiError(crate::core::error::AppError::Forbidden(
            "Cannot access another user's files".into(),
        )));
    }

    let virtual_path = format!("user://{user_id}/{filename}");
    serve_file(&virtual_path, &state).await
}

async fn download_agent_file(
    _auth: AuthUser,
    State(state): State<AppState>,
    AxumPath((agent_id, filepath)): AxumPath<(String, String)>,
) -> Result<Response, ApiError> {
    let virtual_path = format!("agent://{agent_id}/{filepath}");
    serve_file(&virtual_path, &state).await
}

async fn delete_user_file(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath((user_id, filename)): AxumPath<(String, String)>,
) -> Result<(), ApiError> {
    if user_id != auth.user_id {
        return Err(ApiError(crate::core::error::AppError::Forbidden(
            "Cannot delete another user's files".into(),
        )));
    }

    let virtual_path = format!("user://{user_id}/{filename}");
    let resolved = resolve_virtual_path(&virtual_path, &state.config)?;

    if !resolved.exists() {
        return Err(ApiError(crate::core::error::AppError::NotFound(
            "File not found".into(),
        )));
    }

    fs::remove_file(&resolved)
        .await
        .map_err(|e| ApiError(crate::core::error::AppError::Internal(e.to_string())))?;

    Ok(())
}

async fn serve_file(virtual_path: &str, state: &AppState) -> Result<Response, ApiError> {
    let resolved = resolve_virtual_path(virtual_path, &state.config)?;

    if !resolved.exists() {
        return Err(ApiError(crate::core::error::AppError::NotFound(
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
        .map_err(|e| ApiError(crate::core::error::AppError::Internal(e.to_string())))?;
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

fn validate_relative_path(path: &str) -> Result<(), ApiError> {
    if path.contains("..") {
        return Err(ApiError(crate::core::error::AppError::Validation(
            "Path traversal not allowed".into(),
        )));
    }
    if path.starts_with('/') {
        return Err(ApiError(crate::core::error::AppError::Validation(
            "Path must be relative".into(),
        )));
    }
    Ok(())
}
