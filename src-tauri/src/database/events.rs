use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Sqlite, SqlitePool};
use std::collections::HashMap;
use uuid::Uuid;

use crate::capture::activity::{predict, ActivityPrediction};
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
    #[sqlx(skip)]
    #[serde(default)]
    pub activity: Option<ActivityPrediction>,
}

// Stay below SQLite's conservative 999-bind limit even on older installations.
const ACTIVITY_READ_BATCH_SIZE: usize = 200;

#[derive(FromRow)]
struct ActivityAnnotationRow {
    event_id: String,
    category: String,
    confidence: f32,
    uncertain: bool,
    model_version: String,
}

/// Hydrate only the requested rows; reads never classify or sweep history.
async fn attach_activity_annotations(
    db: &SqlitePool,
    events: &mut [Event],
) -> Result<(), sqlx::Error> {
    for batch in events.chunks_mut(ACTIVITY_READ_BATCH_SIZE) {
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT event_id, category, confidence, uncertain, model_version \
             FROM activity_annotations WHERE event_id IN (",
        );
        let mut ids = query.separated(", ");
        for event in batch.iter() {
            ids.push_bind(&event.id);
        }
        ids.push_unseparated(")");
        let mut annotations: HashMap<_, _> = query
            .build_query_as::<ActivityAnnotationRow>()
            .fetch_all(db)
            .await?
            .into_iter()
            .map(|row| {
                (
                    row.event_id,
                    ActivityPrediction {
                        category: row.category,
                        confidence: row.confidence,
                        uncertain: row.uncertain,
                        model_version: row.model_version,
                    },
                )
            })
            .collect();
        drop(query);
        for event in batch {
            event.activity = annotations.remove(&event.id);
        }
    }
    Ok(())
}

fn predict_event(event: &Event) -> Option<ActivityPrediction> {
    predict(
        event.app_name.as_deref(),
        event.window_title.as_deref(),
        event.url.as_deref(),
        // Never pass unsanitized captured text into the prediction path.
        if event.is_sanitized == 1 {
            event.content.as_deref()
        } else {
            None
        },
    )
}

/// Run only after event writes have committed. Annotation failure must not
/// turn an already-persisted capture into a failed capture or retry it.
async fn annotate_committed_events(db: &SqlitePool, events: &mut [Event]) {
    if events.is_empty() {
        return;
    }
    let predictions: Vec<_> = events.iter().map(predict_event).collect();
    let result: Result<(), sqlx::Error> = async {
        let mut transaction = db.begin().await?;
        for (event, prediction) in events.iter().zip(&predictions) {
            if let Some(prediction) = prediction {
                sqlx::query(
                    "INSERT INTO activity_annotations \
                     (event_id, category, confidence, uncertain, model_version) \
                     VALUES (?1, ?2, ?3, ?4, ?5) \
                     ON CONFLICT(event_id) DO UPDATE SET category = excluded.category, \
                     confidence = excluded.confidence, uncertain = excluded.uncertain, \
                     model_version = excluded.model_version",
                )
                .bind(&event.id)
                .bind(&prediction.category)
                .bind(prediction.confidence)
                .bind(prediction.uncertain)
                .bind(&prediction.model_version)
                .execute(&mut *transaction)
                .await?;
            } else {
                sqlx::query("DELETE FROM activity_annotations WHERE event_id = ?1")
                    .bind(&event.id)
                    .execute(&mut *transaction)
                    .await?;
            }
        }
        transaction.commit().await
    }
    .await;
    match result {
        Ok(()) => {
            for (event, prediction) in events.iter_mut().zip(predictions) {
                event.activity = prediction;
            }
        }
        Err(error) => log::warn!("Activity annotation persistence failed: {error}"),
    }
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

    let mut event = get_event_row_by_id(db, &id).await?;
    annotate_committed_events(db, std::slice::from_mut(&mut event)).await;
    Ok(event)
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
    annotate_committed_events(db, &mut events).await;

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

    let mut event = get_event_row_by_id(db, id).await?;
    annotate_committed_events(db, std::slice::from_mut(&mut event)).await;
    Ok(event)
}

