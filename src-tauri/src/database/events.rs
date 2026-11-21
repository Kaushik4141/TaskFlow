use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::capture::chunker::ContentChunk;
use crate::capture::types::CapturedContent;

// SQLite's one-argument trim removes only ASCII spaces. Use the same Unicode
// whitespace set as Rust str::trim, preserving the capture-stat API semantics
// for clipboard/OCR content consisting only of tabs, newlines or NBSPs.
const CAPTURE_STATS_WHITESPACE: &str = "\u{0009}\u{000a}\u{000b}\u{000c}\u{000d}\u{0020}\u{0085}\u{00a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}";

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    pub task_id: String,
    pub event_type: String,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub content: Option<String>,
    pub url: Option<String>,
    pub content_type: Option<String>,
    pub capture_method: Option<String>,
    pub is_sanitized: i64,
    pub chunk_index: i64,
    pub relevance: f64,
    pub timestamp: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub task_id: String,
    pub content: String,
    pub app_name: Option<String>,
    pub timestamp: String,
}

/// Counts used by the capture statistics view. This deliberately contains no
/// event content: the stats command must remain cheap even for a large task
/// with retained screen text.
#[derive(Debug, Clone, FromRow)]
pub struct CaptureStatsAggregate {
    pub total_events: i64,
    pub with_content: i64,
    pub with_url: i64,
    pub title_only: i64,
}

pub async fn get_capture_stats_aggregate(
    db: &SqlitePool,
    task_id: &str,
) -> Result<CaptureStatsAggregate, sqlx::Error> {
    sqlx::query_as::<_, CaptureStatsAggregate>(
        r#"
        SELECT
            COUNT(*) AS total_events,
            COALESCE(SUM(CASE WHEN content IS NOT NULL AND trim(content, ?2) <> '' THEN 1 ELSE 0 END), 0)
                AS with_content,
            COALESCE(SUM(CASE WHEN url IS NOT NULL AND trim(url, ?2) <> '' THEN 1 ELSE 0 END), 0)
                AS with_url,
            COALESCE(SUM(CASE WHEN capture_method = 'title_only' THEN 1 ELSE 0 END), 0) AS title_only
        FROM events
        WHERE task_id = ?1
        "#,
    )
    .bind(task_id)
    .bind(CAPTURE_STATS_WHITESPACE)
    .fetch_one(db)
    .await
}

/// Counts events by app without selecting any event payload columns. NULL app
/// names are omitted to preserve the command's historical API behavior.
pub async fn get_capture_stats_by_app(
    db: &SqlitePool,
    task_id: &str,
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    sqlx::query_as::<_, (String, i64)>(
        r#"
        SELECT app_name, COUNT(*)
        FROM events
        WHERE task_id = ?1 AND app_name IS NOT NULL
        GROUP BY app_name
        ORDER BY app_name ASC
        "#,
    )
    .bind(task_id)
    .fetch_all(db)
    .await
}

pub async fn insert_event(
    db: &SqlitePool,
    task_id: String,
    event_type: String,
    app_name: Option<String>,
    window_title: Option<String>,
    content: Option<String>,
    url: Option<String>,
) -> Result<Event, sqlx::Error> {
    insert_event_with_metadata(
        db,
        task_id,
        event_type,
        app_name,
        window_title,
        content,
        url,
        None,
        None,
        false,
        0,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_event_with_metadata(
    db: &SqlitePool,
    task_id: String,
    event_type: String,
    app_name: Option<String>,
    window_title: Option<String>,
    content: Option<String>,
    url: Option<String>,
    content_type: Option<String>,
    capture_method: Option<String>,
    is_sanitized: bool,
    chunk_index: i64,
) -> Result<Event, sqlx::Error> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO events
            (id, task_id, event_type, app_name, window_title, content, url, content_type,
             capture_method, is_sanitized, chunk_index, relevance, timestamp, created_at)
        VALUES
            (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0.0, ?12, ?12)
        "#,
    )
    .bind(&id)
    .bind(task_id)
    .bind(event_type)
    .bind(app_name)
    .bind(window_title)
    .bind(content)
    .bind(url)
    .bind(content_type)
    .bind(capture_method)
    .bind(if is_sanitized { 1 } else { 0 })
    .bind(chunk_index)
    .bind(&now)
    .execute(db)
    .await?;

    get_event_by_id(db, &id).await
}

