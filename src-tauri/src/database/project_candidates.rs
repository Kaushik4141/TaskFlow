//! Staging table for auto-detected project candidates.
//!
//! Candidates are discovered mechanically at rollup time (no LLM call) by
//! [`detect_project_signals`](crate::wiki::detect_project_signals), persisted to
//! `project_candidates`, and promoted to `wiki_known_projects` when the user says
//! "yes" in the review UI. A rejection writes to `wiki_rejected_projects` instead.
//!
//! The promotion rule — see `should_promote` — requires ≥2 distinct days OR ≥2
//! signal kinds. One sighting across one day with one signal is noise; two
//! independent corroborating signals, or persistence across days, is a project.
//!
//! The `match_key` column is the normalized form of the name (`project_key`),
//! used as the table primary key, so renaming keeps the same row and a name
//! inserted as "SignalFlow" matches the signal that already sees "Signal Flow".
//! `days_seen` is a JSON array of `YYYY-MM-DD` strings — a set, not a counter —
//! because multiple rollups fire per day and counting them would clear the 2-day
//! bar within the first hour.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

use crate::wiki::{ProjectSignal, SignalKind};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCandidate {
    pub match_key: String,
    pub display_name: String,
    /// JSON array of `SignalKind` strings.
    pub signal_kinds: String,
    /// JSON array of short evidence strings (one per kind, ≤160 chars).
    pub evidence: String,
    /// JSON array of `YYYY-MM-DD` strings.
    pub days_seen: String,
    pub event_count: i64,
    pub first_seen: String,
    pub last_seen: String,
    /// `pending` | `asked` | `approved` | `rejected`.
    pub status: String,
    /// ISO timestamp of the last time the user was asked, or NULL.
    pub asked_at: Option<String>,
}

impl ProjectCandidate {
    /// Distinct signal kinds recorded for this candidate.
    ///
    /// Deduplicated: the merge path appends on every sighting, so a candidate
    /// seen in ten editor titles stores ten `editor_workspace` entries. The
    /// promotion rule counts *independent corroboration*, so only distinct kinds
    /// may be counted — without this, one signal repeated twice would promote.
    pub fn signal_kinds_vec(&self) -> Vec<SignalKind> {
        let parsed: Vec<String> = serde_json::from_str(&self.signal_kinds).unwrap_or_default();
        let mut distinct: Vec<SignalKind> = parsed
            .iter()
            .filter_map(|s| signal_kind_from_str(s))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        distinct.sort();
        distinct
    }

    /// Parse the JSON-encoded `days_seen` column into a set of date strings.
    pub fn days_set(&self) -> std::collections::BTreeSet<String> {
        serde_json::from_str(&self.days_seen).unwrap_or_default()
    }

    /// Distinct evidence lines, most recent first, for the review UI.
    pub fn evidence_vec(&self) -> Vec<String> {
        let parsed: Vec<String> = serde_json::from_str(&self.evidence).unwrap_or_default();
        let mut seen = std::collections::BTreeSet::new();
        parsed
            .into_iter()
            .rev()
            .filter(|line| seen.insert(line.clone()))
            .take(5)
            .collect()
    }
}

fn signal_kind_from_str(s: &str) -> Option<SignalKind> {
    match s {
        "source_project" => Some(SignalKind::SourceProject),
        "repo_url" => Some(SignalKind::RepoUrl),
        "editor_workspace" => Some(SignalKind::EditorWorkspace),
        "terminal_repo" => Some(SignalKind::TerminalRepo),
        _ => None,
    }
}

fn signal_kind_to_str(kind: SignalKind) -> &'static str {
    kind.as_str()
}

