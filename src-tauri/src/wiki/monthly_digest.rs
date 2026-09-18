//! Monthly memory digest writer.
//!
//! Provides the Level 3 hierarchical compaction layer (Stanford RAPTOR style)
//! so agents traversing 100K+ nodes never scan daily notes or rollups linearly.
//! An agent inspects 12 monthly digest nodes (`TaskFlow/Memory/<YYYY>/<MM>.md`)
//! across an entire year, each strictly bounded in size (~30-50 lines, < 400 tokens).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, Utc};
use sqlx::SqlitePool;

use crate::database::rollups::{self, Rollup};
use crate::wiki::links::{domain_from_url, hub_slug_for_app};

/// Regenerate `TaskFlow/Memory/<YYYY>/<MM>.md` from SQLite rollups.
///
/// `year_month` must be in "YYYY-MM" format (e.g. "2026-09").
pub async fn rebuild_monthly_digest(
    db: &SqlitePool,
    vault: &Path,
    year_month: &str,
) -> Result<Option<PathBuf>, String> {
    let parts: Vec<&str> = year_month.split('-').collect();
    if parts.len() != 2 || parts[0].len() != 4 || parts[1].len() != 2 {
        return Err(format!("Invalid year_month format '{year_month}', expected YYYY-MM"));
    }
    let year = parts[0];
    let month = parts[1];

    let start_bound = format!("{year_month}-01T00:00:00Z");
    let end_bound = format!("{year_month}-31T23:59:59Z");

    let rollups = rollups::get_rollups_in_range(db, &start_bound, &end_bound)
        .await
        .map_err(|err| err.to_string())?;

    if rollups.is_empty() {
        return Ok(None);
    }

    let markdown = render_monthly_digest(year, month, &rollups);
    let folder = vault.join("TaskFlow").join("Memory").join(year);
    fs::create_dir_all(&folder)
        .map_err(|err| format!("Could not create monthly digest directory: {err}"))?;

    let path = folder.join(format!("{month}.md"));
    fs::write(&path, markdown)
        .map_err(|err| format!("Could not write monthly digest file: {err}"))?;

    Ok(Some(path))
}

fn month_name(month_num: &str) -> &'static str {
    match month_num {
        "01" => "January",
        "02" => "February",
        "03" => "March",
        "04" => "April",
        "05" => "May",
        "06" => "June",
        "07" => "July",
        "08" => "August",
        "09" => "September",
        "10" => "October",
        "11" => "November",
        "12" => "December",
        _ => "Month",
    }
}