pub async fn insert_captured_chunks(
    db: &SqlitePool,
    captured: CapturedContent,
    chunks: Vec<ContentChunk>,
) -> Result<Vec<Event>, sqlx::Error> {
    // A capture can produce several chunks. Commit them as one unit so an
    // SQLITE_BUSY/error cannot leave a partial screen capture in the feed.
    // RETURNING avoids a separate pool checkout/readback for every chunk.
    // Rows are only exposed to the caller after a successful commit.
    let mut transaction = db.begin().await?;
    let mut events = Vec::with_capacity(chunks.len().max(1));
    let content_type = captured.content_type.as_str().to_string();
    let (task_id, app_name, window_title, url, capture_method, content) = (
        captured.task_id,
        captured.app_name,
        captured.window_title,
        captured.url,
        captured.capture_method,
        captured.text,
    );

    if chunks.is_empty() {
        let event = insert_captured_chunk_row(
            &mut transaction,
            &task_id,
            &app_name,
            &window_title,
            content,
            url.as_deref(),
            &content_type,
            &capture_method,
            0,
        )
        .await?;
        events.push(event);
    } else {
        for (index, chunk) in chunks.into_iter().enumerate() {
            let event = insert_captured_chunk_row(
                &mut transaction,
                &task_id,
                &app_name,
                &window_title,
                Some(chunk.text),
                url.as_deref(),
                &content_type,
                &capture_method,
                index as i64,
            )
            .await?;
            events.push(event);
        }
    }
    transaction.commit().await?;

    Ok(events)
}

#[allow(clippy::too_many_arguments)]
async fn insert_captured_chunk_row(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    task_id: &str,
    app_name: &str,
    window_title: &str,
    content: Option<String>,
    url: Option<&str>,
    content_type: &str,
    capture_method: &str,
    chunk_index: i64,
) -> Result<Event, sqlx::Error> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query_as::<_, Event>(
        r#"
        INSERT INTO events
            (id, task_id, event_type, app_name, window_title, content, url,
             content_type, capture_method, is_sanitized, chunk_index, relevance,
             timestamp, created_at)
        VALUES (?1, ?2, 'window_switch', ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9, 0.0, ?10, ?10)
        RETURNING id, task_id, event_type, app_name, window_title, content, url,
                  content_type, capture_method, is_sanitized, chunk_index,
                  relevance, timestamp, created_at
        "#,
    )
    .bind(&id)
    .bind(task_id)
    .bind(app_name)
    .bind(window_title)
    .bind(content)
    .bind(url)
    .bind(content_type)
    .bind(capture_method)
    .bind(chunk_index)
    .bind(&now)
    .fetch_one(&mut **transaction)
    .await
}

pub async fn update_event_content(
    db: &SqlitePool,
    id: &str,
    content: Option<String>,
    url: Option<String>,
    content_type: Option<String>,
    capture_method: Option<String>,
    is_sanitized: bool,
) -> Result<Event, sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE events
        SET content = ?2,
            url = COALESCE(?3, url),
            content_type = COALESCE(?4, content_type),
            capture_method = COALESCE(?5, capture_method),
            is_sanitized = ?6
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .bind(content)
    .bind(url)
    .bind(content_type)
    .bind(capture_method)
    .bind(if is_sanitized { 1 } else { 0 })
    .execute(db)
    .await?;

    get_event_by_id(db, id).await
}

pub async fn get_events_for_task(
    db: &SqlitePool,
    task_id: String,
) -> Result<Vec<Event>, sqlx::Error> {
    sqlx::query_as::<_, Event>(
        r#"
        SELECT id, task_id, event_type, app_name, window_title, content, url,
               content_type, capture_method, is_sanitized, chunk_index, relevance, timestamp, created_at
        FROM events
        WHERE task_id = ?1
        ORDER BY timestamp ASC
        "#,
    )
    .bind(task_id)
    .fetch_all(db)
    .await
}

/// Events in the half-open window (start, end] for a task, oldest first.
/// Timestamps are UTC RFC-3339 strings, so lexicographic comparison is valid
/// as long as callers pass bounds in the same format (`Utc::to_rfc3339()`).
pub async fn get_events_in_window(
    db: &SqlitePool,
    task_id: &str,
    start: &str,
    end: &str,
) -> Result<Vec<Event>, sqlx::Error> {
    sqlx::query_as::<_, Event>(
        r#"
        SELECT id, task_id, event_type, app_name, window_title, content, url,
               content_type, capture_method, is_sanitized, chunk_index, relevance, timestamp, created_at
        FROM events
        WHERE task_id = ?1 AND timestamp > ?2 AND timestamp <= ?3
        ORDER BY timestamp ASC
        "#,
    )
    .bind(task_id)
    .bind(start)
    .bind(end)
    .fetch_all(db)
    .await
}

/// Distinct `content_type` values captured in [start, end) (any task). Used by
/// the derived daily index to mint Activity-hub links without loading full
/// event rows.
pub async fn get_content_types_in_range(
    db: &SqlitePool,
    start: &str,
    end: &str,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT DISTINCT content_type FROM events
        WHERE content_type IS NOT NULL AND timestamp >= ?1 AND timestamp < ?2
        ORDER BY content_type ASC
        "#,
    )
    .bind(start)
    .bind(end)
    .fetch_all(db)
    .await
}

