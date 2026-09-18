use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

use crate::database::rollups::Rollup;
use crate::wiki::links::{domain_from_url, hub_slug_for_app};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdgeRow {
    pub id: i64,
    pub source_entity: String,
    pub target_entity: String,
    pub relation_type: String,
    pub weight: f64,
    pub time_bucket: String,
    pub last_seen: String,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedEntity {
    pub entity: String,
    pub relation: String,
    pub weight: f64,
    pub last_seen: String,
}

/// Upsert a single edge in the knowledge graph.
/// Idempotent: increments weight and updates last_seen on conflict within the same time bucket.
pub async fn upsert_edge(
    db: &SqlitePool,
    source: &str,
    target: &str,
    relation: &str,
    time_bucket: &str,
    last_seen: &str,
    metadata_json: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO graph_edges
            (source_entity, target_entity, relation_type, weight, time_bucket, last_seen, metadata_json)
        VALUES (?1, ?2, ?3, 1.0, ?4, ?5, ?6)
        ON CONFLICT(source_entity, target_entity, relation_type, time_bucket)
        DO UPDATE SET
            weight = graph_edges.weight + 1.0,
            last_seen = excluded.last_seen;
        "#,
    )
    .bind(source)
    .bind(target)
    .bind(relation)
    .bind(time_bucket)
    .bind(last_seen)
    .bind(metadata_json)
    .execute(db)
    .await?;

    Ok(())
}

/// Ingest all entity relationships extracted from a single activity rollup into the graph.
/// Extracts:
/// - Project <-> Daily note (`active_on`)
/// - Project <-> Apps (`used_tool`)
/// - Project <-> Sites (`visited_site`)
/// - Apps <-> Sites (`co_occurred`)
pub async fn record_rollup_edges(db: &SqlitePool, rollup: &Rollup) -> Result<usize, sqlx::Error> {
    let mut count = 0;

    let time_bucket = DateTime::parse_from_rfc3339(&rollup.window_end)
        .ok()
        .map(|dt| dt.with_timezone(&Local).format("%Y-%m").to_string())
        .or_else(|| {
            DateTime::parse_from_rfc3339(&rollup.window_start)
                .ok()
                .map(|dt| dt.with_timezone(&Local).format("%Y-%m").to_string())
        })
        .unwrap_or_else(|| Utc::now().format("%Y-%m").to_string());

    let date_str = DateTime::parse_from_rfc3339(&rollup.window_end)
        .ok()
        .map(|dt| dt.with_timezone(&Local).format("%Y-%m-%d").to_string())
        .or_else(|| {
            DateTime::parse_from_rfc3339(&rollup.window_start)
                .ok()
                .map(|dt| dt.with_timezone(&Local).format("%Y-%m-%d").to_string())
        })
        .unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string());

    let last_seen = if !rollup.window_end.is_empty() {
        rollup.window_end.clone()
    } else {
        Utc::now().to_rfc3339()
    };

    let daily_node = format!("Memory/Daily/{date_str}");

    let mut app_entities = Vec::new();
    if let Some(raw_apps) = &rollup.apps {
        if let Ok(apps) = serde_json::from_str::<Vec<String>>(raw_apps) {
            for app in apps {
                if let Some(slug) = hub_slug_for_app(&app) {
                    let ent = format!("Apps/{slug}");
                    if !app_entities.contains(&ent) {
                        app_entities.push(ent);
                    }
                }
            }
        }
    }

    let mut site_entities = Vec::new();
    if let Some(raw_resources) = &rollup.resources {
        if let Ok(urls) = serde_json::from_str::<Vec<String>>(raw_resources) {
            for url in urls {
                if url.starts_with("http") {
                    if let Some(domain) = domain_from_url(&url) {
                        let ent = format!("Sites/{domain}");
                        if !site_entities.contains(&ent) {
                            site_entities.push(ent);
                        }
                    }
                }
            }
        }
    }

    if let Some(project_slug) = &rollup.workstream_slug {
        if !project_slug.trim().is_empty() {
            let project_node = format!("Projects/{project_slug}");

            // Edge: Project -> Daily note
            upsert_edge(db, &project_node, &daily_node, "active_on", &time_bucket, &last_seen, None).await?;
            count += 1;

            // Edges: Project -> Apps
            for app in &app_entities {
                upsert_edge(db, &project_node, app, "used_tool", &time_bucket, &last_seen, None).await?;
                count += 1;
            }

            // Edges: Project -> Sites
            for site in &site_entities {
                upsert_edge(db, &project_node, site, "visited_site", &time_bucket, &last_seen, None).await?;
                count += 1;
            }
        }
    }

    // Edges: Apps <-> Sites co-occurrence
    for app in &app_entities {
        for site in &site_entities {
            upsert_edge(db, app, site, "co_occurred", &time_bucket, &last_seen, None).await?;
            count += 1;
        }
    }

    Ok(count)
}

