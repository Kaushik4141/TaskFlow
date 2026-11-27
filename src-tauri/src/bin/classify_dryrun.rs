//! Dry-run replay & latency benchmark CLI for TaskFlow-TinyNet v1.
//!
//! Replays real captured events from an existing SQLite database (read-only)
//! through the embedded pure-Rust neural network and prints category counts,
//! productivity ratios, and microsecond latency statistics.
//!
//! Usage:
//!   cargo run --bin classify_dryrun -- [path-to-taskflow.sqlite] [--limit N]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use taskflow_lib::{
    classifier::{ActivityClassifier, IngestionDecision},
    database::events::Event,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut db_path: Option<String> = None;
    let mut limit: i64 = 500;
    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--limit" => {
                limit = args.get(idx + 1).and_then(|v| v.parse().ok()).unwrap_or(500);
                idx += 2;
            }
            other => {
                db_path = Some(other.to_string());
                idx += 1;
            }
        }
    }

    let clf = ActivityClassifier::load_embedded()?;
    let status = clf.status();
    println!(
        "TaskFlow-TinyNet loaded: {} (vocab={}, hash={}, sha256={})",
        status.model_version, status.vocab_size, status.hash_buckets, status.sha256_short
    );

    let db_path = db_path.unwrap_or_else(|| default_db_path().to_string_lossy().into_owned());
    if !PathBuf::from(&db_path).exists() {
        println!("\nNo SQLite DB found at {db_path} — running built-in smoke benchmark:\n");
        let samples: [(&str, &str, Option<&str>); 9] = [
            ("Cursor", "window_monitor.rs — TaskFlow — Cursor", None),
            ("Alacritty", "cargo test --lib — Alacritty", None),
            ("Google Chrome", "MIT 6.824 Distributed Systems Lecture 6 - YouTube - Google Chrome", Some("https://www.youtube.com/watch?v=raft")),
            ("Google Chrome", "Funny Dog Tricks #shorts - YouTube - Google Chrome", Some("https://www.youtube.com/shorts/abc")),
            ("Figma", "Mobile Onboarding Prototype — Figma", Some("https://www.figma.com/file/123")),
            ("Zoom", "Zoom Meeting — Engineering Architecture Sync", None),
            ("Microsoft Excel", "Q3_SaaS_Revenue_Model.xlsx - Excel", None),
            ("Google Chrome", "Amazon.com Shopping Cart — Proceed to Checkout - Google Chrome", Some("https://www.amazon.com/gp/cart/view.html")),
            ("Steam", "Counter-Strike 2 — Steam", None),
        ];
        for (app, title, url) in samples {
            let res = clf.classify_window(app, title, url, false);
            println!(
                "  [{:?}] p={:.3} rel={:.2} cat={:<18} ({}us) | {} — {}",
                res.decision, res.productivity_prob, res.relevance_score, res.category.as_str(), res.latency_us, app, title
            );
        }
        return Ok(());
    }

    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(false)
        .read_only(true);
    let db = sqlx::SqlitePool::connect_with(options).await?;

    let known_projects: Vec<String> = sqlx::query_scalar::<_, String>(
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
    clf.refresh_context(None, None, &known_projects);

    let events: Vec<Event> = sqlx::query_as::<_, Event>(
        r#"
        SELECT id, task_id, event_type, app_name, window_title, content, url,
               content_type, capture_method, is_sanitized, chunk_index,
               relevance, timestamp, created_at
        FROM events
        ORDER BY timestamp DESC
        LIMIT ?1
        "#,
    )
    .bind(limit)
    .fetch_all(&db)
    .await?;

    let mut by_cat: BTreeMap<String, usize> = BTreeMap::new();
    let mut dropped = 0usize;
    let mut provisional = 0usize;
    let mut productive = 0usize;

    let t0 = Instant::now();
    for ev in &events {
        let app = ev.app_name.as_deref().unwrap_or("");
        let title = ev.window_title.as_deref().unwrap_or("");
        let res = if ev.content.is_some() {
            clf.classify_deep(app, title, ev.url.as_deref(), ev.content.as_deref(), false)
        } else {
            clf.classify_window(app, title, ev.url.as_deref(), false)
        };
        *by_cat.entry(res.category.as_str().to_string()).or_insert(0) += 1;
        match res.decision {
            IngestionDecision::DropNonProductive => dropped += 1,
            IngestionDecision::CaptureProvisional => provisional += 1,
            IngestionDecision::CaptureProductive => productive += 1,
        }
    }
    let elapsed_us = t0.elapsed().as_micros();
    let avg_us = if !events.is_empty() {
        elapsed_us as f64 / events.len() as f64
    } else {
        0.0
    };

    println!("\nReplayed {} events in {:.2} ms (avg {:.1} us/event)", events.len(), elapsed_us as f64 / 1000.0, avg_us);
    println!("Verdicts: productive={productive} provisional={provisional} dropped={dropped}");
    println!("Category distribution:");
    for (cat, count) in by_cat {
        println!("  {cat:<20} {count}");
    }
    Ok(())
}

fn default_db_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("com.taskflow.desktop").join("taskflow.sqlite");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".local/share/com.taskflow.desktop/taskflow.sqlite");
    }
    PathBuf::from("taskflow.sqlite")
}