/// Upsert a set of signals into `project_candidates`, bumping counts and merging
/// signal kinds / evidence / dates as needed. Rejected candidates are skipped
/// entirely — once the user says "no", the row must not be re-minted.
///
/// Returns the set of `match_key`s that crossed the promotion threshold this
/// call, so the async caller can set `status = 'promotable'` (or hold off until
/// the next UI session).
pub async fn upsert_candidates(
    db: &SqlitePool,
    signals: &[ProjectSignal],
    today: &str,
) -> Result<Vec<String>, sqlx::Error> {
    use crate::wiki::project_key;

    let mut promotable = Vec::new();
    for signal in signals {
        let key = project_key(&signal.name);
        if key.is_empty() {
            continue;
        }
        if let Some(existing) = get_candidate(db, &key).await? {
            if existing.status == "rejected" || existing.status == "approved" {
                continue;
            }
            merge_candidate(db, &key, signal, today).await?;
        } else {
            insert_candidate(db, &key, signal, today).await?;
        }
        // Fresh enough to keep: re-read so the promotion check is accurate.
        if let Some(row) = get_candidate(db, &key).await? {
            if row.status == "pending" && should_promote(&row) {
                promotable.push(key);
            }
        }
    }
    Ok(promotable)
}

fn should_promote(c: &ProjectCandidate) -> bool {
    let kinds: Vec<SignalKind> = c.signal_kinds_vec();
    let days: std::collections::BTreeSet<String> = c.days_set();
    // The 2-signal rule is for independent corroboration within one day:
    // an editor workspace AND a repo URL → not noise. The 2-day rule
    // requires persistence but is permissive about signal count.
    kinds.len() >= 2 || days.len() >= 2
}

async fn insert_candidate(
    db: &SqlitePool,
    match_key: &str,
    signal: &ProjectSignal,
    today: &str,
) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    let signal_kinds = serde_json::to_string(&[signal_kind_to_str(signal.kind)])
        .unwrap_or_else(|_| "[]".to_string());
    let evidence = serde_json::to_string(&[&signal.evidence])
        .unwrap_or_else(|_| "[]".to_string());
    let days_seen = serde_json::to_string(&[today]).unwrap_or_else(|_| "[]".to_string());
    sqlx::query(
        r#"
        INSERT INTO project_candidates
            (match_key, display_name, signal_kinds, evidence, days_seen,
             event_count, first_seen, last_seen, status)
        VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6, 'pending')
        "#,
    )
    .bind(match_key)
    .bind(&signal.name)
    .bind(&signal_kinds)
    .bind(&evidence)
    .bind(&days_seen)
    .bind(&now)
    .execute(db)
    .await?;
    Ok(())
}

async fn merge_candidate(
    db: &SqlitePool,
    match_key: &str,
    signal: &ProjectSignal,
    today: &str,
) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    let kind_str = signal_kind_to_str(signal.kind);

    // Keep the display name that the user last chose; this signal can
    // contribute an alternative spelling but not overwrite a rename.
    sqlx::query(
        r#"
        UPDATE project_candidates
        SET last_seen     = ?2,
            event_count   = event_count + 1,
            signal_kinds  = json_insert(signal_kinds,  '$[#]', ?3),
            evidence      = json_insert(evidence, '$[#]', ?4),
            days_seen     = CASE
                WHEN NOT (?5 IN (SELECT value FROM json_each(days_seen)))
                THEN json_insert(days_seen, '$[#]', ?5)
                ELSE days_seen
            END
        WHERE match_key = ?1
        "#,
    )
    .bind(match_key)
    .bind(&now)
    .bind(kind_str)
    .bind(&signal.evidence)
    .bind(today)
    .execute(db)
    .await?;
    Ok(())
}

async fn get_candidate(db: &SqlitePool, match_key: &str) -> Result<Option<ProjectCandidate>, sqlx::Error> {
    sqlx::query_as::<_, ProjectCandidate>(
        r#"
        SELECT match_key, display_name, signal_kinds, evidence, days_seen,
               event_count, first_seen, last_seen, status, asked_at
        FROM project_candidates
        WHERE match_key = ?1
        "#,
    )
    .bind(match_key)
    .fetch_optional(db)
    .await
}

/// List candidates for the review UI. `status` filters to one state; pass
/// `"pending"` for the launch-time review queue. Sorted by recency.
pub async fn list_candidates(
    db: &SqlitePool,
    status: &str,
) -> Result<Vec<ProjectCandidate>, sqlx::Error> {
    sqlx::query_as::<_, ProjectCandidate>(
        r#"
        SELECT match_key, display_name, signal_kinds, evidence, days_seen,
               event_count, first_seen, last_seen, status, asked_at
        FROM project_candidates
        WHERE status = ?1
        ORDER BY last_seen DESC
        "#,
    )
    .bind(status)
    .fetch_all(db)
    .await
}

