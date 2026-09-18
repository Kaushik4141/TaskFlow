use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub source: String,
    pub source_id: Option<String>,
    pub source_url: Option<String>,
    pub source_title: Option<String>,
    pub source_body: Option<String>,
    pub source_labels: Option<String>,
    pub source_assignee: Option<String>,
    pub source_priority: Option<String>,
    pub source_project: Option<String>,
    pub source_branch: Option<String>,
    pub status: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct NewTaskSource {
    pub source_id: Option<String>,
    pub source_url: Option<String>,
    pub source_title: Option<String>,
    pub source_body: Option<String>,
    pub source_labels: Option<String>,
    pub source_assignee: Option<String>,
    pub source_priority: Option<String>,
    pub source_project: Option<String>,
    pub source_branch: Option<String>,
}

pub async fn create_task(
    db: &SqlitePool,
    title: String,
    description: Option<String>,
    source: String,
) -> Result<Task, sqlx::Error> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query("UPDATE tasks SET status = 'paused' WHERE status = 'active'")
        .execute(db)
        .await?;

    sqlx::query(
        r#"
        INSERT INTO tasks (id, title, description, source, status, started_at, created_at)
        VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?5)
        "#,
    )
    .bind(&id)
    .bind(title)
    .bind(description)
    .bind(source)
    .bind(&now)
    .execute(db)
    .await?;

    get_task_by_id(db, &id).await
}

pub async fn create_task_with_source_context(
    db: &SqlitePool,
    title: String,
    description: Option<String>,
    source: String,
    context: NewTaskSource,
) -> Result<Task, sqlx::Error> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query("UPDATE tasks SET status = 'paused' WHERE status = 'active'")
        .execute(db)
        .await?;

    sqlx::query(
        r#"
        INSERT INTO tasks (
            id, title, description, source, source_id, source_url, source_title, source_body,
            source_labels, source_assignee, source_priority, source_project, source_branch,
            status, started_at, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'active', ?14, ?14)
        "#,
    )
    .bind(&id)
    .bind(title)
    .bind(description)
    .bind(source)
    .bind(context.source_id)
    .bind(context.source_url)
    .bind(context.source_title)
    .bind(context.source_body)
    .bind(context.source_labels)
    .bind(context.source_assignee)
    .bind(context.source_priority)
    .bind(context.source_project)
    .bind(context.source_branch)
    .bind(&now)
    .execute(db)
    .await?;

    get_task_by_id(db, &id).await
}

pub async fn get_or_create_daily_capture_task(db: &SqlitePool) -> Result<Task, sqlx::Error> {
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let source_id = format!("daily-capture-{date}");

    if let Some(task) = sqlx::query_as::<_, Task>(
        r#"
        SELECT id, title, description, source, source_id, source_url, source_title, source_body,
               source_labels, source_assignee, source_priority, source_project, source_branch, status,
               started_at, ended_at, created_at
        FROM tasks
        WHERE source = 'memory' AND source_id = ?1
        LIMIT 1
        "#,
    )
    .bind(&source_id)
    .fetch_optional(db)
    .await?
    {
        if task.status == "active" {
            return Ok(task);
        }
        return update_task_status(db, task.id, "active".to_string()).await;
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let title = format!("Memory Capture - {date}");
    let description = "Continuous capture for the day. TaskFlow turns this timeline into a private Memory Tree / Obsidian-style knowledge base.".to_string();

    sqlx::query("UPDATE tasks SET status = 'paused' WHERE status = 'active'")
        .execute(db)
        .await?;

    sqlx::query(
        r#"
        INSERT INTO tasks (
            id, title, description, source, source_id, source_title,
            status, started_at, created_at
        )
        VALUES (?1, ?2, ?3, 'memory', ?4, ?2, 'active', ?5, ?5)
        "#,
    )
    .bind(&id)
    .bind(title)
    .bind(description)
    .bind(source_id)
    .bind(&now)
    .execute(db)
    .await?;

    get_task_by_id(db, &id).await
}

pub async fn get_all_tasks(db: &SqlitePool) -> Result<Vec<Task>, sqlx::Error> {
    sqlx::query_as::<_, Task>(
        r#"
        SELECT id, title, description, source, source_id, source_url, source_title, source_body,
               source_labels, source_assignee, source_priority, source_project, source_branch, status,
               started_at, ended_at, created_at
        FROM tasks
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(db)
    .await
}

pub async fn get_active_task(db: &SqlitePool) -> Result<Option<Task>, sqlx::Error> {
    sqlx::query_as::<_, Task>(
        r#"
        SELECT id, title, description, source, source_id, source_url, source_title, source_body,
               source_labels, source_assignee, source_priority, source_project, source_branch, status,
               started_at, ended_at, created_at
        FROM tasks
        WHERE status = 'active'
        ORDER BY started_at DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(db)
    .await
}

pub async fn update_task_status(
    db: &SqlitePool,
    id: String,
    status: String,
) -> Result<Task, sqlx::Error> {
    let now = Utc::now().to_rfc3339();

    if status == "active" {
        sqlx::query("UPDATE tasks SET status = 'paused' WHERE status = 'active' AND id != ?1")
            .bind(&id)
            .execute(db)
            .await?;

        sqlx::query(
            r#"
            UPDATE tasks
            SET status = 'active',
                started_at = COALESCE(started_at, ?2),
                ended_at = NULL
            WHERE id = ?1
            "#,
        )
        .bind(&id)
        .bind(&now)
        .execute(db)
        .await?;
    } else if status == "completed" {
        sqlx::query("UPDATE tasks SET status = 'completed', ended_at = ?2 WHERE id = ?1")
            .bind(&id)
            .bind(&now)
            .execute(db)
            .await?;
    } else {
        sqlx::query("UPDATE tasks SET status = ?2 WHERE id = ?1")
            .bind(&id)
            .bind(&status)
            .execute(db)
            .await?;
    }

    get_task_by_id(db, &id).await
}

pub async fn end_task(db: &SqlitePool, id: String) -> Result<Task, sqlx::Error> {
    update_task_status(db, id, "completed".to_string()).await
}

pub async fn get_task_by_id(db: &SqlitePool, id: &str) -> Result<Task, sqlx::Error> {
    sqlx::query_as::<_, Task>(
        r#"
        SELECT id, title, description, source, source_id, source_url, source_title, source_body,
               source_labels, source_assignee, source_priority, source_project, source_branch, status,
               started_at, ended_at, created_at
        FROM tasks
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(db)
    .await
}

/// Returns the `n` most recent daily-capture date slugs (`YYYY-MM-DD`) in descending
/// order, excluding `date` itself. Used to bootstrap prior-context for the summarizer.
pub async fn recent_daily_dates(
    db: &SqlitePool,
    date: &str,
    n: usize,
) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT source_id FROM tasks
        WHERE source = 'memory' AND source_id LIKE 'daily-capture-%' AND source_id < ?1
        ORDER BY source_id DESC
        LIMIT ?2
        "#,
    )
    .bind(format!("daily-capture-{date}"))
    .bind(n as i64)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(source_id,)| source_id.strip_prefix("daily-capture-").map(ToString::to_string))
        .collect())
}
