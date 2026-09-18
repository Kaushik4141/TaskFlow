use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::Duration,
};

use chrono::Utc;
use reqwest::Client;
use serde_json::json;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, State};

use crate::{
    ai_client::ScoredEvent,
    capture::privacy::PrivacyFilter,
    database::{
        documentation::{self, Documentation},
        events::{self, Event, Note},
        tasks::{self, Task},
    },
    integrations::{
        github::GitHubClient, jira::JiraClient, linear::LinearClient, store as integration_store,
        Integration, TestResult, Ticket,
    },
    AppState, CaptureSettings, WorkflowMode,
};
#[allow(unused_imports)]
use crate::wiki::hubs::HubKind;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureStats {
    total_events: u32,
    with_content: u32,
    with_url: u32,
    title_only: u32,
    excluded: u32,
    by_app: HashMap<String, u32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummarySettings {
    pub mode: String,
    pub cloud_base_url: String,
    pub cloud_api_key: String,
    pub cloud_model: String,
    pub ollama_model: String,
    pub ollama_url: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudModels {
    pub models: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaStatus {
    pub is_running: bool,
    pub available_models: Vec<String>,
    pub has_recommended_model: bool,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    task_id: String,
    task_title: String,
    matched_snippet: String,
    relevance_score: f32,
    created_at: String,
    source: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStats {
    total_tasks: u32,
    total_time_secs: u64,
    tasks_this_week: u32,
    time_this_week_secs: u64,
    most_used_apps: Vec<(String, u32)>,
    tasks_by_source: HashMap<String, u32>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsidianSyncResult {
    path: String,
}

#[tauri::command]
pub async fn create_task(
    state: State<'_, AppState>,
    title: String,
    description: Option<String>,
    source: String,
) -> Result<Task, String> {
    let task = tasks::create_task(&state.db, title, description, source)
        .await
        .map_err(|err| err.to_string())?;
    set_active_task_id(&state, Some(task.id.clone()))?;
    Ok(task)
}

#[tauri::command]
pub async fn get_all_tasks(state: State<'_, AppState>) -> Result<Vec<Task>, String> {
    tasks::get_all_tasks(&state.db)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn get_active_task(state: State<'_, AppState>) -> Result<Option<Task>, String> {
    tasks::get_active_task(&state.db)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn start_task(state: State<'_, AppState>, id: String) -> Result<Task, String> {
    let task = tasks::update_task_status(&state.db, id, "active".to_string())
        .await
        .map_err(|err| err.to_string())?;
    set_active_task_id(&state, Some(task.id.clone()))?;
    Ok(task)
}

#[tauri::command]
pub async fn stop_task(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<Task, String> {
    let task = tasks::end_task(&state.db, id)
        .await
        .map_err(|err| err.to_string())?;
    set_active_task_id(&state, None)?;
    // Stop = flush: roll up whatever was captured since the last window
    // boundary so the final stretch lands in the workstream timeline now,
    // rather than waiting for the scheduler. A skip (no pending events, or
    // roll-ups disabled) never fails the stop itself.
    match crate::rollup::flush_pending(&state, &task.id, "stop").await {
        Ok(Some(rollup)) => {
            let _ = app.emit("rollup-created", &rollup);
        }
        Ok(None) => {}
        Err(err) => eprintln!("[taskflow:rollup] stop-flush failed: {err}"),
    }
    Ok(task)
}

#[tauri::command]
pub async fn get_rollups(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<Vec<crate::database::rollups::Rollup>, String> {
    crate::database::rollups::get_rollups_for_task(&state.db, &task_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn get_task_events(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<Vec<Event>, String> {
    events::get_recent_events(&state.db, task_id, 50)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn add_note(
    state: State<'_, AppState>,
    task_id: String,
    content: String,
) -> Result<Note, String> {
    events::add_note(&state.db, task_id, content, Some("TaskFlow".to_string()))
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn generate_documentation(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<Documentation, String> {
    let task = tasks::get_task_by_id(&state.db, &task_id)
        .await
        .map_err(|err| err.to_string())?;
    let events = events::get_events_for_task(&state.db, task_id.clone())
        .await
        .map_err(|err| err.to_string())?;
    let description = task.description.clone().unwrap_or_default();
    let summary_settings = load_summary_settings(&state.db, true).await?;

    let sidecar_ready =
        state.sidecar_ready.load(Ordering::SeqCst) || state.ai_client.is_ready().await;
    if sidecar_ready {
        state.sidecar_ready.store(true, Ordering::SeqCst);
    }

    // Phase 2b: load prior Memory Tree context so the summarizer can maintain
    // continuity with previously captured days and hub pages. Only memory tasks
    // contribute to / consume the wiki, so we skip the work otherwise.
    let prior_context: Option<String> = if task.source == "memory" {
        let today = task
            .started_at
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string());
        let prior_dates = tasks::recent_daily_dates(&state.db, &today, 3)
            .await
            .unwrap_or_default();
        if prior_dates.is_empty() {
            None
        } else if let Ok(settings) = load_obsidian_settings(&state.db).await {
            if settings.enabled && !settings.vault_path.trim().is_empty() {
                let vault = std::path::PathBuf::from(&settings.vault_path);
                let known_projects = load_known_projects(&state.db).await;
                crate::wiki::load_prior_context(
                    &vault,
                    &today,
                    &task,
                    &events,
                    &prior_dates,
                    &known_projects,
                )
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let mut generated = if sidecar_ready {
        if summary_settings.mode == "basic" {
            let relevant_events = events.iter().map(scored_from_event).collect();
            state
                .ai_client
                .summarize(
                    &task.title,
                    &description,
                    relevant_events,
                    &summary_settings,
                    prior_context.clone(),
                )
                .await
                .unwrap_or_else(|err| {
                    eprintln!("[taskflow:ai] basic summarization failed: {err}");
                    fallback_summary(&task, &events, Some(FALLBACK_REASON_NO_AI.to_string()))
                })
        } else {
            match state
                .ai_client
                .filter_events(
                    &task,
                    &description,
                    events.clone().into_iter().map(Into::into).collect(),
                )
                .await
            {
                Ok(filter_response) => {
                    let relevant_events: Vec<ScoredEvent> = filter_response
                        .scored_events
                        .into_iter()
                        .filter(|event| event.included)
                        .collect();
                    state
                        .ai_client
                        .summarize(
                            &task.title,
                            &description,
                            relevant_events,
                            &summary_settings,
                            prior_context.clone(),
                        )
                        .await
                        .unwrap_or_else(|err| {
                            eprintln!("[taskflow:ai] AI summarization failed: {err}");
                            fallback_summary(&task, &events, Some(FALLBACK_REASON_NO_AI.to_string()))
                        })
                }
                Err(err) => {
                    eprintln!("[taskflow:ai] AI event filtering failed: {err}");
                    fallback_summary(&task, &events, Some(FALLBACK_REASON_NO_AI.to_string()))
                }
            }
        }
    } else {
        fallback_summary(&task, &events, Some(FALLBACK_REASON_NO_AI.to_string()))
    };

    // The LLM appends machine-readable per-hub synthesis blocks to its markdown
    // (fenced as ```hub:<Folder>/<slug>). For memory tasks we parse them out for
    // the wiki upsert and strip them so they never reach the human-facing daily
    // note. When the sidecar was offline the markdown has none, so this is a
    // no-op and the mechanical-only wiki path runs unchanged.
    let hub_synthesis: std::collections::HashMap<String, String> =
        crate::wiki::parse_hub_synthesis(&generated.markdown);
    if !hub_synthesis.is_empty() {
        generated.markdown = crate::wiki::strip_hub_blocks(&generated.markdown);
    }

    let ai_mode_label = if sidecar_ready {
        Some(summary_settings.mode.clone())
    } else {
        Some("basic".to_string())
    };

    let doc = documentation::save_documentation(
        &state.db,
        task_id,
        generated.markdown,
        Some(generated.summary),
        ai_mode_label,
    )
    .await
    .map_err(|err| err.to_string())?;

    // Store embedding for search (best effort, don't fail if sidecar is down)
    if let Some(ref summary) = doc.summary {
        if state.sidecar_ready.load(Ordering::SeqCst) {
            if let Ok(embedding) = state.ai_client.embed_text(summary).await {
                let _ = documentation::save_embedding(&state.db, &doc.id, &embedding).await;
            }
        }
    }

    if task.source != "memory" {
        if let Err(error) = export_documentation_to_obsidian(&state.db, &task, &doc).await {
            eprintln!("[taskflow:obsidian] failed to sync task note: {error}");
        }
    }
    // NOTE: memory tasks no longer write `Memory/Daily/<date>.md` or hub pages
    // from this command. The roll-up engine owns those paths (workstream nodes +
    // derived daily index via `wiki::daily_index`); writing here too was the
    // two-writers clobbering hazard.

    Ok(doc)
}

#[tauri::command]
pub async fn get_summary_settings(state: State<'_, AppState>) -> Result<SummarySettings, String> {
    load_summary_settings(&state.db, false).await
}

#[tauri::command]
pub async fn save_summary_settings(
    settings: SummarySettings,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let cloud_api_key = if settings.cloud_api_key == "********" {
        sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = ?1")
            .bind("summary_cloud_api_key")
            .fetch_optional(&state.db)
            .await
            .map_err(|err| err.to_string())?
            .map(|row| row.0)
            .unwrap_or_default()
    } else {
        encrypt_token(&settings.cloud_api_key)
    };

    save_setting(&state.db, "summary_mode", &settings.mode).await?;
    save_setting(
        &state.db,
        "summary_cloud_base_url",
        &settings.cloud_base_url,
    )
    .await?;
    save_setting(&state.db, "summary_cloud_api_key", &cloud_api_key).await?;
    save_setting(
        &state.db,
        "summary_cloud_model",
        &settings.cloud_model,
    )
    .await?;
    save_setting(&state.db, "summary_ollama_model", &settings.ollama_model).await?;
    save_setting(
        &state.db,
        "summary_ollama_url",
        &normalize_ollama_url(&settings.ollama_url),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn test_summary_settings(
    settings: SummarySettings,
    _state: State<'_, AppState>,
) -> Result<TestResult, String> {
    let result = match settings.mode.as_str() {
        "local_ai" => test_ollama_settings(&settings).await,
        "cloud_ai" => test_cloud_settings(&settings).await,
        "basic" => Ok("Basic summaries are ready and work offline.".to_string()),
        _ => Err(format!("Unsupported summary mode: {}", settings.mode)),
    };
    Ok(match result {
        Ok(message) => TestResult {
            success: true,
            message,
        },
        Err(message) => TestResult {
            success: false,
            message,
        },
    })
}

#[tauri::command]
pub async fn check_ollama_status(
    ollama_url: String,
    _state: State<'_, AppState>,
) -> Result<OllamaStatus, String> {
    fetch_ollama_status(&ollama_url).await
}

#[tauri::command]
pub async fn get_documentation(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<Option<Documentation>, String> {
    documentation::get_latest_documentation(&state.db, task_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn get_sidecar_status(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.sidecar_ready.load(Ordering::SeqCst))
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<HashMap<String, String>, String> {
    let rows = sqlx::query_as::<_, (String, String)>("SELECT key, value FROM settings")
        .fetch_all(&state.db)
        .await
        .map_err(|err| err.to_string())?;
    Ok(rows.into_iter().collect())
}

#[tauri::command]
pub async fn update_setting(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    sqlx::query(
        r#"
        INSERT INTO settings (key, value)
        VALUES (?1, ?2)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(&state.db)
    .await
    .map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_capture_stats(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<CaptureStats, String> {
    let events = events::get_events_for_task(&state.db, task_id)
        .await
        .map_err(|err| err.to_string())?;
    let mut by_app = HashMap::new();
    let mut with_content = 0u32;
    let mut with_url = 0u32;
    let mut title_only = 0u32;

    for event in &events {
        if event
            .content
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            with_content += 1;
        }
        if event
            .url
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            with_url += 1;
        }
        if event.capture_method.as_deref() == Some("title_only") {
            title_only += 1;
        }
        if let Some(app_name) = &event.app_name {
            *by_app.entry(app_name.clone()).or_insert(0) += 1;
        }
    }

    Ok(CaptureStats {
        total_events: events.len() as u32,
        with_content,
        with_url,
        title_only,
        excluded: 0,
        by_app,
    })
}

#[tauri::command]
pub async fn update_privacy_settings(
    state: State<'_, AppState>,
    excluded_apps: Vec<String>,
    capture_clipboard: bool,
    capture_screen_text: bool,
    capture_window_titles: bool,
) -> Result<(), String> {
    save_setting(&state.db, "excluded_apps", &excluded_apps.join("\n")).await?;
    save_setting(
        &state.db,
        "capture_clipboard",
        &capture_clipboard.to_string(),
    )
    .await?;
    save_setting(
        &state.db,
        "capture_screen_text",
        &capture_screen_text.to_string(),
    )
    .await?;
    save_setting(
        &state.db,
        "capture_window_titles",
        &capture_window_titles.to_string(),
    )
    .await?;

    {
        let mut privacy = state
            .privacy_filter
            .write()
            .map_err(|_| "privacy filter lock poisoned".to_string())?;
        *privacy = PrivacyFilter::new(excluded_apps);
    }
    {
        let mut settings = state
            .capture_settings
            .write()
            .map_err(|_| "capture settings lock poisoned".to_string())?;
        *settings = CaptureSettings {
            capture_clipboard,
            capture_screen_text,
            capture_window_titles,
        };
    }
    Ok(())
}

#[tauri::command]
pub async fn update_capture_workflow(
    state: State<'_, AppState>,
    mode: String,
    selective_apps: Vec<String>,
    retention_hours: u32,
) -> Result<Option<Task>, String> {
    let workflow_mode = match mode.as_str() {
        "manual" => WorkflowMode::Manual,
        "continuous" => WorkflowMode::Continuous,
        "selective" => WorkflowMode::Selective,
        _ => return Err(format!("Unsupported capture workflow: {mode}")),
    };
    let retention_hours = retention_hours.clamp(1, 168);

    save_setting(&state.db, "capture_workflow", &mode).await?;
    save_setting(&state.db, "selective_capture_apps", &selective_apps.join("\n")).await?;
    save_setting(
        &state.db,
        "raw_capture_retention_hours",
        &retention_hours.to_string(),
    )
    .await?;

    {
        let mut workflow = state
            .workflow_mode
            .write()
            .map_err(|_| "workflow mode lock poisoned".to_string())?;
        *workflow = workflow_mode.clone();
    }
    {
        let mut apps = state
            .selective_capture_apps
            .write()
            .map_err(|_| "selective apps lock poisoned".to_string())?;
        *apps = selective_apps;
    }

    if workflow_mode.captures_without_manual_task() {
        let task = tasks::get_or_create_daily_capture_task(&state.db)
            .await
            .map_err(|err| err.to_string())?;
        set_active_task_id(&state, Some(task.id.clone()))?;
        Ok(Some(task))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub async fn ensure_daily_capture_task(state: State<'_, AppState>) -> Result<Option<Task>, String> {
    let workflow_mode = state
        .workflow_mode
        .read()
        .map_err(|_| "workflow mode lock poisoned".to_string())?
        .clone();

    if !workflow_mode.captures_without_manual_task() {
        return Ok(None);
    }

    let task = tasks::get_or_create_daily_capture_task(&state.db)
        .await
        .map_err(|err| err.to_string())?;
    set_active_task_id(&state, Some(task.id.clone()))?;
    Ok(Some(task))
}

#[tauri::command]
pub async fn update_obsidian_vault_settings(
    state: State<'_, AppState>,
    enabled: bool,
    vault_path: String,
) -> Result<(), String> {
    let vault_path = vault_path.trim();
    if enabled {
        let path = Path::new(vault_path);
        if vault_path.is_empty() {
            return Err("Choose an Obsidian vault folder before enabling sync.".to_string());
        }
        if !path.exists() {
            return Err("The Obsidian vault folder does not exist.".to_string());
        }
        if !path.is_dir() {
            return Err("The Obsidian vault path must be a folder.".to_string());
        }
        fs::create_dir_all(path.join("TaskFlow").join("Memory").join("Daily"))
            .map_err(|err| format!("Could not prepare TaskFlow folder in vault: {err}"))?;
    }

    save_setting(&state.db, "obsidian_sync_enabled", &enabled.to_string()).await?;
    save_setting(&state.db, "obsidian_vault_path", vault_path).await?;
    Ok(())
}

#[tauri::command]
pub async fn sync_task_to_obsidian(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<ObsidianSyncResult, String> {
    let task = tasks::get_task_by_id(&state.db, &task_id)
        .await
        .map_err(|err| err.to_string())?;
    if task.source == "memory" {
        // The daily note is a derived index owned by the roll-up engine —
        // regenerate it from the rollups table instead of exporting a doc.
        let settings = load_obsidian_settings(&state.db).await?;
        if !settings.enabled {
            return Err("Obsidian sync is disabled.".to_string());
        }
        if settings.vault_path.trim().is_empty() {
            return Err("No Obsidian vault folder is configured.".to_string());
        }
        let vault = PathBuf::from(&settings.vault_path);
        let date = task
            .started_at
            .as_deref()
            .and_then(|started| chrono::DateTime::parse_from_rfc3339(started).ok())
            .map(|started| started.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string());
        let path = crate::wiki::daily_index::rebuild_daily_index(&state.db, &vault, &date)
            .await?
            .ok_or_else(|| format!("No roll-ups recorded for {date} yet."))?;
        return Ok(ObsidianSyncResult {
            path: path.to_string_lossy().into_owned(),
        });
    }
    let doc = documentation::get_latest_documentation(&state.db, task_id)
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "Generate documentation before syncing to Obsidian.".to_string())?;
    let path = export_documentation_to_obsidian(&state.db, &task, &doc).await?;
    Ok(ObsidianSyncResult { path })
}

/// Mechanical health-check of the TaskFlow-owned section of the Obsidian vault.
/// Reports orphan pages, dangling `[[wikilinks]]`, and hub pages that are
/// missing backlinks to daily notes which link to them. v1 is graph-only —
/// no LLM call. LLM-driven stale-claim / contradiction detection is a follow-up.
#[tauri::command]
pub async fn lint_wiki(state: State<'_, AppState>) -> Result<crate::wiki::LintReport, String> {
    let settings = load_obsidian_settings(&state.db).await?;
    if !settings.enabled {
        return Err("Obsidian sync is disabled.".to_string());
    }
    if settings.vault_path.trim().is_empty() {
        return Err("No Obsidian vault folder is configured.".to_string());
    }
    let vault = std::path::PathBuf::from(&settings.vault_path);
    if !vault.is_dir() {
        return Err("The configured Obsidian vault folder is not available.".to_string());
    }
    let report = tokio::task::spawn_blocking(move || crate::wiki::lint_vault(&vault))
        .await
        .map_err(|err| format!("Lint task panicked: {err}"))?
        .map_err(|err| format!("Lint failed: {err}"))?;
    Ok(report)
}

async fn save_setting(db: &sqlx::SqlitePool, key: &str, value: &str) -> Result<(), String> {
    sqlx::query(
        r#"
        INSERT INTO settings (key, value)
        VALUES (?1, ?2)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(db)
    .await
    .map_err(|err| err.to_string())?;
    Ok(())
}

pub(crate) async fn load_summary_settings(
    db: &sqlx::SqlitePool,
    decrypt_key: bool,
) -> Result<SummarySettings, String> {
    let rows = sqlx::query_as::<_, (String, String)>("SELECT key, value FROM settings")
        .fetch_all(db)
        .await
        .map_err(|err| err.to_string())?;
    let settings = rows.into_iter().collect::<HashMap<_, _>>();
    let encrypted_key = settings
        .get("summary_cloud_api_key")
        .cloned()
        .unwrap_or_default();
    let cloud_api_key = if decrypt_key {
        decrypt_token(&encrypted_key)
    } else if encrypted_key.is_empty() {
        String::new()
    } else {
        "********".to_string()
    };
    Ok(SummarySettings {
        mode: settings
            .get("summary_mode")
            .cloned()
            .unwrap_or_else(|| "basic".to_string()),
        cloud_base_url: settings
            .get("summary_cloud_base_url")
            .cloned()
            .unwrap_or_default(),
        cloud_api_key,
        cloud_model: settings
            .get("summary_cloud_model")
            .cloned()
            .unwrap_or_default(),
        ollama_model: settings
            .get("summary_ollama_model")
            .cloned()
            .unwrap_or_else(|| "llama3.1:8b".to_string()),
        ollama_url: normalize_ollama_url(
            &settings
                .get("summary_ollama_url")
                .cloned()
                .unwrap_or_else(|| "http://localhost:11434".to_string()),
        ),
    })
}

async fn test_ollama_settings(settings: &SummarySettings) -> Result<String, String> {
    let status = fetch_ollama_status(&settings.ollama_url).await?;
    if !status.is_running {
        return Err("Ollama is not running. Start Ollama, then try again.".to_string());
    }
    if status
        .available_models
        .iter()
        .any(|model| model == &settings.ollama_model)
    {
        Ok(format!("Ollama is ready with {}.", settings.ollama_model))
    } else {
        Err(format!(
            "Ollama is running, but {} is not installed. Available models: {}",
            settings.ollama_model,
            if status.available_models.is_empty() {
                "none".to_string()
            } else {
                status.available_models.join(", ")
            }
        ))
    }
}

async fn fetch_ollama_status(ollama_url: &str) -> Result<OllamaStatus, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|err| err.to_string())?;
    let response = client
        .get(format!("{}/api/tags", normalize_ollama_url(ollama_url)))
        .send()
        .await;
    match response {
        Ok(response) if response.status().is_success() => {
            let payload = response
                .json::<serde_json::Value>()
                .await
                .map_err(|err| err.to_string())?;
            let available_models = payload
                .get("models")
                .and_then(|models| models.as_array())
                .map(|models| {
                    models
                        .iter()
                        .filter_map(|model| model.get("name").and_then(|name| name.as_str()))
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let has_recommended_model = available_models.iter().any(|model| {
                let lower = model.to_lowercase();
                lower.contains("llama") || lower.contains("mistral") || lower.contains("qwen")
            });
            Ok(OllamaStatus {
                is_running: true,
                available_models,
                has_recommended_model,
            })
        }
        _ => Ok(OllamaStatus {
            is_running: false,
            available_models: Vec::new(),
            has_recommended_model: false,
        }),
    }
}

async fn test_cloud_settings(settings: &SummarySettings) -> Result<String, String> {
    if settings.cloud_api_key.trim().is_empty() || settings.cloud_api_key == "********" {
        return Err("Enter an API key before testing Cloud AI.".to_string());
    }
    if settings.cloud_base_url.trim().is_empty() {
        return Err("Enter a Base URL before testing Cloud AI.".to_string());
    }
    let base_url = settings.cloud_base_url.trim_end_matches('/');
    let model = if settings.cloud_model.trim().is_empty() {
        "gpt-4o-mini"
    } else {
        &settings.cloud_model
    };
    test_cloud_endpoint(base_url, &settings.cloud_api_key, model).await
}

async fn test_cloud_endpoint(base_url: &str, api_key: &str, model: &str) -> Result<String, String> {
    let response = Client::new()
        .post(format!("{base_url}/chat/completions"))
        .bearer_auth(api_key)
        .json(&json!({"model": model, "max_tokens": 10, "messages": [{"role": "user", "content": "Hi"}]}))
        .send()
        .await
        .map_err(|err| format!("Connection test failed: {err}"))?;
    if response.status().is_success() {
        Ok("Cloud AI connection works.".to_string())
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(format!("Cloud API returned {status}: {body}"))
    }
}

#[tauri::command]
pub async fn fetch_cloud_models(
    base_url: String,
    api_key: String,
) -> Result<CloudModels, String> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let response = Client::new()
        .get(&url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|err| format!("Failed to fetch models: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("Models endpoint returned {}", response.status()));
    }
    let payload: serde_json::Value = response
        .json()
        .await
        .map_err(|err| format!("Failed to parse model list: {err}"))?;
    let models = payload
        .get("data")
        .and_then(|data| data.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(|id| id.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    Ok(CloudModels { models })
}

fn normalize_ollama_url(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        "http://localhost:11434".to_string()
    } else {
        trimmed.to_string()
    }
}

fn encrypt_token(token: &str) -> String {
    hex::encode(xor_bytes(token.as_bytes()))
}

fn decrypt_token(token: &str) -> String {
    match hex::decode(token) {
        Ok(bytes) => String::from_utf8(xor_bytes(&bytes)).unwrap_or_default(),
        Err(_) => token.to_string(),
    }
}

fn xor_bytes(bytes: &[u8]) -> Vec<u8> {
    let key = machine_key();
    bytes
        .iter()
        .enumerate()
        .map(|(index, byte)| byte ^ key[index % key.len()])
        .collect()
}

fn machine_key() -> Vec<u8> {
    let hostname = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| {
            std::fs::read_to_string("/etc/hostname")
                .map(|hostname| hostname.trim().to_string())
                .unwrap_or_else(|_| "taskflow-local-machine".to_string())
        })
        .trim()
        .to_string();
    let digest = Sha256::digest(hostname.as_bytes());
    hex::encode(digest)[..32].as_bytes().to_vec()
}

#[tauri::command]
pub async fn save_integration(
    state: State<'_, AppState>,
    provider: String,
    name: String,
    token: String,
    workspace: Option<String>,
    extra: Option<String>,
) -> Result<Integration, String> {
    integration_store::save_integration(
        &state.db,
        &provider,
        &name,
        &token,
        workspace.as_deref(),
        extra.as_deref(),
    )
    .await
}

#[tauri::command]
pub async fn get_integrations(state: State<'_, AppState>) -> Result<Vec<Integration>, String> {
    integration_store::get_integrations(&state.db).await
}

#[tauri::command]
pub async fn delete_integration(state: State<'_, AppState>, id: String) -> Result<(), String> {
    integration_store::delete_integration(&state.db, &id).await
}

#[tauri::command]
pub async fn test_integration(
    provider: String,
    token: String,
    workspace: Option<String>,
    extra: Option<String>,
) -> Result<TestResult, String> {
    let result = match provider.as_str() {
        "jira" => {
            JiraClient::new(
                workspace.unwrap_or_default(),
                extra.unwrap_or_default(),
                token,
            )
            .test_connection()
            .await
        }
        "github" => GitHubClient::new(token).test_connection().await,
        "linear" => LinearClient::new(token).test_connection().await,
        _ => Err(format!("Unsupported integration provider: {provider}")),
    };

    Ok(match result {
        Ok(name) => TestResult {
            success: true,
            message: format!("Connected as {name}"),
        },
        Err(message) => TestResult {
            success: false,
            message,
        },
    })
}

#[tauri::command]
pub async fn fetch_tickets(
    state: State<'_, AppState>,
    integration_id: String,
) -> Result<Vec<Ticket>, String> {
    let integration = integration_store::get_integration(&state.db, &integration_id).await?;
    fetch_and_store_for_integration(&state.db, integration).await
}

#[tauri::command]
pub async fn sync_tickets(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<Ticket>, String> {
    let integrations = integration_store::get_integrations(&state.db).await?;
    let mut join_set = tokio::task::JoinSet::new();

    for integration in integrations {
        let db = state.db.clone();
        let app = app.clone();
        join_set.spawn(async move {
            let _ = app.emit(
                "ticket-sync-progress",
                format!("Syncing {}...", provider_label(&integration.provider)),
            );
            let provider = integration.provider.clone();
            let result = fetch_and_store_for_integration(&db, integration).await;
            let message = match &result {
                Ok(tickets) => {
                    format!(
                        "Synced {} tickets from {}",
                        tickets.len(),
                        provider_label(&provider)
                    )
                }
                Err(error) => format!("{} sync failed: {error}", provider_label(&provider)),
            };
            let _ = app.emit("ticket-sync-progress", message);
            result
        });
    }

    let mut tickets = Vec::new();
    let mut errors = Vec::new();
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(mut fetched)) => tickets.append(&mut fetched),
            Ok(Err(error)) => errors.push(error),
            Err(error) => errors.push(format!("Ticket sync task failed: {error}")),
        }
    }

    if tickets.is_empty() && !errors.is_empty() {
        Err(errors.join("; "))
    } else {
        Ok(tickets)
    }
}

#[tauri::command]
pub async fn search_tickets(
    state: State<'_, AppState>,
    query: String,
    provider: Option<String>,
) -> Result<Vec<Ticket>, String> {
    integration_store::search_tickets(&state.db, &query, provider.as_deref()).await
}

#[tauri::command]
pub async fn get_ticket(
    state: State<'_, AppState>,
    integration_id: String,
    ticket_id: String,
) -> Result<Ticket, String> {
    let integration = integration_store::get_integration(&state.db, &integration_id).await?;
    let cached = integration_store::get_ticket_by_identity(&state.db, &integration_id, &ticket_id)
        .await
        .ok();
    let mut ticket = match integration.provider.as_str() {
        "jira" => {
            JiraClient::new(
                integration.workspace.clone().unwrap_or_default(),
                integration.extra.clone().unwrap_or_default(),
                integration.token.clone(),
            )
            .get_issue(&ticket_id)
            .await?
        }
        "github" => {
            let project = cached
                .as_ref()
                .and_then(|ticket| ticket.project.clone())
                .ok_or_else(|| "Sync this GitHub issue before refreshing it.".to_string())?;
            let (owner, repo) = project
                .split_once('/')
                .ok_or_else(|| "Cached GitHub project was not owner/repo.".to_string())?;
            let number = ticket_id
                .trim_start_matches('#')
                .parse::<u32>()
                .map_err(|_| "GitHub issue number was invalid.".to_string())?;
            GitHubClient::new(integration.token.clone())
                .get_issue(owner, repo, number)
                .await?
        }
        "linear" => {
            LinearClient::new(integration.token.clone())
                .get_issue(&ticket_id)
                .await?
        }
        _ => {
            return Err(format!(
                "Unsupported integration provider: {}",
                integration.provider
            ))
        }
    };
    ticket.integration_id = Some(integration.id);
    integration_store::save_ticket(&state.db, ticket).await
}

#[tauri::command]
pub async fn create_task_from_ticket(
    state: State<'_, AppState>,
    ticket_id: String,
    source_branch: Option<String>,
) -> Result<Task, String> {
    let ticket = integration_store::get_ticket_by_id(&state.db, &ticket_id).await?;
    let task = tasks::create_task_with_source_context(
        &state.db,
        ticket.title.clone(),
        ticket.description.clone(),
        ticket.provider.clone(),
        tasks::NewTaskSource {
            source_id: Some(ticket.ticket_id.clone()),
            source_url: ticket.url.clone(),
            source_title: Some(ticket.title.clone()),
            source_body: ticket.description.clone(),
            source_labels: ticket.labels.clone(),
            source_assignee: ticket.assignee.clone(),
            source_priority: ticket.priority.clone(),
            source_project: ticket.project.clone(),
            source_branch: source_branch.or_else(|| ticket.branch.clone()),
        },
    )
    .await
    .map_err(|err| err.to_string())?;
    set_active_task_id(&state, Some(task.id.clone()))?;
    Ok(task)
}

async fn fetch_and_store_for_integration(
    db: &sqlx::SqlitePool,
    integration: Integration,
) -> Result<Vec<Ticket>, String> {
    let fetched = match integration.provider.as_str() {
        "jira" => {
            JiraClient::new(
                integration.workspace.clone().unwrap_or_default(),
                integration.extra.clone().unwrap_or_default(),
                integration.token.clone(),
            )
            .get_assigned_issues(50)
            .await?
        }
        "github" => {
            GitHubClient::new(integration.token.clone())
                .get_assigned_issues(50)
                .await?
        }
        "linear" => {
            LinearClient::new(integration.token.clone())
                .get_assigned_issues(50)
                .await?
        }
        _ => {
            return Err(format!(
                "Unsupported integration provider: {}",
                integration.provider
            ))
        }
    };

    let mut saved = Vec::with_capacity(fetched.len());
    for mut ticket in fetched {
        ticket.integration_id = Some(integration.id.clone());
        saved.push(integration_store::save_ticket(db, ticket).await?);
    }
    Ok(saved)
}

fn provider_label(provider: &str) -> &'static str {
    match provider {
        "jira" => "Jira",
        "github" => "GitHub",
        "linear" => "Linear",
        _ => "Integration",
    }
}

fn set_active_task_id(state: &State<'_, AppState>, id: Option<String>) -> Result<(), String> {
    let mut active_task_id = state
        .active_task_id
        .lock()
        .map_err(|_| "active task state lock poisoned".to_string())?;
    *active_task_id = id;
    Ok(())
}

pub(crate) fn scored_from_event(event: &Event) -> ScoredEvent {
    ScoredEvent {
        id: event.id.clone(),
        app_name: event.app_name.clone(),
        window_title: event.window_title.clone(),
        content: event.content.clone(),
        url: event.url.clone(),
        event_type: event.event_type.clone(),
        timestamp: event.timestamp.clone(),
        relevance_score: 1.0,
        reason: "Included for basic local summarization".to_string(),
        included: true,
    }
}

/// Vault-safe fallback reason. `fallback_summary` embeds its `reason` argument
/// into markdown, key points, and the summary — all of which can flow into the
/// Obsidian vault via roll-ups. Technical error strings (HTTP statuses, URLs,
/// sidecar internals) must NEVER be passed as `reason`; log them instead.
pub(crate) const FALLBACK_REASON_NO_AI: &str =
    "AI summarization was unavailable; showing a local summary of captured events.";

pub(crate) fn fallback_summary(
    task: &Task,
    events: &[Event],
    reason: Option<String>,
) -> crate::ai_client::SummarizeResponse {
    let fallback_reason =
        reason.unwrap_or_else(|| "AI documentation was not available.".to_string());
    let mut lines = vec![
        format!("# {}", task.title),
        String::new(),
        format!(
            "**Description:** {}",
            task.description
                .as_deref()
                .filter(|value| !value.is_empty())
                .unwrap_or("No description provided.")
        ),
        format!("**Events captured:** {}", events.len()),
        String::new(),
        "## Summary".to_string(),
        format!("{fallback_reason} TaskFlow generated a structured local event summary."),
        String::new(),
        "## Activity Timeline".to_string(),
    ];

    for event in events {
        let app_name = event.app_name.as_deref().unwrap_or("Unknown");
        let detail = if event
            .capture_method
            .as_deref()
            .is_some_and(|method| method != "title_only")
        {
            match (
                event.window_title.as_deref(),
                event.content.as_deref(),
                event.url.as_deref(),
            ) {
                (Some(title), Some(content), _) => {
                    format!(
                        "{} | {}",
                        title,
                        clean_markdown_line(&content.chars().take(240).collect::<String>())
                    )
                }
                (_, Some(content), _) => {
                    clean_markdown_line(&content.chars().take(240).collect::<String>())
                }
                (Some(title), _, _) => title.to_string(),
                (_, _, Some(url)) => url.to_string(),
                _ => event.event_type.clone(),
            }
        } else {
            event
                .window_title
                .as_deref()
                .or(event.url.as_deref())
                .unwrap_or(&event.event_type)
                .to_string()
        };
        lines.push(format!(
            "- **{}** `{}`: {}",
            app_name,
            event.timestamp,
            clean_markdown_line(&detail)
        ));
    }

    let resources: Vec<String> = events
        .iter()
        .filter_map(|event| event.url.clone())
        .filter(|url| url.starts_with("http"))
        .collect();

    lines.extend([
        String::new(),
        "## Key Points".to_string(),
        "- Local events were captured successfully.".to_string(),
        format!("- {fallback_reason}"),
        String::new(),
        "## Resources Referenced".to_string(),
    ]);

    if resources.is_empty() {
        lines.push("- No external resources captured.".to_string());
    } else {
        for resource in &resources {
            lines.push(format!("- {}", resource));
        }
    }

    crate::ai_client::SummarizeResponse {
        markdown: format!("{}\n", lines.join("\n")),
        summary: format!("{fallback_reason} TaskFlow generated a structured local event summary."),
        key_points: vec![
            "Local events were captured successfully.".to_string(),
            fallback_reason,
        ],
        resources,
        duration_seconds: None,
        generated_locally: true,
    }
}

async fn export_documentation_to_obsidian(
    db: &sqlx::SqlitePool,
    task: &Task,
    doc: &Documentation,
) -> Result<String, String> {
    let settings = load_obsidian_settings(db).await?;
    if !settings.enabled {
        return Err("Obsidian sync is disabled.".to_string());
    }
    if settings.vault_path.trim().is_empty() {
        return Err("No Obsidian vault folder is configured.".to_string());
    }

    let vault = PathBuf::from(&settings.vault_path);
    if !vault.is_dir() {
        return Err("The configured Obsidian vault folder is not available.".to_string());
    }

    let relative_path = if task.source == "memory" {
        // Guard: daily notes are owned by the roll-up engine
        // (`wiki::daily_index`). This writer was retired because the two
        // writers clobbered each other's formats.
        return Err(
            "Memory daily notes are generated by the roll-up engine; use Sync for memory tasks."
                .to_string(),
        );
    } else {
        PathBuf::from("TaskFlow")
            .join("Tasks")
            .join(format!("{}.md", markdown_filename(&task.title)))
    };

    let path = vault.join(relative_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Could not create Obsidian note folder: {err}"))?;
    }
    fs::write(&path, ensure_trailing_newline(&doc.content))
        .map_err(|err| format!("Could not write Obsidian note: {err}"))?;
    Ok(path.to_string_lossy().into_owned())
}

pub(crate) struct ObsidianSettings {
    pub enabled: bool,
    pub vault_path: String,
}

pub(crate) async fn load_obsidian_settings(db: &sqlx::SqlitePool) -> Result<ObsidianSettings, String> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT key, value FROM settings WHERE key IN ('obsidian_sync_enabled', 'obsidian_vault_path')",
    )
    .fetch_all(db)
    .await
    .map_err(|err| err.to_string())?;
    let settings = rows.into_iter().collect::<HashMap<_, _>>();
    Ok(ObsidianSettings {
        enabled: settings
            .get("obsidian_sync_enabled")
            .map(|value| value == "true")
            .unwrap_or(false),
        vault_path: settings
            .get("obsidian_vault_path")
            .cloned()
            .unwrap_or_default(),
    })
}

/// Load the user's curated project registry from the `wiki_known_projects` setting.
/// Stored as a single newline- or comma-separated blob; split, trim, drop empties.
/// Empty list (the default) means the resolver falls back to the leading-word
/// heuristic, preserving existing behavior until the user configures real projects.
pub(crate) async fn load_known_projects(db: &sqlx::SqlitePool) -> Vec<String> {
    let row = sqlx::query_scalar::<_, String>(
        "SELECT value FROM settings WHERE key = 'wiki_known_projects'",
    )
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
    .unwrap_or_default();
    split_project_list(&row)
}

// ---------- Auto-detected project candidates ----------

/// One candidate, flattened for the review UI: JSON columns parsed into real
/// arrays and the promotion signals pre-computed so the frontend does no logic.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCandidateView {
    pub match_key: String,
    pub display_name: String,
    /// Human labels, e.g. `["Repository URL", "Editor workspace"]`.
    pub signals: Vec<String>,
    /// Up to five distinct capture lines that produced this candidate.
    pub evidence: Vec<String>,
    pub day_count: usize,
    pub event_count: i64,
    pub first_seen: String,
    pub last_seen: String,
    /// True once ≥2 distinct signal kinds or ≥2 distinct days back the name.
    /// The review UI leads with these; the rest are still listed, marked as
    /// "watching", so nothing is hidden but weak guesses aren't pushed.
    pub ready: bool,
}

fn candidate_view(c: &crate::database::project_candidates::ProjectCandidate) -> ProjectCandidateView {
    let kinds = c.signal_kinds_vec();
    let days = c.days_set();
    ProjectCandidateView {
        match_key: c.match_key.clone(),
        display_name: c.display_name.clone(),
        signals: kinds.iter().map(|kind| kind.label().to_string()).collect(),
        evidence: c.evidence_vec(),
        day_count: days.len(),
        event_count: c.event_count,
        first_seen: c.first_seen.clone(),
        last_seen: c.last_seen.clone(),
        ready: kinds.len() >= 2 || days.len() >= 2,
    }
}

/// Pending candidates for the launch-time review queue, strongest first.
#[tauri::command]
pub async fn get_project_candidates(
    state: State<'_, AppState>,
) -> Result<Vec<ProjectCandidateView>, String> {
    let rows = crate::database::project_candidates::list_candidates(&state.db, "pending")
        .await
        .map_err(|err| err.to_string())?;
    let mut views: Vec<ProjectCandidateView> = rows.iter().map(candidate_view).collect();
    // Ready ones first, then by breadth of evidence — the review queue should
    // put the confident detections where the user's eye lands first.
    views.sort_by(|a, b| {
        b.ready
            .cmp(&a.ready)
            .then(b.day_count.cmp(&a.day_count))
            .then(b.event_count.cmp(&a.event_count))
    });
    Ok(views)
}

/// Confirm a candidate: adds it to `wiki_known_projects` so the next ingest
/// resolves it registry-first and writes a real `Projects/<name>.md` hub page.
///
/// `display_name` is the third answer to the prompt — "the name isn't right".
/// Passing it renames the candidate before approving, and because the row is
/// keyed by the normalized `match_key`, the corrected name is an **alias**: the
/// activity that produced the detection keeps matching under the new spelling.
#[tauri::command]
pub async fn approve_project_candidate(
    state: State<'_, AppState>,
    match_key: String,
    display_name: Option<String>,
) -> Result<String, String> {
    let candidates = crate::database::project_candidates::list_candidates(&state.db, "pending")
        .await
        .map_err(|err| err.to_string())?;
    if !candidates.iter().any(|c| c.match_key == match_key) {
        return Err(format!("no pending candidate named '{match_key}'"));
    }
    if let Some(name) = display_name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        crate::database::project_candidates::rename_candidate(&state.db, &match_key, name)
            .await
            .map_err(|err| err.to_string())?;
    }
    crate::database::project_candidates::approve_candidate(&state.db, &match_key)
        .await
        .map_err(|err| err.to_string())
}

/// Reject a candidate. Durable — the extractors skip it forever, so an "MDN" or
/// "Mozilla" misfire is answered once, not once per launch.
///
/// Also refreshes the in-process veto cache the synchronous wiki paths read, so
/// the "no" holds from the very next roll-up rather than after a restart. Without
/// this the heuristic fallback in `resolve_projects` could still mint
/// `Projects/Mozilla.md` for the rest of the session.
#[tauri::command]
pub async fn reject_project_candidate(
    state: State<'_, AppState>,
    match_key: String,
) -> Result<(), String> {
    crate::database::project_candidates::reject_candidate(&state.db, &match_key)
        .await
        .map_err(|err| err.to_string())?;
    refresh_project_veto(&state.db).await;
    Ok(())
}

/// Reload the rejected-project keys into the process-wide veto cache read by
/// `wiki::resolve_projects`. Best-effort: on a read failure the previous snapshot
/// stays in place, which is strictly safer than clearing it.
pub(crate) async fn refresh_project_veto(db: &sqlx::SqlitePool) {
    match crate::database::project_candidates::load_rejected_set(db).await {
        Ok(keys) => crate::wiki::set_rejected_projects(keys),
        Err(err) => eprintln!("[taskflow:candidates] veto refresh failed: {err}"),
    }
}

/// Split a project-registry blob on newlines or commas into trimmed, non-empty
/// owned names. Exported via a thin helper so a future settings-save command can
/// mirror the same parsing for round-tripping.
fn split_project_list(value: &str) -> Vec<String> {
    value
        .split([',', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn markdown_filename(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || matches!(ch, ' ' | '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned = cleaned.trim_matches('-').trim();
    if cleaned.is_empty() {
        "Untitled Task".to_string()
    } else {
        cleaned.to_string()
    }
}

fn ensure_trailing_newline(value: &str) -> String {
    if value.ends_with('\n') {
        value.to_string()
    } else {
        format!("{value}\n")
    }
}

fn clean_markdown_line(value: &str) -> String {
    value
        .replace('\n', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------- Phase 5: Search, Stats, History, Settings ----------

#[tauri::command]
pub async fn search_documentation(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<SearchResult>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let docs = documentation::get_all_with_embeddings(&state.db)
        .await
        .map_err(|err| err.to_string())?;

    if docs.is_empty() {
        return Ok(Vec::new());
    }

    // Try to get query embedding from sidecar
    let query_embedding = if state.sidecar_ready.load(Ordering::SeqCst) {
        state.ai_client.embed_text(&query).await.ok()
    } else {
        None
    };

    let query_lower = query.to_lowercase();
    let query_words: Vec<&str> = query_lower.split_whitespace().collect();

    let mut results: Vec<SearchResult> = Vec::new();

    for (_doc_id, task_id, task_title, summary, embedding_bytes, generated_at) in &docs {
        let summary_text = summary.as_deref().unwrap_or("");
        let search_text = format!("{} {}", task_title, summary_text).to_lowercase();

        // Semantic similarity score
        let semantic_score = if let (Some(ref q_emb), Some(ref emb_bytes)) =
            (&query_embedding, &embedding_bytes)
        {
            if emb_bytes.len() >= 4 {
                let doc_embedding: Vec<f32> = emb_bytes
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();
                cosine_similarity(q_emb, &doc_embedding)
            } else {
                0.0
            }
        } else {
            0.0
        };

        // Keyword score
        let keyword_score = if query_words.is_empty() {
            0.0
        } else {
            let matched = query_words.iter().filter(|w| search_text.contains(**w)).count();
            matched as f32 / query_words.len() as f32
        };

        let combined_score = if query_embedding.is_some() {
            0.7 * semantic_score + 0.3 * keyword_score
        } else {
            keyword_score
        };

        if combined_score > 0.1 {
            let snippet = find_snippet(summary_text, &query_lower, 150);

            let source = tasks::get_task_by_id(&state.db, task_id)
                .await
                .map(|t| t.source)
                .unwrap_or_else(|_| "manual".to_string());

            results.push(SearchResult {
                task_id: task_id.clone(),
                task_title: task_title.clone(),
                matched_snippet: snippet,
                relevance_score: combined_score,
                created_at: generated_at.clone(),
                source,
            });
        }
    }

    results.sort_by(|a, b| {
        b.relevance_score
            .partial_cmp(&a.relevance_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(10);
    Ok(results)
}

#[tauri::command]
pub async fn get_task_stats(state: State<'_, AppState>) -> Result<TaskStats, String> {
    let all_tasks = tasks::get_all_tasks(&state.db)
        .await
        .map_err(|err| err.to_string())?;

    let now = chrono::Utc::now();
    let week_ago = now - chrono::Duration::days(7);

    let mut total_time_secs: u64 = 0;
    let mut time_this_week_secs: u64 = 0;
    let mut tasks_this_week: u32 = 0;
    let mut tasks_by_source: HashMap<String, u32> = HashMap::new();
    let mut app_counts: HashMap<String, u32> = HashMap::new();

    for task in &all_tasks {
        *tasks_by_source.entry(task.source.clone()).or_insert(0) += 1;

        if let Some(ref started) = task.started_at {
            if let Ok(start_time) = chrono::DateTime::parse_from_rfc3339(started) {
                let end_time = if let Some(ref ended) = task.ended_at {
                    chrono::DateTime::parse_from_rfc3339(ended)
                        .map(|t| t.with_timezone(&chrono::Utc))
                        .unwrap_or(now)
                } else {
                    now
                };
                let duration = (end_time - start_time.with_timezone(&chrono::Utc))
                    .num_seconds()
                    .max(0) as u64;
                total_time_secs += duration;

                if start_time.with_timezone(&chrono::Utc) >= week_ago {
                    tasks_this_week += 1;
                    time_this_week_secs += duration;
                }
            }
        }

        if let Ok(task_events) = events::get_events_for_task(&state.db, task.id.clone()).await {
            for event in &task_events {
                if let Some(ref app) = event.app_name {
                    *app_counts.entry(app.clone()).or_insert(0) += 1;
                }
            }
        }
    }

    let mut most_used_apps: Vec<(String, u32)> = app_counts.into_iter().collect();
    most_used_apps.sort_by(|a, b| b.1.cmp(&a.1));
    most_used_apps.truncate(10);

    Ok(TaskStats {
        total_tasks: all_tasks.len() as u32,
        total_time_secs,
        tasks_this_week,
        time_this_week_secs,
        most_used_apps,
        tasks_by_source,
    })
}

#[tauri::command]
pub async fn get_documentation_history(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<Vec<Documentation>, String> {
    documentation::get_documentation_history(&state.db, task_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn get_setting(
    state: State<'_, AppState>,
    key: String,
) -> Result<Option<String>, String> {
    let row = sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = ?1")
        .bind(&key)
        .fetch_optional(&state.db)
        .await
        .map_err(|err| err.to_string())?;
    Ok(row.map(|r| r.0))
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    (dot / (norm_a * norm_b)).clamp(-1.0, 1.0)
}

fn find_snippet(text: &str, query: &str, max_len: usize) -> String {
    if text.is_empty() {
        return String::new();
    }
    let text_lower = text.to_lowercase();
    let query_words: Vec<&str> = query.split_whitespace().collect();

    let best_pos = query_words
        .iter()
        .filter_map(|word| text_lower.find(word))
        .min()
        .unwrap_or(0);

    let start = best_pos.saturating_sub(max_len / 3);
    let chars: Vec<char> = text.chars().collect();
    let snippet_chars: String = chars.iter().skip(start).take(max_len).collect();

    let mut snippet = snippet_chars;
    if start > 0 {
        snippet = format!("...{}", snippet.trim_start());
    }
    if start + max_len < chars.len() {
        snippet = format!("{}...", snippet.trim_end());
    }
    snippet
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::events::Event;
    use crate::database::tasks::Task;

    fn test_task() -> Task {
        Task {
            id: "t1".to_string(),
            title: "Memory Capture".to_string(),
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
            status: "active".to_string(),
            started_at: None,
            ended_at: None,
            created_at: "2026-07-27T10:00:00+00:00".to_string(),
        }
    }

    fn test_event() -> Event {
        Event {
            id: "e1".to_string(),
            task_id: "t1".to_string(),
            event_type: "window_switch".to_string(),
            app_name: Some("Code.exe".to_string()),
            window_title: Some("main.rs".to_string()),
            content: None,
            url: None,
            content_type: Some("CodeContent".to_string()),
            capture_method: Some("uitautomation".to_string()),
            is_sanitized: 1,
            chunk_index: 0,
            relevance: 0.0,
            timestamp: "2026-07-27T10:00:00+00:00".to_string(),
            created_at: "2026-07-27T10:00:00+00:00".to_string(),
        }
    }

    #[test]
    fn fallback_summary_never_embeds_technical_error_text() {
        // The vault-facing fallback must only ever carry the generic reason.
        // A technical error (HTTP status, URL, sidecar internals) passed as
        // `reason` would flow into rollups.summary_md and the Obsidian vault.
        let technical =
            "AI summarization failed: HTTP status server error (500) for url http://127.0.0.1:7878/summarize";
        let task = test_task();
        let events = vec![test_event()];

        // Callers must pass FALLBACK_REASON_NO_AI — assert the constant output
        // is clean, and assert the danger pattern would be detectable.
        let safe = fallback_summary(&task, &events, Some(FALLBACK_REASON_NO_AI.to_string()));
        for text in [&safe.markdown, &safe.summary] {
            assert!(!text.contains("HTTP"), "no technical error in output: {text}");
            assert!(!text.contains("127.0.0.1"), "no sidecar URL in output: {text}");
            assert!(
                text.contains(FALLBACK_REASON_NO_AI),
                "generic reason present: {text}"
            );
        }
        assert!(
            safe.key_points
                .iter()
                .all(|point| !point.contains("HTTP") && !point.contains("127.0.0.1")),
            "key points clean: {:?}",
            safe.key_points
        );

        // Guard the guard: if someone passes a technical reason directly, it
        // lands in the output — which is exactly why call sites must not.
        let unsafe_out = fallback_summary(&task, &events, Some(technical.to_string()));
        assert!(unsafe_out.markdown.contains(technical));
    }
}
