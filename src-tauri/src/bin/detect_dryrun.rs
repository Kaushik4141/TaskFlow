//! Dry-run harness for the project-candidate detector.
//!
//! Replays events already captured in the live SQLite database through the REAL
//! extractors (`wiki::detect_project_signals`) and prints what would be staged,
//! grouped by candidate, with the promotion verdict. Writes nothing.
//!
//! This exists because the GUI build can't complete on this machine (Windows
//! PDB linker limit LNK1318), so "does detection actually work on my data" can't
//! be answered by launching the app. Unit tests prove the logic against synthetic
//! titles; this proves it against real captures.
//!
//! Usage:
//!   cargo run --bin detect_dryrun -- [path-to-taskflow.sqlite] [--days N]
//!
//! Defaults to the standard Windows app-data path and the last 7 days.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use taskflow_lib::{
    database::{events::Event, project_candidates, tasks::Task},
    wiki,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut db_path: Option<String> = None;
    let mut days: i64 = 7;
    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--days" => {
                days = args
                    .get(idx + 1)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(7);
                idx += 2;
            }
            other => {
                db_path = Some(other.to_string());
                idx += 1;
            }
        }
    }
    let db_path = db_path.unwrap_or_else(|| default_db_path().to_string_lossy().into_owned());

    println!("db:   {db_path}");
    println!("days: {days}\n");

    // Read-only: open the existing file, never create one.
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(false)
        .read_only(true);
    let db = sqlx::SqlitePool::connect_with(options).await?;

    let cutoff = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();

    // Pull the window of events, plus the task rows they belong to (source_project
    // is a signal, so the task matters).
    let events: Vec<Event> = sqlx::query_as::<_, Event>(
        r#"
        SELECT id, task_id, event_type, app_name, window_title, content, url,
               content_type, capture_method, is_sanitized, chunk_index,
               relevance, timestamp, created_at
        FROM events
        WHERE timestamp > ?1
        ORDER BY timestamp ASC
        "#,
    )
    .bind(&cutoff)
    .fetch_all(&db)
    .await?;

    println!("events in window: {}", events.len());
    if events.is_empty() {
        println!("\nNothing captured in this window — try a larger --days value.");
        return Ok(());
    }

    // Mirror the real rollup's exclusion set exactly: configured registry names
    // plus anything already approved/rejected.
    let registry: Vec<String> = sqlx::query_scalar::<_, String>(
        "SELECT value FROM settings WHERE key = 'wiki_known_projects'",
    )
    .fetch_optional(&db)
    .await?
    .unwrap_or_default()
    .split([',', '\n'])
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .map(str::to_string)
    .collect();

    let mut decided = project_candidates::load_decided_set(&db).await?;
    decided.extend(registry.iter().map(|n| wiki::project_key(n)));

    println!("registry:  {registry:?}");
    println!("suppressed (registry + already decided): {} key(s)\n", decided.len());

    // Group events by day so the day-set logic is exercised the way the scheduler
    // would exercise it (one detection pass per rollup window, many per day).
    let mut by_day: BTreeMap<String, Vec<Event>> = BTreeMap::new();
    for event in events {
        let day = event
            .timestamp
            .get(..10)
            .unwrap_or("unknown")
            .to_string();
        by_day.entry(day).or_default().push(event);
    }

    // Accumulate per candidate: which kinds, which days, sample evidence.
    struct Agg {
        display: String,
        kinds: BTreeSet<String>,
        days: BTreeSet<String>,
        evidence: Vec<String>,
        hits: usize,
    }
    let mut agg: BTreeMap<String, Agg> = BTreeMap::new();

    for (day, day_events) in &by_day {
        let task = placeholder_task();
        let signals = wiki::detect_project_signals(&task, day_events, &decided);
        for signal in signals {
            let key = wiki::project_key(&signal.name);
            let entry = agg.entry(key).or_insert_with(|| Agg {
                display: signal.name.clone(),
                kinds: BTreeSet::new(),
                days: BTreeSet::new(),
                evidence: Vec::new(),
                hits: 0,
            });
            entry.kinds.insert(signal.kind.label().to_string());
            entry.days.insert(day.clone());
            entry.hits += 1;
            if entry.evidence.len() < 3 && !entry.evidence.contains(&signal.evidence) {
                entry.evidence.push(signal.evidence.clone());
            }
        }
    }

    if agg.is_empty() {
        println!("No candidates detected.");
        println!("Expected when every project you touched is already in the registry.");
        return Ok(());
    }

    // Promotion rule, same as `should_promote`: ≥2 distinct kinds OR ≥2 days.
    let mut ready: Vec<(&String, &Agg)> = Vec::new();
    let mut watching: Vec<(&String, &Agg)> = Vec::new();
    for (key, a) in &agg {
        if a.kinds.len() >= 2 || a.days.len() >= 2 {
            ready.push((key, a));
        } else {
            watching.push((key, a));
        }
    }
    ready.sort_by_key(|(_, a)| std::cmp::Reverse(a.hits));
    watching.sort_by_key(|(_, a)| std::cmp::Reverse(a.hits));

    println!("==== WOULD PROMPT ({}) — meets ≥2 signals or ≥2 days ====\n", ready.len());
    for (key, a) in &ready {
        print_candidate(key, a.display.as_str(), &a.kinds, &a.days, a.hits, &a.evidence);
    }

    println!("\n==== WATCHING ({}) — staged, not yet prompted ====\n", watching.len());
    for (key, a) in &watching {
        print_candidate(key, a.display.as_str(), &a.kinds, &a.days, a.hits, &a.evidence);
    }

    println!("\n(dry run — nothing was written)");
    Ok(())
}

fn default_db_path() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("com.taskflow.desktop")
            .join("taskflow.sqlite")
    }

    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library")
            .join("Application Support")
            .join("com.taskflow.desktop")
            .join("taskflow.sqlite")
    }

    #[cfg(target_os = "linux")]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|home| PathBuf::from(home).join(".local").join("share"))
            })
            .unwrap_or_else(|| PathBuf::from("."))
            .join("com.taskflow.desktop")
            .join("taskflow.sqlite")
    }

    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        PathBuf::from("taskflow.sqlite")
    }
}

fn print_candidate(
    key: &str,
    display: &str,
    kinds: &BTreeSet<String>,
    days: &BTreeSet<String>,
    hits: usize,
    evidence: &[String],
) {
    println!("  {display}   [key: {key}]");
    println!(
        "    signals: {}  |  days: {}  |  sightings: {hits}",
        kinds.iter().cloned().collect::<Vec<_>>().join(", "),
        days.len()
    );
    for line in evidence {
        println!("    · {line}");
    }
    println!();
}

/// The dry run groups by calendar day rather than by task, so there is no single
/// owning task. `source_project` is left None: a ticket-linked project is the one
/// signal that needs a specific task row, and it is already the highest-precision
/// path (an integration told us the name outright), so excluding it here only
/// makes this report more conservative than the real pipeline.
fn placeholder_task() -> Task {
    Task {
        id: "dryrun".to_string(),
        title: "dry run".to_string(),
        description: None,
        source: "memory".to_string(),
        source_id: None,
        source_url: None,
        source_title: None,
        source_body: None,
        source_labels: None,
        source_assignee: None,
        source_priority: None,
        source_project: None,
        source_branch: None,
        status: "done".to_string(),
        started_at: None,
        ended_at: None,
        created_at: "1970-01-01T00:00:00+00:00".to_string(),
    }
}
