#![cfg(target_os = "windows")]

use std::sync::Arc;

use chrono::Utc;
use windows::{
    core::BSTR,
    Win32::{
        Foundation::HWND,
        System::Com::{
            CoCreateInstance, CoInitializeEx, CLSCTX_ALL, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        },
        UI::Accessibility::{
            CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern,
            IUIAutomationValuePattern, TreeScope_Subtree, UIA_DocumentControlTypeId,
            UIA_EditControlTypeId, UIA_TextControlTypeId, UIA_TextPatternId, UIA_ValuePatternId,
        },
    },
};

use super::{
    ocr,
    privacy::PrivacyFilter,
    types::{CapturedContent, ContentType},
};

pub struct WindowsReader {
    privacy: Arc<PrivacyFilter>,
    ocr_enabled: bool,
}

enum AppType {
    Browser,
    Editor,
    Terminal,
    Generic,
}

impl WindowsReader {
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

    pub fn read_window_content(
        &self,
        hwnd: HWND,
        app_name: &str,
        window_title: &str,
        task_id: String,
    ) -> Option<CapturedContent> {
        if !self.privacy.should_capture_window(app_name, window_title) {
            eprintln!(
                "[taskflow:capture] Deep capture blocked by privacy filter: {} - {}",
                app_name, window_title
            );
            return None;
        }

        match app_type(app_name) {
            AppType::Browser => self.read_browser(hwnd, app_name, window_title, task_id.clone()),
            AppType::Editor => self.read_editor(hwnd, app_name, window_title, task_id.clone()),
            AppType::Terminal => self.read_terminal(hwnd, app_name, window_title, task_id.clone()),
            AppType::Generic => self.read_generic(hwnd, app_name, window_title, task_id.clone()),
        }
        // UI Automation found nothing readable (GPU-rendered apps like Warp /
        // WezTerm / Alacritty expose no UIA text). OCR is the last-resort
        // fallback: transient in-memory pixels, text-only output.
        .or_else(|| self.read_via_ocr(hwnd, app_name, window_title, task_id))
    }

    fn read_via_ocr(
        &self,
        hwnd: HWND,
        app_name: &str,
        window_title: &str,
        task_id: String,
    ) -> Option<CapturedContent> {
        if !self.ocr_enabled {
            return None;
        }
        let text = ocr::ocr_window_text(hwnd, 3000)?;
        eprintln!("[taskflow:capture] UIA unreadable; OCR fallback succeeded for {app_name}");
        Some(CapturedContent {
            content_type: match app_type(app_name) {
                AppType::Browser => ContentType::BrowserContent,
                AppType::Editor => ContentType::CodeContent,
                AppType::Terminal => ContentType::TerminalContent,
                AppType::Generic => ContentType::GenericContent,
            },
            app_name: app_name.to_string(),
            window_title: window_title.to_string(),
            text: Some(self.privacy.sanitize_content(&text)),
            url: None,
            timestamp: Utc::now().to_rfc3339(),
            task_id,
            capture_method: "ocr".to_string(),
        })
    }

    fn read_browser(
        &self,
        hwnd: HWND,
        app_name: &str,
        window_title: &str,
        task_id: String,
    ) -> Option<CapturedContent> {
        if app_name.to_lowercase().contains("chrome") {
            if let Some(content) =
                self.read_chrome_content(hwnd, app_name, window_title, task_id.clone())
            {
                return Some(content);
            }
            eprintln!(
                "[taskflow:capture] Chrome-specific UIAutomation failed; falling back to generic browser traversal"
            );
        }

        let root = root_element(hwnd).ok()?;
        let elements = collect_elements(&root, 180).ok();
        let url = elements
            .as_ref()
            .and_then(|items| find_address_bar_value(items, app_name));
        let text = elements
            .as_ref()
            .map(|items| collect_visible_text(items, 3000))
            .filter(|value| !value.trim().is_empty())
            .map(|value| self.privacy.sanitize_content(&value));

        Some(CapturedContent {
            content_type: ContentType::BrowserContent,
            app_name: app_name.to_string(),
            window_title: window_title.to_string(),
            text,
            url,
            timestamp: Utc::now().to_rfc3339(),
            task_id,
            capture_method: "uitautomation".to_string(),
        })
    }

