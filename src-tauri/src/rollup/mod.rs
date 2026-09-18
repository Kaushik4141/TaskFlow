//! Event-driven roll-up engine (Pieces-style workstream timeline).
//!
//! Instead of one giant summarize pass when the user stops capture, the
//! [`scheduler`] flushes a small time window of events every ~10 minutes (or on
//! idle / workstream-switch / stop triggers) through [`run_rollup`]. Each
//! roll-up is persisted to the `rollups` table, routed to a workstream node
//! (`TaskFlow/Projects/<slug>.md`) in the vault, and the date-based daily note
//! is regenerated as a thin derived index over the day's roll-ups.
//!
//! AI usage is adaptive: windows with fewer than [`MIN_EVENTS_FOR_AI`] events
//! get a local template summary (no sidecar call), substantial windows use the
//! configured summary mode, and each roll-up receives the previous roll-up's
//! summary as chained context for continuity.

pub mod scheduler;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use chrono::{DateTime, Duration as ChronoDuration, Local, Utc};

use crate::ai_client::{ScoredEvent, SummarizeResponse};
use crate::commands;
use crate::database::{
    events::{self, Event},
    project_candidates,
    rollups::{self, Rollup},
    tasks::Task,
};
use crate::{wiki, AppState};

/// Default wall-clock cadence for the scheduler's interval flush.
pub const DEFAULT_INTERVAL_MINUTES: i64 = 10;
/// Windows smaller than this are captured with a local template — calling an
/// LLM for 1–2 events produces fluff, not signal, and would 50x the AI cost.
pub const MIN_EVENTS_FOR_AI: usize = 3;
/// Settings keys (SQLite `settings` KV).
const KEY_ENABLED: &str = "rollup_enabled";
const KEY_INTERVAL_MINUTES: &str = "rollup_interval_minutes";
const KEY_WATERMARK: &str = "rollup_last_window_end";
const KEY_RAW_RETENTION_HOURS: &str = "raw_capture_retention_hours";
/// Mirrors the clamp in `commands::update_capture_workflow`, which is the only
/// writer of the setting — a hand-edited `settings` row must not widen it.
const DEFAULT_RAW_RETENTION_HOURS: i64 = 72;

