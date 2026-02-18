use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::ListQuery;
use crate::db::models::OpenQuestionRecord;
use crate::db::queries;
use crate::error::AppResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects/{slug}/questions", get(list_open_questions))
        .route(
            "/projects/{slug}/tasks/{task_ref}/questions",
            post(create_question),
        )
        .route(
            "/projects/{slug}/tasks/{task_ref}/questions/{question_id}",
            patch(answer_question),
        )
}

#[derive(Debug, Deserialize)]
struct CreateQuestionRequest {
    question: String,
    context: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnswerQuestionRequest {
    answer: String,
}

#[derive(Debug, Serialize)]
struct ProjectOpenQuestionResponse {
    id: String,
    task_id: String,
    task_number: i64,
    task_display_key: String,
    question: String,
    context: String,
    answer: Option<String>,
    status: String,
    asked_by: String,
    resolved_by: Option<String>,
    created_at: String,
    resolved_at: Option<String>,
}

async fn list_open_questions(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<Vec<ProjectOpenQuestionResponse>>> {
    let (limit, offset) = query.normalize()?;
    let records = queries::list_project_open_questions(&state.db, &slug, limit, offset).await?;

    let payload = records
        .into_iter()
        .map(|record| ProjectOpenQuestionResponse {
            task_display_key: queries::display_key(&slug, record.task_number),
            id: record.id,
            task_id: record.task_id,
            task_number: record.task_number,
            question: record.question,
            context: record.context,
            answer: record.answer,
            status: record.status,
            asked_by: record.asked_by,
            resolved_by: record.resolved_by,
            created_at: record.created_at,
            resolved_at: record.resolved_at,
        })
        .collect();

    Ok(Json(payload))
}

async fn create_question(
    State(state): State<AppState>,
    Path((slug, task_ref)): Path<(String, String)>,
    headers: HeaderMap,
    Json(payload): Json<CreateQuestionRequest>,
) -> AppResult<(StatusCode, Json<OpenQuestionRecord>)> {
    let question = queries::create_open_question(
        &state.db,
        &slug,
        &task_ref,
        &payload.question,
        payload.context.as_deref().unwrap_or_default(),
        &actor_from_headers(&headers),
    )
    .await?;

    Ok((StatusCode::CREATED, Json(question)))
}

async fn answer_question(
    State(state): State<AppState>,
    Path((slug, task_ref, question_id)): Path<(String, String, String)>,
    headers: HeaderMap,
    Json(payload): Json<AnswerQuestionRequest>,
) -> AppResult<Json<OpenQuestionRecord>> {
    let question = queries::answer_open_question(
        &state.db,
        &slug,
        &task_ref,
        &question_id,
        &payload.answer,
        &actor_from_headers(&headers),
    )
    .await?;

    Ok(Json(question))
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
