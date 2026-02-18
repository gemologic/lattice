use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::db::models::{SubtaskRecord, TaskDetails, TaskRecord};
use crate::db::queries;
use crate::db::queries::{
    MoveTaskInput, NewTaskInput, TaskFilters, UpdateSubtaskInput, UpdateTaskInput,
};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects/{slug}/tasks", get(list_tasks).post(create_task))
        .route(
            "/projects/{slug}/tasks/{task_ref}",
            get(get_task).patch(update_task).delete(delete_task),
        )
        .route("/projects/{slug}/tasks/{task_ref}/move", post(move_task))
        .route(
            "/projects/{slug}/tasks/{task_ref}/subtasks",
            post(add_subtask),
        )
        .route(
            "/projects/{slug}/tasks/{task_ref}/subtasks/{subtask_id}",
            patch(update_subtask).delete(delete_subtask),
        )
}

#[derive(Debug, Deserialize)]
struct TaskListQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    status: Option<String>,
    label: Option<String>,
    review_state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateTaskRequest {
    title: String,
    description: Option<String>,
    status: Option<String>,
    priority: Option<String>,
    review_state: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateTaskRequest {
    title: Option<String>,
    description: Option<String>,
    status: Option<String>,
    priority: Option<String>,
    review_state: Option<String>,
    labels: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct MoveTaskRequest {
    status: String,
    sort_order: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct CreateSubtaskRequest {
    title: String,
}

#[derive(Debug, Deserialize)]
struct UpdateSubtaskRequest {
    title: Option<String>,
    done: Option<bool>,
    sort_order: Option<f64>,
}

#[derive(Debug, Serialize)]
struct TaskResponse {
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

#[derive(Debug, Serialize)]
struct TaskDetailsResponse {
    task: TaskResponse,
    labels: Vec<String>,
    subtasks: Vec<crate::db::models::SubtaskRecord>,
    open_questions: Vec<crate::db::models::OpenQuestionRecord>,
    attachments: Vec<crate::db::models::AttachmentRecord>,
    history: Vec<crate::db::models::TaskHistoryRecord>,
}

#[derive(Debug, Serialize)]
struct SubtaskResponse {
    id: String,
    task_id: String,
    title: String,
    done: bool,
    sort_order: f64,
    created_at: String,
}

async fn list_tasks(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<TaskListQuery>,
) -> AppResult<Json<Vec<TaskResponse>>> {
    let (limit, offset) = normalize_list_query(query.limit, query.offset)?;

    let tasks = queries::list_tasks(
        &state.db,
        &slug,
        TaskFilters {
            status: query.status,
            label: query.label,
            review_state: query.review_state,
        },
        limit,
        offset,
    )
    .await?;

    let payload = tasks
        .into_iter()
        .map(|task| map_task_record(&slug, task))
        .collect();

    Ok(Json(payload))
}

async fn create_task(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<CreateTaskRequest>,
) -> AppResult<(StatusCode, Json<TaskResponse>)> {
    let actor = actor_from_headers(&headers);

    let task = queries::create_task(
        &state.db,
        &slug,
        NewTaskInput {
            title: payload.title,
            description: payload.description.unwrap_or_default(),
            status: payload.status.unwrap_or_else(|| "backlog".to_string()),
            priority: payload.priority.unwrap_or_else(|| "medium".to_string()),
            review_state: payload.review_state.unwrap_or_else(|| "ready".to_string()),
            labels: payload.labels,
            created_by: actor,
        },
    )
    .await?;

    Ok((StatusCode::CREATED, Json(map_task_record(&slug, task))))
}

async fn get_task(
    State(state): State<AppState>,
    Path((slug, task_ref)): Path<(String, String)>,
) -> AppResult<Json<TaskDetailsResponse>> {
    let details = queries::get_task_details(&state.db, &slug, &task_ref).await?;
    Ok(Json(map_task_details(&slug, details)))
}

async fn update_task(
    State(state): State<AppState>,
    Path((slug, task_ref)): Path<(String, String)>,
    headers: HeaderMap,
    Json(payload): Json<UpdateTaskRequest>,
) -> AppResult<Json<TaskResponse>> {
    if payload.title.is_none()
        && payload.description.is_none()
        && payload.status.is_none()
        && payload.priority.is_none()
        && payload.review_state.is_none()
        && payload.labels.is_none()
    {
        return Err(AppError::BadRequest(
            "at least one field must be provided".to_string(),
        ));
    }

    let task = queries::update_task(
        &state.db,
        &slug,
        &task_ref,
        UpdateTaskInput {
            title: payload.title,
            description: payload.description,
            status: payload.status,
            priority: payload.priority,
            review_state: payload.review_state,
            labels: payload.labels,
            actor: actor_from_headers(&headers),
        },
    )
    .await?;

    Ok(Json(map_task_record(&slug, task)))
}

async fn move_task(
    State(state): State<AppState>,
    Path((slug, task_ref)): Path<(String, String)>,
    headers: HeaderMap,
    Json(payload): Json<MoveTaskRequest>,
) -> AppResult<Json<TaskResponse>> {
    let actor = actor_from_headers(&headers);
    let task = queries::move_task(
        &state.db,
        &slug,
        &task_ref,
        MoveTaskInput {
            status: payload.status,
            sort_order: payload.sort_order,
            actor,
            mcp_origin: headers.get("MCP-Client").is_some(),
        },
    )
    .await?;

    Ok(Json(map_task_record(&slug, task)))
}

async fn add_subtask(
    State(state): State<AppState>,
    Path((slug, task_ref)): Path<(String, String)>,
    headers: HeaderMap,
    Json(payload): Json<CreateSubtaskRequest>,
) -> AppResult<(StatusCode, Json<SubtaskResponse>)> {
    let subtask = queries::add_subtask(
        &state.db,
        &slug,
        &task_ref,
        &payload.title,
        &actor_from_headers(&headers),
    )
    .await?;

    Ok((StatusCode::CREATED, Json(map_subtask(subtask))))
}

async fn update_subtask(
    State(state): State<AppState>,
    Path((slug, task_ref, subtask_id)): Path<(String, String, String)>,
    headers: HeaderMap,
    Json(payload): Json<UpdateSubtaskRequest>,
) -> AppResult<Json<SubtaskResponse>> {
    if payload.title.is_none() && payload.done.is_none() && payload.sort_order.is_none() {
        return Err(AppError::BadRequest(
            "at least one field must be provided".to_string(),
        ));
    }

    let subtask = queries::update_subtask(
        &state.db,
        &slug,
        &task_ref,
        &subtask_id,
        UpdateSubtaskInput {
            title: payload.title,
            done: payload.done,
            sort_order: payload.sort_order,
            actor: actor_from_headers(&headers),
        },
    )
    .await?;

    Ok(Json(map_subtask(subtask)))
}

async fn delete_subtask(
    State(state): State<AppState>,
    Path((slug, task_ref, subtask_id)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    queries::delete_subtask(
        &state.db,
        &slug,
        &task_ref,
        &subtask_id,
        &actor_from_headers(&headers),
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_task(
    State(state): State<AppState>,
    Path((slug, task_ref)): Path<(String, String)>,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    queries::delete_task(&state.db, &slug, &task_ref, &actor_from_headers(&headers)).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn map_task_record(slug: &str, task: TaskRecord) -> TaskResponse {
    TaskResponse {
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

fn map_task_details(slug: &str, details: TaskDetails) -> TaskDetailsResponse {
    TaskDetailsResponse {
        task: map_task_record(slug, details.task),
        labels: details.labels,
        subtasks: details.subtasks,
        open_questions: details.open_questions,
        attachments: details.attachments,
        history: details.history,
    }
}

fn map_subtask(subtask: SubtaskRecord) -> SubtaskResponse {
    SubtaskResponse {
        id: subtask.id,
        task_id: subtask.task_id,
        title: subtask.title,
        done: subtask.done == 1,
        sort_order: subtask.sort_order,
        created_at: subtask.created_at,
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

fn normalize_list_query(limit: Option<i64>, offset: Option<i64>) -> AppResult<(i64, i64)> {
    let limit = limit.unwrap_or(50);
    let offset = offset.unwrap_or(0);

    if limit <= 0 {
        return Err(AppError::BadRequest(
            "limit must be greater than 0".to_string(),
        ));
    }

    if limit > 100 {
        return Err(AppError::BadRequest(
            "limit must be less than or equal to 100".to_string(),
        ));
    }

    if offset < 0 {
        return Err(AppError::BadRequest(
            "offset cannot be negative".to_string(),
        ));
    }

    Ok((limit, offset))
}
