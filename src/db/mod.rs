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

    let mut normalized = normalize_sqlite_db_url_path(db_url);
    if !normalized.contains('?') {
        normalized.push_str("?mode=rwc");
    }

    normalized
}

fn normalize_sqlite_db_url_path(db_url: &str) -> String {
    let Some(path_and_query) = db_url.strip_prefix("sqlite://") else {
        return db_url.to_string();
    };

    let (path, query) = match path_and_query.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (path_and_query, None),
    };

    let mut normalized_path = path.replace('\\', "/");
    if normalized_path.starts_with("//?/") {
        normalized_path = normalized_path.replacen("//?/", "/", 1);
    }

    let has_windows_drive_prefix = normalized_path.len() >= 2
        && normalized_path.as_bytes()[0].is_ascii_alphabetic()
        && normalized_path.as_bytes()[1] == b':';
    if has_windows_drive_prefix {
        normalized_path.insert(0, '/');
    }

    let mut normalized = format!("sqlite://{normalized_path}");
    if let Some(query) = query {
        normalized.push('?');
        normalized.push_str(query);
    }

    normalized
}

#[cfg(test)]
mod tests {
    use crate::db::normalized_db_url;

    #[test]
    fn normalized_db_url_preserves_non_sqlite_urls() {
        assert_eq!(
            normalized_db_url("postgres://localhost/lattice"),
            "postgres://localhost/lattice"
        );
    }

    #[test]
    fn normalized_db_url_adds_mode_when_missing() {
        assert_eq!(
            normalized_db_url("sqlite://./lattice.db"),
            "sqlite://./lattice.db?mode=rwc"
        );
    }

    #[test]
    fn normalized_db_url_normalizes_windows_paths() {
        assert_eq!(
            normalized_db_url(r"sqlite://C:\Temp\lattice.db"),
            "sqlite:///C:/Temp/lattice.db?mode=rwc"
        );
        assert_eq!(
            normalized_db_url(r"sqlite://C:\Temp\lattice.db?mode=rwc"),
            "sqlite:///C:/Temp/lattice.db?mode=rwc"
        );
    }
}
