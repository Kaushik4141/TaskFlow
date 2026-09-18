//! Workstream node writer. Each workstream is the existing Project hub page
//! (`TaskFlow/Projects/<slug>.md`); this writer owns exactly one section on it —
//! `## Activity Timeline` — a newest-first list of roll-up entries. The rest of
//! the page (synthesis, cross-links, backlinks, frontmatter) stays owned by
//! `update_wiki`, so the two writers compose via sequential read-patch-write.
//!
//! Entries are deduped by an HTML-comment marker carrying the roll-up id, so a
//! retried flush replaces rather than duplicates, and the section is capped at
//! [`MAX_ENTRIES`] to bound file size (older entries age out of the node but
//! remain in the `rollups` table).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::Local;

use super::hubs::{self, HubKind};
use crate::database::rollups::Rollup;

pub const TIMELINE_HEADING: &str = "## Activity Timeline";
/// Roll-up entries kept on the workstream node; older ones stay queryable in SQLite.
const MAX_ENTRIES: usize = 50;
/// Insert the timeline section before this anchor when the page lacks one.
const ANCHOR_HEADING: &str = hubs::SYNTHESIS_HEADING;

/// Insert (or idempotently replace) one roll-up's entry at the top of the
/// workstream node's `## Activity Timeline` section. `entry_body` is the
/// roll-up's human-facing markdown with the H1 stripped and `##` demoted.
pub fn upsert_timeline_entry(
    vault: &Path,
    workstream_slug: &str,
    rollup: &Rollup,
    entry_body: &str,
) -> io::Result<PathBuf> {
    let folder = vault.join("TaskFlow").join(HubKind::Project.folder_name());
    fs::create_dir_all(&folder)?;
    let path = folder.join(format!("{workstream_slug}.md"));

    let marker = format!("<!-- rollup:{} -->", rollup.id);
    let banner = entry_context_banner(rollup);
    let new_entry = if banner.is_empty() {
        format!(
            "{marker}\n### {} — {}\n\n{}",
            entry_window_label(rollup),
            rollup.title.trim(),
            entry_body.trim(),
        )
    } else {
        format!(
            "{marker}\n### {} — {}\n\n{}\n\n{}",
            entry_window_label(rollup),
            rollup.title.trim(),
            banner,
            entry_body.trim(),
        )
    };

    let existing = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            let body = bootstrap_page(workstream_slug, rollup, &new_entry);
            fs::write(&path, body)?;
            return Ok(path);
        }
        Err(err) => return Err(err),
    };

    let mut lines: Vec<String> = existing.lines().map(ToString::to_string).collect();

    // Collect the current entries in the timeline section, split on entry
    // markers / `### ` headings, so we can dedupe this roll-up's entry and
    // prepend the new one newest-first.
    let section = extract_section(&lines, TIMELINE_HEADING);
    let mut entries: Vec<String> = section
        .map(|(_, body)| split_entries(&body))
        .unwrap_or_default();
    entries.retain(|entry| !entry.contains(&marker));
    entries.insert(0, new_entry);
    entries.truncate(MAX_ENTRIES);

    let body = entries
        .iter()
        .map(|entry| entry.trim())
        .collect::<Vec<_>>()
        .join("\n\n");
    hubs::replace_section(&mut lines, TIMELINE_HEADING, &body, ANCHOR_HEADING);

    let mut out = lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    fs::write(&path, out)?;
    Ok(path)
}

/// Minimal page for a workstream `update_wiki` hasn't created yet (e.g. the
/// `Inbox` catch-all, which hub resolution never produces on its own).
fn bootstrap_page(slug: &str, rollup: &Rollup, first_entry: &str) -> String {
    let date = chrono::DateTime::parse_from_rfc3339(&rollup.window_end)
        .map(|end| end.with_timezone(&Local).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|_| Local::now().format("%Y-%m-%d").to_string());
    format!(
        "---\ntype: hub_page\nkind: Projects\nslug: {slug}\ntags:\n  - taskflow/project\nlast_touched: {date}\n---\n\n\
         # Projects/{slug}\n\n\
         Rolling workstream node — TaskFlow appends one entry per activity roll-up.\n\n\
         {TIMELINE_HEADING}\n\n{}\n",
        first_entry.trim(),
    )
}

