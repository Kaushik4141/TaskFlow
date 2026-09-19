use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use chrono::Utc;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::{capture::types::ContentType, database, AppState, CaptureFailure, WorkflowMode};

use super::{
    chunker::chunk_content,
    cleaner::{clean_captured_content, is_meaningful_content},
    types::CapturedContent,
    url_extractor,
};

#[cfg(target_os = "windows")]
use super::windows_reader::WindowsReader;

#[cfg(target_os = "linux")]
use super::linux_reader::LinuxReader;

#[cfg(target_os = "macos")]
use super::mac_reader::MacReader;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WindowSnapshot {
    pub app_name: String,
    pub window_title: String,
    pub platform_handle: isize,
    pub pid: i32,
}

#[derive(Default)]
struct WindowCache {
    last_content_hash: HashMap<String, u64>,
    last_captured_at: HashMap<String, Instant>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowChangedPayload {
    pub app_name: String,
    pub window_title: String,
    pub url: Option<String>,
    pub content_preview: Option<String>,
    pub content_type: String,
    pub timestamp: String,
    pub task_id: Option<String>,
}

pub fn start(app: AppHandle, state: AppState) {
    thread::spawn(move || {
        let mut last_window: Option<WindowSnapshot> = None;
        let mut last_deep_read: HashMap<String, Instant> = HashMap::new();
        let content_cache = Arc::new(Mutex::new(WindowCache::default()));
        let own_process_name = std::env::current_exe()
            .ok()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().to_lowercase())
            })
            .unwrap_or_default();

        loop {
            if let Some(snapshot) = active_window() {
                let app_lower = snapshot.app_name.to_lowercase();
                if !own_process_name.is_empty() && app_lower.contains(&own_process_name) {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }

                let changed = last_window.as_ref() != Some(&snapshot);
                let window_key = format!("{}|{}", snapshot.app_name, snapshot.window_title);

                let settings = state
                    .capture_settings
                    .read()
                    .ok()
                    .map(|guard| guard.clone());
                let capture_window_titles = settings
                    .as_ref()
                    .map(|settings| settings.capture_window_titles)
                    .unwrap_or(true);
                let capture_screen_text = settings
                    .as_ref()
                    .map(|settings| settings.capture_screen_text)
                    .unwrap_or(false);

                let should_capture = state
                    .privacy_filter
                    .read()
                    .map(|privacy| {
                        privacy.should_capture_window(&snapshot.app_name, &snapshot.window_title)
                    })
                    .unwrap_or(false)
                    && workflow_allows_app(&state, &snapshot.app_name);

                if !should_capture {
                    last_window = Some(snapshot);
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }

                let task_id = active_task_id(&state).or_else(|| ensure_daily_capture_task(&state));
                let mut stored_event_id = None;
                if changed && capture_window_titles {
                    stored_event_id =
                        emit_and_store_title_only(&app, &state, &snapshot, task_id.clone());
                }

                if capture_screen_text
                    && task_id.is_some()
                    && should_deep_capture(&window_key, changed, &last_deep_read)
                    && !in_capture_backoff(&state, &snapshot.app_name)
                    && (changed || user_idle_for(Duration::from_secs(5)))
                {
                    last_deep_read.insert(window_key, Instant::now());
                    eprintln!(
                        "[taskflow:capture] Attempting deep capture for: {} - {}",
                        snapshot.app_name, snapshot.window_title
                    );
                    log::debug!(
                        "Attempting deep capture for: {} - {}",
                        snapshot.app_name,
                        snapshot.window_title
                    );
                    spawn_deep_capture(
                        app.clone(),
                        state.clone(),
                        snapshot.clone(),
                        task_id.unwrap(),
                        stored_event_id,
                        content_cache.clone(),
                    );
                }

                if changed {
                    last_window = Some(snapshot);
                }
            }

            thread::sleep(Duration::from_secs(1));
        }
    });
}

