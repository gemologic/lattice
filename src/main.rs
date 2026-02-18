mod api;
mod config;
mod db;
mod error;
mod mcp;
mod rate_limit;
mod state;
mod static_files;
mod webhooks;

use std::net::SocketAddr;

use anyhow::Context;
use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::get;
use axum::Router;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let config = Config::from_env();
    config.log_startup_warnings();
    config
        .ensure_storage_dir()
        .context("failed to create storage directory")?;

    let pool = db::connect_and_migrate(&config)
        .await
        .context("failed to initialize database")?;

    let state = AppState::new(config.clone(), pool);
    webhooks::spawn_dispatcher(state.clone());
    let mcp_service = mcp::service(state.clone());
    let max_request_body_bytes = state.config.rate_limits.max_request_body_bytes;

    let app = Router::new()
        .nest_service("/mcp", mcp_service)
        .nest("/api/v1", api::router())
        .route("/healthz", get(api::healthz))
        .fallback(get(static_files::serve_embedded_asset))
        .layer(DefaultBodyLimit::max(max_request_body_bytes))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            api::auth::require_auth,
        ))
        // Keep this outermost so abusive requests are throttled before auth checks.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit::enforce_limits,
        ))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind to {addr}"))?;

    info!(%addr, "lattice server listening");
    axum::serve(listener, app)
        .await
        .context("axum server error")?;

    Ok(())
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .compact()
        .init();
}