/// Build a contextual metadata banner for an activity timeline entry.
/// Links the entry to the daily index note, referenced application hubs, and site hubs.
/// E.g. `> **Date**: [[Memory/Daily/2026-09-18]] · **Tools**: [[Apps/cursor]] · **Resources**: [[Sites/github.com]]`
pub fn entry_context_banner(rollup: &Rollup) -> String {
    let date = chrono::DateTime::parse_from_rfc3339(&rollup.window_end)
        .ok()
        .map(|end| end.with_timezone(&Local).format("%Y-%m-%d").to_string())
        .or_else(|| {
            chrono::DateTime::parse_from_rfc3339(&rollup.window_start)
                .ok()
                .map(|s| s.with_timezone(&Local).format("%Y-%m-%d").to_string())
        });
    let mut segments = Vec::new();
    if let Some(d) = date {
        segments.push(format!("**Date**: [[Memory/Daily/{d}]]"));
    }
    if let Some(raw) = rollup.apps.as_deref() {
        if let Ok(apps) = serde_json::from_str::<Vec<String>>(raw) {
            let mut app_links = std::collections::BTreeSet::new();
            for app in apps {
                if let Some(slug) = super::links::hub_slug_for_app(&app) {
                    app_links.insert(format!("[[Apps/{slug}]]"));
                }
            }
            if !app_links.is_empty() {
                let joined = app_links.into_iter().collect::<Vec<_>>().join(", ");
                segments.push(format!("**Tools**: {joined}"));
            }
        }
    }
    if let Some(raw) = rollup.resources.as_deref() {
        if let Ok(urls) = serde_json::from_str::<Vec<String>>(raw) {
            let mut site_links = std::collections::BTreeSet::new();
            for url in urls {
                if url.starts_with("http") {
                    if let Some(domain) = super::links::domain_from_url(&url) {
                        site_links.insert(format!("[[Sites/{domain}]]"));
                    }
                }
            }
            if !site_links.is_empty() {
                let joined = site_links.into_iter().collect::<Vec<_>>().join(", ");
                segments.push(format!("**Resources**: {joined}"));
            }
        }
    }
    if segments.is_empty() {
        String::new()
    } else {
        format!("> {}", segments.join(" · "))
    }
}

/// `YYYY-MM-DD HH:MM–HH:MM` in local time, from the roll-up's UTC window.
fn entry_window_label(rollup: &Rollup) -> String {
    let start = chrono::DateTime::parse_from_rfc3339(&rollup.window_start).ok();
    let end = chrono::DateTime::parse_from_rfc3339(&rollup.window_end).ok();
    match (start, end) {
        (Some(start), Some(end)) => {
            let start = start.with_timezone(&Local);
            let end = end.with_timezone(&Local);
            format!(
                "{} {}–{}",
                end.format("%Y-%m-%d"),
                start.format("%H:%M"),
                end.format("%H:%M"),
            )
        }
        _ => rollup.window_end.clone(),
    }
}

/// Locate a `## ` section's body (lines between its heading and the next `## `).
fn extract_section(lines: &[String], heading: &str) -> Option<(usize, Vec<String>)> {
    let start = lines.iter().position(|line| line.trim() == heading)?;
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, line)| line.trim_start().starts_with("## "))
        .map(|(idx, _)| idx)
        .unwrap_or(lines.len());
    Some((start, lines[start + 1..end].to_vec()))
}

