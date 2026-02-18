pub mod models;
pub mod queries;

use std::str::FromStr;

use anyhow::Context;
use sqlx::any::{AnyConnectOptions, AnyPoolOptions};
use sqlx::{AnyPool, ConnectOptions, Executor};

use crate::config::Config;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./src/db/migrations");

pub async fn connect_and_migrate(config: &Config) -> anyhow::Result<AnyPool> {
    sqlx::any::install_default_drivers();

    let db_url = normalized_db_url(&config.db_url);

    let connect_options = AnyConnectOptions::from_str(&db_url)
        .with_context(|| format!("invalid LATTICE_DB_URL: {}", config.db_url))?
        .disable_statement_logging();

    let pool = AnyPoolOptions::new()
        .max_connections(8)
        .connect_with(connect_options)
        .await
        .context("failed to establish sqlx AnyPool")?;

    if db_url.starts_with("sqlite://") {
        pool.execute("PRAGMA foreign_keys = ON;")
            .await
            .context("failed to enable sqlite foreign keys")?;
        pool.execute("PRAGMA journal_mode = WAL;")
            .await
            .context("failed to set sqlite WAL mode")?;
    }

    MIGRATOR
        .run(&pool)
        .await
        .context("failed to run migrations")?;

    Ok(pool)
}

fn normalized_db_url(db_url: &str) -> String {
    if !db_url.starts_with("sqlite://") {
        return db_url.to_string();
    }

    if db_url.contains('?') {
        return db_url.to_string();
    }

    format!("{db_url}?mode=rwc")
}
