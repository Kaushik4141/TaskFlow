use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::capture::chunker::ContentChunk;
use crate::capture::types::CapturedContent;

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
    let mut events = Vec::with_capacity(chunks.len().max(1));
    if chunks.is_empty() {
        events.push(
            insert_event_with_metadata(
                db,
                captured.task_id,
                "window_switch".to_string(),
                Some(captured.app_name),
                Some(captured.window_title),
                captured.text,
                captured.url,
                Some(captured.content_type.as_str().to_string()),
                Some(captured.capture_method),
                true,
                0,
            )
            .await?,
        );
        return Ok(events);
    }

    for (index, chunk) in chunks.into_iter().enumerate() {
        events.push(
            insert_event_with_metadata(
                db,
                captured.task_id.clone(),
                "window_switch".to_string(),
                Some(captured.app_name.clone()),
                Some(captured.window_title.clone()),
                Some(chunk.text),
                captured.url.clone(),
                Some(captured.content_type.as_str().to_string()),
                Some(captured.capture_method.clone()),
                true,
                index as i64,
            )
            .await?,
        );
    }
    Ok(events)
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