/// Mark one candidate approved and add its **display name** to the
/// `wiki_known_projects` blob. Returns the new blob so the caller can refresh
/// any cached registry.
///
/// The display name is what lands in the registry, never the `match_key` — the
/// key is normalized (`signalflow`), and writing that would produce a hub page
/// named `signalflow.md` instead of `Signal-Flow.md`.
pub async fn approve_candidate(db: &SqlitePool, match_key: &str) -> Result<String, sqlx::Error> {
    let Some(candidate) = get_candidate(db, match_key).await? else {
        // Nothing to approve; return the registry unchanged.
        return sqlx::query_scalar::<_, String>(
            "SELECT value FROM settings WHERE key = 'wiki_known_projects'",
        )
        .fetch_optional(db)
        .await
        .map(|value| value.unwrap_or_default());
    };
    sqlx::query("UPDATE project_candidates SET status = 'approved' WHERE match_key = ?1")
        .bind(match_key)
        .execute(db)
        .await?;
    add_to_known_projects(db, &candidate.display_name).await
}

/// Persist a user-edited name for a candidate (and touch the status to signal
/// intent).
pub async fn rename_candidate(
    db: &SqlitePool,
    match_key: &str,
    new_display_name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE project_candidates SET display_name = ?2 WHERE match_key = ?1",
    )
    .bind(match_key)
    .bind(new_display_name.trim())
    .execute(db)
    .await?;
    Ok(())
}

/// Mark one candidate rejected — durable, so it is never re-proposed.
pub async fn reject_candidate(db: &SqlitePool, match_key: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE project_candidates SET status = 'rejected' WHERE match_key = ?1",
    )
    .bind(match_key)
    .execute(db)
    .await?;
    Ok(())
}

/// Load every approved and rejected match key as a single `decided` set for
/// `detect_project_signals`, so the extractors never re-survey settled ground.
pub async fn load_decided_set(db: &SqlitePool) -> Result<std::collections::BTreeSet<String>, sqlx::Error> {
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT match_key FROM project_candidates WHERE status IN ('approved', 'rejected')",
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Load the rejected match keys — the same set that the bug-fixed fallback
/// (`infer_project_from_events`) should also suppress so a project the user
/// said "no" to never shows up in hub pages.
pub async fn load_rejected_set(db: &SqlitePool) -> Result<std::collections::BTreeSet<String>, sqlx::Error> {
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT match_key FROM project_candidates WHERE status = 'rejected'",
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Append one project name to the `wiki_known_projects` settings blob, keeping
/// the existing lines. Dedup by normalized key so re-approval doesn't write
/// both "SignalFlow" and "Signal Flow".
async fn add_to_known_projects(db: &SqlitePool, display_name: &str) -> Result<String, sqlx::Error> {
    use crate::wiki::project_key;

    let name = display_name.trim();
    if name.is_empty() {
        return sqlx::query_scalar::<_, String>(
            "SELECT value FROM settings WHERE key = 'wiki_known_projects'",
        )
        .fetch_optional(db)
        .await
        .map(|value| value.unwrap_or_default());
    }

    let current = sqlx::query_scalar::<_, String>(
        "SELECT value FROM settings WHERE key = 'wiki_known_projects'",
    )
    .fetch_optional(db)
    .await?
    .unwrap_or_default();

    let target_key = project_key(name);
    let mut merged: Vec<String> = current
        .split([',', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        // Drop any existing spelling of the same project; the approved display
        // name replaces it.
        .filter(|existing| project_key(existing) != target_key)
        .map(str::to_string)
        .collect();
    merged.push(name.to_string());

    let value = merged.join("\n");
    sqlx::query(
        r#"
        INSERT INTO settings (key, value) VALUES ('wiki_known_projects', ?1)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        "#,
    )
    .bind(&value)
    .execute(db)
    .await?;
    Ok(value)
}