/// Summarize every pending event in `(window_start, window_end]` for `task_id`,
/// persist the resulting [`Rollup`], and reflect it into the vault (workstream
/// node + derived daily index). Returns an error when the window is empty, so
/// callers can treat "nothing to roll up" as a skip rather than a failure.
pub async fn run_rollup(
    state: &AppState,
    task_id: &str,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    trigger: &str,
) -> Result<Rollup, String> {
    let task = crate::database::tasks::get_task_by_id(&state.db, task_id)
        .await
        .map_err(|err| err.to_string())?;
    let events = events::get_events_in_window(
        &state.db,
        task_id,
        &window_start.to_rfc3339(),
        &window_end.to_rfc3339(),
    )
    .await
    .map_err(|err| err.to_string())?;
    if events.is_empty() {
        return Err("no events pending in window".to_string());
    }

    let summary_settings = commands::load_summary_settings(&state.db, true).await?;
    let sidecar_ready =
        state.sidecar_ready.load(Ordering::SeqCst) || state.ai_client.is_ready().await;
    if sidecar_ready {
        state.sidecar_ready.store(true, Ordering::SeqCst);
    }

    let window_label = window_label(window_start, window_end);
    let title_with_window = format!("{} ({window_label})", task.title);
    let description = task.description.clone().unwrap_or_default();

    // Chained context: the previous roll-up's summary gives this window
    // continuity (Pieces-style) without re-reading the whole vault.
    let prior_context = rollups::get_latest_rollup(&state.db, task_id)
        .await
        .ok()
        .flatten()
        .map(|previous| chain_context(&previous));

    let ai_mode_label;
    let generated: SummarizeResponse = if events.len() < MIN_EVENTS_FOR_AI {
        ai_mode_label = "template".to_string();
        commands::fallback_summary(
            &task,
            &events,
            Some(format!(
                "Small window ({} event{}) — captured without AI.",
                events.len(),
                if events.len() == 1 { "" } else { "s" }
            )),
        )
    } else if sidecar_ready {
        ai_mode_label = summary_settings.mode.clone();
        if summary_settings.mode == "basic" {
            let relevant_events = events.iter().map(commands::scored_from_event).collect();
            state
                .ai_client
                .summarize(
                    &title_with_window,
                    &description,
                    relevant_events,
                    &summary_settings,
                    prior_context,
                )
                .await
                .unwrap_or_else(|err| {
                    eprintln!("[taskflow:rollup] basic summarization failed: {err}");
                    commands::fallback_summary(
                        &task,
                        &events,
                        Some(commands::FALLBACK_REASON_NO_AI.to_string()),
                    )
                })
        } else {
            let relevant_events: Vec<ScoredEvent> = if task.source == "memory" {
                events.iter().map(commands::scored_from_event).collect()
            } else {
                let filter_result = state
                    .ai_client
                    .filter_events(
                        &task,
                        &description,
                        events.clone().into_iter().map(Into::into).collect(),
                    )
                    .await;
                match filter_result {
                    Ok(filter_response) => filter_response
                        .scored_events
                        .into_iter()
                        .filter(|event| event.included)
                        .collect(),
                    Err(err) => {
                        eprintln!("[taskflow:rollup] AI event filtering failed: {err}");
                        events.iter().map(commands::scored_from_event).collect()
                    }
                }
            };
            state
                .ai_client
                .summarize(
                    &title_with_window,
                    &description,
                    relevant_events,
                    &summary_settings,
                    prior_context,
                )
                .await
                .unwrap_or_else(|err| {
                    eprintln!("[taskflow:rollup] AI summarization failed: {err}");
                    commands::fallback_summary(
                        &task,
                        &events,
                        Some(commands::FALLBACK_REASON_NO_AI.to_string()),
                    )
                })
        }
    } else {
        ai_mode_label = "template".to_string();
        commands::fallback_summary(
            &task,
            &events,
            Some(commands::FALLBACK_REASON_NO_AI.to_string()),
        )
    };

    // The LLM may append machine-readable ```hub:<Folder>/<slug> synthesis
    // blocks; parse them out for the wiki upsert and keep the human-facing
    // entry markdown clean (identical protocol to generate_documentation).
    let hub_synthesis = wiki::parse_hub_synthesis(&generated.markdown);
    let clean_markdown = if hub_synthesis.is_empty() {
        generated.markdown.clone()
    } else {
        wiki::strip_hub_blocks(&generated.markdown)
    };

    let known_projects = commands::load_known_projects(&state.db).await;

    // Project candidate detection. Purely mechanical (no LLM), and deliberately
    // non-fatal: a detection failure must never fail the roll-up, whose row in
    // SQLite is the source of truth for everything downstream.
    //
    // Runs on every window rather than only on days with new activity because
    // the raw `events.content` this reads is pruned on the retention clock
    // (72h default) — evidence not captured now cannot be recovered later.
    //
    // Ordered BEFORE route_workstream: it also refreshes the rejected-project
    // veto that both the router and update_wiki consult.
    detect_and_stage_candidates(state, &task, &events, &known_projects).await;

    let workstream_slug = route_workstream(&task, &events, &known_projects);
    let title = derive_title(&generated, &events);

    let apps: Vec<String> = events
        .iter()
        .filter_map(|event| event.app_name.clone())
        .filter(|app| !app.trim().is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let rollup = rollups::insert_rollup(
        &state.db,
        task_id,
        &window_start.to_rfc3339(),
        &window_end.to_rfc3339(),
        &title,
        &clean_markdown,
        &generated.key_points,
        &apps,
        &generated.resources,
        Some(&workstream_slug),
        Some(&ai_mode_label),
        events.len() as i64,
        trigger,
    )
    .await
    .map_err(|err| err.to_string())?;

    // Vault reflection. Only memory tasks write to the wiki (same rule as the
    // legacy daily-note flow); failures here never fail the roll-up itself —
    // the row in SQLite is the source of truth and the index can be rebuilt.
    if task.source == "memory" {
        if let Ok(settings) = commands::load_obsidian_settings(&state.db).await {
            if settings.enabled && !settings.vault_path.trim().is_empty() {
                let vault = PathBuf::from(&settings.vault_path);
                let date = window_end.with_timezone(&Local).format("%Y-%m-%d").to_string();
                let events_for_wiki = events.clone();
                let summary = generated.summary.clone();
                let synthesis = hub_synthesis.clone();
                let known = known_projects.clone();
                let entry_body = rollup_entry_markdown(&clean_markdown);
                let rollup_for_entry = rollup.clone();
                let vault_for_index = vault.clone();
                let db = state.db.clone();
                let task_for_wiki = task.clone();
                let date_for_index = date.clone();
                let slug = workstream_slug.clone();
                let write_result = tokio::task::spawn_blocking(move || {
                    let report = wiki::update_wiki(
                        &vault,
                        &date,
                        &task_for_wiki,
                        &events_for_wiki,
                        &summary,
                        &synthesis,
                        &known,
                    )
                    .map_err(|err| err.to_string())?;
                    wiki::workstream::upsert_timeline_entry(
                        &vault,
                        &slug,
                        &rollup_for_entry,
                        &entry_body,
                    )
                    .map_err(|err| err.to_string())?;
                    Ok::<_, String>(report)
                })
                .await;
                match write_result {
                    Ok(Ok(report)) => {
                        // Registry cleanup deleted dust workstream pages: point
                        // their roll-ups at Inbox and rebuild the affected days
                        // so the derived index stops linking the deleted pages.
                        if !report.deleted_project_slugs.is_empty() {
                            match rollups::reassign_workstreams(
                                &db,
                                &report.deleted_project_slugs,
                                "Inbox",
                            )
                            .await
                            {
                                Ok(windows) => {
                                    let dates: std::collections::BTreeSet<String> = windows
                                        .iter()
                                        .filter_map(|w| {
                                            chrono::DateTime::parse_from_rfc3339(w)
                                                .ok()
                                                .map(|ts| {
                                                    ts.with_timezone(&Local)
                                                        .format("%Y-%m-%d")
                                                        .to_string()
                                                })
                                        })
                                        .collect();
                                    for date in dates {
                                        if let Err(err) = wiki::daily_index::rebuild_daily_index(
                                            &db,
                                            &vault_for_index,
                                            &date,
                                        )
                                        .await
                                        {
                                            eprintln!(
                                                "[taskflow:rollup] daily index rebuild after cleanup failed: {err}"
                                            );
                                        }
                                    }
                                }
                                Err(err) => eprintln!(
                                    "[taskflow:rollup] workstream reassignment failed: {err}"
                                ),
                            }
                        }
                    }
                    Ok(Err(err)) => eprintln!("[taskflow:rollup] vault write failed: {err}"),
                    Err(err) => eprintln!("[taskflow:rollup] vault task panicked: {err}"),
                }
                if let Err(err) =
                    wiki::daily_index::rebuild_daily_index(&db, &vault_for_index, &date_for_index)
                        .await
                {
                    eprintln!("[taskflow:rollup] daily index rebuild failed: {err}");
                }
            }
        }
    }

    Ok(rollup)
}

/// Flush whatever is pending since the last watermark for `task_id` right now
/// (used by stop-capture and any future "roll up now" action). Returns `Ok(None)`
/// when roll-ups are disabled or nothing is pending.
pub async fn flush_pending(
    state: &AppState,
    task_id: &str,
    trigger: &str,
) -> Result<Option<Rollup>, String> {
    if !rollups_enabled(&state.db).await {
        return Ok(None);
    }
    let now = Utc::now();
    let interval = rollup_interval_minutes(&state.db).await;
    let watermark = get_watermark(&state.db)
        .await
        .unwrap_or_else(|| now - ChronoDuration::minutes(interval))
        .min(now);
    match run_rollup(state, task_id, watermark, now, trigger).await {
        Ok(rollup) => {
            set_watermark(&state.db, now).await?;
            Ok(Some(rollup))
        }
        Err(err) if err == "no events pending in window" => {
            // Nothing pending — still slide the watermark forward so the next
            // window doesn't include this idle gap.
            set_watermark(&state.db, now).await?;
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

/// Detect project candidates in this window and stage them for user review.
///
/// Names already in the registry are excluded (they're settled), as are names
/// the user approved or rejected earlier — a rejection is durable, so a project
/// you said "no" to is never proposed again.
///
/// Every failure path here is logged and swallowed: candidate staging is a
/// convenience layered on top of the roll-up, never a precondition for it.
async fn detect_and_stage_candidates(
    state: &AppState,
    task: &Task,
    events: &[Event],
    known_projects: &[String],
) {
    // Refresh the veto cache the synchronous wiki paths read. MUST happen before
    // route_workstream and update_wiki, both of which resolve projects and would
    // otherwise mint a page for a name the user already rejected.
    commands::refresh_project_veto(&state.db).await;

    let mut decided = match project_candidates::load_decided_set(&state.db).await {
        Ok(set) => set,
        Err(err) => {
            eprintln!("[taskflow:candidates] could not load decided set: {err}");
            return;
        }
    };
    // Configured projects are already resolved by the registry; proposing them
    // again would ask the user to confirm what they already told us.
    decided.extend(known_projects.iter().map(|name| wiki::project_key(name)));

    let signals = wiki::detect_project_signals(task, events, &decided);
    if signals.is_empty() {
        return;
    }
    let today = Local::now().format("%Y-%m-%d").to_string();
    match project_candidates::upsert_candidates(&state.db, &signals, &today).await {
        Ok(promotable) if !promotable.is_empty() => {
            eprintln!(
                "[taskflow:candidates] {} candidate(s) ready for review: {}",
                promotable.len(),
                promotable.join(", ")
            );
        }
        Ok(_) => {}
        Err(err) => eprintln!("[taskflow:candidates] staging failed: {err}"),
    }
}

/// Route a window of events to its workstream node slug. Registry-first project
/// resolution (same as the hub system), then the leading-word heuristic baked
/// into `resolve_projects`, then a catch-all `Inbox` node so no roll-up is lost.
pub fn route_workstream(task: &Task, events: &[Event], known_projects: &[String]) -> String {
    wiki::resolve_projects(task, events, known_projects)
        .into_iter()
        .next()
        .unwrap_or_else(|| "Inbox".to_string())
}

pub async fn rollups_enabled(db: &sqlx::SqlitePool) -> bool {
    get_setting(db, KEY_ENABLED)
        .await
        .map(|value| value != "false")
        .unwrap_or(true)
}

pub async fn rollup_interval_minutes(db: &sqlx::SqlitePool) -> i64 {
    get_setting(db, KEY_INTERVAL_MINUTES)
        .await
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(DEFAULT_INTERVAL_MINUTES)
        .clamp(2, 120)
}

/// How long raw captured `events.content` may be retained before the scheduler
/// nulls it out. Clamped to the same `1..=168` range the Settings UI writes.
pub async fn raw_retention_hours(db: &sqlx::SqlitePool) -> i64 {
    get_setting(db, KEY_RAW_RETENTION_HOURS)
        .await
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(DEFAULT_RAW_RETENTION_HOURS)
        .clamp(1, 168)
}

pub async fn get_watermark(db: &sqlx::SqlitePool) -> Option<DateTime<Utc>> {
    let raw = get_setting(db, KEY_WATERMARK).await?;
    DateTime::parse_from_rfc3339(&raw)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

pub async fn set_watermark(db: &sqlx::SqlitePool, ts: DateTime<Utc>) -> Result<(), String> {
    sqlx::query(
        r#"
        INSERT INTO settings (key, value)
        VALUES (?1, ?2)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        "#,
    )
    .bind(KEY_WATERMARK)
    .bind(ts.to_rfc3339())
    .execute(db)
    .await
    .map_err(|err| err.to_string())?;
    Ok(())
}

async fn get_setting(db: &sqlx::SqlitePool, key: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?1")
        .bind(key)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

/// `HH:MM–HH:MM` in local time, for titles and entry headings.
pub fn window_label(window_start: DateTime<Utc>, window_end: DateTime<Utc>) -> String {
    let start = window_start.with_timezone(&Local);
    let end = window_end.with_timezone(&Local);
    format!("{}–{}", start.format("%H:%M"), end.format("%H:%M"))
}

fn chain_context(previous: &Rollup) -> String {
    let mut context = format!(
        "# Previous activity roll-up\n\nWorkstream: {}\nTitle: {}\n\n",
        previous.workstream_slug.as_deref().unwrap_or("Inbox"),
        previous.title,
    );
    let body: String = previous.summary_md.chars().take(600).collect();
    context.push_str(&body);
    context
}

/// Build the markdown body of one workstream-timeline entry from a roll-up's
/// clean summary markdown: drop the H1 (the entry heading carries the title)
/// and demote every remaining heading by two levels (`##`→`####`,
/// `###`→`#####`, `####`→`######`) so nothing in the body collides with the
/// `###` entry headings the timeline parser splits on.
fn rollup_entry_markdown(clean_markdown: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in clean_markdown.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("# ") {
            continue;
        }
        // Longest prefix first so `###` doesn't match the `##` arm.
        let demoted = trimmed
            .strip_prefix("#### ")
            .map(|rest| format!("###### {rest}"))
            .or_else(|| trimmed.strip_prefix("### ").map(|rest| format!("##### {rest}")))
            .or_else(|| trimmed.strip_prefix("## ").map(|rest| format!("#### {rest}")));
        match demoted {
            Some(heading) => out.push(heading),
            None => out.push(line.to_string()),
        }
    }
    // Collapse runs of blank lines left by stripping.
    let mut cleaned: Vec<String> = Vec::with_capacity(out.len());
    let mut prev_blank = false;
    for line in out {
        let blank = line.trim().is_empty();
        if blank && prev_blank {
            continue;
        }
        prev_blank = blank;
        cleaned.push(line);
    }
    cleaned.join("\n").trim().to_string()
}

/// Human title for a roll-up. Prefer the first substantive LLM key point
/// (short, topical); fall back to "DominantApp — top window/domain".
fn derive_title(generated: &SummarizeResponse, events: &[Event]) -> String {
    let substantive = generated.key_points.iter().find(|point| {
        let trimmed = point.trim();
        trimmed.len() >= 12
            && !trimmed.starts_with("Local events")
            && !trimmed.starts_with("AI ")
            && !trimmed.contains("unavailable")
            && !trimmed.contains("without AI")
    });
    if let Some(point) = substantive {
        return truncate_words(point.trim(), 70);
    }

    let mut app_counts: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for event in events {
        if let Some(app) = event.app_name.as_deref().filter(|app| !app.trim().is_empty()) {
            *app_counts.entry(app.to_string()).or_default() += 1;
        }
    }
    let dominant_app = app_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(app, _)| app.trim_end_matches(".exe").to_string())
        .unwrap_or_else(|| "Activity".to_string());

    let label = events
        .iter()
        .find_map(|event| {
            event
                .window_title
                .as_deref()
                .filter(|title| !title.trim().is_empty())
                .map(|title| truncate_words(title.trim(), 40))
                .or_else(|| {
                    event
                        .url
                        .as_deref()
                        .filter(|url| url.starts_with("http"))
                        .and_then(wiki::domain_from_url)
                })
        })
        .unwrap_or_else(|| format!("{} events", events.len()));

    format!("{dominant_app} — {label}")
}

fn truncate_words(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let truncated: String = value.chars().take(max_chars).collect();
    let cut = truncated
        .rfind(char::is_whitespace)
        .map(|idx| &truncated[..idx])
        .unwrap_or(&truncated);
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::rollup_entry_markdown;

    /// The sidecar's real output shape (`# title`, `##` sections, per-app `###`
    /// subsections) must never leak a `###` line into the entry body — the
    /// workstream timeline parser splits entries on exactly that prefix.
    #[test]
    fn entry_markdown_strips_h1_and_demotes_all_headings() {
        let raw = "# Memory Capture - 2026-07-27\n\n**Duration:** 10m\n\n## Summary\nDid things.\n\n\
                   ## Activity Timeline\n\n### Cursor\n- editing rollup/mod.rs\n\n\
                   ## Resources Referenced\n- https://x.com\n";
        let body = rollup_entry_markdown(raw);
        assert!(
            !body.lines().any(|line| line.trim_start().starts_with("### ")
                || line.trim_start().starts_with("# ")),
            "no H1 or ### lines survive demotion: {body}"
        );
        assert!(body.contains("#### Summary"), "## demoted to ####: {body}");
        assert!(
            body.contains("##### Cursor"),
            "### app subsection demoted to #####: {body}"
        );
        assert!(body.contains("- editing rollup/mod.rs"), "bullets preserved: {body}");
    }
}
