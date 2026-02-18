use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::api::ListQuery;
use crate::db::models::{SpecRevisionRecord, SpecSectionRecord};
use crate::db::queries;
use crate::error::AppResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects/{slug}/spec", get(list_spec_sections))
        .route(
            "/projects/{slug}/spec/{section}",
            get(get_spec_section).put(update_spec_section),
        )
        .route(
            "/projects/{slug}/spec/{section}/history",
            get(get_spec_section_history),
        )
}

#[derive(Debug, Deserialize)]
struct UpdateSpecSectionRequest {
    content: String,
}

async fn list_spec_sections(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> AppResult<Json<Vec<SpecSectionRecord>>> {
    let sections = queries::list_spec_sections(&state.db, &slug).await?;
    Ok(Json(sections))
}

async fn get_spec_section(
    State(state): State<AppState>,
    Path((slug, section)): Path<(String, String)>,
) -> AppResult<Json<SpecSectionRecord>> {
    let record = queries::get_spec_section(&state.db, &slug, &section).await?;
    Ok(Json(record))
}

async fn update_spec_section(
    State(state): State<AppState>,
    Path((slug, section)): Path<(String, String)>,
    headers: HeaderMap,
    Json(payload): Json<UpdateSpecSectionRequest>,
) -> AppResult<Json<SpecSectionRecord>> {
    let record = queries::update_spec_section(
        &state.db,
        &slug,
        &section,
        &payload.content,
        &actor_from_headers(&headers),
    )
    .await?;

    Ok(Json(record))
}

async fn get_spec_section_history(
    State(state): State<AppState>,
    Path((slug, section)): Path<(String, String)>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<Vec<SpecRevisionRecord>>> {
    let (limit, offset) = query.normalize()?;
    let history = queries::list_spec_history(&state.db, &slug, &section, limit, offset).await?;
    Ok(Json(history))
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
