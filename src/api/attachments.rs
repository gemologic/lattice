use std::io::ErrorKind;
use std::path::{Component, Path as FsPath, PathBuf};

use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use uuid::Uuid;

use crate::db::models::AttachmentRecord;
use crate::db::queries;
use crate::db::queries::NewAttachmentInput;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/projects/{slug}/tasks/{task_ref}/attachments",
            post(upload_attachment),
        )
        .route(
            "/projects/{slug}/tasks/{task_ref}/attachments/{attachment_id}",
            delete(delete_attachment),
        )
        .route("/files/{id}", get(download_attachment))
}

async fn upload_attachment(
    State(state): State<AppState>,
    Path((slug, task_ref)): Path<(String, String)>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> AppResult<(StatusCode, Json<AttachmentRecord>)> {
    let mut filename: Option<String> = None;
    let mut content_type: Option<String> = None;
    let mut file_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart.next_field().await.map_err(|error| {
        tracing::warn!(error = ?error, "invalid multipart upload");
        AppError::BadRequest("invalid multipart payload".to_string())
    })? {
        if file_bytes.is_some() {
            return Err(AppError::BadRequest(
                "only one file upload is supported per request".to_string(),
            ));
        }

        filename = Some(sanitize_filename(field.file_name().unwrap_or("upload.bin")));
        content_type = Some(
            field
                .content_type()
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| guess_mime_type(filename.as_deref().unwrap_or("upload.bin"))),
        );

        let bytes = field.bytes().await.map_err(|error| {
            tracing::warn!(error = ?error, "failed to read multipart file field");
            AppError::BadRequest("invalid file payload".to_string())
        })?;

        let size = u64::try_from(bytes.len()).map_err(|_| {
            AppError::BadRequest("uploaded file is too large to process".to_string())
        })?;

        if size > state.config.max_file_size {
            return Err(AppError::BadRequest(format!(
                "file exceeds max size of {} bytes",
                state.config.max_file_size
            )));
        }

        file_bytes = Some(bytes.to_vec());
    }

    let file_bytes = file_bytes.ok_or_else(|| {
        AppError::BadRequest("multipart payload must include one file field".to_string())
    })?;

    let attachment_id = Uuid::new_v4().to_string();
    let filename = filename.unwrap_or_else(|| "upload.bin".to_string());
    let content_type = content_type.unwrap_or_else(|| "application/octet-stream".to_string());
    let storage_path = format!("{attachment_id}.blob");
    let absolute_path = storage_file_path(&state.config.storage_dir, &storage_path)?;

    tokio::fs::write(&absolute_path, &file_bytes)
        .await
        .map_err(|error| {
            tracing::error!(error = ?error, path = %absolute_path.display(), "failed to write attachment");
            AppError::Internal
        })?;

    let size_bytes = i64::try_from(file_bytes.len())
        .map_err(|_| AppError::BadRequest("uploaded file is too large to store".to_string()))?;

    let created = queries::create_attachment(
        &state.db,
        &slug,
        &task_ref,
        NewAttachmentInput {
            id: attachment_id.clone(),
            filename,
            content_type,
            size_bytes,
            storage_path,
            uploaded_by: actor_from_headers(&headers),
        },
    )
    .await;

    match created {
        Ok(record) => Ok((StatusCode::CREATED, Json(record))),
        Err(error) => {
            if let Err(remove_error) = tokio::fs::remove_file(&absolute_path).await {
                if remove_error.kind() != ErrorKind::NotFound {
                    tracing::warn!(
                        error = ?remove_error,
                        path = %absolute_path.display(),
                        "failed to cleanup attachment file after db error"
                    );
                }
            }
            Err(error)
        }
    }
}