/// Split a timeline section body into entry chunks. A new entry starts at a
/// `<!-- rollup:` marker line or, for entries written before markers existed,
/// at a `### ` heading line.
fn split_entries(body: &[String]) -> Vec<String> {
    let mut entries: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for line in body {
        let has_marker = current
            .iter()
            .any(|l| l.trim_start().starts_with("<!-- rollup:"));
        let starts_entry = line.trim_start().starts_with("<!-- rollup:")
            || (line.trim_start().starts_with("### ") && !has_marker);
        if starts_entry && !current.is_empty() {
            entries.push(current.join("\n").trim().to_string());
            current.clear();
        }
        current.push(line.clone());
    }
    if !current.is_empty() {
        let chunk = current.join("\n").trim().to_string();
        if !chunk.is_empty() {
            entries.push(chunk);
        }
    }
    entries.into_iter().filter(|entry| !entry.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_SEQ: AtomicUsize = AtomicUsize::new(0);

    fn fresh_vault(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "taskflow-workstream-test-{name}-{}",
            TEST_SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).ok();
        }
        std::fs::create_dir_all(&dir).expect("create temp vault");
        dir
    }

    fn rollup(id: &str, start: &str, end: &str, title: &str) -> Rollup {
        Rollup {
            id: id.to_string(),
            task_id: "task-1".to_string(),
            window_start: start.to_string(),
            window_end: end.to_string(),
            title: title.to_string(),
            summary_md: "body".to_string(),
            key_points: None,
            apps: None,
            resources: None,
            workstream_slug: Some("TaskFlow".to_string()),
            ai_mode: Some("template".to_string()),
            event_count: 5,
            trigger_kind: Some("interval".to_string()),
            created_at: end.to_string(),
        }
    }

    #[test]
    fn timeline_entry_prepend_and_idempotent_retry() {
        let vault = fresh_vault("prepend");
        let first = rollup("r1", "2026-07-27T09:00:00+00:00", "2026-07-27T09:10:00+00:00", "First window");
        let second = rollup("r2", "2026-07-27T09:10:00+00:00", "2026-07-27T09:20:00+00:00", "Second window");

        // First entry bootstraps the page.
        upsert_timeline_entry(&vault, "TaskFlow", &first, "first body").expect("write ok");
        let path = vault.join("TaskFlow").join("Projects").join("TaskFlow.md");
        let page = std::fs::read_to_string(&path).expect("page exists");
        assert!(page.contains(TIMELINE_HEADING), "timeline section present: {page}");
        assert!(page.contains("First window"), "first entry present: {page}");

        // Newer entry lands on top.
        upsert_timeline_entry(&vault, "TaskFlow", &second, "second body").expect("write ok");
        let page = std::fs::read_to_string(&path).unwrap();
        let second_pos = page.find("Second window").expect("second entry present");
        let first_pos = page.find("First window").expect("first entry still present");
        assert!(second_pos < first_pos, "newest entry first: {page}");

        // A retried flush of the SAME roll-up replaces instead of duplicating.
        upsert_timeline_entry(&vault, "TaskFlow", &first, "first body retry").expect("write ok");
        let page = std::fs::read_to_string(&path).unwrap();
        assert_eq!(page.matches("<!-- rollup:r1 -->").count(), 1, "r1 deduped: {page}");
        assert!(page.contains("first body retry"), "r1 body refreshed: {page}");
    }

    #[test]
    fn entries_with_demoted_headings_survive_repeated_upserts() {
        // Real roll-up bodies contain ####/##### headings (demoted sidecar
        // output). A later upsert re-parses the section — these must not be
        // mistaken for new entries.
        let vault = fresh_vault("demoted");
        let first = rollup("r1", "2026-07-27T09:00:00+00:00", "2026-07-27T09:10:00+00:00", "First window");
        let second = rollup("r2", "2026-07-27T09:10:00+00:00", "2026-07-27T09:20:00+00:00", "Second window");
        let realistic_body = "#### Summary\nWorked on roll-ups.\n\n#### Activity Timeline\n\n##### Cursor\n- editing mod.rs";

        upsert_timeline_entry(&vault, "TaskFlow", &first, realistic_body).expect("write ok");
        upsert_timeline_entry(&vault, "TaskFlow", &second, "second body").expect("write ok");
        let path = vault.join("TaskFlow").join("Projects").join("TaskFlow.md");
        let page = std::fs::read_to_string(&path).unwrap();
        assert!(page.contains("##### Cursor"), "demoted app heading intact: {page}");

        // A third upsert must still find exactly two entries (r1 body didn't
        // shatter at its internal headings).
        let third = rollup("r3", "2026-07-27T09:20:00+00:00", "2026-07-27T09:30:00+00:00", "Third window");
        upsert_timeline_entry(&vault, "TaskFlow", &third, "third body").expect("write ok");
        let page = std::fs::read_to_string(&path).unwrap();
        assert_eq!(page.matches("<!-- rollup:").count(), 3, "three entries exactly: {page}");
        let h3_lines = page
            .lines()
            .filter(|line| line.starts_with("### "))
            .count();
        assert_eq!(h3_lines, 3, "exactly the three entry headings at ### level: {page}");
    }

    #[test]
    fn timeline_preserves_other_sections() {
        let vault = fresh_vault("preserve");
        let folder = vault.join("TaskFlow").join("Projects");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("Inbox.md");
        std::fs::write(
            &path,
            "---\ntype: hub_page\nkind: Projects\nslug: Inbox\n---\n\n# Projects/Inbox\n\n\
             Daily captures related to this project.\n\n## Synthesis\nHand-written notes stay.\n\n\
             ## Recent Daily Notes\n- [[Memory/Daily/2026-07-26]]\n",
        )
        .unwrap();

        let entry = rollup("r9", "2026-07-27T09:00:00+00:00", "2026-07-27T09:10:00+00:00", "Inbox window");
        upsert_timeline_entry(&vault, "Inbox", &entry, "entry body").expect("write ok");
        let page = std::fs::read_to_string(&path).unwrap();
        assert!(page.contains("## Synthesis"), "synthesis preserved: {page}");
        assert!(page.contains("Hand-written notes stay."), "synthesis body preserved: {page}");
        assert!(page.contains("## Recent Daily Notes"), "backlinks preserved: {page}");
        assert!(page.contains(TIMELINE_HEADING), "timeline added: {page}");
        // Timeline lands before the synthesis section (its anchor).
        assert!(
            page.find(TIMELINE_HEADING).unwrap() < page.find("## Synthesis").unwrap(),
            "timeline before synthesis anchor: {page}"
        );
    }

    #[test]
    fn timeline_entry_renders_rich_context_banner_with_wikilinks() {
        let vault = fresh_vault("rich_context");
        let mut r = rollup("r1", "2026-07-27T09:00:00+00:00", "2026-07-27T09:10:00+00:00", "Coding window");
        r.apps = Some(serde_json::to_string(&vec!["Code.exe".to_string(), "Firefox".to_string()]).unwrap());
        r.resources = Some(serde_json::to_string(&vec!["https://github.com/Kaushik4141/TaskFlow".to_string()]).unwrap());

        upsert_timeline_entry(&vault, "TaskFlow", &r, "Implemented feature").expect("write ok");
        let path = vault.join("TaskFlow").join("Projects").join("TaskFlow.md");
        let page = std::fs::read_to_string(&path).expect("page exists");

        assert!(page.contains("tags:\n  - taskflow/project"), "frontmatter tags present: {page}");
        assert!(page.contains("[[Memory/Daily/"), "daily note link present: {page}");
        assert!(page.contains("[[Apps/code]]"), "app link present: {page}");
        assert!(page.contains("[[Apps/firefox]]"), "firefox link present: {page}");
        assert!(page.contains("[[Sites/github.com]]"), "site link present: {page}");
    }
}