fn emit_and_store_title_only(
    app: &AppHandle,
    state: &AppState,
    snapshot: &WindowSnapshot,
    task_id: Option<String>,
) -> Option<String> {
    let extracted = url_extractor::extract_from_title(&snapshot.app_name, &snapshot.window_title);
    let url = extracted.and_then(|value| value.likely_url);
    let payload = WindowChangedPayload {
        app_name: snapshot.app_name.clone(),
        window_title: snapshot.window_title.clone(),
        url: url.clone(),
        content_preview: None,
        content_type: ContentType::WindowTitleOnly.as_str().to_string(),
        timestamp: Utc::now().to_rfc3339(),
        task_id: task_id.clone(),
    };

    let _ = app.emit("window-changed", payload);

    let task_id = task_id?;
    let db = state.db.clone();
    let app_name = snapshot.app_name.clone();
    let window_title = snapshot.window_title.clone();
    let event = tauri::async_runtime::block_on(async move {
        database::events::insert_event_with_metadata(
            &db,
            task_id,
            "window_switch".to_string(),
            Some(app_name),
            Some(window_title),
            None,
            url,
            Some(ContentType::WindowTitleOnly.as_str().to_string()),
            Some("title_only".to_string()),
            true,
            0,
        )
        .await
    });
    match event {
        Ok(event) => Some(event.id),
        Err(error) => {
            eprintln!("[taskflow:capture] failed to save title event: {error}");
            None
        }
    }
}

fn spawn_deep_capture(
    app: AppHandle,
    state: AppState,
    snapshot: WindowSnapshot,
    task_id: String,
    title_event_id: Option<String>,
    content_cache: Arc<Mutex<WindowCache>>,
) {
    tauri::async_runtime::spawn_blocking(move || {
        let captured = read_deep_content(&state, &snapshot, task_id);
        if captured.is_none() {
            record_capture_failure(&state, &snapshot.app_name);
            eprintln!(
                "[taskflow:capture] Deep capture failed for: {} - {}",
                snapshot.app_name, snapshot.window_title
            );
            log::debug!(
                "Deep capture failed for: {} - {}",
                snapshot.app_name,
                snapshot.window_title
            );
            return;
        }
        record_capture_success(&state, &snapshot.app_name);

        let mut captured = captured.unwrap();
        if captured.url.is_none() {
            captured.url =
                url_extractor::extract_from_title(&captured.app_name, &captured.window_title)
                    .and_then(|value| value.likely_url);
        }

        captured.text = captured.text.as_deref().map(clean_captured_content);
        if captured
            .text
            .as_deref()
            .is_some_and(|content| !is_meaningful_content(content))
        {
            captured.text = None;
        }

        let preview = captured
            .text
            .as_ref()
            .map(|value| value.chars().take(200).collect::<String>());
        let payload = WindowChangedPayload {
            app_name: captured.app_name.clone(),
            window_title: captured.window_title.clone(),
            url: captured.url.clone(),
            content_preview: preview,
            content_type: captured.content_type.as_str().to_string(),
            timestamp: captured.timestamp.clone(),
            task_id: Some(captured.task_id.clone()),
        };
        let _ = app.emit("window-changed", payload);

        let db = state.db.clone();
        tauri::async_runtime::spawn(async move {
            let window_key = format!("{}:{}", captured.app_name, captured.window_title);
            if let Some(content) = captured.text.as_deref() {
                let new_hash = content_hash(content);
                let mut cache = match content_cache.lock() {
                    Ok(cache) => cache,
                    Err(_) => return,
                };

                if cache
                    .last_content_hash
                    .get(&window_key)
                    .is_some_and(|last_hash| *last_hash == new_hash)
                {
                    eprintln!(
                        "[taskflow:capture] duplicate content skipped for {}",
                        window_key
                    );
                    return;
                }

                if cache
                    .last_captured_at
                    .get(&window_key)
                    .is_some_and(|last_time| last_time.elapsed() < Duration::from_secs(30))
                {
                    eprintln!(
                        "[taskflow:capture] recent duplicate skipped for {}",
                        window_key
                    );
                    return;
                }

                cache.last_content_hash.insert(window_key.clone(), new_hash);
                cache
                    .last_captured_at
                    .insert(window_key.clone(), Instant::now());
            }

            let chunks = captured
                .text
                .as_ref()
                .map(|text| chunk_content(text, &captured.content_type, 500))
                .unwrap_or_default();
            let content_length = captured.text.as_ref().map(|text| text.len()).unwrap_or(0);
            eprintln!(
                "[taskflow:capture] Deep capture result: {} chars, url={:?}, method={}",
                content_length, captured.url, captured.capture_method
            );
            log::debug!(
                "Deep capture result: {} chars, url={:?}, method={}",
                content_length,
                captured.url,
                captured.capture_method
            );
            if let Some(event_id) = title_event_id {
                let first_chunk = chunks.first().map(|chunk| chunk.text.clone());
                let content_to_save = first_chunk.or_else(|| captured.text.clone());
                eprintln!(
                    "[taskflow:capture] Saving event with content: {}",
                    if content_to_save
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty())
                    {
                        "yes"
                    } else {
                        "no"
                    }
                );
                let _ = database::events::update_event_content(
                    &db,
                    &event_id,
                    content_to_save,
                    captured.url.clone(),
                    Some(captured.content_type.as_str().to_string()),
                    Some(captured.capture_method.clone()),
                    true,
                )
                .await;

                if chunks.len() > 1 {
                    let remaining = chunks.into_iter().skip(1).collect::<Vec<_>>();
                    let _ =
                        database::events::insert_captured_chunks(&db, captured, remaining).await;
                }
            } else {
                eprintln!(
                    "[taskflow:capture] Saving event with content: {}",
                    if content_length > 0 { "yes" } else { "no" }
                );
                let _ = database::events::insert_captured_chunks(&db, captured, chunks).await;
            }
        });
    });
}