/// Retrieve degree-capped connected entities for a given node.
/// When `time_bucket` is provided (e.g. "2026-09"), queries only that bucket.
/// When None, aggregates weights across all buckets, sorted by total weight DESC.
pub async fn get_connected_entities(
    db: &SqlitePool,
    entity: &str,
    time_bucket: Option<&str>,
    limit: usize,
) -> Result<Vec<ConnectedEntity>, sqlx::Error> {
    let limit_i64 = limit.min(100) as i64;
    if let Some(bucket) = time_bucket {
        let rows = sqlx::query_as::<_, (String, String, f64, String)>(
            r#"
            SELECT target_entity, relation_type, weight, last_seen
            FROM graph_edges
            WHERE source_entity = ?1 AND time_bucket = ?2
            ORDER BY weight DESC
            LIMIT ?3
            "#,
        )
        .bind(entity)
        .bind(bucket)
        .bind(limit_i64)
        .fetch_all(db)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(ent, rel, w, ls)| ConnectedEntity {
                entity: ent,
                relation: rel,
                weight: w,
                last_seen: ls,
            })
            .collect())
    } else {
        let rows = sqlx::query_as::<_, (String, String, f64, String)>(
            r#"
            SELECT target_entity, relation_type, SUM(weight) as total_weight, MAX(last_seen) as latest_seen
            FROM graph_edges
            WHERE source_entity = ?1
            GROUP BY target_entity, relation_type
            ORDER BY total_weight DESC
            LIMIT ?2
            "#,
        )
        .bind(entity)
        .bind(limit_i64)
        .fetch_all(db)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(ent, rel, w, ls)| ConnectedEntity {
                entity: ent,
                relation: rel,
                weight: w,
                last_seen: ls,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::database::schema::run_migrations(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn upsert_edge_increments_weight() {
        let db = test_db().await;
        upsert_edge(
            &db,
            "Projects/TaskFlow",
            "Apps/cursor",
            "used_tool",
            "2026-09",
            "2026-09-18T12:00:00Z",
            None,
        )
        .await
        .unwrap();

        upsert_edge(
            &db,
            "Projects/TaskFlow",
            "Apps/cursor",
            "used_tool",
            "2026-09",
            "2026-09-18T13:00:00Z",
            None,
        )
        .await
        .unwrap();

        let connected = get_connected_entities(&db, "Projects/TaskFlow", Some("2026-09"), 10)
            .await
            .unwrap();

        assert_eq!(connected.len(), 1);
        assert_eq!(connected[0].entity, "Apps/cursor");
        assert_eq!(connected[0].weight, 2.0);
        assert_eq!(connected[0].last_seen, "2026-09-18T13:00:00Z");
    }

    #[tokio::test]
    async fn record_rollup_edges_indexes_tools_sites_and_daily() {
        let db = test_db().await;
        let rollup = Rollup {
            id: "r1".to_string(),
            task_id: "t1".to_string(),
            window_start: "2026-09-18T10:00:00Z".to_string(),
            window_end: "2026-09-18T10:10:00Z".to_string(),
            title: "Testing edge ingestion".to_string(),
            summary_md: "Summary body".to_string(),
            key_points: Some("[]".to_string()),
            apps: Some(r#"["Cursor.exe", "Alacritty"]"#.to_string()),
            resources: Some(r#"["https://github.com/Kaushik4141/TaskFlow"]"#.to_string()),
            workstream_slug: Some("TaskFlow".to_string()),
            ai_mode: Some("basic".to_string()),
            event_count: 5,
            trigger_kind: Some("interval".to_string()),
            created_at: "2026-09-18T10:10:00Z".to_string(),
        };

        let edge_count = record_rollup_edges(&db, &rollup).await.unwrap();
        assert!(edge_count >= 4);

        let connected = get_connected_entities(&db, "Projects/TaskFlow", Some("2026-09"), 10)
            .await
            .unwrap();

        let entities: Vec<String> = connected.into_iter().map(|c| c.entity).collect();
        assert!(entities.contains(&"Memory/Daily/2026-09-18".to_string()));
        assert!(entities.contains(&"Apps/cursor".to_string()));
        assert!(entities.contains(&"Apps/alacritty".to_string()));
        assert!(entities.contains(&"Sites/github.com".to_string()));
    }
}
