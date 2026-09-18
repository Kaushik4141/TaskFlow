//! Derived daily index. The date-based daily note (`TaskFlow/Memory/Daily/<date>.md`)
//! is no longer the source of truth — the workstream nodes are. This rebuilds the
//! daily note from the `rollups` table after every roll-up as a thin index: which
//! workstreams were active today, and one line per roll-up in chronological order.
//!
//! Because the file is 100% derived data, whole-file replace is correct here
//! (unlike the old flow, where the daily note held the only copy of the summary).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDate, NaiveTime, TimeZone, Utc};
use sqlx::SqlitePool;

use super::daily_note_relative_path;
use crate::database::rollups::{self, Rollup};

/// Regenerate `Memory/Daily/<date>.md` from the day's roll-ups. Returns the
/// written path. Errors (and leaves the file untouched) when the date is
/// malformed; writes nothing but returns Ok(None) when the day has no roll-ups
/// yet — an existing file is preserved rather than blanked.
pub async fn rebuild_daily_index(
    db: &SqlitePool,
    vault: &Path,
    date: &str,
) -> Result<Option<PathBuf>, String> {
    let (day_start, day_end) = local_day_bounds_utc(date)?;
    let rollups = rollups::get_rollups_in_range(db, &day_start, &day_end)
        .await
        .map_err(|err| err.to_string())?;
    if rollups.is_empty() {
        return Ok(None);
    }

    let markdown = render_daily_index(db, date, &day_start, &day_end, &rollups).await;
    let path = vault.join(daily_note_relative_path(date));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Could not create daily index folder: {err}"))?;
    }
    let mut content = markdown;
    if !content.ends_with('\n') {
        content.push('\n');
    }
    fs::write(&path, content).map_err(|err| format!("Could not write daily index: {err}"))?;
    Ok(Some(path))
}

