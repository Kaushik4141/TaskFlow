#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ContentType {
    BrowserContent,
    CodeContent,
    TerminalContent,
    GenericContent,
    ClipboardContent,
    WindowTitleOnly,
}

impl ContentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BrowserContent => "BrowserContent",
            Self::CodeContent => "CodeContent",
            Self::TerminalContent => "TerminalContent",
            Self::GenericContent => "GenericContent",
            Self::ClipboardContent => "ClipboardContent",
            Self::WindowTitleOnly => "WindowTitleOnly",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CapturedContent {
    pub content_type: ContentType,
    pub app_name: String,
    pub window_title: String,
    pub text: Option<String>,
    pub url: Option<String>,
    pub timestamp: String,
    pub task_id: String,
    pub capture_method: String,
}