pub async fn get_events_for_task(
    db: &SqlitePool,
    task_id: String,
) -> Result<Vec<Event>, sqlx::Error> {
    let mut events = sqlx::query_as::<_, Event>(
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
    .await?;
    attach_activity_annotations(db, &mut events).await?;
    Ok(events)
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
    let mut events = sqlx::query_as::<_, Event>(
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
    .await?;
    attach_activity_annotations(db, &mut events).await?;
    Ok(events)
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

pub async fn get_recent_events(
    db: &SqlitePool,
    task_id: String,
    limit: i64,
) -> Result<Vec<Event>, sqlx::Error> {
    let mut events = sqlx::query_as::<_, Event>(
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
    .await?;
    attach_activity_annotations(db, &mut events).await?;
    Ok(events)
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
    use super::*;
    use crate::capture::types::ContentType;
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
            (
                "e1",
                "Editor",
                Some("screen text"),
                Some("https://example.test"),
                "uia",
            ),
            ("e2", "Editor", Some("   "), Some("  "), "title_only"),
            (
                "e3",
                "Browser",
                None,
                Some("https://example.test/docs"),
                "uia",
            ),
            ("e4", "Browser", None, None, "title_only"),
            ("e5", "Noisy", Some("ignored task"), None, "uia"),
        ] {
            sqlx::query(
                "INSERT INTO events \
                 (id, task_id, event_type, app_name, content, url, capture_method, timestamp) \
                 VALUES (?1, ?2, 'window_switch', ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(id)
            .bind(if id == "e5" {
                "other-task"
            } else {
                "stats-task"
            })
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
        assert_eq!(
            by_app,
            vec![("Browser".to_string(), 2), ("Editor".to_string(), 2)]
        );
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

    async fn captured_event(db: &SqlitePool, sanitized: bool) -> Event {
        insert_event_with_metadata(
            db,
            "stats-task".into(),
            "window_switch".into(),
            Some("Code.exe".into()),
            Some("main.rs - Visual Studio Code".into()),
            Some("fn main() { println!(\"hello\"); }".into()),
            None,
            Some("CodeContent".into()),
            Some("uia".into()),
            sanitized,
            0,
        )
        .await
        .expect("insert capture")
    }

    fn capture() -> CapturedContent {
        CapturedContent {
            task_id: "stats-task".into(),
            app_name: "Code.exe".into(),
            window_title: "main.rs - Visual Studio Code".into(),
            text: Some("fn main() {}".into()),
            url: None,
            content_type: ContentType::CodeContent,
            capture_method: "uia".into(),
            timestamp: Utc::now().to_rfc3339(),
        }
    }

    fn chunks() -> Vec<ContentChunk> {
        (0..2)
            .map(|position| ContentChunk {
                text: format!("fn part_{position}() {{}}"),
                chunk_type: "code_block".into(),
                position,
            })
            .collect()
    }

    #[tokio::test]
    async fn activity_persists_and_survives_raw_content_retention() {
        let db = memory_db().await;
        let inserted = captured_event(&db, true).await;
        assert!(
            inserted.activity.is_some(),
            "coding metadata produces a prediction"
        );
        let recent = get_recent_events(&db, "stats-task".into(), 50)
            .await
            .unwrap();
        assert_eq!(recent[0].activity, inserted.activity);
        let all = get_events_for_task(&db, "stats-task".into()).await.unwrap();
        assert_eq!(all[0].activity, inserted.activity);
        let in_window = get_events_in_window(
            &db,
            "stats-task",
            "2000-01-01T00:00:00+00:00",
            &inserted.timestamp,
        )
        .await
        .unwrap();
        assert_eq!(in_window[0].activity, inserted.activity);

        assert_eq!(
            clear_content_before(&db, &inserted.timestamp)
                .await
                .unwrap(),
            1
        );
        let retained = get_event_by_id(&db, &inserted.id).await.unwrap();
        assert_eq!(retained.content, None);
        assert_eq!(retained.activity, inserted.activity);

        sqlx::query("DELETE FROM events WHERE id = ?1")
            .bind(&inserted.id)
            .execute(&db)
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_annotations")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 0, "deleting an event cascades its annotation");
    }

    #[tokio::test]
    async fn unsanitized_capture_uses_only_metadata_for_prediction() {
        let db = memory_db().await;
        let inserted = captured_event(&db, false).await;
        assert_eq!(
            inserted.activity,
            predict(
                inserted.app_name.as_deref(),
                inserted.window_title.as_deref(),
                inserted.url.as_deref(),
                None,
            )
        );
        // With no metadata at all, raw text cannot provide any signal.
        let raw = insert_event_with_metadata(
            &db,
            "stats-task".into(),
            "clipboard".into(),
            None,
            None,
            Some("fn main() {} cargo test compiler error".into()),
            None,
            None,
            None,
            false,
            0,
        )
        .await
        .unwrap();
        assert_eq!(raw.activity, predict(None, None, None, None));
    }

    #[tokio::test]
    async fn enrichment_replaces_existing_annotation_without_duplicate_rows() {
        let db = memory_db().await;
        let inserted = captured_event(&db, true).await;
        // Seed an intentionally stale label to verify replacement, regardless
        // of the model's thresholds or category scoring details.
        sqlx::query("UPDATE activity_annotations SET category = 'other', model_version = 'old-model' WHERE event_id = ?1")
            .bind(&inserted.id).execute(&db).await.unwrap();
        let updated = update_event_content(
            &db,
            &inserted.id,
            Some("fn enriched() { assert_eq!(1, 1); }".into()),
            Some("https://github.com/taskflow/taskflow/issues/1".into()),
            None,
            Some("ocr".into()),
            true,
        )
        .await
        .unwrap();
        let expected = predict_event(&updated);
        assert_eq!(updated.activity, expected);
        assert_ne!(
            updated.activity.as_ref().unwrap().model_version,
            "old-model"
        );
        assert_eq!(
            get_event_by_id(&db, &inserted.id).await.unwrap().activity,
            expected
        );
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM activity_annotations WHERE event_id = ?1")
                .bind(&inserted.id)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn enrichment_abstention_removes_stale_content_only_prediction() {
        let db = memory_db().await;
        let inserted = insert_event_with_metadata(
            &db,
            "stats-task".into(),
            "clipboard".into(),
            None,
            None,
            Some("fn main() {} cargo test compiler error".into()),
            None,
            None,
            Some("clipboard".into()),
            true,
            0,
        )
        .await
        .unwrap();
        assert!(inserted.activity.is_some());
        let updated = update_event_content(
            &db,
            &inserted.id,
            Some("fn main() {} cargo test compiler error".into()),
            None,
            None,
            None,
            false,
        )
        .await
        .unwrap();
        assert_eq!(
            updated.activity, None,
            "unsanitized text cannot retain a content-based classification"
        );
        assert_eq!(
            get_event_by_id(&db, &inserted.id).await.unwrap().activity,
            None
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_annotations")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn legacy_events_are_not_classified_on_read_and_deserialize_without_activity() {
        let db = memory_db().await;
        db.execute("INSERT INTO events (id, task_id, event_type, app_name, window_title, timestamp) \
            VALUES ('legacy', 'stats-task', 'window_switch', 'Code.exe', 'main.rs', '2026-07-01T00:00:00+00:00')").await.unwrap();
        let events = get_recent_events(&db, "stats-task".into(), 50)
            .await
            .unwrap();
        assert_eq!(events[0].activity, None);
        assert_eq!(
            get_events_for_task(&db, "stats-task".into()).await.unwrap()[0].activity,
            None
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_annotations")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 0, "feed reads do not sweep private history");
        let mut legacy_payload = serde_json::to_value(&events[0]).unwrap();
        legacy_payload.as_object_mut().unwrap().remove("activity");
        assert_eq!(
            serde_json::from_value::<Event>(legacy_payload)
                .unwrap()
                .activity,
            None
        );
    }

    #[tokio::test]
    async fn annotation_readback_spans_multiple_bounded_batches() {
        let db = memory_db().await;
        let mut transaction = db.begin().await.unwrap();
        for index in 0..(ACTIVITY_READ_BATCH_SIZE * 2 + 1) {
            let id = format!("batch-{index}");
            sqlx::query("INSERT INTO events (id, task_id, event_type, timestamp) VALUES (?1, 'stats-task', 'window_switch', '2026-07-01T00:00:00+00:00')")
                .bind(&id).execute(&mut *transaction).await.unwrap();
            sqlx::query("INSERT INTO activity_annotations (event_id, category, confidence, uncertain, model_version) VALUES (?1, 'research_learning', 0.75, 1, 'batch-model')")
                .bind(&id).execute(&mut *transaction).await.unwrap();
        }
        transaction.commit().await.unwrap();
        let events = get_events_for_task(&db, "stats-task".into()).await.unwrap();
        assert_eq!(events.len(), ACTIVITY_READ_BATCH_SIZE * 2 + 1);
        assert!(events
            .iter()
            .all(|event| event.activity.as_ref().is_some_and(|activity| {
                activity.category == "research_learning"
                    && activity.confidence == 0.75
                    && activity.uncertain
                    && activity.model_version == "batch-model"
            })));
    }

    #[tokio::test]
    async fn chunk_annotations_follow_successful_capture_commit_only() {
        let db = memory_db().await;
        db.execute(
            "CREATE TRIGGER fail_second_chunk BEFORE INSERT ON events WHEN NEW.chunk_index = 1 \
            BEGIN SELECT RAISE(ABORT, 'forced chunk failure'); END;",
        )
        .await
        .unwrap();
        assert!(insert_captured_chunks(&db, capture(), chunks())
            .await
            .is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 0, "no partial capture survives rollback");
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_annotations")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 0, "failed capture never creates annotations");
        db.execute("DROP TRIGGER fail_second_chunk").await.unwrap();
        let events = insert_captured_chunks(&db, capture(), chunks())
            .await
            .unwrap();
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|event| event.activity.is_some()));
        let readback = get_events_for_task(&db, "stats-task".into()).await.unwrap();
        assert!(readback.iter().all(|event| event.activity.is_some()));
        let title_only = CapturedContent {
            text: None,
            content_type: ContentType::WindowTitleOnly,
            capture_method: "title_only".into(),
            ..capture()
        };
        let events = insert_captured_chunks(&db, title_only, vec![])
            .await
            .unwrap();
        assert!(
            events[0].activity.is_some(),
            "metadata-only capture is annotated too"
        );
    }

    #[tokio::test]
    async fn annotation_failure_keeps_capture_successful_and_annotation_batch_atomic() {
        let db = memory_db().await;
        db.execute(
            "CREATE TRIGGER fail_second_annotation BEFORE INSERT ON activity_annotations \
            WHEN (SELECT COUNT(*) FROM activity_annotations) >= 1 \
            BEGIN SELECT RAISE(ABORT, 'forced annotation failure'); END;",
        )
        .await
        .unwrap();
        let events = insert_captured_chunks(&db, capture(), chunks())
            .await
            .expect("capture stays successful");
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|event| event.activity.is_none()));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 2);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_annotations")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(
            count, 0,
            "annotation batch rolls back instead of exposing partial predictions"
        );
    }
}

async fn get_event_row_by_id(db: &SqlitePool, id: &str) -> Result<Event, sqlx::Error> {
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

#[cfg(test)]
async fn get_event_by_id(db: &SqlitePool, id: &str) -> Result<Event, sqlx::Error> {
    let mut event = get_event_row_by_id(db, id).await?;
    attach_activity_annotations(db, std::slice::from_mut(&mut event)).await?;
    Ok(event)
}
