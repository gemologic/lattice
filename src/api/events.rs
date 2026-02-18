use std::collections::BTreeSet;
use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;
use tokio_stream::{wrappers::ReceiverStream, Stream};

use crate::db::models::SystemEventRecord;
use crate::db::queries;
use crate::error::AppResult;
use crate::state::AppState;

const SSE_POLL_LIMIT: i64 = 100;
const SSE_POLL_INTERVAL_MS: u64 = 750;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/events", get(stream_events))
        .route("/projects/{slug}/events", get(stream_project_events))
}

#[derive(Debug, Deserialize)]
struct EventsQuery {
    #[serde(default)]
    project: Vec<String>,
}

#[derive(Debug, Serialize)]
struct TaskEventPayload {
    id: String,
    project: String,
    task_id: Option<String>,
    task_number: Option<i64>,
    task_display_key: Option<String>,
    action: String,
    actor: String,
    detail: Value,
    created_at: String,
}

async fn stream_events(
    State(state): State<AppState>,
    Query(query): Query<EventsQuery>,
) -> AppResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let projects = normalize_project_filters(query.project)?;
    Ok(build_sse_stream(state, projects))
}

async fn stream_project_events(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> AppResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let project_slug = queries::normalize_slug(&slug)?;
    let _ = queries::get_project(&state.db, &project_slug).await?;
    Ok(build_sse_stream(state, vec![project_slug]))
}

fn build_sse_stream(
    state: AppState,
    project_slugs: Vec<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (sender, receiver) = mpsc::channel::<Result<Event, Infallible>>(64);
    let db = state.db.clone();

    tokio::spawn(async move {
        let (mut last_created_at, mut last_event_id) =
            match queries::latest_system_event_cursor(&db, &project_slugs).await {
                Ok(Some((created_at, event_id))) => (Some(created_at), Some(event_id)),
                Ok(None) => (None, None),
                Err(error) => {
                    tracing::error!(error = ?error, "failed to initialize sse cursor");
                    (None, None)
                }
            };
        let mut interval = tokio::time::interval(Duration::from_millis(SSE_POLL_INTERVAL_MS));
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            interval.tick().await;

            let events = match queries::list_system_events(
                &db,
                &project_slugs,
                last_created_at.as_deref(),
                last_event_id.as_deref(),
                SSE_POLL_LIMIT,
            )
            .await
            {
                Ok(value) => value,
                Err(error) => {
                    tracing::error!(error = ?error, "failed to query system events for sse");
                    break;
                }
            };

            for event in events {
                last_created_at = Some(event.created_at.clone());
                last_event_id = Some(event.id.clone());

                let payload = map_task_event(event);
                let serialized = match serde_json::to_string(&payload) {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::error!(error = ?error, "failed to serialize sse event");
                        continue;
                    }
                };

                let event = Event::default()
                    .id(payload.id)
                    .event(payload.action)
                    .data(serialized);

                if sender.send(Ok(event)).await.is_err() {
                    return;
                }
            }
        }
    });

    Sse::new(ReceiverStream::new(receiver)).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

fn normalize_project_filters(projects: Vec<String>) -> AppResult<Vec<String>> {
    let mut normalized = BTreeSet::new();
    for project in projects {
        normalized.insert(queries::normalize_slug(&project)?);
    }
    Ok(normalized.into_iter().collect())
}

fn map_task_event(event: SystemEventRecord) -> TaskEventPayload {
    let display_key = event
        .task_number
        .map(|task_number| queries::display_key(&event.project_slug, task_number));
    TaskEventPayload {
        id: event.id,
        project: event.project_slug,
        task_id: event.task_id,
        task_number: event.task_number,
        task_display_key: display_key,
        action: event.action,
        actor: event.actor,
        detail: parse_event_detail(&event.detail),
        created_at: event.created_at,
    }
}

fn parse_event_detail(value: &str) -> Value {
    serde_json::from_str::<Value>(value).unwrap_or_else(|_| Value::String(value.to_string()))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use reqwest::header::ACCEPT;
    use reqwest::StatusCode;
    use tempfile::tempdir;
    use tokio::time::timeout;

    use crate::api;
    use crate::config::{Config, RateLimitConfig};
    use crate::db;
    use crate::db::queries;
    use crate::db::queries::NewTaskInput;
    use crate::state::AppState;

    #[tokio::test]
    async fn project_events_stream_emits_task_created() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let db_path = temp_dir.path().join("phase6_events_test.db");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
        let storage_dir = temp_dir.path().join("storage");

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
        queries::create_project_with_slug(&pool, "Events", "SSE", "EVENTS")
            .await
            .expect("project should be created");

        let state = AppState::new(config, pool.clone());
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

        let stream_url = format!("http://{addr}/api/v1/projects/EVENTS/events");
        let mut stream_response = client
            .get(stream_url)
            .header(ACCEPT, "text/event-stream")
            .send()
            .await
            .expect("sse request should succeed");
        assert_eq!(stream_response.status(), StatusCode::OK);

        queries::create_task(
            &pool,
            "EVENTS",
            NewTaskInput {
                title: "Trigger SSE".to_string(),
                description: String::new(),
                status: "backlog".to_string(),
                priority: "medium".to_string(),
                review_state: "ready".to_string(),
                labels: Vec::new(),
                created_by: "human".to_string(),
            },
        )
        .await
        .expect("task creation should succeed");

        let mut payload = String::new();
        for _ in 0..30 {
            let chunk = match timeout(Duration::from_millis(400), stream_response.chunk()).await {
                Ok(result) => result.expect("stream should not error"),
                Err(_) => continue,
            };

            let Some(chunk) = chunk else {
                break;
            };
            payload.push_str(&String::from_utf8_lossy(&chunk));
            if payload.contains("event: task.created") {
                break;
            }
        }

        assert!(
            payload.contains("event: task.created"),
            "sse payload should include task.created event"
        );
        assert!(
            payload.contains("\"project\":\"EVENTS\""),
            "sse payload should include project slug"
        );

        server.abort();
    }
}
