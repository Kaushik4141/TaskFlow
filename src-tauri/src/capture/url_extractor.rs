#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ExtractedUrl {
    pub page_title: String,
    pub domain: Option<String>,
    pub likely_url: Option<String>,
    pub search_query: Option<String>,
    pub is_documentation: bool,
    pub is_search: bool,
    pub is_issue_tracker: bool,
    pub is_code_repository: bool,
}

pub fn extract_from_title(app_name: &str, window_title: &str) -> Option<ExtractedUrl> {
    let app = app_name.to_lowercase();
    let browser = [
        "chrome", "firefox", "edge", "brave", "arc", "opera", "safari",
    ]
    .iter()
    .any(|name| app.contains(name));
    if !browser && !looks_like_known_site(window_title) {
        return None;
    }

    let page_title = strip_browser_suffix(window_title);
    let (domain, query) = infer_domain_and_query(&page_title);
    let likely_url = domain.as_ref().map(|domain| format!("https://{domain}"));
    let is_search = domain.as_deref() == Some("google.com");
    let is_issue_tracker = matches!(domain.as_deref(), Some("jira") | Some("linear.app"));
    let is_code_repository = domain
        .as_deref()
        .map(is_code_repository_site)
        .unwrap_or(false);
    let is_documentation = domain
        .as_deref()
        .map(is_documentation_site)
        .unwrap_or(false);

    Some(ExtractedUrl {
        page_title,
        domain,
        likely_url,
        search_query: query,
        is_documentation,
        is_search,
        is_issue_tracker,
        is_code_repository,
    })
}

fn strip_browser_suffix(window_title: &str) -> String {
    let known_suffixes = [
        " - Google Chrome",
        " - Mozilla Firefox",
        " - Microsoft Edge",
        " - Brave",
        " - Arc",
        " - Opera",
        " - Safari",
    ];
    known_suffixes
        .iter()
        .find_map(|suffix| window_title.strip_suffix(suffix))
        .unwrap_or(window_title)
        .trim()
        .to_string()
}

fn infer_domain_and_query(title: &str) -> (Option<String>, Option<String>) {
    let lower = title.to_lowercase();
    let domain = if lower.contains("stack overflow") {
        Some("stackoverflow.com")
    } else if lower.contains("github") {
        Some("github.com")
    } else if lower.contains("mdn web docs") {
        Some("developer.mozilla.org")
    } else if lower.contains("npm") {
        Some("npmjs.com")
    } else if lower.contains("pypi") {
        Some("pypi.org")
    } else if lower.contains("google drive") {
        Some("drive.google.com")
    } else if lower.contains("google forms") {
        Some("docs.google.com/forms")
    } else if lower.contains("google docs") {
        Some("docs.google.com")
    } else if lower.contains("google sheets") {
        Some("docs.google.com/spreadsheets")
    } else if lower.contains("google slides") {
        Some("docs.google.com/presentation")
    } else if lower.contains("gmail") {
        Some("mail.google.com")
    } else if lower.starts_with("google - ") || lower.ends_with("- google search") {
        Some("google.com")
    } else if lower.contains("notion") {
        Some("notion.so")
    } else if lower.contains("jira") || lower.contains("atlassian") {
        Some("jira")
    } else if lower.contains("linear") {
        Some("linear.app")
    } else if lower.contains("figma") {
        Some("figma.com")
    } else if lower.contains("vercel") {
        Some("vercel.com")
    } else {
        None
    };

    let query = if domain == Some("google.com") {
        Some(
            title
                .replace("Google -", "")
                .replace("- Google Search", "")
                .trim()
                .to_string(),
        )
    } else if domain == Some("stackoverflow.com") {
        Some(
            title
                .replace("Stack Overflow -", "")
                .replace("- Stack Overflow", "")
                .trim()
                .to_string(),
        )
    } else {
        None
    }
    .filter(|value| !value.is_empty());

    (domain.map(ToString::to_string), query)
}

fn looks_like_known_site(title: &str) -> bool {
    let lower = title.to_lowercase();
    [
        "stack overflow",
        "github",
        "mdn web docs",
        "npm",
        "pypi",
        "google",
        "notion",
        "jira",
        "atlassian",
        "linear",
        "figma",
        "vercel",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn is_documentation_site(domain: &str) -> bool {
    domain.contains("mdn")
        || domain.starts_with("docs.")
        || domain.starts_with("developer.")
        || domain.contains("wiki.")
        || domain.contains("readme.com")
        || domain.contains("gitbook.io")
        || domain.contains("readthedocs.io")
}

fn is_code_repository_site(domain: &str) -> bool {
    matches!(
        domain,
        "github.com" | "gitlab.com" | "bitbucket.org" | "codeberg.org" | "sourcehut.org"
    )
}