fn content_hash(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content
        .chars()
        .take(500)
        .collect::<String>()
        .hash(&mut hasher);
    hasher.finish()
}

fn read_deep_content(
    state: &AppState,
    snapshot: &WindowSnapshot,
    task_id: String,
) -> Option<CapturedContent> {
    let privacy = state
        .privacy_filter
        .read()
        .ok()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    let privacy = std::sync::Arc::new(privacy);
    // Read per-capture (not cached) so the Settings toggle takes effect
    // immediately; it only costs one KV query on deep-capture attempts.
    let ocr_enabled = tauri::async_runtime::block_on(async {
        sqlx::query_scalar::<_, String>(
            "SELECT value FROM settings WHERE key = 'capture_ocr_fallback'",
        )
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .map(|value| value != "false")
        .unwrap_or(true)
    });

    #[cfg(target_os = "windows")]
    {
        let reader = WindowsReader::new(privacy).with_ocr(ocr_enabled);
        return reader.read_window_content(
            windows::Win32::Foundation::HWND(snapshot.platform_handle),
            &snapshot.app_name,
            &snapshot.window_title,
            task_id,
        );
    }

    #[cfg(target_os = "linux")]
    {
        let reader = LinuxReader::new(privacy).with_ocr(ocr_enabled);
        return reader.read_window_content(
            snapshot.platform_handle,
            snapshot.pid,
            &snapshot.app_name,
            &snapshot.window_title,
            task_id,
        );
    }

    #[cfg(target_os = "macos")]
    {
        let reader = MacReader::new(privacy);
        return reader.read_window_content(
            snapshot.pid,
            &snapshot.app_name,
            &snapshot.window_title,
            task_id,
        );
    }

    #[allow(unreachable_code)]
    None
}

fn should_deep_capture(
    window_key: &str,
    changed: bool,
    last_deep_read: &HashMap<String, Instant>,
) -> bool {
    changed
        || last_deep_read
            .get(window_key)
            .map(|instant| instant.elapsed() >= Duration::from_secs(30))
            .unwrap_or(true)
}

