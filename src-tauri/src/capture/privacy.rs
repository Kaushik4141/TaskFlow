use regex::Regex;

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
    // records the vault recording itself.
    "obsidian",
];

const EXCLUDED_WINDOW_PATTERNS: &[&str] = &[
    "private browsing",
    "incognito",
    "inprivate",
    "private - ",
    " - private",
];

const SENSITIVE_FILE_PATTERNS: &[&str] = &[
    ".env",
    "credentials.json",
    "client_secret",
    "id_rsa",
    "id_ed25519",
    "id_ecdsa",
    "id_dsa",
    ".pem",
    ".key",
    ".keystore",
    ".pfx",
    ".p12",
    "secrets.yaml",
    "secrets.yml",
    "secrets.json",
    "wp-config.php",
    "token.json",
    "access_tokens",
];

const SENSITIVE_CONTENT_PATTERNS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "api_key",
    "api key",
    "apikey",
    "private key",
    "access token",
    "access_token",
    "authorization",
    "bearer ",
    "credit card",
    "ssn",
    "social security",
];

/// Calculate the Shannon entropy in bits per character for a string.
/// High entropy (> 4.0 for length >= 16) indicates cryptographic keys,
/// tokens, or random passwords.
pub fn shannon_entropy(s: &str) -> f32 {
    if s.is_empty() {
        return 0.0;
    }
    let mut counts = std::collections::HashMap::new();
    let mut total = 0usize;
    for ch in s.chars() {
        *counts.entry(ch).or_insert(0usize) += 1;
        total += 1;
    }
    let total_f = total as f32;
    let mut entropy = 0.0;
    for &count in counts.values() {
        let p = count as f32 / total_f;
        entropy -= p * p.log2();
    }
    entropy
}

/// Checks if a single word or token is likely a secret based on length,
/// character complexity (mixed alphanumeric/symbols), and Shannon entropy.
pub fn is_high_entropy_secret(token: &str) -> bool {
    let len = token.chars().count();
    if len < 18 || len > 256 {
        return false;
    }
    // URLs, domain paths, and common file paths are not raw secrets
    if token.starts_with("http")
        || token.contains("://")
        || token.contains('/')
        || token.contains('\\')
    {
        return false;
    }
    // Must contain characters typical of encoded keys (both letters and numbers/symbols)
    let has_letters = token.chars().any(|c| c.is_alphabetic());
    let has_digits_or_symbols =
        token.chars().any(|c| c.is_numeric() || "-_!@#$%^&*+=~".contains(c));
    if !has_letters || !has_digits_or_symbols {
        return false;
    }
    shannon_entropy(token) >= 4.0
}

#[derive(Debug, Clone)]
pub struct PrivacyFilter {
    excluded_apps: Vec<String>,
    excluded_patterns: Vec<String>,
    sensitive_file_patterns: Vec<String>,
    secret_regexes: Vec<Regex>,
    uri_credentials_regex: Regex,
    assignment_regex: Regex,
    private_key_regex: Regex,
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

        let secret_patterns = [
            r"sk-[a-zA-Z0-9_\-]{20,}",
            r"(?:ghp|gho|ghu|ghs|ghr)_[a-zA-Z0-9]{36}",
            r"github_pat_[a-zA-Z0-9_]{82}",
            r"AKIA[0-9A-Z]{16}",
            r"AIza[0-9A-Za-z\-_]{35}",
            r"xox[baprs]-[0-9a-zA-Z]{10,48}",
            r"(?:sk|rk)_(?:live|test)_[0-9a-zA-Z]{24,}",
            r"eyJ[A-Za-z0-9\-_=]+\.eyJ[A-Za-z0-9\-_=]+\.?[A-Za-z0-9\-_.+/=]*",
        ];
        let secret_regexes = secret_patterns
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();

        let uri_credentials_regex =
            Regex::new(r"([a-zA-Z][a-zA-Z0-9+.\-]*://)([^:\s/@]+):([^/\s]+)@")
                .expect("valid uri creds regex");