fn render_monthly_digest(year: &str, month: &str, rollups: &[Rollup]) -> String {
    let m_name = month_name(month);
    let today = Utc::now().format("%Y-%m-%d").to_string();

    // Group rollups by workstream
    let mut project_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut day_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut apps: BTreeSet<String> = BTreeSet::new();
    let mut sites: BTreeSet<String> = BTreeSet::new();

    for rollup in rollups {
        let slug = rollup
            .workstream_slug
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("Inbox");
        *project_counts.entry(slug.to_string()).or_default() += 1;

        let date_str = DateTime::parse_from_rfc3339(&rollup.window_end)
            .ok()
            .map(|dt| dt.with_timezone(&Local).format("%Y-%m-%d").to_string())
            .or_else(|| {
                DateTime::parse_from_rfc3339(&rollup.window_start)
                    .ok()
                    .map(|dt| dt.with_timezone(&Local).format("%Y-%m-%d").to_string())
            })
            .unwrap_or_else(|| format!("{year}-{month}-01"));
        *day_counts.entry(date_str).or_default() += 1;

        if let Some(raw_apps) = &rollup.apps {
            if let Ok(app_list) = serde_json::from_str::<Vec<String>>(raw_apps) {
                for app in app_list {
                    if let Some(slug) = hub_slug_for_app(&app) {
                        apps.insert(format!("[[Apps/{slug}]]"));
                    }
                }
            }
        }

        if let Some(raw_resources) = &rollup.resources {
            if let Ok(urls) = serde_json::from_str::<Vec<String>>(raw_resources) {
                for url in urls {
                    if url.starts_with("http") {
                        if let Some(domain) = domain_from_url(&url) {
                            sites.insert(format!("[[Sites/{domain}]]"));
                        }
                    }
                }
            }
        }
    }

    let mut out = String::new();
    out.push_str("---\n");
    out.push_str("type: monthly_digest\n");
    out.push_str(&format!("year: {year}\n"));
    out.push_str(&format!("month: \"{month}\"\n"));
    out.push_str("tags:\n");
    out.push_str("  - taskflow/digest\n");
    out.push_str("  - memory\n");
    out.push_str(&format!("last_updated: {today}\n"));
    out.push_str("---\n\n");

    out.push_str(&format!("# {m_name} {year} Memory Digest\n\n"));
    out.push_str("Hierarchical monthly synthesis connecting active workstreams and daily memory nodes.\n\n");

    out.push_str("## Active Workstreams\n");
    for (slug, count) in &project_counts {
        let label = if *count == 1 { "rollup" } else { "rollups" };
        out.push_str(&format!("- [[Projects/{slug}]] — {count} {label}\n"));
    }
    out.push('\n');

    out.push_str("## Primary Tools & Resources\n");
    if apps.is_empty() {
        out.push_str("- **Tools**: None recorded\n");
    } else {
        let app_links = apps.into_iter().collect::<Vec<_>>().join(", ");
        out.push_str(&format!("- **Tools**: {app_links}\n"));
    }
    if sites.is_empty() {
        out.push_str("- **Sites**: None recorded\n");
    } else {
        let site_links = sites.into_iter().collect::<Vec<_>>().join(", ");
        out.push_str(&format!("- **Sites**: {site_links}\n"));
    }
    out.push('\n');

    out.push_str("## Daily Memory Notes\n");
    for (day, count) in day_counts.iter().rev() {
        let label = if *count == 1 { "rollup" } else { "rollups" };
        out.push_str(&format!("- [[Memory/Daily/{day}]] — {count} {label}\n"));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use crate::database::rollups::insert_rollup;
    use crate::database::tasks::create_task;

    static TEST_SEQ: AtomicUsize = AtomicUsize::new(0);

    fn fresh_vault(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "taskflow-monthly-digest-test-{name}-{}",
            TEST_SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).ok();
        }
        std::fs::create_dir_all(&dir).expect("create temp vault");
        dir
    }

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::database::schema::run_migrations(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn rebuild_monthly_digest_writes_bounded_month_summary() {
        let db = setup_db().await;
        let vault = fresh_vault("monthly_test");

        let task = create_task(&db, "Test Task".to_string(), None, "memory".to_string())
            .await
            .unwrap();

        insert_rollup(
            &db,
            &task.id,
            "2026-09-18T10:00:00Z",
            "2026-09-18T10:10:00Z",
            "Added monthly digest layer",
            "Implemented hierarchical RAPTOR temporal compaction",
            &[],
            &["Cursor.exe".to_string()],
            &["https://github.com/Kaushik4141/TaskFlow".to_string()],
            Some("TaskFlow"),
            Some("basic"),
            5,
            "interval",
        )
        .await
        .unwrap();

        let path = rebuild_monthly_digest(&db, &vault, "2026-09")
            .await
            .expect("rebuild ok")
            .expect("path returned");

        assert!(path.exists());
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("type: monthly_digest"));
        assert!(content.contains("# September 2026 Memory Digest"));
        assert!(content.contains("- [[Projects/TaskFlow]] — 1 rollup"));
        assert!(content.contains("- **Tools**: [[Apps/cursor]]"));
        assert!(content.contains("- **Sites**: [[Sites/github.com]]"));
        assert!(content.contains("- [[Memory/Daily/2026-09-18]] — 1 rollup"));
    }
}
