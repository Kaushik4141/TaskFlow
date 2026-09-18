use std::{thread, time::Duration};

use arboard::Clipboard;
use chrono::Utc;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::{
    capture::cleaner::{clean_captured_content, is_meaningful_content},
    database, AppState,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardChangedPayload {
    pub content: String,
    pub content_type: String,
    pub timestamp: String,
}

pub fn start(app: AppHandle, state: AppState) {
    thread::spawn(move || {
        let mut clipboard = Clipboard::new().ok();
        let mut last_content = String::new();

        loop {
            if !state
                .capture_settings
                .read()
                .map(|settings| settings.capture_clipboard)
                .unwrap_or(true)
            {
                thread::sleep(Duration::from_millis(500));
                continue;
            }

            if clipboard.is_none() {
                clipboard = Clipboard::new().ok();
            }

            if let Some(clipboard) = clipboard.as_mut() {
                if let Ok(text) = clipboard.get_text() {
                    let content = state
                        .privacy_filter
                        .read()
                        .map(|privacy| {
                            clean_captured_content(
                                &privacy.sanitize_content(&truncate(&text, 2000)),
                            )
                        })
                        .unwrap_or_else(|_| clean_captured_content(&truncate(&text, 2000)));
                    if !is_meaningful_content(&content) && !content.starts_with("http") {
                        thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                    if !content.is_empty() && content != last_content {
                        last_content = content.clone();

                        let content_type = classify_content(&content);
                        let payload = ClipboardChangedPayload {
                            content: content.clone(),
                            content_type: content_type.clone(),
                            timestamp: Utc::now().to_rfc3339(),
                        };

                        let _ = app.emit("clipboard-changed", payload);

                        if let Some(task_id) =
                            active_task_id(&state).or_else(|| ensure_daily_capture_task(&state))
                        {
                            let db = state.db.clone();
                            let url = if content_type == "url" {
                                Some(content.clone())
                            } else {
                                None
                            };

                            tauri::async_runtime::spawn(async move {
                                let _ = database::events::insert_event_with_metadata(
                                    &db,
                                    task_id,
                                    if url.is_some() { "url" } else { "clipboard" }.to_string(),
                                    None,
                                    None,
                                    Some(content),
                                    url,
                                    Some("ClipboardContent".to_string()),
                                    Some("clipboard".to_string()),
                                    true,
                                    0,
                                )
                                .await;
                            });
                        }
                    }
                }
            }

            thread::sleep(Duration::from_millis(500));
        }
    });
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

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn classify_content(content: &str) -> String {
    let trimmed = content.trim();

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return "url".to_string();
    }

    let code_patterns = [
        "function ",
        "const ",
        "let ",
        "var ",
        "=>",
        "import ",
        "export ",
        "class ",
        "fn ",
        "pub ",
        "impl ",
        "SELECT ",
        "INSERT ",
        "UPDATE ",
        "DELETE ",
        "{",
        "};",
    ];

    if code_patterns
        .iter()
        .any(|pattern| trimmed.contains(pattern))
    {
        return "code".to_string();
    }

    if trimmed.is_empty() {
        "other".to_string()
    } else {
        "text".to_string()
    }
}
