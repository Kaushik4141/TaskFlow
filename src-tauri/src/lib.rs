mod ai_client;
mod capture;
mod commands;
// `database` and `wiki` are public so `src/bin/detect_dryrun.rs` can replay real
// captures through the real extractors. The GUI build can't complete on Windows
// (LNK1318 PDB limit), so this is the only way to verify detection against live
// data rather than synthetic test fixtures.
pub mod database;
mod integrations;
mod rollup;
pub mod wiki;

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock,
    },
    time::Duration,
};

use ai_client::AiClient;
use capture::privacy::PrivacyFilter;
use sqlx::SqlitePool;
use tauri::{Emitter, Manager};

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub active_task_id: Arc<Mutex<Option<String>>>,
    pub ai_client: Arc<AiClient>,
    pub sidecar_ready: Arc<AtomicBool>,
    pub privacy_filter: Arc<RwLock<PrivacyFilter>>,
    pub capture_settings: Arc<RwLock<CaptureSettings>>,
    pub workflow_mode: Arc<RwLock<WorkflowMode>>,
    pub selective_capture_apps: Arc<RwLock<Vec<String>>>,
    /// Per-app deep-capture failure tracking, keyed by lowercased app name.
    /// Drives exponential backoff (see `capture::window_monitor::backoff_delay`)
    /// so an unreadable window stops burning OCR passes, but still self-heals
    /// once the app becomes readable — unlike the permanent blacklist this
    /// replaced, which could never recover.
    pub capture_failures: Arc<Mutex<std::collections::HashMap<String, CaptureFailure>>>,
}

/// One app's consecutive deep-capture failure streak and when it last tried.
#[derive(Debug, Clone, Copy)]
pub struct CaptureFailure {
    pub consecutive: u32,
    pub last_attempt: std::time::Instant,
}

#[derive(Debug, Clone)]
pub struct CaptureSettings {
    pub capture_clipboard: bool,
    pub capture_screen_text: bool,
    pub capture_window_titles: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowMode {
    Manual,
    Continuous,
    Selective,
}

impl WorkflowMode {
    pub fn from_setting(value: Option<&String>) -> Self {
        match value.map(String::as_str) {
            Some("continuous") => Self::Continuous,
            Some("selective") => Self::Selective,
            _ => Self::Manual,
        }
    }

    pub fn captures_without_manual_task(&self) -> bool {
        matches!(self, Self::Continuous | Self::Selective)
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|err| format!("failed to resolve app data directory: {err}"))?;
            std::fs::create_dir_all(&app_data_dir)
                .map_err(|err| format!("failed to create app data directory: {err}"))?;

            let db_path = app_data_dir.join("taskflow.sqlite");
            let db = tauri::async_runtime::block_on(database::init_database(&db_path))
                .map_err(|err| format!("failed to initialize database: {err}"))?;

            let active_task_id =
                tauri::async_runtime::block_on(database::tasks::get_active_task(&db))
                    .map_err(|err| format!("failed to load active task: {err}"))?
                    .map(|task| task.id);
            let (privacy_filter, capture_settings, workflow_mode, selective_capture_apps) =
                tauri::async_runtime::block_on(load_capture_configuration(&db))
                    .map_err(|err| format!("failed to load capture settings: {err}"))?;
            let active_task_id = if workflow_mode.captures_without_manual_task() {
                tauri::async_runtime::block_on(database::tasks::get_or_create_daily_capture_task(
                    &db,
                ))
                .map_err(|err| format!("failed to start daily capture: {err}"))
                .map(|task| Some(task.id))?
            } else {
                active_task_id
            };

            let state = AppState {
                db,
                active_task_id: Arc::new(Mutex::new(active_task_id)),
                ai_client: Arc::new(AiClient::new()),
                sidecar_ready: Arc::new(AtomicBool::new(false)),
                privacy_filter: Arc::new(RwLock::new(privacy_filter)),
                capture_settings: Arc::new(RwLock::new(capture_settings)),
                workflow_mode: Arc::new(RwLock::new(workflow_mode)),
                selective_capture_apps: Arc::new(RwLock::new(selective_capture_apps)),
                capture_failures: Arc::new(Mutex::new(std::collections::HashMap::new())),
            };

            // Seed the rejected-project veto before anything can resolve a
            // project. The roll-up refreshes it every window, but seeding here
            // means a "no" the user gave in a previous session is in force from
            // process start rather than from the first roll-up.
            tauri::async_runtime::block_on(commands::refresh_project_veto(&state.db));

