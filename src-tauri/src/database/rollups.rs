use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

/// A persisted summary of one time window of captured activity.
/// Roll-ups are the atomic unit of the workstream timeline: the scheduler
/// produces them every ~10 minutes (or on idle/stop flushes), and the vault
/// writer routes each one to a workstream node.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Rollup {
    pub id: String,
    pub task_id: String,
    pub window_start: String,
    pub window_end: String,
    pub title: String,
    pub summary_md: String,
    /// JSON array of key-point strings.
    pub key_points: Option<String>,
    /// JSON array of app names touched in the window.
    pub apps: Option<String>,
    /// JSON array of resource URLs touched in the window.
    pub resources: Option<String>,
    pub workstream_slug: Option<String>,
    pub ai_mode: Option<String>,
    pub event_count: i64,
    /// What caused this roll-up: "interval" | "idle" | "context_switch" | "stop" | "manual".
    pub trigger_kind: Option<String>,
    pub created_at: String,
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_rollup(
    db: &SqlitePool,
    task_id: &str,
    window_start: &str,
    window_end: &str,
    title: &str,
    summary_md: &str,
    key_points: &[String],
    apps: &[String],
    resources: &[String],
    workstream_slug: Option<&str>,
    ai_mode: Option<&str>,
    event_count: i64,
    trigger_kind: &str,
) -> Result<Rollup, sqlx::Error> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let key_points_json = serde_json::to_string(key_points).unwrap_or_else(|_| "[]".to_string());
    let apps_json = serde_json::to_string(apps).unwrap_or_else(|_| "[]".to_string());
    let resources_json = serde_json::to_string(resources).unwrap_or_else(|_| "[]".to_string());

    sqlx::query(
        r#"
        INSERT INTO rollups
            (id, task_id, window_start, window_end, title, summary_md,
             key_points, apps, resources, workstream_slug, ai_mode,
             event_count, trigger_kind, created_at)
        VALUES
            (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        "#,
    )
    .bind(&id)
    .bind(task_id)
    .bind(window_start)
    .bind(window_end)
    .bind(title)
    .bind(summary_md)
    .bind(&key_points_json)
    .bind(&apps_json)
    .bind(&resources_json)
    .bind(workstream_slug)
    .bind(ai_mode)
    .bind(event_count)
    .bind(trigger_kind)
    .bind(&now)
    .execute(db)
    .await?;

    get_rollup_by_id(db, &id).await
}