/// How long to wait before retrying deep capture on an app that has failed
/// `consecutive` times in a row: 30s doubling to a 15-minute ceiling.
///
/// Backoff rather than a permanent blacklist because a `None` capture is
/// ambiguous — genuinely unreadable window, OCR toggled off, privacy-rejected,
/// or an app that simply hadn't finished painting. All of those can become
/// readable later, so the breaker has to be able to close again.
fn backoff_delay(consecutive: u32) -> Duration {
    const BASE_SECS: u64 = 30;
    const MAX_SECS: u64 = 15 * 60;
    if consecutive == 0 {
        return Duration::ZERO;
    }
    // Saturating shift: consecutive is unbounded, so cap the exponent before
    // it can overflow the multiply.
    let factor = 1u64.checked_shl(consecutive - 1).unwrap_or(u64::MAX);
    let secs = BASE_SECS.saturating_mul(factor).min(MAX_SECS);
    Duration::from_secs(secs)
}

/// True when `app_name` is inside its failure backoff window and deep capture
/// should be skipped. Fails open (allows capture) if the mutex is poisoned.
fn in_capture_backoff(state: &AppState, app_name: &str) -> bool {
    state
        .capture_failures
        .lock()
        .map(|failures| {
            failures
                .get(&app_name.to_lowercase())
                .is_some_and(|failure| {
                    failure.last_attempt.elapsed() < backoff_delay(failure.consecutive)
                })
        })
        .unwrap_or(false)
}

/// Record a failed deep capture, extending this app's backoff window.
fn record_capture_failure(state: &AppState, app_name: &str) {
    if let Ok(mut failures) = state.capture_failures.lock() {
        let entry = failures
            .entry(app_name.to_lowercase())
            .or_insert(CaptureFailure {
                consecutive: 0,
                last_attempt: Instant::now(),
            });
        entry.consecutive = entry.consecutive.saturating_add(1);
        entry.last_attempt = Instant::now();
    }
}

/// Clear an app's failure streak after a successful deep capture.
fn record_capture_success(state: &AppState, app_name: &str) {
    if let Ok(mut failures) = state.capture_failures.lock() {
        failures.remove(&app_name.to_lowercase());
    }
}

