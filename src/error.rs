use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("not found: {0}")]
    NotFound(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("internal server error")]
    Internal,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, "bad_request", message),
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "missing or invalid bearer token".to_string(),
            ),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, "not_found", message),
            Self::Conflict(message) => (StatusCode::CONFLICT, "conflict", message),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "unexpected error".to_string(),
            ),
        };

        let body = Json(ErrorBody {
            error: error.to_string(),
            message,
        });

        (status, body).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        match error {
            sqlx::Error::RowNotFound => Self::NotFound("record not found".to_string()),
            sqlx::Error::Database(db_error) => {
                let message = db_error.message().to_string();
                if db_error.is_unique_violation() {
                    Self::Conflict(message)
                } else {
                    tracing::error!(?db_error, "database error");
                    Self::Internal
                }
            }
            other => {
                tracing::error!(error = ?other, "sqlx error");
                Self::Internal
            }
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(error: anyhow::Error) -> Self {
        tracing::error!(error = ?error, "unexpected error");
        Self::Internal
    }
}
