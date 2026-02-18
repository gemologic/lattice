use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::api::ListQuery;
use crate::db::models::ProjectSummary;
use crate::db::queries;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects", get(list_projects).post(create_project))
        .route(
            "/projects/{slug}",
            get(get_project)
                .patch(update_project)
                .delete(delete_project),
        )
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub goal: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub goal: Option<String>,
}

async fn list_projects(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<Vec<ProjectSummary>>> {
    let (limit, offset) = query.normalize()?;
    let projects = queries::list_projects(&state.db, limit, offset).await?;
    Ok(Json(projects))
}

async fn create_project(
    State(state): State<AppState>,
    Json(payload): Json<CreateProjectRequest>,
) -> AppResult<(StatusCode, Json<ProjectSummary>)> {
    let project =
        queries::create_project_with_slug(&state.db, &payload.name, &payload.goal, &payload.slug)
            .await?;
    Ok((StatusCode::CREATED, Json(project)))
}

async fn get_project(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> AppResult<Json<ProjectSummary>> {
    let project = queries::get_project(&state.db, &slug).await?;
    Ok(Json(project))
}

async fn update_project(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdateProjectRequest>,
) -> AppResult<Json<ProjectSummary>> {
    if payload.name.is_none() && payload.goal.is_none() {
        return Err(AppError::BadRequest(
            "at least one field must be provided".to_string(),
        ));
    }

    let project = queries::update_project(
        &state.db,
        &slug,
        payload.name,
        payload.goal,
        &actor_from_headers(&headers),
    )
    .await?;
    Ok(Json(project))
}

async fn delete_project(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> AppResult<StatusCode> {
    queries::delete_project(&state.db, &slug).await?;
    Ok(StatusCode::NO_CONTENT)
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
