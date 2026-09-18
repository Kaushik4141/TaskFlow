use sqlx::{Executor, SqlitePool};

pub async fn run_migrations(db: &SqlitePool) -> Result<(), sqlx::Error> {
    db.execute(
        r#"
        CREATE TABLE IF NOT EXISTS tasks (
            id          TEXT PRIMARY KEY,
            title       TEXT NOT NULL,
            description TEXT,
            source      TEXT NOT NULL DEFAULT 'manual',
            source_id   TEXT,
            source_url  TEXT,
            status      TEXT NOT NULL DEFAULT 'active',
            started_at  TEXT,
            ended_at    TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )
    .await?;

    db.execute(
        r#"
        CREATE TABLE IF NOT EXISTS events (
            id           TEXT PRIMARY KEY,
            task_id      TEXT NOT NULL REFERENCES tasks(id),
            event_type   TEXT NOT NULL,
            app_name     TEXT,
            window_title TEXT,
            content      TEXT,
            url          TEXT,
            relevance    REAL DEFAULT 0.0,
            timestamp    TEXT NOT NULL,
            created_at   TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )
    .await?;

    db.execute(
        r#"
        CREATE TABLE IF NOT EXISTS documentation (
            id           TEXT PRIMARY KEY,
            task_id      TEXT NOT NULL REFERENCES tasks(id),
            content      TEXT NOT NULL,
            summary      TEXT,
            version      INTEGER NOT NULL DEFAULT 1,
            generated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )
    .await?;

    db.execute(
        r#"
        CREATE TABLE IF NOT EXISTS notes (
            id         TEXT PRIMARY KEY,
            task_id    TEXT NOT NULL REFERENCES tasks(id),
            content    TEXT NOT NULL,
            app_name   TEXT,
            timestamp  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )
    .await?;

    db.execute(
        r#"
        CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )
    .await?;

    add_column_if_missing(db, "tasks", "source_title", "TEXT").await?;
    add_column_if_missing(db, "tasks", "source_body", "TEXT").await?;
    add_column_if_missing(db, "tasks", "source_labels", "TEXT").await?;
    add_column_if_missing(db, "tasks", "source_assignee", "TEXT").await?;
    add_column_if_missing(db, "tasks", "source_priority", "TEXT").await?;
    add_column_if_missing(db, "tasks", "source_project", "TEXT").await?;
    add_column_if_missing(db, "tasks", "source_branch", "TEXT").await?;
    add_column_if_missing(db, "events", "content_type", "TEXT").await?;
    add_column_if_missing(db, "events", "url", "TEXT").await?;
    add_column_if_missing(db, "events", "capture_method", "TEXT").await?;
    add_column_if_missing(db, "events", "is_sanitized", "INTEGER DEFAULT 0").await?;
    add_column_if_missing(db, "events", "chunk_index", "INTEGER DEFAULT 0").await?;

    add_column_if_missing(db, "documentation", "summary_embedding", "BLOB").await?;
    add_column_if_missing(db, "documentation", "ai_mode", "TEXT").await?;

    db.execute(
        r#"
        CREATE TABLE IF NOT EXISTS rollups (
            id              TEXT PRIMARY KEY,
            task_id         TEXT NOT NULL REFERENCES tasks(id),
            window_start    TEXT NOT NULL,
            window_end      TEXT NOT NULL,
            title           TEXT NOT NULL,
            summary_md      TEXT NOT NULL,
            key_points      TEXT,
            apps            TEXT,
            resources       TEXT,
            workstream_slug TEXT,
            ai_mode         TEXT,
            event_count     INTEGER NOT NULL DEFAULT 0,
            trigger_kind    TEXT,
            created_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )
    .await?;

    db.execute(
        r#"
        CREATE TABLE IF NOT EXISTS integrations (
            id          TEXT PRIMARY KEY,
            provider    TEXT NOT NULL,
            name        TEXT NOT NULL,
            token       TEXT NOT NULL,
            workspace   TEXT,
            extra       TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )
    .await?;

    // Staging area for auto-detected projects awaiting the user's yes/no/rename.
    //
    // `match_key` is the normalized identity (`project_key`) and the primary key,
    // NOT the display name: renaming a candidate must not fork it into a second
    // row, and re-detecting it must find the existing one.
    //
    // `days_seen` is a JSON array of `YYYY-MM-DD` strings rather than an integer
    // counter, because the promotion rule is "seen on N distinct days" and many
    // roll-ups fire per day — an incrementing counter would clear a 2-day bar
    // within the first hour. It also has to survive raw-capture pruning (72h
    // default), so the evidence cannot be recomputed from `events` later.
    db.execute(
        r#"
        CREATE TABLE IF NOT EXISTS project_candidates (
            match_key      TEXT PRIMARY KEY,
            display_name   TEXT NOT NULL,
            signal_kinds   TEXT NOT NULL DEFAULT '[]',
            evidence       TEXT NOT NULL DEFAULT '[]',
            days_seen      TEXT NOT NULL DEFAULT '[]',
            event_count    INTEGER NOT NULL DEFAULT 0,
            first_seen     TEXT NOT NULL,
            last_seen      TEXT NOT NULL,
            status         TEXT NOT NULL DEFAULT 'pending',
            asked_at       TEXT
        );
        "#,
    )
    .await?;

    // The review queue reads by status on every app launch.
    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_project_candidates_status ON project_candidates(status)",
    )
    .await?;

    db.execute(
        r#"
        CREATE TABLE IF NOT EXISTS tickets (
            id             TEXT PRIMARY KEY,
            integration_id TEXT REFERENCES integrations(id),
            provider       TEXT NOT NULL,
            ticket_id      TEXT NOT NULL,
            title          TEXT NOT NULL,
            description    TEXT,
            labels         TEXT,
            priority       TEXT,
            assignee       TEXT,
            project        TEXT,
            url            TEXT,
            branch         TEXT,
            raw            TEXT,
            fetched_at     TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )
    .await?;

    // Retention pruning and the daily index both scan by timestamp across all
    // tasks, which the task_id/timestamp access paths don't cover.
    db.execute("CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp)")
        .await?;

    drop_snapshot_artifacts(db).await?;

    Ok(())
}

/// Remove the abandoned `snapshots` subsystem.
///
/// It shipped as schema only — no Rust or TS ever read or wrote it — while its
/// FTS5 index and three sync triggers kept it the single largest object in the
/// database. Dropping it here rather than in a one-off script means existing
/// installs converge on the next launch. Every statement is `IF EXISTS`, so
/// this is a no-op on fresh databases and idempotent on every boot after the
/// first.
///
/// The three orphaned `events` columns from the same commit (`snapshot_id`,
/// `correlation_id`, `metadata_json`) are deliberately left in place: SQLite's
/// `DROP COLUMN` is fragile, and empty columns cost effectively nothing.
async fn drop_snapshot_artifacts(db: &SqlitePool) -> Result<(), sqlx::Error> {
    // Triggers first — they reference the FTS table, and dropping their target
    // out from under them would leave statements that fail at write time.
    for trigger in ["snapshots_ai", "snapshots_ad", "snapshots_au"] {
        db.execute(format!("DROP TRIGGER IF EXISTS {trigger}").as_str())
            .await?;
    }
    // Dropping the FTS5 virtual table takes its shadow tables (_data, _idx,
    // _docsize, _config) with it.
    db.execute("DROP TABLE IF EXISTS snapshots_fts").await?;
    db.execute("DROP TABLE IF EXISTS snapshots").await?;
    db.execute("DROP INDEX IF EXISTS idx_events_snapshot_id")
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{drop_snapshot_artifacts, run_migrations};
    use sqlx::{Executor, SqlitePool};

    /// Recreates the abandoned subsystem exactly as commit 1e8adb4 left it in
    /// existing installs, then asserts the teardown clears every object —
    /// table, FTS5 index, shadow tables and all three triggers. Dropping the
    /// FTS5 virtual table requires the fts5 module to actually be compiled in,
    /// so this doubles as proof that migrations won't fail on real databases.
    #[tokio::test]
    async fn drops_every_legacy_snapshot_object() {
        let db = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");
        run_migrations(&db).await.expect("migrations");

        // Fresh migrations no longer add this column, so the fixture restores
        // it the way the abandoned commit did — via a bare ALTER, with no FK.
        db.execute("ALTER TABLE events ADD COLUMN snapshot_id TEXT")
            .await
            .expect("legacy events column");

        db.execute(
            r#"
            CREATE TABLE snapshots (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                captured_at TEXT NOT NULL,
                app_name TEXT,
                window_title TEXT,
                browser_url TEXT,
                full_text TEXT
            );
            CREATE VIRTUAL TABLE snapshots_fts USING fts5(
                full_text, app_name, window_title, browser_url,
                content='snapshots', content_rowid='rowid'
            );
            CREATE TRIGGER snapshots_ai AFTER INSERT ON snapshots BEGIN
                INSERT INTO snapshots_fts(rowid, full_text, app_name, window_title, browser_url)
                VALUES (new.rowid, new.full_text, new.app_name, new.window_title, new.browser_url);
            END;
            CREATE TRIGGER snapshots_ad AFTER DELETE ON snapshots BEGIN
                INSERT INTO snapshots_fts(snapshots_fts, rowid, full_text, app_name, window_title, browser_url)
                VALUES ('delete', old.rowid, old.full_text, old.app_name, old.window_title, old.browser_url);
            END;
            CREATE TRIGGER snapshots_au AFTER UPDATE ON snapshots BEGIN
                INSERT INTO snapshots_fts(snapshots_fts, rowid, full_text, app_name, window_title, browser_url)
                VALUES ('delete', old.rowid, old.full_text, old.app_name, old.window_title, old.browser_url);
                INSERT INTO snapshots_fts(rowid, full_text, app_name, window_title, browser_url)
                VALUES (new.rowid, new.full_text, new.app_name, new.window_title, new.browser_url);
            END;
            CREATE INDEX idx_events_snapshot_id ON events(snapshot_id);
            "#,
        )
        .await
        .expect("recreate legacy snapshot subsystem");

        db.execute(
            "INSERT INTO snapshots (id, task_id, captured_at, full_text) \
             VALUES ('s1', 't1', '2026-07-01T00:00:00+00:00', 'captured screen text')",
        )
        .await
        .expect("seed snapshot through the fts triggers");

        drop_snapshot_artifacts(&db).await.expect("teardown");

        let remaining: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE name LIKE '%snapshot%'",
        )
        .fetch_one(&db)
        .await
        .expect("count leftovers");
        assert_eq!(remaining, 0, "no snapshot table, index, trigger or shadow table survives");

        // Idempotent: migrations re-run on every launch.
        drop_snapshot_artifacts(&db)
            .await
            .expect("second teardown is a no-op");

        // Events are untouched and still writable.
        db.execute(
            "INSERT INTO tasks (id, title, source, source_id, status, created_at) \
             VALUES ('t1', 'T', 'memory', 'm-1', 'active', '2026-07-01T00:00:00+00:00')",
        )
        .await
        .expect("seed task");
        db.execute(
            "INSERT INTO events (id, task_id, event_type, timestamp) \
             VALUES ('e1', 't1', 'window_switch', '2026-07-01T00:00:00+00:00')",
        )
        .await
        .expect("events still writable after teardown");
    }

    /// A fresh database has none of these objects; the teardown must not error.
    #[tokio::test]
    async fn teardown_is_a_noop_on_fresh_databases() {
        let db = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");
        run_migrations(&db).await.expect("migrations include teardown");
        drop_snapshot_artifacts(&db).await.expect("no-op");
    }
}

async fn add_column_if_missing(
    db: &SqlitePool,
    table: &str,
    column: &str,
    column_type: &str,
) -> Result<(), sqlx::Error> {
    let pragma = format!("PRAGMA table_info({table})");
    let columns: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as(&pragma).fetch_all(db).await?;

    if columns.iter().any(|(_, name, _, _, _, _)| name == column) {
        return Ok(());
    }

    let statement = format!("ALTER TABLE {table} ADD COLUMN {column} {column_type}");
    db.execute(statement.as_str()).await?;
    Ok(())
}