            capture::window_monitor::start(app.handle().clone(), state.clone());
            capture::clipboard::start(app.handle().clone(), state.clone());
            start_ai_sidecar(app.handle().clone(), state.clone());
            rollup::scheduler::start(app.handle().clone(), state.clone());
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::create_task,
            commands::get_all_tasks,
            commands::get_active_task,
            commands::start_task,
            commands::stop_task,
            commands::get_task_events,
            commands::add_note,
            commands::generate_documentation,
            commands::get_documentation,
            commands::get_rollups,
            commands::get_sidecar_status,
            commands::get_settings,
            commands::update_setting,
            commands::get_summary_settings,
            commands::save_summary_settings,
            commands::test_summary_settings,
            commands::check_ollama_status,
            commands::fetch_cloud_models,
            commands::save_integration,
            commands::get_integrations,
            commands::delete_integration,
            commands::test_integration,
            commands::fetch_tickets,
            commands::sync_tickets,
            commands::search_tickets,
            commands::get_ticket,
            commands::create_task_from_ticket,
            commands::get_capture_stats,
            commands::update_privacy_settings,
            commands::update_capture_workflow,
            commands::ensure_daily_capture_task,
            commands::update_obsidian_vault_settings,
            commands::sync_task_to_obsidian,
            commands::search_documentation,
            commands::get_task_stats,
            commands::get_documentation_history,
            commands::get_setting,
            commands::get_platform,
            commands::lint_wiki,
            commands::get_project_candidates,
            commands::approve_project_candidate,
            commands::reject_project_candidate,
            commands::query_graph_memory,
            commands::search_memory,
            commands::get_cloud_status,
            commands::save_cloud_settings,
            commands::sync_cloud_now,
            commands::get_google_oauth_url,
            commands::sign_out_cloud,
        ])
        .run(tauri::generate_context!())
        .expect("error while running TaskFlow");
}

async fn load_capture_configuration(
    db: &SqlitePool,
) -> Result<(PrivacyFilter, CaptureSettings, WorkflowMode, Vec<String>), sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, String)>("SELECT key, value FROM settings")
        .fetch_all(db)
        .await?;
    let settings = rows
        .into_iter()
        .collect::<std::collections::HashMap<_, _>>();
    let excluded_apps = settings
        .get("excluded_apps")
        .map(|value| value.lines().map(ToString::to_string).collect())
        .unwrap_or_else(Vec::new);
    let selective_capture_apps = settings
        .get("selective_capture_apps")
        .map(|value| {
            value
                .lines()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_else(Vec::new);

    let workflow_mode = WorkflowMode::from_setting(settings.get("capture_workflow"));

    Ok((
        PrivacyFilter::new(excluded_apps),
        CaptureSettings {
            capture_clipboard: settings
                .get("capture_clipboard")
                .map(|value| value == "true")
                .unwrap_or(true),
            capture_screen_text: settings
                .get("capture_screen_text")
                .map(|value| value == "true")
                .unwrap_or(true),
            capture_window_titles: settings
                .get("capture_window_titles")
                .map(|value| value == "true")
                .unwrap_or(true),
        },
        workflow_mode,
        selective_capture_apps,
    ))
}

fn start_ai_sidecar(app: tauri::AppHandle, state: AppState) {
    let app_for_launch = app.clone();
    let script = sidecar_script(&app);
    let _ = spawn_python_sidecar(&script);

    tauri::async_runtime::spawn(async move {
        for _ in 0..30 {
            if state.ai_client.is_ready().await {
                state.sidecar_ready.store(true, Ordering::SeqCst);
                let _ = app_for_launch.emit("sidecar-ready", true);
                return;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

fn sidecar_script(app: &tauri::AppHandle) -> PathBuf {
    if let Ok(resource_dir) = app.path().resource_dir() {
        let bundled = resource_dir.join("sidecar").join("main.py");
        if bundled.is_file() {
            return bundled;
        }

        let bundled = resource_dir.join("main.py");
        if bundled.is_file() {
            return bundled;
        }
    }

    dev_sidecar_script()
}

fn dev_sidecar_script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|root| root.join("sidecar").join("main.py"))
        .unwrap_or_else(|| PathBuf::from("sidecar").join("main.py"))
}

fn spawn_python_sidecar(script: &Path) -> std::io::Result<std::process::Child> {
    let sidecar_dir = script.parent().map(Path::to_path_buf);
    let mut executables = Vec::<PathBuf>::new();

    if let Some(sidecar_dir) = sidecar_dir.as_deref() {
        let root = sidecar_dir.parent();
        if let Some(root) = root {
            let venv_python = if cfg!(windows) {
                root.join(".venv-sidecar")
                    .join("Scripts")
                    .join("python.exe")
            } else {
                root.join(".venv-sidecar").join("bin").join("python")
            };
            if venv_python.is_file() {
                executables.push(venv_python);
            }
        }
    }

    if cfg!(windows) {
        executables.push("py".into());
        executables.push("python".into());
    } else {
        executables.push("python3".into());
        executables.push("python".into());
    }

    let mut last_error = std::io::Error::other("no Python executable found");
    for executable in executables {
        let mut command = std::process::Command::new(executable);
        command.arg(script);
        if let Some(directory) = sidecar_dir.as_deref() {
            command.current_dir(directory);
        }
        match command.spawn() {
            Ok(child) => return Ok(child),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}
