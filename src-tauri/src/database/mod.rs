pub mod documentation;
pub mod events;
pub mod project_candidates;
pub mod rollups;
pub mod schema;
pub mod tasks;

use std::path::Path;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    ConnectOptions, SqlitePool,
};

pub async fn init_database(path: &Path) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .disable_statement_logging();

    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    sqlx::query("PRAGMA foreign_keys = ON").execute(&db).await?;
    schema::run_migrations(&db).await?;
    Ok(db)
}