        let assignment_regex = Regex::new(
            r"(?i)(api[_\-]?key|access[_\-]?token|secret[_\-]?key|password|passwd|auth[_\-]?token|client[_\-]?secret)\s*([:=])\s*['\x22]?([^\s'\x22#;]+)['\x22]?",
        )
        .expect("valid assignment regex");

        let private_key_regex = Regex::new(
            r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
        )
        .expect("valid private key regex");

        Self {
            excluded_apps,
            excluded_patterns: EXCLUDED_WINDOW_PATTERNS
                .iter()
                .map(|value| value.to_lowercase())
                .collect(),
            sensitive_file_patterns: SENSITIVE_FILE_PATTERNS
                .iter()
                .map(|value| value.to_lowercase())
                .collect(),
            secret_regexes,
            uri_credentials_regex,
            assignment_regex,
            private_key_regex,
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

    /// Checks if a window is viewing a sensitive file (e.g. .env, id_rsa, credentials.json)
    /// where deep text capture should be completely disabled (downgraded to metadata-only).
    pub fn is_sensitive_window_for_deep_capture(&self, window_title: &str) -> bool {
        let title = window_title.to_lowercase();
        self.sensitive_file_patterns
            .iter()
            .any(|pattern| title.contains(pattern))
    }

    pub fn sanitize_content(&self, content: &str) -> String {
        // 1. Redact full private key blocks
        let step1 = self
            .private_key_regex
            .replace_all(content, "[REDACTED_PRIVATE_KEY]");

        // 2. Redact URI embedded credentials (e.g. postgres://user:pass@host)
        let step2 = self
            .uri_credentials_regex
            .replace_all(&step1, "${1}[REDACTED]:[REDACTED]@");

        // 3. Redact key-value credential assignments
        let step3 = self
            .assignment_regex
            .replace_all(&step2, "${1}${2}[REDACTED]");

        // 4. Redact known token signatures
        let mut step4 = step3.to_string();
        for re in &self.secret_regexes {
            step4 = re.replace_all(&step4, "[REDACTED]").to_string();
        }

        // 5. Line-by-line pass for keyword assignments and Shannon entropy
        step4
            .lines()
            .map(|line| {
                let mut sanitized_line = line.to_string();

                // Keyword check fallback
                if self.is_sensitive_content(&sanitized_line) {
                    if let Some((key, _value)) = sanitized_line.split_once('=') {
                        sanitized_line = format!("{}=[REDACTED]", key.trim_end());
                    } else if let Some((key, _value)) = sanitized_line.split_once(':') {
                        sanitized_line = format!("{}: [REDACTED]", key.trim_end());
                    }
                }

                // Token-level Shannon entropy check to catch unknown secrets/tokens
                let tokens: Vec<String> = sanitized_line
                    .split(|c: char| c.is_whitespace() || "=,:;\"'()<>{}[]@".contains(c))
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
                for token in tokens {
                    if token.contains("REDACTED") {
                        continue;
                    }
                    if is_high_entropy_secret(&token) {
                        sanitized_line = sanitized_line.replace(&token, "[REDACTED]");
                    }
                }

                sanitized_line
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

    /// Determines if a clipboard text copy is a secret/token/password and should be quarantined.
    pub fn is_clipboard_quarantined(&self, content: &str) -> bool {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return false;
        }

        // Clean HTTP(S) URLs without credentials are safe
        if (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
            && !self.uri_credentials_regex.is_match(trimmed)
        {
            return false;
        }

        // 1. Private keys in clipboard
        if self.private_key_regex.is_match(trimmed)
            || (trimmed.contains("BEGIN ") && trimmed.contains("PRIVATE KEY"))
        {
            return true;
        }

        // 2. Known token signatures (OpenAI, GitHub, AWS, JWT, Stripe, etc.)
        if self.secret_regexes.iter().any(|re| re.is_match(trimmed)) {
            return true;
        }

        // 3. Contains URI credentials
        if self.uri_credentials_regex.is_match(trimmed) {
            return true;
        }

        // 4. Single-token secret quarantine (e.g. copied password, raw API token)
        // If single line, length 8..=128, no whitespace, and high entropy or mixed chars
        if !trimmed.contains(char::is_whitespace) {
            let len = trimmed.chars().count();
            if (8..=128).contains(&len) {
                if is_high_entropy_secret(trimmed)
                    || (shannon_entropy(trimmed) >= 3.9
                        && !trimmed.contains('/')
                        && !trimmed.contains('.')
                        && trimmed.chars().any(|c| c.is_numeric() || !c.is_alphanumeric()))
                {
                    return true;
                }
            }
        }

        false
    }
}

impl Default for PrivacyFilter {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_known_secret_signatures() {
        let filter = PrivacyFilter::default();

        let openai_key = "sk-proj-abc123xyz456def789ghi012jkl345mno678pqr901stu234";
        let text = format!("export OPENAI_API_KEY=\"{}\"", openai_key);
        let sanitized = filter.sanitize_content(&text);
        assert!(!sanitized.contains(openai_key), "OpenAI key must be redacted: {sanitized}");
        assert!(sanitized.contains("[REDACTED]"), "Must contain REDACTED marker");

        let github_token = "ghp_123456789012345678901234567890123456";
        let text = format!("curl -H 'Authorization: token {}' https://api.github.com", github_token);
        let sanitized = filter.sanitize_content(&text);
        assert!(!sanitized.contains(github_token), "GitHub token must be redacted: {sanitized}");

        let aws_key = "AKIAIOSFODNN7EXAMPLE";
        let text = format!("AWS_ACCESS_KEY_ID={}", aws_key);
        let sanitized = filter.sanitize_content(&text);
        assert!(!sanitized.contains(aws_key), "AWS key must be redacted: {sanitized}");
    }

    #[test]
    fn redacts_database_uri_credentials() {
        let filter = PrivacyFilter::default();
        let uri = "DATABASE_URL=postgres://superadmin:MegaSecretP@ssw0rd!@db.internal:5432/production";
        let sanitized = filter.sanitize_content(uri);
        assert!(!sanitized.contains("MegaSecretP@ssw0rd!"), "URI password must be redacted: {sanitized}");
        assert!(sanitized.contains("postgres://[REDACTED]:[REDACTED]@db.internal:5432/production"), "Structure preserved: {sanitized}");
    }

    #[test]
    fn shannon_entropy_detects_unknown_random_tokens() {
        let filter = PrivacyFilter::default();
        let custom_secret = "xK9#mQ2$pL8*vR1!zW4@tY7";
        assert!(is_high_entropy_secret(custom_secret), "Entropy must detect random custom token");

        let text = format!("const token = \"{}\";", custom_secret);
        let sanitized = filter.sanitize_content(&text);
        assert!(!sanitized.contains(custom_secret), "Custom secret must be redacted by entropy: {sanitized}");
    }

    #[test]
    fn detects_sensitive_windows_for_deep_capture_gating() {
        let filter = PrivacyFilter::default();
        assert!(filter.is_sensitive_window_for_deep_capture(".env - TaskFlow - Cursor"));
        assert!(filter.is_sensitive_window_for_deep_capture(".env.production - vim"));
        assert!(filter.is_sensitive_window_for_deep_capture("credentials.json"));
        assert!(filter.is_sensitive_window_for_deep_capture("id_rsa - nano"));
        assert!(!filter.is_sensitive_window_for_deep_capture("main.rs - TaskFlow - Cursor"));
        assert!(!filter.is_sensitive_window_for_deep_capture("README.md - Firefox"));
    }

    #[test]
    fn quarantines_sensitive_clipboard_copies() {
        let filter = PrivacyFilter::default();

        // Direct token or password copies must be quarantined
        assert!(filter.is_clipboard_quarantined("ghp_123456789012345678901234567890123456"));
        assert!(filter.is_clipboard_quarantined("sk-proj-abc123xyz456def789ghi012jkl345mno678"));
        assert!(filter.is_clipboard_quarantined("MyP@ssw0rd!X9#z123"));

        // Normal text and clean URLs must NOT be quarantined
        assert!(!filter.is_clipboard_quarantined("https://github.com/Kaushik4141/TaskFlow"));
        assert!(!filter.is_clipboard_quarantined("git checkout -b feature/auth"));
        assert!(!filter.is_clipboard_quarantined("This is a regular sentence copied from documentation."));
    }
}

