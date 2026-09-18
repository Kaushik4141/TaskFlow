#![cfg(target_os = "linux")]

use std::{
    io::{self, Read},
    process::{Command, Stdio},
    sync::Arc,
};

use chrono::Utc;
use serde_json::Value;

use super::{
    privacy::PrivacyFilter,
    types::{CapturedContent, ContentType},
};

pub struct LinuxWindow {
    pub app_name: String,
    pub window_title: String,
    pub platform_handle: isize,
    pub pid: i32,
}

pub struct LinuxReader {
    privacy: Arc<PrivacyFilter>,
    ocr_enabled: bool,
}

impl LinuxReader {
    pub fn new(privacy: Arc<PrivacyFilter>) -> Self {
        Self {
            privacy,
            ocr_enabled: true,
        }
    }

    pub fn with_ocr(mut self, enabled: bool) -> Self {
        self.ocr_enabled = enabled;
        self
    }

    pub fn active_window() -> Option<LinuxWindow> {
        hyprland_active_window().or_else(x11_active_window)
    }

    pub fn read_window_content(
        &self,
        platform_handle: isize,
        pid: i32,
        app_name: &str,
        window_title: &str,
        task_id: String,
    ) -> Option<CapturedContent> {
        if !self.privacy.should_capture_window(app_name, window_title) || !self.ocr_enabled {
            return None;
        }

        if let Some(reason) = linux_ocr_unavailable_reason(platform_handle) {
            eprintln!("[taskflow:capture] Linux OCR unavailable: {reason}");
            return None;
        }

        let text = (if platform_handle != 0 {
            self.ocr_x11_window(platform_handle)
        } else {
            self.ocr_hyprland_window(pid, window_title)
        })?;

        let text = self
            .privacy
            .sanitize_content(&truncate(&text, 5000))
            .trim()
            .to_string();
        if text.is_empty() {
            return None;
        }

        Some(CapturedContent {
            content_type: content_type(app_name),
            app_name: app_name.to_string(),
            window_title: window_title.to_string(),
            text: Some(text),
            url: None,
            timestamp: Utc::now().to_rfc3339(),
            task_id,
            capture_method: "linux_ocr".to_string(),
        })
    }

    fn ocr_hyprland_window(&self, pid: i32, window_title: &str) -> Option<String> {
        let output = Command::new("hyprctl")
            .args(["clients", "-j"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }

        let clients: Vec<Value> = serde_json::from_slice(&output.stdout).ok()?;
        let window = clients.into_iter().find(|window| {
            window.get("pid").and_then(Value::as_i64) == Some(i64::from(pid))
                && window.get("title").and_then(Value::as_str) == Some(window_title)
        })?;
        let x = window.pointer("/at/x").and_then(Value::as_i64)?;
        let y = window.pointer("/at/y").and_then(Value::as_i64)?;
        let width = window.pointer("/size/width").and_then(Value::as_i64)?;
        let height = window.pointer("/size/height").and_then(Value::as_i64)?;
        if width <= 0 || height <= 0 {
            return None;
        }

        let geometry = format!("{x},{y} {width}x{height}");
        self.ocr_command(
            Command::new("grim")
                .arg("-g")
                .arg(geometry)
                .arg("-")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn(),
        )
    }

    fn ocr_x11_window(&self, window_id: isize) -> Option<String> {
        let import = Command::new("import")
            .arg("-window")
            .arg(window_id.to_string())
            .arg("png:-")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        if import.is_ok() {
            return self.ocr_command(import);
        }

        let maim = Command::new("maim")
            .arg("-i")
            .arg(window_id.to_string())
            .stdout(Stdio::piped())
            .spawn();
        self.ocr_command(maim)
    }

    fn ocr_command(&self, screenshot: std::io::Result<std::process::Child>) -> Option<String> {
        let mut screenshot = screenshot.ok()?;
        let mut image = screenshot.stdout.take()?;

        let mut tesseract = Command::new("tesseract")
            .arg("stdin")
            .arg("stdout")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .ok()?;
        let mut tesseract_stdin = tesseract.stdin.take()?;
        let copy_result = io::copy(&mut image, &mut tesseract_stdin);
        drop(tesseract_stdin);

        let screenshot_result = screenshot.wait();
        let mut screenshot_stderr = screenshot.stderr.take();
        let screenshot_error = screenshot_stderr
            .as_mut()
            .map(|stderr| {
                let mut message = String::new();
                let _ = stderr.read_to_string(&mut message);
                message.trim().to_string()
            })
            .unwrap_or_default();
        let output = tesseract.wait_with_output().ok()?;
        if let Err(error) = &copy_result {
            eprintln!("[taskflow:capture] OCR image pipe failed: {error}");
            return None;
        }
        if let Err(error) = &screenshot_result {
            eprintln!("[taskflow:capture] screenshot process failed: {error}");
            return None;
        }
        if !screenshot_result.is_ok_and(|status| status.success()) {
            eprintln!("[taskflow:capture] screenshot failed: {screenshot_error}");
            return None;
        }
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
            eprintln!("[taskflow:capture] tesseract failed: {error}");
            return None;
        }
        if !output.status.success() {
            return None;
        }

        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!text.is_empty()).then_some(text)
    }
}