async fn render_daily_index(
    db: &SqlitePool,
    date: &str,
    day_start: &str,
    day_end: &str,
    rollups: &[Rollup],
) -> String {
    // Workstream → (count, first window start, last window end), in slug order.
    let mut workstreams: BTreeMap<String, (usize, String, String)> = BTreeMap::new();
    for rollup in rollups {
        let slug = rollup
            .workstream_slug
            .clone()
            .unwrap_or_else(|| "Inbox".to_string());
        let entry = workstreams.entry(slug).or_insert((0, rollup.window_start.clone(), rollup.window_end.clone()));
        entry.0 += 1;
        entry.2 = rollup.window_end.clone();
    }

    // Hubs touched today: apps + sites aggregate from the roll-ups' own JSON;
    // activity types come from the day's raw events (roll-ups don't store
    // content types). These links are what keep App/Site/Activity hub pages
    // connected into the graph — without them, hubs first seen today would be
    // orphans with zero inbound links.
    let mut apps: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut sites: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for rollup in rollups {
        if let Some(raw) = rollup.apps.as_deref() {
            if let Ok(names) = serde_json::from_str::<Vec<String>>(raw) {
                for name in names {
                    if let Some(slug) = super::links::hub_slug_for_app(&name) {
                        apps.insert(slug);
                    }
                }
            }
        }
        if let Some(raw) = rollup.resources.as_deref() {
            if let Ok(urls) = serde_json::from_str::<Vec<String>>(raw) {
                for url in urls {
                    if url.starts_with("http") {
                        if let Some(domain) = super::links::domain_from_url(&url) {
                            sites.insert(domain);
                        }
                    }
                }
            }
        }
    }
    let mut activities: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    if let Ok(content_types) =
        crate::database::events::get_content_types_in_range(db, day_start, day_end).await
    {
        for content_type in content_types {
            if let Some(slug) = super::links::activity_slug_for(&content_type) {
                activities.insert(slug);
            }
        }
    }

    let mut lines = vec![
        "---".to_string(),
        "type: daily_memory_index".to_string(),
        "source: rollup_index".to_string(),
        format!("date: {date}"),
        format!("rollups: {}", rollups.len()),
        "tags:".to_string(),
        "  - taskflow/daily".to_string(),
        "  - memory".to_string(),
        "workstreams:".to_string(),
    ];
    for slug in workstreams.keys() {
        lines.push(format!("  - {slug}"));
    }
    if !apps.is_empty() {
        lines.push("apps:".to_string());
        for app in &apps {
            lines.push(format!("  - {app}"));
        }
    }
    if !sites.is_empty() {
        lines.push("sites:".to_string());
        for site in &sites {
            lines.push(format!("  - {site}"));
        }
    }
    lines.extend([
        "---".to_string(),
        String::new(),
        format!("# Daily Index - {date}"),
        String::new(),
        "> Derived view — regenerated by TaskFlow after every roll-up. \
         The linked workstream nodes hold the full activity timeline."
            .to_string(),
        String::new(),
        "## Workstreams Active".to_string(),
    ]);

    for (slug, (count, first_start, last_end)) in &workstreams {
        lines.push(format!(
            "- [[Projects/{slug}]] — {count} roll-up{}, {}–{}",
            if *count == 1 { "" } else { "s" },
            local_time(first_start),
            local_time(last_end),
        ));
    }

    if !apps.is_empty() || !sites.is_empty() || !activities.is_empty() {
        lines.push(String::new());
        lines.push("## Hubs Touched".to_string());
        for app in &apps {
            lines.push(format!("- [[Apps/{app}]]"));
        }
        for site in &sites {
            lines.push(format!("- [[Sites/{site}]]"));
        }
        for activity in &activities {
            lines.push(format!("- [[Activity/{activity}]]"));
        }
    }

    lines.push(String::new());
    lines.push("## Timeline".to_string());
    for rollup in rollups {
        let slug = rollup
            .workstream_slug
            .clone()
            .unwrap_or_else(|| "Inbox".to_string());
        let apps = rollup
            .apps
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
            .unwrap_or_default();
        let apps_suffix = if apps.is_empty() {
            String::new()
        } else {
            format!(" · {}", apps.join(", "))
        };
        lines.push(format!(
            "- `{}–{}` **{}** → [[Projects/{}]] ({} events{})",
            local_time(&rollup.window_start),
            local_time(&rollup.window_end),
            rollup.title.trim(),
            slug,
            rollup.event_count,
            apps_suffix,
        ));
    }

    // Prev/next day navigation over days that actually HAVE roll-ups (a day
    // with no roll-ups has no daily note, so linking it would be broken).
    if let Ok((prev_start, next_start)) =
        rollups::get_adjacent_rollup_window_starts(db, day_start, day_end).await
    {
        let prev_date = prev_start.as_deref().and_then(local_date);
        let next_date = next_start.as_deref().and_then(local_date);
        lines.push(String::new());
        lines.push("### Navigation".to_string());
        if let Some(prev) = prev_date {
            lines.push(format!("- [[Memory/Daily/{prev}|Previous]]"));
        }
        lines.push("- [[index|Daily Index]]".to_string());
        if let Some(next) = next_date {
            lines.push(format!("- [[Memory/Daily/{next}|Next]]"));
        }
    }

    lines.join("\n")
}

/// Convert a local calendar date (`YYYY-MM-DD`) to the UTC RFC-3339 bounds
/// `[midnight, next midnight)` used for the rollups range query.
fn local_day_bounds_utc(date: &str) -> Result<(String, String), String> {
    let naive = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|err| format!("invalid date {date}: {err}"))?;
    let midnight = naive
        .and_time(NaiveTime::MIN);
    let start_local = match Local.from_local_datetime(&midnight) {
        chrono::LocalResult::Single(value) => value,
        chrono::LocalResult::Ambiguous(earliest, _) => earliest,
        chrono::LocalResult::None => return Err(format!("local midnight does not exist for {date}")),
    };
    let start_utc = start_local.with_timezone(&Utc);
    let end_utc = start_utc + chrono::Duration::days(1);
    Ok((start_utc.to_rfc3339(), end_utc.to_rfc3339()))
}