/// Discard raw captured `content` for events at or before `cutoff`, keeping the
/// rows themselves. The row is the activity record — the app, title, URL and
/// timing stay queryable and the event feed keeps rendering — while the
/// verbatim screen text, which is the sensitive part, stops being retained.
///
/// `cutoff` MUST be an RFC-3339 string produced by `Utc::to_rfc3339()`. SQLite's
/// own `datetime('now', ...)` returns a space-separated string, and because
/// `'T'` (0x54) sorts above `' '` (0x20), comparing the two formats silently
/// matches nothing. Build the bound in Rust, never in SQL.
///
/// Returns the number of rows cleared.
pub async fn clear_content_before(db: &SqlitePool, cutoff: &str) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE events
        SET content = NULL
        WHERE content IS NOT NULL AND timestamp <= ?1
        "#,
    )
    .bind(cutoff)
    .execute(db)
    .await?;
    Ok(result.rows_affected())
}

pub async fn get_recent_events(    db: &SqlitePool,
    task_id: String,
    limit: i64,
) -> Result<Vec<Event>, sqlx::Error> {
    sqlx::query_as::<_, Event>(
        r#"
        SELECT * FROM (
            SELECT id, task_id, event_type, app_name, window_title, content, url,
                   content_type, capture_method, is_sanitized, chunk_index, relevance, timestamp, created_at
            FROM events
            WHERE task_id = ?1
            ORDER BY timestamp DESC
            LIMIT ?2
        )
        ORDER BY timestamp ASC
        "#,
    )
    .bind(task_id)
    .bind(limit)
    .fetch_all(db)
    .await
}

pub async fn add_note(
    db: &SqlitePool,
    task_id: String,
    content: String,
    app_name: Option<String>,
) -> Result<Note, sqlx::Error> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO notes (id, task_id, content, app_name, timestamp)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(&id)
    .bind(&task_id)
    .bind(&content)
    .bind(&app_name)
    .bind(&now)
    .execute(db)
    .await?;

    insert_event(
        db,
        task_id.clone(),
        "note".to_string(),
        app_name.clone(),
        None,
        Some(content.clone()),
        None,
    )
    .await?;

    Ok(Note {
        id,
        task_id,
        content,
        app_name,
        timestamp: now,
    })
}

#[cfg(test)]
mod tests {
    use super::{get_capture_stats_aggregate, get_capture_stats_by_app};
    use crate::database::schema::run_migrations;
    use sqlx::{Executor, SqlitePool};

    async fn memory_db() -> SqlitePool {
        let db = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");
        run_migrations(&db).await.expect("migrations");
        db.execute(
            "INSERT INTO tasks (id, title, source, status, created_at) \
             VALUES ('stats-task', 'Stats', 'manual', 'active', '2026-07-01T00:00:00+00:00')",
        )
        .await
        .expect("seed task");
        db.execute(
            "INSERT INTO tasks (id, title, source, status, created_at) \
             VALUES ('other-task', 'Other', 'manual', 'active', '2026-07-01T00:00:00+00:00')",
        )
        .await
        .expect("seed other task");
        db
    }

    #[tokio::test]
    async fn capture_stats_aggregate_counts_without_loading_content() {
        let db = memory_db().await;
        for (id, app, content, url, method) in [
            ("e1", "Editor", Some("screen text"), Some("https://example.test"), "uia"),
            ("e2", "Editor", Some("   "), Some("  "), "title_only"),
            ("e3", "Browser", None, Some("https://example.test/docs"), "uia"),
            ("e4", "Browser", None, None, "title_only"),
            ("e5", "Noisy", Some("ignored task"), None, "uia"),
        ] {
            sqlx::query(
                "INSERT INTO events \
                 (id, task_id, event_type, app_name, content, url, capture_method, timestamp) \
                 VALUES (?1, ?2, 'window_switch', ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(id)
            .bind(if id == "e5" { "other-task" } else { "stats-task" })
            .bind(app)
            .bind(content)
            .bind(url)
            .bind(method)
            .bind("2026-07-01T00:00:00+00:00")
            .execute(&db)
            .await
            .expect("seed event");
        }

        let aggregate = get_capture_stats_aggregate(&db, "stats-task")
            .await
            .expect("aggregate");
        assert_eq!(aggregate.total_events, 4);
        assert_eq!(aggregate.with_content, 1);
        assert_eq!(aggregate.with_url, 2);
        assert_eq!(aggregate.title_only, 2);

        let by_app = get_capture_stats_by_app(&db, "stats-task")
            .await
            .expect("app aggregate");
        assert_eq!(by_app, vec![("Browser".to_string(), 2), ("Editor".to_string(), 2)]);
    }

    #[tokio::test]
    async fn capture_stats_empty_task_returns_zeroes() {
        let db = memory_db().await;
        let aggregate = get_capture_stats_aggregate(&db, "stats-task")
            .await
            .expect("aggregate");
        assert_eq!(aggregate.total_events, 0);
        assert_eq!(aggregate.with_content, 0);
        assert_eq!(aggregate.with_url, 0);
        assert_eq!(aggregate.title_only, 0);
    }
}

async fn get_event_by_id(db: &SqlitePool, id: &str) -> Result<Event, sqlx::Error> {
    sqlx::query_as::<_, Event>(
        r#"
        SELECT id, task_id, event_type, app_name, window_title, content, url,
               content_type, capture_method, is_sanitized, chunk_index, relevance, timestamp, created_at
        FROM events
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(db)
    .await
}