fn linux_ocr_unavailable_reason(platform_handle: isize) -> Option<String> {
    let languages = Command::new("tesseract")
        .arg("--list-langs")
        .output()
        .ok()?;
    if !languages.status.success() {
        let error = String::from_utf8_lossy(&languages.stderr).trim().to_string();
        return Some(format!("tesseract could not list languages: {error}"));
    }

    let language_list = String::from_utf8_lossy(&languages.stdout);
    if !language_list.lines().any(|language| language.trim() == "eng") {
        return Some(
            "English OCR data is missing; install tesseract-data-eng".to_string(),
        );
    }

    let screenshot_tool = if platform_handle == 0 {
        "grim"
    } else {
        "import or maim"
    };
    let tool_exists = if platform_handle == 0 {
        Command::new("grim").arg("-h").output().is_ok_and(|output| {
            output.status.success()
        })
    } else {
        Command::new("import")
            .arg("-version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
            || Command::new("maim")
                .arg("--version")
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
    };
    if !tool_exists {
        return Some(format!("{screenshot_tool} is not installed"));
    }

    None
}

fn hyprland_active_window() -> Option<LinuxWindow> {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return None;
    }

    let output = Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let window: Value = serde_json::from_slice(&output.stdout).ok()?;
    let app_name = ["initialClass", "class"]
        .into_iter()
        .find_map(|field| window.get(field).and_then(Value::as_str))
        .map(str::trim)
        .filter(|app_name| !app_name.is_empty())?
        .to_string();
    if app_name.is_empty() {
        return None;
    }

    Some(LinuxWindow {
        app_name,
        window_title: window
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        platform_handle: 0,
        pid: window
            .get("pid")
            .and_then(Value::as_i64)
            .and_then(|pid| i32::try_from(pid).ok())
            .unwrap_or_default(),
    })
}