/// `HH:MM` local time from a UTC RFC-3339 timestamp (falls back to the raw
/// string when parsing fails).
fn local_time(rfc3339: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(rfc3339)
        .map(|ts| ts.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_else(|_| rfc3339.to_string())
}

/// Local `YYYY-MM-DD` from a UTC RFC-3339 timestamp; None when unparseable
/// (callers skip the navigation link rather than emit a broken one).
fn local_date(rfc3339: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(rfc3339)
        .ok()
        .map(|ts| ts.with_timezone(&Local).format("%Y-%m-%d").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{rollups::insert_rollup, schema::run_migrations};
    use sqlx::SqlitePool;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_SEQ: AtomicUsize = AtomicUsize::new(0);

    fn fresh_vault(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "taskflow-daily-index-test-{name}-{}",
            TEST_SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).ok();
        }
        std::fs::create_dir_all(&dir).expect("create temp vault");
        dir
    }

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
    async fn rebuild_writes_derived_index_and_is_replace_safe() {
        let db = memory_db().await;
        let vault = fresh_vault("rebuild");
        let date = Local::now().format("%Y-%m-%d").to_string();

        // Two roll-ups today (computed against TODAY so the local-day bounds
        // query matches regardless of the test machine's timezone).
        for (idx, slug) in ["TaskFlow", "Inbox"].into_iter().enumerate() {
            insert_rollup(
                &db,
                "task-1",
                &Utc::now().to_rfc3339(),
                &Utc::now().to_rfc3339(),
                &format!("Window {idx}"),
                "summary body",
                &[],
                &["Code".to_string()],
                &["https://github.com/kaush/TaskFlow".to_string()],
                Some(slug),
                Some("template"),
                4,
                "interval",
            )
            .await
            .expect("insert rollup");
        }

        // One raw event with a content type → Activity-hub link in the index.
        crate::database::events::insert_event_with_metadata(
            &db,
            "task-1".to_string(),
            "window_switch".to_string(),
            Some("WindowsTerminal.exe".to_string()),
            Some("pwsh".to_string()),
            Some("$ cargo test".to_string()),
            None,
            Some("TerminalContent".to_string()),
            Some("uitautomation".to_string()),
            true,
            0,
        )
        .await
        .expect("insert event");

        // A roll-up from an earlier day → the prev-navigation link must point
        // at that day (and never at a day that has no roll-ups).
        let yesterday = (Utc::now() - chrono::Duration::days(2)).to_rfc3339();
        insert_rollup(
            &db,
            "task-1",
            &yesterday,
            &yesterday,
            "Old window",
            "old summary",
            &[],
            &[],
            &[],
            Some("Inbox"),
            Some("template"),
            2,
            "interval",
        )
        .await
        .expect("insert old rollup");
        let yesterday_local = chrono::DateTime::parse_from_rfc3339(&yesterday)
            .unwrap()
            .with_timezone(&Local)
            .format("%Y-%m-%d")
            .to_string();

        let path = rebuild_daily_index(&db, &vault, &date)
            .await
            .expect("rebuild ok")
            .expect("file written");
        let note = std::fs::read_to_string(&path).expect("note readable");
        assert!(note.contains("type: daily_memory_index"), "frontmatter: {note}");
        assert!(note.contains("## Workstreams Active"), "workstreams section: {note}");
        assert!(note.contains("[[Projects/TaskFlow]]"), "links workstream: {note}");
        assert!(note.contains("## Timeline"), "timeline section: {note}");
        assert!(note.contains("**Window 0**"), "first rollup listed: {note}");

        // Hubs Touched: apps + sites from roll-up JSON, activity from events.
        assert!(note.contains("## Hubs Touched"), "hubs section: {note}");
        assert!(note.contains("[[Apps/code]]"), "app hub linked (lowercased): {note}");
        assert!(note.contains("[[Sites/github.com]]"), "site hub linked: {note}");
        assert!(note.contains("[[Activity/Terminal]]"), "activity hub linked: {note}");

        // Navigation: prev points to the day that has roll-ups; the index link
        // targets the vault index note, not the Memory/Daily folder.
        assert!(
            note.contains(&format!("[[Memory/Daily/{yesterday_local}|Previous]]")),
            "prev nav targets a day with roll-ups: {note}"
        );
        assert!(note.contains("[[index|Daily Index]]"), "index link resolves to a note: {note}");
        assert!(!note.contains("[[Memory/Daily|"), "no folder-target link: {note}");

        // Whole-file replace: a second rebuild must not duplicate anything.
        rebuild_daily_index(&db, &vault, &date).await.expect("rebuild ok");
        let note = std::fs::read_to_string(&path).unwrap();
        assert_eq!(note.matches("## Timeline").count(), 1, "no duplication: {note}");
        assert_eq!(note.matches("## Hubs Touched").count(), 1, "no duplicated hubs: {note}");

        // A date with no roll-ups writes nothing and keeps any existing file.
        let untouched = rebuild_daily_index(&db, &vault, "1999-01-01")
            .await
            .expect("rebuild ok");
        assert!(untouched.is_none(), "empty day writes nothing");
    }
}