fn active_task_id(state: &AppState) -> Option<String> {
    state
        .active_task_id
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

fn ensure_daily_capture_task(state: &AppState) -> Option<String> {
    let workflow_mode = state.workflow_mode.read().ok()?.clone();
    if !workflow_mode.captures_without_manual_task() {
        return None;
    }

    let db = state.db.clone();
    let task = tauri::async_runtime::block_on(async move {
        database::tasks::get_or_create_daily_capture_task(&db).await
    })
    .ok()?;
    if let Ok(mut active_task_id) = state.active_task_id.lock() {
        *active_task_id = Some(task.id.clone());
    }
    Some(task.id)
}

fn workflow_allows_app(state: &AppState, app_name: &str) -> bool {
    let workflow_mode = state
        .workflow_mode
        .read()
        .map(|workflow| workflow.clone())
        .unwrap_or(WorkflowMode::Manual);
    if workflow_mode != WorkflowMode::Selective {
        return true;
    }

    let app_name = app_name.to_lowercase();
    state
        .selective_capture_apps
        .read()
        .map(|apps| {
            apps.iter()
                .map(|app| app.to_lowercase())
                .any(|allowed| !allowed.is_empty() && app_name.contains(&allowed))
        })
        .unwrap_or(false)
}

#[cfg(windows)]
pub(crate) fn user_idle_for(duration: Duration) -> bool {
    use winapi::um::winuser::{GetLastInputInfo, LASTINPUTINFO};

    unsafe {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if GetLastInputInfo(&mut info) == 0 {
            return true;
        }
        let tick_count = winapi::um::sysinfoapi::GetTickCount();
        let idle_ms = tick_count.saturating_sub(info.dwTime);
        idle_ms >= duration.as_millis() as u32
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn user_idle_for(duration: Duration) -> bool {
    let _ = duration;
    true
}

#[cfg(target_os = "linux")]
pub(crate) fn user_idle_for(duration: Duration) -> bool {
    if let Ok(output) = std::process::Command::new("xprintidle").output() {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Ok(idle_ms) = text.parse::<u128>() {
                return idle_ms >= duration.as_millis();
            }
        }
    }

    if let Ok(output) = std::process::Command::new("busctl")
        .args([
            "--user",
            "get-property",
            "org.freedesktop.login1",
            "/org/freedesktop/login1/session/self",
            "org.freedesktop.login1.Session",
            "IdleHint",
        ])
        .output()
    {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout)
                .trim()
                .eq_ignore_ascii_case("true");
        }
    }

    true
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub(crate) fn user_idle_for(_duration: Duration) -> bool {
    true
}

#[cfg(windows)]
pub(crate) fn active_window() -> Option<WindowSnapshot> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, ptr};
    use winapi::{
        shared::windef::HWND,
        um::{
            handleapi::CloseHandle,
            processthreadsapi::OpenProcess,
            psapi::GetModuleBaseNameW,
            winnt::{PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
            winuser::{GetForegroundWindow, GetWindowThreadProcessId},
        },
    };

    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }

        let window_title = window_text(hwnd).unwrap_or_default();
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);

        let process = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        let app_name = if process.is_null() {
            "Unknown".to_string()
        } else {
            let mut buffer = [0u16; 260];
            let len = GetModuleBaseNameW(
                process,
                ptr::null_mut(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
            );
            let name = if len == 0 {
                "Unknown".to_string()
            } else {
                OsString::from_wide(&buffer[..len as usize])
                    .to_string_lossy()
                    .into_owned()
            };
            CloseHandle(process);
            name
        };

        Some(WindowSnapshot {
            app_name,
            window_title,
            platform_handle: hwnd as isize,
            pid: pid as i32,
        })
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn active_window() -> Option<WindowSnapshot> {
    LinuxReader::active_window().map(|window| WindowSnapshot {
        app_name: window.app_name,
        window_title: window.window_title,
        platform_handle: window.platform_handle,
        pid: window.pid,
    })
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub(crate) fn active_window() -> Option<WindowSnapshot> {
    None
}

#[cfg(windows)]
unsafe fn window_text(hwnd: winapi::shared::windef::HWND) -> Option<String> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    use winapi::um::winuser::{GetWindowTextLengthW, GetWindowTextW};

    let len = GetWindowTextLengthW(hwnd);
    if len == 0 {
        return Some(String::new());
    }

    let mut buffer = vec![0u16; len as usize + 1];
    let copied = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
    if copied == 0 {
        return None;
    }

    Some(
        OsString::from_wide(&buffer[..copied as usize])
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn active_window() -> Option<WindowSnapshot> {
    use objc2::rc::Id;
    use objc2_app_kit::NSWorkspace;

    unsafe {
        let workspace: Id<NSWorkspace> = NSWorkspace::sharedWorkspace();
        let app = workspace.frontmostApplication()?;
        let app_name = app
            .localizedName()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        let pid = app.processIdentifier();
        Some(WindowSnapshot {
            app_name,
            window_title: String::new(),
            platform_handle: 0,
            pid,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_delay_grows_then_caps() {
        assert_eq!(backoff_delay(0), Duration::ZERO);
        assert_eq!(backoff_delay(1), Duration::from_secs(30));
        assert_eq!(backoff_delay(2), Duration::from_secs(60));
        assert_eq!(backoff_delay(3), Duration::from_secs(120));
        assert_eq!(backoff_delay(4), Duration::from_secs(240));
        assert_eq!(backoff_delay(5), Duration::from_secs(480));
        // Ceiling reached, and held from here on.
        assert_eq!(backoff_delay(6), Duration::from_secs(900));
        assert_eq!(backoff_delay(7), Duration::from_secs(900));
    }

    #[test]
    fn backoff_delay_never_overflows_on_long_streaks() {
        // `consecutive` is unbounded in principle; a huge streak must clamp to
        // the ceiling rather than panic on shift/multiply overflow.
        for consecutive in [32u32, 63, 64, 65, 1000, u32::MAX] {
            assert_eq!(
                backoff_delay(consecutive),
                Duration::from_secs(900),
                "streak {consecutive} should clamp to the ceiling"
            );
        }
    }

    #[test]
    fn backoff_delay_is_monotonic() {
        let mut previous = Duration::ZERO;
        for consecutive in 0..20 {
            let delay = backoff_delay(consecutive);
            assert!(
                delay >= previous,
                "delay shrank at {consecutive}: {previous:?} -> {delay:?}"
            );
            previous = delay;
        }
    }
}