    fn read_chrome_content(
        &self,
        hwnd: HWND,
        app_name: &str,
        window_title: &str,
        task_id: String,
    ) -> Option<CapturedContent> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL).ok()?;
            let root = automation.ElementFromHandle(hwnd).ok()?;

            let elements = collect_elements(&root, 240).ok()?;
            let url = elements
                .iter()
                .find(|element| {
                    current_automation_id(element)
                        .unwrap_or_default()
                        .eq_ignore_ascii_case("addressEditBox")
                })
                .and_then(value_pattern_text)
                .map(|value| value.trim().to_string())
                .filter(|value| value.starts_with("http://") || value.starts_with("https://"));

            let page_text = elements
                .iter()
                .find(|element| current_control_type(element) == Some(UIA_DocumentControlTypeId))
                .and_then(|element| text_pattern_text_limited(element, 3000))
                .filter(|value| !value.trim().is_empty())
                .map(|value| self.privacy.sanitize_content(&value));

            if url.is_none() && page_text.is_none() {
                return None;
            }

            Some(CapturedContent {
                content_type: ContentType::BrowserContent,
                app_name: app_name.to_string(),
                window_title: window_title.to_string(),
                text: page_text,
                url,
                timestamp: Utc::now().to_rfc3339(),
                task_id,
                capture_method: "uitautomation_chrome".to_string(),
            })
        }
    }

    fn read_editor(
        &self,
        hwnd: HWND,
        app_name: &str,
        window_title: &str,
        task_id: String,
    ) -> Option<CapturedContent> {
        let root = root_element(hwnd).ok()?;
        let elements = collect_elements(&root, 160).ok()?;
        let text = elements
            .iter()
            .filter_map(text_pattern_text)
            .chain(elements.iter().filter_map(value_pattern_text))
            .max_by_key(|value| value.len())
            .map(|value| truncate(&value, 5000))
            .map(|value| self.privacy.sanitize_content(&value))?;

        Some(CapturedContent {
            content_type: ContentType::CodeContent,
            app_name: app_name.to_string(),
            window_title: window_title.to_string(),
            text: Some(text),
            url: None,
            timestamp: Utc::now().to_rfc3339(),
            task_id,
            capture_method: "uitautomation".to_string(),
        })
    }

    fn read_terminal(
        &self,
        hwnd: HWND,
        app_name: &str,
        window_title: &str,
        task_id: String,
    ) -> Option<CapturedContent> {
        let root = root_element(hwnd).ok()?;
        let elements = collect_elements(&root, 140).ok()?;
        let buffer = elements
            .iter()
            .filter_map(text_pattern_text)
            .chain(elements.iter().filter_map(value_pattern_text))
            .max_by_key(|value| value.len())
            .unwrap_or_else(|| collect_visible_text(&elements, 5000));
        let commands = extract_recent_commands(&buffer);
        let text = if commands.trim().is_empty() {
            // TUI programs (vim, Claude Code, htop, SSH sessions) render text
            // with no prompt-prefixed command lines, so the command extractor
            // finds nothing. Falling back to the raw buffer tail keeps those
            // sessions from being lost entirely.
            let tail = buffer_tail(&buffer, 15, 2000);
            if tail.is_empty() {
                return None;
            }
            tail
        } else {
            commands
        };

        Some(CapturedContent {
            content_type: ContentType::TerminalContent,
            app_name: app_name.to_string(),
            window_title: window_title.to_string(),
            text: Some(self.privacy.sanitize_content(&text)),
            url: None,
            timestamp: Utc::now().to_rfc3339(),
            task_id,
            capture_method: "uitautomation".to_string(),
        })
    }

    fn read_generic(
        &self,
        hwnd: HWND,
        app_name: &str,
        window_title: &str,
        task_id: String,
    ) -> Option<CapturedContent> {
        let root = root_element(hwnd).ok()?;
        let elements = collect_elements(&root, 120).ok()?;
        let text = collect_visible_text(&elements, 2000);
        if text.trim().is_empty() {
            return None;
        }

        Some(CapturedContent {
            content_type: ContentType::GenericContent,
            app_name: app_name.to_string(),
            window_title: window_title.to_string(),
            text: Some(self.privacy.sanitize_content(&text)),
            url: None,
            timestamp: Utc::now().to_rfc3339(),
            task_id,
            capture_method: "uitautomation".to_string(),
        })
    }
}

fn root_element(hwnd: HWND) -> windows::core::Result<IUIAutomationElement> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        automation.ElementFromHandle(hwnd)
    }
}

fn collect_elements(
    root: &IUIAutomationElement,
    limit: i32,
) -> windows::core::Result<Vec<IUIAutomationElement>> {
    unsafe {
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let condition = automation.CreateTrueCondition()?;
        let array = root.FindAll(TreeScope_Subtree, &condition)?;
        let len = array.Length()?.min(limit);
        let mut elements = Vec::with_capacity(len as usize);
        for index in 0..len {
            if let Ok(element) = array.GetElement(index) {
                elements.push(element);
            }
        }
        Ok(elements)
    }
}

fn find_address_bar_value(elements: &[IUIAutomationElement], app_name: &str) -> Option<String> {
    let app = app_name.to_lowercase();
    for element in elements {
        let automation_id = current_automation_id(element)
            .unwrap_or_default()
            .to_lowercase();
        let class_name = current_class_name(element)
            .unwrap_or_default()
            .to_lowercase();
        let control_type = current_control_type(element);
        let likely_address = (app.contains("firefox") && automation_id == "urlbar-input")
            || automation_id == "addresseditbox"
            || class_name.contains("omniboxviewviews")
            || control_type == Some(UIA_EditControlTypeId);
        if !likely_address {
            continue;
        }
        if let Some(value) = value_pattern_text(element) {
            let trimmed = value.trim().to_string();
            if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
                return Some(trimmed);
            }
        }
    }
    None
}

