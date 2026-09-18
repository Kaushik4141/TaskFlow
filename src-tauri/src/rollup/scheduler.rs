//! Background roll-up scheduler. One tokio task, spawned at app start alongside
//! the capture threads, flushing the pending event window when any trigger fires:
//!
//! - **interval** — the window reached `rollup_interval_minutes` (default 10)
//! - **idle** — activity exists but the user has been idle ≥ 5 minutes (wrap up
//!   the window instead of letting it dangle)
//! - **context_switch** — the pending window's dominant workstream differs from
//!   the last roll-up's, so the previous stretch of work gets its own entry
//!
//! The window watermark lives in the settings KV (`rollup_last_window_end`) so
//! restarts resume without re- or double-summarizing.
//!
//! The same loop also enforces `raw_capture_retention_hours`, since it already
//! owns the only clock that knows how far the roll-up engine has consumed.

use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use tauri::{AppHandle, Emitter};

use crate::capture::window_monitor;
use crate::commands;
use crate::database::{events, rollups, tasks};
use crate::AppState;

const TICK_SECONDS: u64 = 30;
const IDLE_FLUSH_AFTER: Duration = Duration::from_secs(300);
const MIN_FLUSH_WINDOW_MINUTES: i64 = 2;
/// Retention is a slow-moving promise; checking it every 30s tick would burn a
/// full table scan for nothing.
const PRUNE_EVERY: Duration = Duration::from_secs(3600);

pub fn start(app: AppHandle, state: AppState) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(TICK_SECONDS));
        // Prune once at startup: a long-closed app is exactly when raw content
        // has aged past the retention window with nobody watching.
        let mut next_prune = tokio::time::Instant::now();
        loop {
            ticker.tick().await;
            if let Err(err) = tick(&app, &state).await {
                eprintln!("[taskflow:rollup] tick failed: {err}");
            }
            if tokio::time::Instant::now() >= next_prune {
                next_prune = tokio::time::Instant::now() + PRUNE_EVERY;
                if let Err(err) = prune_raw_content(&state).await {
                    eprintln!("[taskflow:rollup] retention prune failed: {err}");
                }
            }
        }
    });
}

/// Enforce `raw_capture_retention_hours` by discarding raw `events.content`
/// older than the cutoff. Runs regardless of the active task or whether
/// roll-ups are enabled — it is a privacy control, not a summarization step.
async fn prune_raw_content(state: &AppState) -> Result<(), String> {
    let retention_hours = super::raw_retention_hours(&state.db).await;
    let rollups_on = super::rollups_enabled(&state.db).await;
    let watermark = super::get_watermark(&state.db).await;

    let Some(cutoff) = retention_cutoff(Utc::now(), retention_hours, watermark, rollups_on) else {
        return Ok(());
    };

    let cleared = events::clear_content_before(&state.db, &cutoff.to_rfc3339())
        .await
        .map_err(|err| err.to_string())?;
    if cleared > 0 {
        println!(
            "[taskflow:rollup] retention: cleared raw content from {cleared} event(s) older than {retention_hours}h"
        );
    }
    Ok(())
}

/// The timestamp at or before which raw content may be discarded, or `None`
/// when nothing may be touched yet.
///
/// Two rules, and they pull in opposite directions:
///
/// 1. Never discard content the roll-up engine has not summarized yet, or a
///    short retention setting would silently starve the memory spine. Hence
///    the clamp to the watermark while roll-ups are running.
/// 2. But when roll-ups are switched off there is no future consumer, so a
///    frozen watermark must not freeze retention along with it — that would
///    turn the privacy setting back into the lie this exists to fix.
fn retention_cutoff(
    now: DateTime<Utc>,
    retention_hours: i64,
    watermark: Option<DateTime<Utc>>,
    rollups_enabled: bool,
) -> Option<DateTime<Utc>> {
    let age_cutoff = now - ChronoDuration::hours(retention_hours);
    if !rollups_enabled {
        return Some(age_cutoff);
    }
    // Roll-ups on but nothing consumed yet: hold off. The next tick with any
    // activity sets a watermark, so this resolves itself within a tick.
    Some(age_cutoff.min(watermark?))
}

