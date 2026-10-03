pub mod documentation;
pub mod events;
pub mod graph;
pub mod project_candidates;
pub mod retrieval;
pub mod rollups;
pub mod schema;
pub mod tasks;

use std::{path::Path, time::Duration};

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    ConnectOptions, SqlitePool,
};

pub async fn init_database(path: &Path) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        // WAL lets the capture readers proceed while the roll-up writer is
        // committing.  Keep the timeout on every pooled connection rather
        // than relying on a one-off PRAGMA sent to whichever connection the
        // pool happened to check out.
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true)
        .disable_statement_logging();

    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    schema::run_migrations(&db).await?;
    Ok(db)
}
