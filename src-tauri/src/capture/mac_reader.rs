#![cfg(target_os = "macos")]

use std::{process::Command, sync::Arc};

use chrono::Utc;

use super::{
    privacy::PrivacyFilter,
    types::{CapturedContent, ContentType},
};

pub struct MacReader {
    privacy: Arc<PrivacyFilter>,
}

impl MacReader {
    pub fn new(privacy: Arc<PrivacyFilter>) -> Self {
        Self { privacy }
    }

    pub fn read_window_content(
        &self,
        _pid: i32,
        app_name: &str,
        window_title: &str,
        task_id: String,
    ) -> Option<CapturedContent> {
        if !self.privacy.should_capture_window(app_name, window_title) {
            return None;
        }

        let app_type = app_type(app_name);
        let (content_type, text, url) = match app_type {
            AppType::Browser => (
                ContentType::BrowserContent,
                run_osascript(browser_text_script(app_name)),
                run_osascript(browser_url_script(app_name)),
            ),
            AppType::Editor => (
                ContentType::CodeContent,
                run_osascript(focused_value_script()),
                None,
            ),
            AppType::Terminal => (
                ContentType::TerminalContent,
                run_osascript(focused_value_script()).map(|text| extract_recent_commands(&text)),
                None,
            ),
            AppType::Generic => (
                ContentType::GenericContent,
                run_osascript(focused_value_script()),
                None,
            ),
        };

        let text = text
            .filter(|value| !value.trim().is_empty())
            .map(|value| self.privacy.sanitize_content(&truncate(&value, 5000)));

        if text.is_none() && url.is_none() {
            return None;
        }

        Some(CapturedContent {
            content_type,
            app_name: app_name.to_string(),
            window_title: window_title.to_string(),
            text,
            url,
            timestamp: Utc::now().to_rfc3339(),
            task_id,
            capture_method: "axapi".to_string(),
        })
    }
}

enum AppType {
    Browser,
    Editor,
    Terminal,
    Generic,
}

fn run_osascript(script: String) -> Option<String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn browser_url_script(app_name: &str) -> String {
    let app = app_name.replace('"', "");
    format!(
        r#"tell application "{app}" to try
  return URL of active tab of front window
end try"#
    )
}

fn browser_text_script(_app_name: &str) -> String {
    focused_value_script()
}

fn focused_value_script() -> String {
    r#"tell application "System Events"
  set frontApp to first process whose frontmost is true
  try
    set focusedElement to value of attribute "AXFocusedUIElement" of frontApp
    try
      return value of focusedElement
    on error
      try
        return title of focusedElement
      end try
    end try
  end try
end tell"#
        .to_string()
}

fn extract_recent_commands(buffer: &str) -> String {
    let mut commands = buffer
        .lines()
        .rev()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.len() > 180 {
                return None;
            }
            if ["$ ", "> ", "# ", "❯ ", "→ "]
                .iter()
                .any(|prompt| trimmed.starts_with(prompt))
                || trimmed.starts_with("npm ")
                || trimmed.starts_with("git ")
                || trimmed.starts_with("cargo ")
                || trimmed.starts_with("pnpm ")
                || trimmed.starts_with("python ")
            {
                Some(trimmed.to_string())
            } else {
                None
            }
        })
        .take(10)
        .collect::<Vec<_>>();
    commands.reverse();
    commands.join("\n")
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn app_type(app_name: &str) -> AppType {
    let app = app_name.to_lowercase();
    if [
        "chrome", "firefox", "safari", "edge", "brave", "arc", "opera",
    ]
    .iter()
    .any(|name| app.contains(name))
    {
        AppType::Browser
    } else if [
        "code",
        "vscodium",
        "antigravity",
        "xcode",
        "pycharm",
        "intellij",
        "webstorm",
        "cursor",
        "sublime",
    ]
    .iter()
    .any(|name| app.contains(name))
    {
        AppType::Editor
    } else if ["terminal", "iterm", "wezterm", "alacritty", "kitty"]
        .iter()
        .any(|name| app.contains(name))
    {
        AppType::Terminal
    } else {
        AppType::Generic
    }
}
