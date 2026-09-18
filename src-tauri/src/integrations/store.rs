use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use uuid::Uuid;

use super::{truncate_opt, Integration, Ticket};

pub async fn save_integration(
    db: &SqlitePool,
    provider: &str,
    name: &str,
    token: &str,
    workspace: Option<&str>,
    extra: Option<&str>,
) -> Result<Integration, String> {
    let id = Uuid::new_v4().to_string();
    let encrypted_token = encrypt_token(token);

    sqlx::query(
        r#"
        INSERT INTO integrations (id, provider, name, token, workspace, extra)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(&id)
    .bind(provider)
    .bind(name)
    .bind(encrypted_token)
    .bind(workspace)
    .bind(extra)
    .execute(db)
    .await
    .map_err(|err| err.to_string())?;

    get_integration(db, &id).await
}

pub async fn get_integrations(db: &SqlitePool) -> Result<Vec<Integration>, String> {
    let rows = sqlx::query_as::<_, Integration>(
        r#"
        SELECT id, provider, name, token, workspace, extra, created_at
        FROM integrations
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(db)
    .await
    .map_err(|err| err.to_string())?;

    Ok(rows
        .into_iter()
        .map(|mut integration| {
            integration.token = decrypt_token(&integration.token);
            integration
        })
        .collect())
}

pub async fn get_integration(db: &SqlitePool, id: &str) -> Result<Integration, String> {
    let mut integration = sqlx::query_as::<_, Integration>(
        r#"
        SELECT id, provider, name, token, workspace, extra, created_at
        FROM integrations
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(|err| err.to_string())?;

    integration.token = decrypt_token(&integration.token);
    Ok(integration)
}

pub async fn delete_integration(db: &SqlitePool, id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM tickets WHERE integration_id = ?1")
        .bind(id)
        .execute(db)
        .await
        .map_err(|err| err.to_string())?;

    sqlx::query("DELETE FROM integrations WHERE id = ?1")
        .bind(id)
        .execute(db)
        .await
        .map_err(|err| err.to_string())?;

    Ok(())
}

pub async fn save_ticket(db: &SqlitePool, mut ticket: Ticket) -> Result<Ticket, String> {
    ticket.description = truncate_opt(ticket.description, 2000);
    ticket.fetched_at = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO tickets (
            id, integration_id, provider, ticket_id, title, description, labels, priority,
            assignee, project, url, branch, raw, fetched_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        ON CONFLICT(id) DO UPDATE SET
            integration_id = excluded.integration_id,
            provider = excluded.provider,
            ticket_id = excluded.ticket_id,
            title = excluded.title,
            description = excluded.description,
            labels = excluded.labels,
            priority = excluded.priority,
            assignee = excluded.assignee,
            project = excluded.project,
            url = excluded.url,
            branch = excluded.branch,
            raw = excluded.raw,
            fetched_at = excluded.fetched_at
        "#,
    )
    .bind(&ticket.id)
    .bind(&ticket.integration_id)
    .bind(&ticket.provider)
    .bind(&ticket.ticket_id)
    .bind(&ticket.title)
    .bind(&ticket.description)
    .bind(&ticket.labels)
    .bind(&ticket.priority)
    .bind(&ticket.assignee)
    .bind(&ticket.project)
    .bind(&ticket.url)
    .bind(&ticket.branch)
    .bind(&ticket.raw)
    .bind(&ticket.fetched_at)
    .execute(db)
    .await
    .map_err(|err| err.to_string())?;

    Ok(ticket)
}

pub async fn search_tickets(
    db: &SqlitePool,
    query: &str,
    provider: Option<&str>,
) -> Result<Vec<Ticket>, String> {
    let pattern = format!("%{}%", query.trim());
    if let Some(provider) = provider {
        sqlx::query_as::<_, Ticket>(
            r#"
            SELECT id, integration_id, provider, ticket_id, title, description, labels, priority,
                   assignee, project, url, branch, raw, fetched_at
            FROM tickets
            WHERE title LIKE ?1 AND provider = ?2
            ORDER BY fetched_at DESC
            LIMIT 20
            "#,
        )
        .bind(pattern)
        .bind(provider)
        .fetch_all(db)
        .await
        .map_err(|err| err.to_string())
    } else {
        sqlx::query_as::<_, Ticket>(
            r#"
            SELECT id, integration_id, provider, ticket_id, title, description, labels, priority,
                   assignee, project, url, branch, raw, fetched_at
            FROM tickets
            WHERE title LIKE ?1
            ORDER BY fetched_at DESC
            LIMIT 20
            "#,
        )
        .bind(pattern)
        .fetch_all(db)
        .await
        .map_err(|err| err.to_string())
    }
}

pub async fn get_ticket_by_id(db: &SqlitePool, id: &str) -> Result<Ticket, String> {
    sqlx::query_as::<_, Ticket>(
        r#"
        SELECT id, integration_id, provider, ticket_id, title, description, labels, priority,
               assignee, project, url, branch, raw, fetched_at
        FROM tickets
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(|err| err.to_string())
}

pub async fn get_ticket_by_identity(
    db: &SqlitePool,
    integration_id: &str,
    ticket_id: &str,
) -> Result<Ticket, String> {
    sqlx::query_as::<_, Ticket>(
        r#"
        SELECT id, integration_id, provider, ticket_id, title, description, labels, priority,
               assignee, project, url, branch, raw, fetched_at
        FROM tickets
        WHERE integration_id = ?1 AND ticket_id = ?2
        LIMIT 1
        "#,
    )
    .bind(integration_id)
    .bind(ticket_id)
    .fetch_one(db)
    .await
    .map_err(|err| err.to_string())
}

fn encrypt_token(token: &str) -> String {
    xor_token(token)
}

fn decrypt_token(token: &str) -> String {
    match hex::decode(token) {
        Ok(bytes) => String::from_utf8(xor_bytes(&bytes)).unwrap_or_default(),
        Err(_) => token.to_string(),
    }
}

fn xor_token(token: &str) -> String {
    hex::encode(xor_bytes(token.as_bytes()))
}

fn xor_bytes(bytes: &[u8]) -> Vec<u8> {
    let key = machine_key();
    bytes
        .iter()
        .enumerate()
        .map(|(index, byte)| byte ^ key[index % key.len()])
        .collect()
}

fn machine_key() -> Vec<u8> {
    let hostname = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| {
            std::fs::read_to_string("/etc/hostname")
                .map(|hostname| hostname.trim().to_string())
                .unwrap_or_else(|_| "taskflow-local-machine".to_string())
        })
        .trim()
        .to_string();
    let digest = Sha256::digest(hostname.as_bytes());
    hex::encode(digest)[..32].as_bytes().to_vec()
}
