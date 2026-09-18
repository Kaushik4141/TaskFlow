use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Documentation {
    pub id: String,
    pub task_id: String,
    pub content: String,
    pub summary: Option<String>,
    pub version: i64,
    pub generated_at: String,
    pub ai_mode: Option<String>,
}

pub async fn save_documentation(
    db: &SqlitePool,
    task_id: String,
    content: String,
    summary: Option<String>,
    ai_mode: Option<String>,
) -> Result<Documentation, sqlx::Error> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let version = next_version(db, &task_id).await?;

    sqlx::query(
        r#"
        INSERT INTO documentation (id, task_id, content, summary, version, generated_at, ai_mode)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
    )
    .bind(&id)
    .bind(&task_id)
    .bind(&content)
    .bind(&summary)
    .bind(version)
    .bind(&now)
    .bind(&ai_mode)
    .execute(db)
    .await?;

    Ok(Documentation {
        id,
        task_id,
        content,
        summary,
        version,
        generated_at: now,
        ai_mode,
    })
}

pub async fn get_latest_documentation(
    db: &SqlitePool,
    task_id: String,
) -> Result<Option<Documentation>, sqlx::Error> {
    sqlx::query_as::<_, Documentation>(
        r#"
        SELECT id, task_id, content, summary, version, generated_at, ai_mode
        FROM documentation
        WHERE task_id = ?1
        ORDER BY version DESC, generated_at DESC
        LIMIT 1
        "#,
    )
    .bind(task_id)
    .fetch_optional(db)
    .await
}

pub async fn get_documentation_history(
    db: &SqlitePool,
    task_id: String,
) -> Result<Vec<Documentation>, sqlx::Error> {
    sqlx::query_as::<_, Documentation>(
        r#"
        SELECT id, task_id, content, summary, version, generated_at, ai_mode
        FROM documentation
        WHERE task_id = ?1
        ORDER BY version DESC
        "#,
    )
    .bind(task_id)
    .fetch_all(db)
    .await
}

pub async fn save_embedding(
    db: &SqlitePool,
    doc_id: &str,
    embedding: &[f32],
) -> Result<(), sqlx::Error> {
    let bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
    sqlx::query("UPDATE documentation SET summary_embedding = ?1 WHERE id = ?2")
        .bind(&bytes)
        .bind(doc_id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn get_all_with_embeddings(
    db: &SqlitePool,
) -> Result<Vec<(String, String, String, Option<String>, Option<Vec<u8>>, String)>, sqlx::Error> {
    // Returns: (doc_id, task_id, task_title, summary, embedding_bytes, generated_at)
    sqlx::query_as(
        r#"
        SELECT d.id, d.task_id, t.title, d.summary, d.summary_embedding, d.generated_at
        FROM documentation d
        JOIN tasks t ON t.id = d.task_id
        WHERE d.version = (
            SELECT MAX(d2.version)
            FROM documentation d2
            WHERE d2.task_id = d.task_id
        )
        "#,
    )
    .fetch_all(db)
    .await
}

async fn next_version(db: &SqlitePool, task_id: &str) -> Result<i64, sqlx::Error> {
    let row: (Option<i64>,) =
        sqlx::query_as("SELECT MAX(version) FROM documentation WHERE task_id = ?1")
            .bind(task_id)
            .fetch_one(db)
            .await?;
    Ok(row.0.unwrap_or(0) + 1)
}