fn x11_active_window() -> Option<LinuxWindow> {
    let output = Command::new("xprop")
        .args(["-root", "-notype", "_NET_ACTIVE_WINDOW"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let active = String::from_utf8_lossy(&output.stdout);
    let window_id = active.lines().find_map(parse_x11_window_id).or_else(|| {
        active
            .trim()
            .split_whitespace()
            .last()
            .and_then(|value| parse_number(value))
    })?;

    let output = Command::new("xprop")
        .arg("-id")
        .arg(window_id.to_string())
        .args(["-notype", "WM_CLASS", "_NET_WM_NAME", "_NET_WM_PID"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let properties = String::from_utf8_lossy(&output.stdout);
    let app_name = properties
        .lines()
        .find_map(parse_x11_wm_class)
        .unwrap_or_else(|| "Unknown".to_string());
    let window_title = properties
        .lines()
        .find(|line| line.starts_with("_NET_WM_NAME"))
        .and_then(parse_x11_property_value)
        .unwrap_or_default();
    let pid = properties
        .lines()
        .find_map(parse_x11_pid)
        .unwrap_or_default();

    Some(LinuxWindow {
        app_name,
        window_title,
        platform_handle: window_id,
        pid,
    })
}

fn parse_x11_window_id(line: &str) -> Option<isize> {
    let value = line.split('#').nth(1)?.split(',').next()?.trim();
    parse_x11_number(value)
}

fn parse_x11_property_value(line: &str) -> Option<String> {
    let value = line
        .split_once('=')
        .or_else(|| line.split_once(':'))?
        .1
        .trim();
    let unquoted = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'));
    Some(unquoted.unwrap_or(value).trim().to_string())
}

fn parse_x11_wm_class(line: &str) -> Option<String> {
    let value = parse_x11_property_value(line)?;
    value
        .rsplit(',')
        .next()
        .map(|class| class.trim().trim_matches('"').to_string())
        .filter(|class| !class.is_empty())
}

fn parse_x11_pid(line: &str) -> Option<i32> {
    let value = line
        .split_once('=')
        .or_else(|| line.split_once(':'))?
        .1
        .trim();
    value.parse().ok()
}

fn parse_number(value: &str) -> Option<isize> {
    parse_x11_number(value.trim())
}

fn parse_x11_number(value: &str) -> Option<isize> {
    let parsed = if let Some(hex) = value.strip_prefix("0x") {
        i64::from_str_radix(hex, 16).ok()?
    } else {
        value.parse::<i64>().ok()?
    };
    isize::try_from(parsed).ok()
}

fn content_type(app_name: &str) -> ContentType {
    let app = app_name.to_lowercase();
    if [
        "chrome", "firefox", "safari", "edge", "brave", "arc", "opera",
    ]
    .iter()
    .any(|name| app.contains(name))
    {
        ContentType::BrowserContent
    } else if [
        "code", "vscodium", "pycharm", "intellij", "webstorm", "cursor", "sublime",
    ]
    .iter()
    .any(|name| app.contains(name))
    {
        ContentType::CodeContent
    } else if [
        "terminal",
        "iterm",
        "wezterm",
        "alacritty",
        "kitty",
        "foot",
        "konsole",
    ]
    .iter()
    .any(|name| app.contains(name))
    {
        ContentType::TerminalContent
    } else {
        ContentType::GenericContent
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_x11_active_window_id() {
        assert_eq!(
            parse_x11_window_id("_NET_ACTIVE_WINDOW: window id # 0x2c00007, 0x0"),
            Some(0x2c00007)
        );
    }

    #[test]
    fn parses_x11_window_metadata() {
        let class = "WM_CLASS: Navigator, \"firefox\"";
        let name = "_NET_WM_NAME: \"TaskFlow - Mozilla Firefox\"";
        let pid = "_NET_WM_PID: 12345";

        assert_eq!(parse_x11_wm_class(class), Some("firefox".to_string()));
        assert_eq!(
            parse_x11_property_value(name),
            Some("TaskFlow - Mozilla Firefox".to_string())
        );
        assert_eq!(parse_x11_pid(pid), Some(12345));
    }

    #[test]
    fn classifies_common_linux_apps() {
        assert!(matches!(
            content_type("Firefox"),
            ContentType::BrowserContent
        ));
        assert!(matches!(content_type("code"), ContentType::CodeContent));
        assert!(matches!(content_type("foot"), ContentType::TerminalContent));
        assert!(matches!(
            content_type("obsidian"),
            ContentType::GenericContent
        ));
    }
}