pub async fn get_rollup_by_id(db: &SqlitePool, id: &str) -> Result<Rollup, sqlx::Error> {
    sqlx::query_as::<_, Rollup>(
        r#"
        SELECT id, task_id, window_start, window_end, title, summary_md,
               key_points, apps, resources, workstream_slug, ai_mode,
               event_count, trigger_kind, created_at
        FROM rollups
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(db)
    .await
}

/// All roll-ups for a task, oldest first (timeline order).
pub async fn get_rollups_for_task(db: &SqlitePool, task_id: &str) -> Result<Vec<Rollup>, sqlx::Error> {
    sqlx::query_as::<_, Rollup>(
        r#"
        SELECT id, task_id, window_start, window_end, title, summary_md,
               key_points, apps, resources, workstream_slug, ai_mode,
               event_count, trigger_kind, created_at
        FROM rollups
        WHERE task_id = ?1
        ORDER BY window_start ASC
        "#,
    )
    .bind(task_id)
    .fetch_all(db)
    .await
}

/// Roll-ups whose window starts within [day_start, day_end), oldest first.
/// Window bounds are stored as UTC RFC-3339 strings (same as events), so the
/// caller converts a local day's midnight boundaries to UTC and lexicographic
/// comparison stays valid.
pub async fn get_rollups_in_range(
    db: &SqlitePool,
    day_start_utc: &str,
    day_end_utc: &str,
) -> Result<Vec<Rollup>, sqlx::Error> {
    sqlx::query_as::<_, Rollup>(
        r#"
        SELECT id, task_id, window_start, window_end, title, summary_md,
               key_points, apps, resources, workstream_slug, ai_mode,
               event_count, trigger_kind, created_at
        FROM rollups
        WHERE window_start >= ?1 AND window_start < ?2
        ORDER BY window_start ASC
        "#,
    )
    .bind(day_start_utc)
    .bind(day_end_utc)
    .fetch_all(db)
    .await
}

/// The nearest roll-up window starts strictly before `day_start_utc` and at/after
/// `day_end_utc` — i.e. the previous/next days that actually HAVE roll-ups. The
/// derived daily index uses these for prev/next navigation so it never links to
/// a daily note that doesn't exist (a day with no roll-ups gets no note).
pub async fn get_adjacent_rollup_window_starts(
    db: &SqlitePool,
    day_start_utc: &str,
    day_end_utc: &str,
) -> Result<(Option<String>, Option<String>), sqlx::Error> {
    // Aggregate queries always return exactly one row; MAX/MIN yield NULL when
    // no roll-up qualifies, so decode as Option<String> with fetch_one.
    let prev = sqlx::query_scalar::<_, Option<String>>(
        "SELECT MAX(window_start) FROM rollups WHERE window_start < ?1",
    )
    .bind(day_start_utc)
    .fetch_one(db)
    .await?;
    let next = sqlx::query_scalar::<_, Option<String>>(
        "SELECT MIN(window_start) FROM rollups WHERE window_start >= ?1",
    )
    .bind(day_end_utc)
    .fetch_one(db)
    .await?;
    Ok((prev, next))
}

/// Reassign every roll-up whose workstream page was deleted as dust to the
/// `to_slug` catch-all (Inbox). Returns the distinct `window_start` timestamps
/// of affected roll-ups so the caller can rebuild those days' daily indexes —
/// the derived index would otherwise keep linking the deleted pages.
pub async fn reassign_workstreams(
    db: &SqlitePool,
    from_slugs: &[String],
    to_slug: &str,
) -> Result<Vec<String>, sqlx::Error> {
    if from_slugs.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = from_slugs.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let select_sql = format!(
        "SELECT DISTINCT window_start FROM rollups WHERE workstream_slug IN ({placeholders})"
    );
    let mut select = sqlx::query_scalar::<_, String>(&select_sql);
    for slug in from_slugs {
        select = select.bind(slug);
    }
    let affected_windows = select.fetch_all(db).await?;

    let update_sql = format!(
        "UPDATE rollups SET workstream_slug = ? WHERE workstream_slug IN ({placeholders})"
    );
    let mut update = sqlx::query(&update_sql).bind(to_slug);
    for slug in from_slugs {
        update = update.bind(slug);
    }
    update.execute(db).await?;

    Ok(affected_windows)
}

/// The most recent roll-up for a task (used to chain context into the next one).
pub async fn get_latest_rollup(db: &SqlitePool, task_id: &str) -> Result<Option<Rollup>, sqlx::Error> {
    sqlx::query_as::<_, Rollup>(
        r#"
        SELECT id, task_id, window_start, window_end, title, summary_md,
               key_points, apps, resources, workstream_slug, ai_mode,
               event_count, trigger_kind, created_at
        FROM rollups
        WHERE task_id = ?1
        ORDER BY window_end DESC
        LIMIT 1
        "#,
    )
    .bind(task_id)
    .fetch_optional(db)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::schema::run_migrations;

    async fn memory_db() -> SqlitePool {
        let db = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");
        run_migrations(&db).await.expect("migrations");
        sqlx::query(
            "INSERT INTO tasks (id, title, source, source_id, status, started_at, created_at) \
             VALUES ('task-1', 'Memory Capture', 'memory', 'daily-capture-2026-07-27', 'active', \
             '2026-07-27T00:00:00+00:00', '2026-07-27T00:00:00+00:00')",
        )
        .execute(&db)
        .await
        .expect("seed task");
        db
    }

    #[tokio::test]
    async fn reassign_workstreams_moves_slugs_and_reports_windows() {
        let db = memory_db().await;
        for (slug, start) in [
            ("Evil", "2026-07-26T10:00:00+00:00"),
            ("Evil", "2026-07-27T11:00:00+00:00"),
            ("TaskFlow", "2026-07-27T12:00:00+00:00"),
        ] {
            insert_rollup(
                &db, "task-1", start, start, "w", "s", &[], &[], &[],
                Some(slug), Some("template"), 3, "interval",
            )
            .await
            .expect("insert rollup");
        }

        let windows = reassign_workstreams(&db, &["Evil".to_string()], "Inbox")
            .await
            .expect("reassign ok");
        assert_eq!(windows.len(), 2, "both Evil windows reported: {windows:?}");

        let remaining = get_rollups_in_range(
            &db,
            "2026-07-25T00:00:00+00:00",
            "2026-07-28T00:00:00+00:00",
        )
        .await
        .expect("range query");
        let slugs: Vec<&str> = remaining
            .iter()
            .map(|r| r.workstream_slug.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(slugs, vec!["Inbox", "Inbox", "TaskFlow"], "reassigned: {slugs:?}");

        // Empty input is a no-op.
        let none = reassign_workstreams(&db, &[], "Inbox").await.expect("no-op ok");
        assert!(none.is_empty());
    }
}