fn collect_visible_text(elements: &[IUIAutomationElement], max_chars: usize) -> String {
    let mut lines = Vec::new();
    let mut total = 0usize;

    for element in elements {
        if total >= max_chars {
            break;
        }
        let control_type = current_control_type(element);
        let readable_control = control_type == Some(UIA_TextControlTypeId)
            || control_type == Some(UIA_DocumentControlTypeId)
            || control_type == Some(UIA_EditControlTypeId);
        if !readable_control {
            continue;
        }

        let text = text_pattern_text(element)
            .or_else(|| value_pattern_text(element))
            .or_else(|| current_name(element))
            .unwrap_or_default();
        let cleaned = text.trim();
        if cleaned.len() < 3 || cleaned.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        total += cleaned.len();
        lines.push(cleaned.to_string());
    }

    truncate(&lines.join("\n"), max_chars)
}

fn text_pattern_text(element: &IUIAutomationElement) -> Option<String> {
    text_pattern_text_limited(element, 5000)
}

fn text_pattern_text_limited(element: &IUIAutomationElement, max_chars: i32) -> Option<String> {
    unsafe {
        let pattern: IUIAutomationTextPattern =
            element.GetCurrentPatternAs(UIA_TextPatternId).ok()?;
        let range = pattern.DocumentRange().ok()?;
        Some(bstr_to_string(range.GetText(max_chars).ok()?))
    }
    .filter(|value| !value.trim().is_empty())
}

fn value_pattern_text(element: &IUIAutomationElement) -> Option<String> {
    unsafe {
        let pattern: IUIAutomationValuePattern =
            element.GetCurrentPatternAs(UIA_ValuePatternId).ok()?;
        Some(bstr_to_string(pattern.CurrentValue().ok()?))
    }
    .filter(|value| !value.trim().is_empty())
}

fn current_name(element: &IUIAutomationElement) -> Option<String> {
    unsafe { element.CurrentName().ok().map(bstr_to_string) }
}

fn current_automation_id(element: &IUIAutomationElement) -> Option<String> {
    unsafe { element.CurrentAutomationId().ok().map(bstr_to_string) }
}

fn current_class_name(element: &IUIAutomationElement) -> Option<String> {
    unsafe { element.CurrentClassName().ok().map(bstr_to_string) }
}

fn current_control_type(
    element: &IUIAutomationElement,
) -> Option<windows::Win32::UI::Accessibility::UIA_CONTROLTYPE_ID> {
    unsafe { element.CurrentControlType().ok() }
}

fn bstr_to_string(value: BSTR) -> String {
    value.to_string()
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
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

/// The last `max_lines` non-empty lines of a terminal buffer, oldest first,
/// capped at `max_chars` total. Used when the buffer has no command-looking
/// lines (TUI apps) so the session still leaves a text trace.
fn buffer_tail(buffer: &str, max_lines: usize, max_chars: usize) -> String {
    let mut lines: Vec<&str> = buffer
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.len() > max_lines {
        lines = lines.split_off(lines.len() - max_lines);
    }
    let joined = lines.join("\n");
    if joined.len() <= max_chars {
        joined
    } else {
        joined[joined.len() - max_chars..].to_string()
    }
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
        "pycharm",
        "intellij",
        "webstorm",
        "vim",
        "nvim",
        "cursor",
        "sublime",
        "notepad++",
    ]
    .iter()
    .any(|name| app.contains(name))
    {
        AppType::Editor
    } else if [
        "terminal",
        "iterm",
        "wezterm",
        "alacritty",
        "cmd",
        "powershell",
        "bash",
        "zsh",
        "kitty",
    ]
    .iter()
    .any(|name| app.contains(name))
    {
        AppType::Terminal
    } else {
        AppType::Generic
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_tail_keeps_last_nonempty_lines_oldest_first() {
        let buffer = "old line\n\n\nmid line\nlast line\n";
        assert_eq!(buffer_tail(buffer, 15, 2000), "old line\nmid line\nlast line");
    }

    #[test]
    fn buffer_tail_respects_line_cap() {
        let buffer: String = (0..30).map(|i| format!("line{i}\n")).collect();
        let tail = buffer_tail(&buffer, 15, 2000);
        assert_eq!(tail.lines().count(), 15, "capped to last 15 lines: {tail}");
        assert!(tail.starts_with("line15"), "oldest kept line first: {tail}");
        assert!(tail.ends_with("line29"), "newest line last: {tail}");
    }

    #[test]
    fn buffer_tail_respects_char_cap_and_empty_input() {
        assert_eq!(buffer_tail("", 15, 2000), "");
        assert_eq!(buffer_tail("   \n  \n", 15, 2000), "");
        let long = "x".repeat(5000);
        assert_eq!(buffer_tail(&long, 15, 2000).len(), 2000);
    }
}
