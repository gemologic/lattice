use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::db::models::WebhookRecord;
use crate::db::queries;
use crate::db::queries::{CreateWebhookInput, UpdateWebhookInput};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::webhooks;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/projects/{slug}/webhooks",
            get(list_webhooks).post(create_webhook),
        )
        .route(
            "/projects/{slug}/webhooks/{webhook_id}",
            axum::routing::patch(update_webhook).delete(delete_webhook),
        )
        .route(
            "/projects/{slug}/webhooks/{webhook_id}/test",
            post(test_webhook),
        )
}

#[derive(Debug, Deserialize)]
struct CreateWebhookRequest {
    name: String,
    url: String,
    platform: String,
    events: Vec<String>,
    secret: Option<String>,
    active: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct UpdateWebhookRequest {
    name: Option<String>,
    url: Option<String>,
    platform: Option<String>,
    events: Option<Vec<String>>,
    secret: Option<String>,
    active: Option<bool>,
}

#[derive(Debug, Serialize)]
struct WebhookResponse {
    id: String,
    name: String,
    url: String,
    platform: String,
    events: Vec<String>,
    active: bool,
    has_secret: bool,
    created_at: String,
    updated_at: String,
}

async fn list_webhooks(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> AppResult<Json<Vec<WebhookResponse>>> {
    let records = queries::list_project_webhooks(&state.db, &slug).await?;
    let mut payload = Vec::with_capacity(records.len());
    for record in records {
        payload.push(map_webhook(record)?);
    }
    Ok(Json(payload))
}

async fn create_webhook(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(request): Json<CreateWebhookRequest>,
) -> AppResult<(StatusCode, Json<WebhookResponse>)> {
    let created = queries::create_webhook(
        &state.db,
        &slug,
        CreateWebhookInput {
            name: request.name,
            url: request.url,
            platform: request.platform,
            events: request.events,
            secret: request.secret,
            active: request.active.unwrap_or(true),
        },
    )
    .await?;

    Ok((StatusCode::CREATED, Json(map_webhook(created)?)))
}

async fn update_webhook(
    State(state): State<AppState>,
    Path((slug, webhook_id)): Path<(String, String)>,
    Json(request): Json<UpdateWebhookRequest>,
) -> AppResult<Json<WebhookResponse>> {
    if request.name.is_none()
        && request.url.is_none()
        && request.platform.is_none()
        && request.events.is_none()
        && request.secret.is_none()
        && request.active.is_none()
    {
        return Err(AppError::BadRequest(
            "at least one field must be provided".to_string(),
        ));
    }

    let updated = queries::update_webhook(
        &state.db,
        &slug,
        &webhook_id,
        UpdateWebhookInput {
            name: request.name,
            url: request.url,
            platform: request.platform,
            events: request.events,
            secret: request.secret,
            active: request.active,
        },
    )
    .await?;

    Ok(Json(map_webhook(updated)?))
}

async fn delete_webhook(
    State(state): State<AppState>,
    Path((slug, webhook_id)): Path<(String, String)>,
) -> AppResult<StatusCode> {
    queries::delete_webhook(&state.db, &slug, &webhook_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn test_webhook(
    State(state): State<AppState>,
    Path((slug, webhook_id)): Path<(String, String)>,
) -> AppResult<StatusCode> {
    webhooks::send_test_webhook(&state, &slug, &webhook_id).await?;
    Ok(StatusCode::ACCEPTED)
}

fn map_webhook(record: WebhookRecord) -> AppResult<WebhookResponse> {
    let events = queries::parse_webhook_events(&record.events)?;
    Ok(WebhookResponse {
        id: record.id,
        name: record.name,
        url: record.url,
        platform: record.platform,
        events,
        active: record.active == 1,
        has_secret: record
            .secret
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty()),
        created_at: record.created_at,
        updated_at: record.updated_at,
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::extract::State;
    use axum::http::HeaderMap;
    use axum::middleware;
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use reqwest::StatusCode;
    use serde_json::json;
    use tempfile::tempdir;
    use tokio::sync::mpsc;
    use tokio::time::timeout;

    use crate::api;
    use crate::config::{Config, RateLimitConfig};
    use crate::db;
    use crate::db::queries;
    use crate::state::AppState;

    #[derive(Debug)]
    struct CapturedWebhook {
        headers: HeaderMap,
        body: String,
    }

    #[tokio::test]
    async fn webhook_crud_and_test_endpoint_delivers_signed_payload() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let db_path = temp_dir.path().join("phase6_webhook_test.db");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
        let storage_dir = temp_dir.path().join("storage");
        std::fs::create_dir_all(&storage_dir).expect("storage dir should be created");

        let config = Config {
            port: 0,
            db_url,
            token: None,
            log_level: "info".to_string(),
            storage_dir,
            max_file_size: 10 * 1024 * 1024,
            rate_limits: RateLimitConfig::default(),
        };
        let pool = db::connect_and_migrate(&config)
            .await
            .expect("database should initialize");
        queries::create_project_with_slug(&pool, "Webhooks", "test", "HOOKS")
            .await
            .expect("project should be created");

        let (capture_tx, mut capture_rx) = mpsc::unbounded_channel::<CapturedWebhook>();
        let capture_app = Router::new()
            .route("/webhook", post(capture_webhook))
            .with_state(capture_tx);
        let capture_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("capture listener should bind");
        let capture_addr = capture_listener
            .local_addr()
            .expect("capture listener addr should be readable");
        let capture_server = tokio::spawn(async move {
            let _ = axum::serve(capture_listener, capture_app).await;
        });

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
            .expect("api listener should bind");
        let addr = listener
            .local_addr()
            .expect("api listener addr should be readable");
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("client should build");
        let create_url = format!("http://{addr}/api/v1/projects/HOOKS/webhooks");
        let create = client
            .post(create_url)
            .json(&json!({
                "name": "capture",
                "url": format!("http://{capture_addr}/webhook"),
                "platform": "generic",
                "events": ["task.created"],
                "secret": "top-secret",
                "active": true
            }))
            .send()
            .await
            .expect("create webhook request should succeed");
        assert_eq!(create.status(), StatusCode::CREATED);

        let created_body: serde_json::Value = create
            .json()
            .await
            .expect("create webhook body should parse");
        let webhook_id = created_body
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned)
            .expect("created webhook should include id");

        let list_url = format!("http://{addr}/api/v1/projects/HOOKS/webhooks");
        let listed = client
            .get(list_url)
            .send()
            .await
            .expect("list webhook request should succeed");
        assert_eq!(listed.status(), StatusCode::OK);
        let list_body: serde_json::Value = listed.json().await.expect("list body should parse");
        assert_eq!(list_body.as_array().map(|items| items.len()), Some(1));

        let test_url = format!("http://{addr}/api/v1/projects/HOOKS/webhooks/{webhook_id}/test");
        let tested = client
            .post(test_url)
            .send()
            .await
            .expect("test webhook request should succeed");
        assert_eq!(tested.status(), StatusCode::ACCEPTED);

        let captured = timeout(Duration::from_secs(3), capture_rx.recv())
            .await
            .expect("capture should arrive before timeout")
            .expect("capture channel should include payload");
        assert!(
            captured.headers.get("X-Lattice-Signature").is_some(),
            "generic webhook should include X-Lattice-Signature header"
        );
        assert!(
            captured.body.contains("\"event\":\"test\""),
            "test payload should include event field"
        );

        let delete_url = format!("http://{addr}/api/v1/projects/HOOKS/webhooks/{webhook_id}");
        let deleted = client
            .delete(delete_url)
            .send()
            .await
            .expect("delete webhook request should succeed");
        assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

        capture_server.abort();
        server.abort();
    }

    async fn capture_webhook(
        State(capture_tx): State<mpsc::UnboundedSender<CapturedWebhook>>,
        headers: HeaderMap,
        body: String,
    ) -> Json<serde_json::Value> {
        let _ = capture_tx.send(CapturedWebhook { headers, body });
        Json(json!({ "ok": true }))
    }
}
