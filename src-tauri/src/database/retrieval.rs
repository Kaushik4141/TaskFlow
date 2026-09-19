use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::database::graph;
use crate::database::rollups::Rollup;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQueryRequest {
    pub query: Option<String>,
    pub project: Option<String>,
    pub time_bucket: Option<String>, // e.g. "2026-09"
    pub start_date: Option<String>,  // e.g. "2026-09-01"
    pub end_date: Option<String>,    // e.g. "2026-09-30"
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoredMemoryResult {
    pub rollup_id: String,
    pub task_id: String,
    pub title: String,
    pub project_slug: Option<String>,
    pub window_start: String,
    pub window_end: String,
    pub summary_md: String,
    pub key_points: Vec<String>,
    pub connected_tools: Vec<String>,
    pub connected_sites: Vec<String>,
    pub daily_note_link: String,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQueryResult {
    pub scoped_count: usize,
    pub results: Vec<ScoredMemoryResult>,
    pub context_summary: String,
}

const STOPWORDS: &[&str] = &[
    "a", "an", "the", "in", "on", "at", "to", "for", "of", "and", "or", "is", "was",
    "with", "by", "that", "this", "it", "from", "as", "be", "how", "what", "why", "where",
    "did", "we", "i", "you", "my", "our",
];

/// Execute a scoped, graph-enriched search across historical memory rollups.
///
/// Follows the Scope-First, Search-Second architecture to scale to 100K+ nodes:
/// 1. Scope: Uses indexed B-tree bounds on project and/or time window to prune candidate space to <= 100.
/// 2. Search & Anti-Saturation Scoring: Multi-signal scoring with stopword filtering and phrase boosting.
/// 3. Graph Enrichment: Queries SQLite graph edges to discover connected tools, sites, and hubs in that time bucket.
/// 4. Context Synthesis: Packages a high-density, ~300-token ground truth payload ready for single-turn agent consumption.
pub async fn query_graph_memory(
    db: &SqlitePool,
    req: GraphQueryRequest,
) -> Result<GraphQueryResult, sqlx::Error> {
    let limit = req.limit.unwrap_or(5).clamp(1, 20);

    // Compute effective time bounds if time_bucket or dates are provided
    let (start_bound, end_bound) = if let Some(bucket) = &req.time_bucket {
        // e.g. "2026-09" -> "2026-09-01T00:00:00", "2026-09-31T23:59:59"
        let start = format!("{bucket}-01T00:00:00");
        let end = format!("{bucket}-31T23:59:59");
        (Some(start), Some(end))
    } else {
        let start = req.start_date.as_ref().map(|d| {
            if d.contains('T') {
                d.clone()
            } else {
                format!("{d}T00:00:00")
            }
        });
        let end = req.end_date.as_ref().map(|d| {
            if d.contains('T') {
                d.clone()
            } else {
                format!("{d}T23:59:59")
            }
        });
        (start, end)
    };

    // 1. Scoped query: read candidate rollups via indexed bounds
    let candidate_rows = match (&req.project, &start_bound, &end_bound) {
        (Some(proj), Some(start), Some(end)) => {
            sqlx::query_as::<_, Rollup>(
                r#"
                SELECT id, task_id, window_start, window_end, title, summary_md,
                       key_points, apps, resources, workstream_slug, ai_mode,
                       event_count, trigger_kind, created_at
                FROM rollups
                WHERE workstream_slug = ?1 AND window_end >= ?2 AND window_start <= ?3
                ORDER BY window_start DESC
                LIMIT 100
                "#,
            )
            .bind(proj)
            .bind(start)
            .bind(end)
            .fetch_all(db)
            .await?
        }
        (Some(proj), _, _) => {
            sqlx::query_as::<_, Rollup>(
                r#"
                SELECT id, task_id, window_start, window_end, title, summary_md,
                       key_points, apps, resources, workstream_slug, ai_mode,
                       event_count, trigger_kind, created_at
                FROM rollups
                WHERE workstream_slug = ?1
                ORDER BY window_start DESC
                LIMIT 100
                "#,
            )
            .bind(proj)
            .fetch_all(db)
            .await?
        }
        (None, Some(start), Some(end)) => {
            sqlx::query_as::<_, Rollup>(
                r#"
                SELECT id, task_id, window_start, window_end, title, summary_md,
                       key_points, apps, resources, workstream_slug, ai_mode,
                       event_count, trigger_kind, created_at
                FROM rollups
                WHERE window_end >= ?1 AND window_start <= ?2
                ORDER BY window_start DESC
                LIMIT 100
                "#,
            )
            .bind(start)
            .bind(end)
            .fetch_all(db)
            .await?
        }
        (None, _, _) => {
            sqlx::query_as::<_, Rollup>(
                r#"
                SELECT id, task_id, window_start, window_end, title, summary_md,
                       key_points, apps, resources, workstream_slug, ai_mode,
                       event_count, trigger_kind, created_at
                FROM rollups
                ORDER BY window_start DESC
                LIMIT 100
                "#,
            )
            .fetch_all(db)
            .await?
        }
    };

    let scoped_count = candidate_rows.len();
    if candidate_rows.is_empty() {
        return Ok(GraphQueryResult {
            scoped_count: 0,
            results: Vec::new(),
            context_summary: "No activity records found matching the specified scope.".to_string(),
        });
    }

    // 2. Token scoring & anti-saturation filtering
    let raw_query = req.query.as_deref().unwrap_or("").trim();
    let query_lower = raw_query.to_lowercase();
    let query_terms: Vec<&str> = query_lower
        .split_whitespace()
        .filter(|t| t.len() > 1 && !STOPWORDS.contains(t))
        .collect();

    let now = Utc::now();

    let mut scored: Vec<(f64, Rollup)> = candidate_rows
        .into_iter()
        .map(|r| {
            let mut score = 0.0;
            let title_lower = r.title.to_lowercase();
            let summary_lower = r.summary_md.to_lowercase();
            let key_points_lower = r.key_points.as_deref().unwrap_or("").to_lowercase();
            let apps_lower = r.apps.as_deref().unwrap_or("").to_lowercase();
            let resources_lower = r.resources.as_deref().unwrap_or("").to_lowercase();

            if !query_terms.is_empty() {
                for term in &query_terms {
                    if title_lower.contains(term) {
                        score += 4.0;
                    }
                    if summary_lower.contains(term) {
                        score += 1.5;
                    }
                    if key_points_lower.contains(term) {
                        score += 2.5;
                    }
                    if apps_lower.contains(term) {
                        score += 3.0;
                    }
                    if resources_lower.contains(term) {
                        score += 2.0;
                    }
                }

                // Phrase boost
                if !raw_query.is_empty() && (title_lower.contains(&query_lower) || summary_lower.contains(&query_lower)) {
                    score += 6.0;
                }
            } else {
                // Default baseline when no query text given: rank purely by recency
                score = 10.0;
            }

            // Recency weighting
            let age_days = DateTime::parse_from_rfc3339(&r.window_end)
                .ok()
                .map(|dt| {
                    let diff = now.signed_duration_since(dt.with_timezone(&Utc));
                    diff.num_days().max(0) as f64
                })
                .unwrap_or(0.0);

            let recency_multiplier = 1.0 / (1.0 + age_days * 0.01);
            let final_score = score * recency_multiplier;

            (final_score, r)
        })
        .collect();

    // If query terms were provided, filter out items with zero matching score
    if !query_terms.is_empty() {
        scored.retain(|(s, _)| *s > 0.0);
    }

    // Sort descending by score
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);

    // 3. Graph context enrichment
    let mut results = Vec::new();
    let mut summary_lines = Vec::new();

    summary_lines.push("### Scoped Memory Context (TaskFlow Graph Engine):".to_string());

    for (score, rollup) in scored {
        let date_str = DateTime::parse_from_rfc3339(&rollup.window_end)
            .ok()
            .map(|dt| dt.with_timezone(&Local).format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let time_bucket = DateTime::parse_from_rfc3339(&rollup.window_end)
            .ok()
            .map(|dt| dt.with_timezone(&Local).format("%Y-%m").to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let daily_note_link = format!("Memory/Daily/{date_str}");

        let mut connected_tools = Vec::new();
        let mut connected_sites = Vec::new();

        if let Some(slug) = &rollup.workstream_slug {
            let project_entity = format!("Projects/{slug}");
            if let Ok(edges) = graph::get_connected_entities(db, &project_entity, Some(&time_bucket), 8).await {
                for edge in edges {
                    if edge.entity.starts_with("Apps/") {
                        let app_name = edge.entity.strip_prefix("Apps/").unwrap_or(&edge.entity);
                        connected_tools.push(format!("[[Apps/{app_name}]]"));
                    } else if edge.entity.starts_with("Sites/") {
                        let site_name = edge.entity.strip_prefix("Sites/").unwrap_or(&edge.entity);
                        connected_sites.push(format!("[[Sites/{site_name}]]"));
                    }
                }
            }
        }

        // Parse key points
        let key_points: Vec<String> = rollup
            .key_points
            .as_deref()
            .and_then(|raw| serde_json::from_str(raw).ok())
            .unwrap_or_default();

        let proj_label = rollup
            .workstream_slug
            .as_deref()
            .unwrap_or("Inbox");

        let tools_display = if connected_tools.is_empty() {
            "None".to_string()
        } else {
            connected_tools.join(", ")
        };

        let sites_display = if connected_sites.is_empty() {
            "None".to_string()
        } else {
            connected_sites.join(", ")
        };

        summary_lines.push(format!(
            "- **[{date_str}] [[Projects/{proj_label}]] — {}**",
            rollup.title.trim()
        ));
        summary_lines.push(format!("  - **Summary**: {}", rollup.summary_md.trim()));
        if !connected_tools.is_empty() {
            summary_lines.push(format!("  - **Connected Tools**: {tools_display}"));
        }
        if !connected_sites.is_empty() {
            summary_lines.push(format!("  - **Referenced Sites**: {sites_display}"));
        }
        summary_lines.push(format!("  - **Daily Reference**: [[{daily_note_link}]]"));

        results.push(ScoredMemoryResult {
            rollup_id: rollup.id,
            task_id: rollup.task_id,
            title: rollup.title,
            project_slug: rollup.workstream_slug,
            window_start: rollup.window_start,
            window_end: rollup.window_end,
            summary_md: rollup.summary_md,
            key_points,
            connected_tools,
            connected_sites,
            daily_note_link,
            score,
        });
    }

    let context_summary = summary_lines.join("\n");

    Ok(GraphQueryResult {
        scoped_count,
        results,
        context_summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use crate::database::rollups::insert_rollup;
    use crate::database::tasks::create_task;

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::database::schema::run_migrations(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn scoped_retrieval_filters_by_project_and_scores_query() {
        let db = setup_db().await;
        let task = create_task(&db, "Test Task".to_string(), None, "memory".to_string())
            .await
            .unwrap();

        // Rollup 1: Auth bug fix
        let r1 = insert_rollup(
            &db,
            &task.id,
            "2026-09-18T10:00:00Z",
            "2026-09-18T10:10:00Z",
            "Fixed OAuth token expiration bug",
            "Resolved loop in refresh_token by setting proper TTL",
            &["auth token fix".to_string()],
            &["Cursor.exe".to_string()],
            &["https://github.com/Kaushik4141/TaskFlow".to_string()],
            Some("AuthService"),
            Some("basic"),
            10,
            "interval",
        )
        .await
        .unwrap();
        graph::record_rollup_edges(&db, &r1).await.unwrap();

        // Rollup 2: Unrelated UI change in TaskFlow
        let r2 = insert_rollup(
            &db,
            &task.id,
            "2026-09-18T11:00:00Z",
            "2026-09-18T11:10:00Z",
            "Refactored navigation buttons",
            "Updated CSS for dark mode toolbar buttons",
            &["ui css button".to_string()],
            &["Cursor.exe".to_string()],
            &[],
            Some("TaskFlow"),
            Some("basic"),
            5,
            "interval",
        )
        .await
        .unwrap();
        graph::record_rollup_edges(&db, &r2).await.unwrap();

        // Search scoped to AuthService and query "token expiration"
        let res = query_graph_memory(
            &db,
            GraphQueryRequest {
                query: Some("token expiration".to_string()),
                project: Some("AuthService".to_string()),
                time_bucket: Some("2026-09".to_string()),
                start_date: None,
                end_date: None,
                limit: Some(5),
            },
        )
        .await
        .unwrap();

        assert_eq!(res.results.len(), 1);
        assert_eq!(res.results[0].title, "Fixed OAuth token expiration bug");
        assert_eq!(res.results[0].project_slug.as_deref(), Some("AuthService"));
        assert!(res.context_summary.contains("[[Projects/AuthService]]"));
        assert!(res.context_summary.contains("[[Apps/cursor]]"));
        assert!(res.context_summary.contains("[[Sites/github.com]]"));
    }
}
