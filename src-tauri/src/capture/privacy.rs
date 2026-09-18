const EXCLUDED_APPS: &[&str] = &[
    "1password",
    "bitwarden",
    "lastpass",
    "keepass",
    "dashlane",
    "keychain",
    "kwallet",
    "mint",
    "quicken",
    "turbotax",
    "spotify",
    "music",
    "vlc",
    "mpv",
    "mplayer",
    "netflix",
    "prime video",
    "disney",
    "keychain access",
    "credential manager",
    "certificate manager",
    // Obsidian is TaskFlow's OUTPUT, not an activity source: capturing it
    // records the vault recording itself (window titles like "Graph view -
    // Obsidian Vault" and UIA dumps of TaskFlow's own notes were polluting
    // workstream nodes with recursive meta-content). The user's genuine
    // Obsidian notes already live in the vault, so nothing is lost.
    "obsidian",
];

const EXCLUDED_WINDOW_PATTERNS: &[&str] = &[
    "private browsing",
    "incognito",
    "inprivate",
    "private - ",
    " - private",
];

const SENSITIVE_CONTENT_PATTERNS: &[&str] = &[
    "password",
    "secret",
    "api_key",
    "api key",
    "private key",
    "access token",
    "authorization",
    "credit card",
    "ssn",
    "social security",
];

#[derive(Debug, Clone)]
pub struct PrivacyFilter {
    excluded_apps: Vec<String>,
    excluded_patterns: Vec<String>,
}

impl PrivacyFilter {
    pub fn new(user_excluded_apps: Vec<String>) -> Self {
        let mut excluded_apps = EXCLUDED_APPS
            .iter()
            .map(|value| value.to_lowercase())
            .collect::<Vec<_>>();
        excluded_apps.extend(
            user_excluded_apps
                .into_iter()
                .map(|value| value.trim().to_lowercase())
                .filter(|value| !value.is_empty()),
        );
        excluded_apps.sort();
        excluded_apps.dedup();

        Self {
            excluded_apps,
            excluded_patterns: EXCLUDED_WINDOW_PATTERNS
                .iter()
                .map(|value| value.to_lowercase())
                .collect(),
        }
    }

    pub fn should_capture_window(&self, app_name: &str, window_title: &str) -> bool {
        let app = app_name.to_lowercase();
        let title = window_title.to_lowercase();

        if app.contains("taskflow") {
            return false;
        }

        !self
            .excluded_apps
            .iter()
            .any(|excluded| app.contains(excluded))
            && !self
                .excluded_patterns
                .iter()
                .any(|pattern| title.contains(pattern))
    }

    pub fn sanitize_content(&self, content: &str) -> String {
        content
            .lines()
            .map(|line| {
                if !self.is_sensitive_content(line) {
                    return line.to_string();
                }

                if let Some((key, _value)) = line.split_once('=') {
                    return format!("{}=[REDACTED]", key.trim_end());
                }
                if let Some((key, _value)) = line.split_once(':') {
                    return format!("{}: [REDACTED]", key.trim_end());
                }

                SENSITIVE_CONTENT_PATTERNS
                    .iter()
                    .find_map(|pattern| {
                        let lower = line.to_lowercase();
                        lower.find(pattern).map(|index| {
                            let end = index + pattern.len();
                            format!("{}[REDACTED]", &line[..end])
                        })
                    })
                    .unwrap_or_else(|| "[REDACTED]".to_string())
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn is_sensitive_content(&self, content: &str) -> bool {
        let lower = content.to_lowercase();
        SENSITIVE_CONTENT_PATTERNS
            .iter()
            .any(|pattern| lower.contains(pattern))
    }
}

impl Default for PrivacyFilter {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}