async fn tick(app: &AppHandle, state: &AppState) -> Result<(), String> {
    if !super::rollups_enabled(&state.db).await {
        return Ok(());
    }
    let task_id = state
        .active_task_id
        .lock()
        .map_err(|_| "active task state lock poisoned".to_string())?
        .clone();
    let Some(task_id) = task_id else {
        return Ok(());
    };
    let task = tasks::get_task_by_id(&state.db, &task_id)
        .await
        .map_err(|err| err.to_string())?;

    let now = Utc::now();
    let interval_minutes = super::rollup_interval_minutes(&state.db).await;
    let watermark = super::get_watermark(&state.db)
        .await
        .or_else(|| {
            task.started_at
                .as_deref()
                .and_then(|started| chrono::DateTime::parse_from_rfc3339(started).ok())
                .map(|started| started.with_timezone(&Utc))
        })
        .unwrap_or(now - ChronoDuration::minutes(interval_minutes))
        .min(now);

    let pending_events = events::get_events_in_window(
        &state.db,
        &task_id,
        &watermark.to_rfc3339(),
        &now.to_rfc3339(),
    )
    .await
    .map_err(|err| err.to_string())?;

    if pending_events.is_empty() {
        // Slide the watermark forward so idle gaps don't accumulate into one
        // giant window the next time activity appears.
        return super::set_watermark(&state.db, now).await;
    }

    let pending = now - watermark;
    let interval_due = pending >= ChronoDuration::minutes(interval_minutes);
    let idle_flush = pending >= ChronoDuration::minutes(MIN_FLUSH_WINDOW_MINUTES)
        && window_monitor::user_idle_for(IDLE_FLUSH_AFTER);
    let context_switch = if pending >= ChronoDuration::minutes(MIN_FLUSH_WINDOW_MINUTES) {
        let known_projects = commands::load_known_projects(&state.db).await;
        let current = super::route_workstream(&task, &pending_events, &known_projects);
        rollups::get_latest_rollup(&state.db, &task_id)
            .await
            .ok()
            .flatten()
            .and_then(|rollup| rollup.workstream_slug)
            .is_some_and(|previous| previous != current)
    } else {
        false
    };

    let trigger = if interval_due {
        "interval"
    } else if idle_flush {
        "idle"
    } else if context_switch {
        "context_switch"
    } else {
        return Ok(());
    };

    let rollup = super::run_rollup(state, &task_id, watermark, now, trigger).await?;
    super::set_watermark(&state.db, now).await?;
    let _ = app.emit("rollup-created", &rollup);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::retention_cutoff;
    use crate::database::events::clear_content_before;
    use crate::database::schema::run_migrations;
    use chrono::{DateTime, Duration as ChronoDuration, Utc};
    use sqlx::SqlitePool;

    fn at(rfc3339: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(rfc3339)
            .expect("valid rfc3339")
            .with_timezone(&Utc)
    }

    /// The whole point of the clamp: a 1-hour retention must not eat events the
    /// roll-up engine hasn't summarized yet.
    #[test]
    fn cutoff_clamps_to_watermark_when_rollups_lag() {
        let now = at("2026-07-29T12:00:00+00:00");
        let watermark = at("2026-07-29T06:00:00+00:00");
        let cutoff = retention_cutoff(now, 1, Some(watermark), true).expect("cutoff");
        assert_eq!(
            cutoff, watermark,
            "cutoff pinned to the watermark, not now-1h"
        );
    }

    /// When the engine is caught up, retention age is the binding constraint.
    #[test]
    fn cutoff_uses_age_when_watermark_is_ahead() {
        let now = at("2026-07-29T12:00:00+00:00");
        let watermark = at("2026-07-29T11:59:00+00:00");
        let cutoff = retention_cutoff(now, 72, Some(watermark), true).expect("cutoff");
        assert_eq!(cutoff, now - ChronoDuration::hours(72));
    }

    /// Roll-ups on but nothing consumed yet — hold off rather than guess.
    #[test]
    fn cutoff_is_none_without_watermark_while_rollups_enabled() {
        let now = at("2026-07-29T12:00:00+00:00");
        assert!(retention_cutoff(now, 72, None, true).is_none());
    }

    /// Roll-ups off means no future consumer, so a stale (or absent) watermark
    /// must not freeze the privacy control.
    #[test]
    fn cutoff_ignores_watermark_when_rollups_disabled() {
        let now = at("2026-07-29T12:00:00+00:00");
        let stale = at("2026-05-01T00:00:00+00:00");
        let expected = now - ChronoDuration::hours(72);
        assert_eq!(
            retention_cutoff(now, 72, Some(stale), false).expect("cutoff"),
            expected
        );
        assert_eq!(
            retention_cutoff(now, 72, None, false).expect("cutoff"),
            expected
        );
    }

    /// Regression guard for the format trap: `events.timestamp` is RFC-3339
    /// with a `'T'`, while SQLite's `datetime('now', ...)` yields a space-
    /// separated string. Because `'T'` (0x54) sorts above `' '` (0x20), the two
    /// formats only compare correctly when the date halves differ — an event
    /// *earlier on the same day* as the cutoff is silently skipped. Fixed
    /// timestamps keep this deterministic instead of depending on where the
    /// wall clock happens to sit relative to midnight.
    #[tokio::test]
    async fn prune_matches_rfc3339_timestamps_not_sqlite_datetime() {
        let db = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");
        run_migrations(&db).await.expect("migrations");
        sqlx::query(
            "INSERT INTO tasks (id, title, source, source_id, status, created_at) \
             VALUES ('task-1', 'T', 'memory', 'm-1', 'active', '2026-07-01T00:00:00+00:00')",
        )
        .execute(&db)
        .await
        .expect("seed task");

        // Same calendar day as the cutoff, six hours earlier: the exact shape
        // the naive comparison gets wrong.
        for (id, ts) in [
            ("ev-old", "2026-07-26T05:00:00+00:00"),
            ("ev-fresh", "2026-07-26T20:00:00+00:00"),
        ] {
            sqlx::query(
                "INSERT INTO events (id, task_id, event_type, content, timestamp) \
                 VALUES (?1, 'task-1', 'window_switch', 'secret screen text', ?2)",
            )
            .bind(id)
            .bind(ts)
            .execute(&db)
            .await
            .expect("seed event");
        }

        // Proof the trap is real, not theoretical: the SQLite-formatted bound
        // finds nothing to prune even though ev-old is six hours past it.
        let sql_bound_hits: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE content IS NOT NULL \
             AND timestamp <= '2026-07-26 12:00:00'",
        )
        .fetch_one(&db)
        .await
        .expect("count");
        assert_eq!(
            sql_bound_hits, 0,
            "space-separated bound silently matches nothing against RFC-3339"
        );

        // The Rust-built bound, same instant, correct result.
        let cleared = clear_content_before(&db, "2026-07-26T12:00:00+00:00")
            .await
            .expect("prune");
        assert_eq!(cleared, 1, "only the aged event is cleared");

        let old_content: Option<String> =
            sqlx::query_scalar("SELECT content FROM events WHERE id = 'ev-old'")
                .fetch_one(&db)
                .await
                .expect("fetch old");
        assert!(old_content.is_none(), "aged raw content discarded");

        // The row itself survives, so the event feed keeps rendering it.
        let old_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE id = 'ev-old'")
            .fetch_one(&db)
            .await
            .expect("count old");
        assert_eq!(old_rows, 1, "row retained, only content nulled");

        let fresh_content: Option<String> =
            sqlx::query_scalar("SELECT content FROM events WHERE id = 'ev-fresh'")
                .fetch_one(&db)
                .await
                .expect("fetch fresh");
        assert_eq!(fresh_content.as_deref(), Some("secret screen text"));
    }
}