async fn delete_attachment(
    State(state): State<AppState>,
    Path((slug, task_ref, attachment_id)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    let attachment = queries::delete_attachment(
        &state.db,
        &slug,
        &task_ref,
        &attachment_id,
        &actor_from_headers(&headers),
    )
    .await?;

    let path = storage_file_path(&state.config.storage_dir, &attachment.storage_path)?;
    if let Err(error) = tokio::fs::remove_file(&path).await {
        if error.kind() != ErrorKind::NotFound {
            tracing::warn!(
                error = ?error,
                path = %path.display(),
                "failed to remove attachment file from storage"
            );
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn download_attachment(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Response> {
    let attachment = queries::get_attachment(&state.db, &id).await?;
    let path = storage_file_path(&state.config.storage_dir, &attachment.storage_path)?;

    let bytes = tokio::fs::read(&path).await.map_err(|error| match error.kind() {
        ErrorKind::NotFound => {
            AppError::NotFound(format!("attachment file '{}' is missing from disk", attachment.id))
        }
        _ => {
            tracing::error!(error = ?error, path = %path.display(), "failed to read attachment file");
            AppError::Internal
        }
    })?;

    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::OK;

    let content_type = HeaderValue::from_str(&attachment.content_type)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    response.headers_mut().insert(CONTENT_TYPE, content_type);

    if let Ok(content_length) = HeaderValue::from_str(&attachment.size_bytes.to_string()) {
        response
            .headers_mut()
            .insert(CONTENT_LENGTH, content_length);
    }

    if let Ok(disposition) = HeaderValue::from_str(&format!(
        "attachment; filename=\"{}\"",
        escape_filename(&attachment.filename)
    )) {
        response
            .headers_mut()
            .insert(CONTENT_DISPOSITION, disposition);
    }

    Ok(response)
}

fn sanitize_filename(raw: &str) -> String {
    let leaf = raw.rsplit(['/', '\\']).next().unwrap_or(raw).trim();
    if leaf.is_empty() {
        return "upload.bin".to_string();
    }

    let sanitized = leaf
        .chars()
        .map(|character| {
            if character.is_control() {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();

    if sanitized.trim().is_empty() {
        "upload.bin".to_string()
    } else {
        sanitized
    }
}

fn escape_filename(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn guess_mime_type(filename: &str) -> String {
    mime_guess::from_path(filename)
        .first_or_octet_stream()
        .essence_str()
        .to_string()
}

fn storage_file_path(storage_dir: &FsPath, storage_path: &str) -> AppResult<PathBuf> {
    let relative = FsPath::new(storage_path);
    if relative.as_os_str().is_empty() {
        return Err(AppError::Internal);
    }

    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        tracing::warn!(storage_path, "rejected unsafe storage path");
        return Err(AppError::Internal);
    }

    Ok(storage_dir.join(relative))
}

fn actor_from_headers(headers: &HeaderMap) -> String {
    headers
        .get("MCP-Client")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "human".to_string())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use reqwest::multipart::{Form, Part};
    use reqwest::StatusCode;
    use tempfile::tempdir;

    use crate::api;
    use crate::config::{Config, RateLimitConfig};
    use crate::db;
    use crate::db::queries;
    use crate::db::queries::NewTaskInput;
    use crate::state::AppState;

    #[tokio::test]
    async fn upload_download_and_delete_attachment_roundtrip() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let db_path = temp_dir.path().join("phase6_attachment_test.db");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
        let storage_dir = temp_dir.path().join("storage");
        std::fs::create_dir_all(&storage_dir).expect("storage dir should be created");

        let config = Config {
            port: 0,
            db_url,
            token: None,
            log_level: "info".to_string(),
            storage_dir: storage_dir.clone(),
            max_file_size: 10 * 1024 * 1024,
            rate_limits: RateLimitConfig::default(),
        };

        let pool = db::connect_and_migrate(&config)
            .await
            .expect("database should initialize");
        queries::create_project_with_slug(&pool, "Attachments", "test", "ATTACH")
            .await
            .expect("project should be created");
        let task = queries::create_task(
            &pool,
            "ATTACH",
            NewTaskInput {
                title: "attachment target".to_string(),
                description: String::new(),
                status: "backlog".to_string(),
                priority: "medium".to_string(),
                review_state: "ready".to_string(),
                labels: Vec::new(),
                created_by: "human".to_string(),
            },
        )
        .await
        .expect("task should be created");

        let state = AppState::new(config, pool);
        let app = Router::new()
            .nest("/api/v1", api::router())
            .route("/healthz", get(api::healthz))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                api::auth::require_auth,
            ))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener
            .local_addr()
            .expect("listener address should be readable");
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("client should build");

        let upload_url = format!(
            "http://{addr}/api/v1/projects/ATTACH/tasks/{}/attachments",
            task.id
        );
        let form = Form::new().part(
            "file",
            Part::bytes(b"hello from lattice".to_vec())
                .file_name("demo.txt")
                .mime_str("text/plain")
                .expect("mime should parse"),
        );
        let upload = client
            .post(upload_url)
            .multipart(form)
            .send()
            .await
            .expect("upload request should succeed");
        assert_eq!(upload.status(), StatusCode::CREATED);

        let upload_body: serde_json::Value = upload
            .json()
            .await
            .expect("upload response json should parse");
        let attachment_id = upload_body
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned)
            .expect("attachment id should be present");
        let stored_path = upload_body
            .get("storage_path")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned)
            .expect("storage path should be present");
        assert_eq!(
            upload_body
                .get("filename")
                .and_then(serde_json::Value::as_str),
            Some("demo.txt")
        );

        let download_url = format!("http://{addr}/api/v1/files/{attachment_id}");
        let download = client
            .get(&download_url)
            .send()
            .await
            .expect("download request should succeed");
        assert_eq!(download.status(), StatusCode::OK);
        let content = download
            .bytes()
            .await
            .expect("downloaded bytes should be readable");
        assert_eq!(content.as_ref(), b"hello from lattice");

        let delete_url = format!(
            "http://{addr}/api/v1/projects/ATTACH/tasks/{}/attachments/{attachment_id}",
            task.id
        );
        let deleted = client
            .delete(delete_url)
            .send()
            .await
            .expect("delete request should succeed");
        assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
        assert!(
            !storage_dir.join(stored_path).exists(),
            "attachment file should be removed from disk"
        );

        let missing = client
            .get(download_url)
            .send()
            .await
            .expect("missing download request should succeed");
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);

        server.abort();
    }
}
