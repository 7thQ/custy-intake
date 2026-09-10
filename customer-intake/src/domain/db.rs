use std::path::PathBuf;

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

/// Where the shop's SQLite database lives — a plain visible file, same
/// pattern as `issuers.json` and every other hardcoded path in this
/// app.
pub fn db_path() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/shop.sqlite"))
}

/// Opens (creating if needed) the shop database and runs any pending
/// migrations. Foreign keys are off by default in SQLite unless asked
/// for per-connection, and this app relies on `ON DELETE CASCADE` when
/// archiving a completed sign-in, so it's turned on here rather than
/// left to each query to remember.
pub async fn connect() -> Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(db_path())
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .context("failed to open shop database")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("failed to run database migrations")?;

    Ok(pool)
}

/// An in-memory database for tests: one connection kept alive by
/// capping the pool at size 1, since a fresh connection to
/// `sqlite::memory:` would otherwise be a brand new empty database
/// each time. Not `#[cfg(test)]`-gated because the integration tests
/// under `tests/` link against this crate as an ordinary external
/// dependency and need to call it too.
pub async fn connect_in_memory() -> SqlitePool {
    let options = SqliteConnectOptions::new()
        .filename(":memory:")
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("failed to open in-memory database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations against in-memory database");

    pool
}
