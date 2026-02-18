use std::sync::Arc;

use sqlx::AnyPool;

use crate::config::Config;
use crate::rate_limit::RateLimiter;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: AnyPool,
    pub rate_limiter: RateLimiter,
}

impl AppState {
    pub fn new(config: Config, db: AnyPool) -> Self {
        let rate_limiter = RateLimiter::new(config.rate_limits.clone());
        Self {
            config: Arc::new(config),
            db,
            rate_limiter,
        }
    }
}
