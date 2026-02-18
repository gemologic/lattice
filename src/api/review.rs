use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::db::models::TaskRecord;
use crate::db::queries;
use crate::error::AppResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/projects/{slug}/tasks/{task_ref}/review",
        post(set_review_state),
    )
}

#[derive(Debug, Deserialize)]
struct SetReviewStateRequest {
    review_state: String,
}

#[derive(Debug, Serialize)]
struct TaskReviewResponse {
    id: String,
    display_key: String,
    task_number: i64,
    title: String,
    description: String,
    status: String,
    priority: String,
    review_state: String,
    sort_order: f64,
    created_by: String,
    created_at: String,
    updated_at: String,
}

async fn set_review_state(
    State(state): State<AppState>,
    Path((slug, task_ref)): Path<(String, String)>,
    headers: HeaderMap,
    Json(payload): Json<SetReviewStateRequest>,
) -> AppResult<Json<TaskReviewResponse>> {
    let task = queries::set_review_state(
        &state.db,
        &slug,
        &task_ref,
        &payload.review_state,
        &actor_from_headers(&headers),
    )
    .await?;

    Ok(Json(map_task_record(&slug, task)))
}

fn map_task_record(slug: &str, task: TaskRecord) -> TaskReviewResponse {
    TaskReviewResponse {
        id: task.id,
        display_key: queries::display_key(slug, task.task_number),
        task_number: task.task_number,
        title: task.title,
        description: task.description,
        status: task.status,
        priority: task.priority,
        review_state: task.review_state,
        sort_order: task.sort_order,
        created_by: task.created_by,
        created_at: task.created_at,
        updated_at: task.updated_at,
    }
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
