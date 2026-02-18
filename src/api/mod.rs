pub mod attachments;
pub mod auth;
pub mod events;
pub mod projects;
pub mod questions;
pub mod review;
pub mod spec;
pub mod tasks;
pub mod webhooks;

use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(attachments::router())
        .merge(projects::router())
        .merge(spec::router())
        .merge(tasks::router())
        .merge(questions::router())
        .merge(review::router())
        .merge(events::router())
        .merge(webhooks::router())
}

#[derive(Debug, Serialize)]
pub struct HealthzResponse {
    pub status: &'static str,
}

pub async fn healthz() -> Json<HealthzResponse> {
    Json(HealthzResponse { status: "ok" })
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl ListQuery {
    pub fn normalize(&self) -> AppResult<(i64, i64)> {
        let limit = self.limit.unwrap_or(50);
        let offset = self.offset.unwrap_or(0);

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
}
