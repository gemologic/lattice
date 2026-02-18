use axum::extract::State;
use axum::http::header::AUTHORIZATION;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub async fn require_auth(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> AppResult<Response> {
    let Some(configured_token) = state.config.token.as_deref() else {
        return Ok(next.run(request).await);
    };

    if configured_token.trim().is_empty() {
        return Ok(next.run(request).await);
    }

    let provided = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_bearer_token);

    match provided {
        Some(value) if value == configured_token => Ok(next.run(request).await),
        _ => Err(AppError::Unauthorized),
    }
}

fn parse_bearer_token(value: &str) -> Option<&str> {
    let mut parts = value.splitn(2, ' ');
    let scheme = parts.next()?;
    let token = parts.next()?.trim();

    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }

    if token.is_empty() {
        return None;
    }

    Some(token)
}
