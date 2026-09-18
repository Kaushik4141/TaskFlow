use std::collections::BTreeMap;

use url::Url;

use crate::database::events::Event;
use crate::database::tasks::Task;

use super::{slugify_hub, strip_www};

/// Normalize an executable-style app name (e.g. `chrome.exe`, `Code.exe`, "VSCode")
/// into a hub slug suitable for `[[Apps/<slug>]]`.
///
/// Rule: strip common extensions, replace runs of non-alphanumeric with a single `-`,
/// collapse empty tokens, lowercase. Returns None for empty/unknown.
pub fn hub_slug_for_app(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Extension strip is case-insensitive: `Explorer.EXE` and `explorer.exe`
    // must collapse to the same hub page, otherwise case variants mint
    // duplicate slugs like `Explorer-EXE` vs `explorer`.
    let mut stripped = trimmed;
    for ext in [".exe", ".app"] {
        if stripped.len() > ext.len() && stripped.to_lowercase().ends_with(ext) {
            stripped = &stripped[..stripped.len() - ext.len()];
        }
    }
    let stripped = stripped.trim_end_matches(" Helper").trim();
    // App slugs are lowercased: app identity is case-insensitive, and mixed
    // case across captures (`Code.exe` vs `code.exe`) must not fork the hub.
    let slug = slugify_hub(stripped).to_lowercase();
    if slug.is_empty() {
        None
    } else {
        Some(slug)
    }
}

/// Map a `content_type` event field (e.g. `CodeContent`, `BrowserContent`) to an
/// activity-hub slug. Returns None for unknown / generic-noise types.
pub fn activity_slug_for(content_type: &str) -> Option<String> {
    let slug = match content_type {
        "CodeContent" => "Code",
        "BrowserContent" => "Browser",
        "TerminalContent" => "Terminal",
        "DocumentationContent" | "DocsContent" => "Documentation",
        "ChatContent" | "MessagingContent" => "Communication",
        "EmailContent" => "Email",
        "DesignContent" => "Design",
        "SpreadsheetContent" => "Spreadsheet",
        "GenericContent" => "General",
        "ClipboardContent" => "Clipboard",
        "WindowTitleOnly" => "WindowSwitch",
        _ => {
            let lower = content_type.to_lowercase();
            if lower.contains("code") {
                "Code"
            } else if lower.contains("browser") {
                "Browser"
            } else if lower.contains("terminal") {
                "Terminal"
            } else if lower.contains("doc") {
                "Documentation"
            } else {
                return None;
            }
        }
    };
    Some(slug.to_string())
}

/// Extract a hub-friendly domain (lowercased, no leading `www.`) from a URL.
/// Returns None for non-parseable inputs or URLs without a host.
pub fn domain_from_url(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    if host.is_empty() {
        return None;
    }
    Some(strip_www(host))
}

/// App names that should never be inferred as a "project". These are tools,
/// communication clients, browsers, and OS shells — they appear frequently as
/// the leading word in window titles but do not identify a project.
pub(crate) const PROJECT_IGNORE: &[&str] = &[
    "gmail",
    "inbox",
    "slack",
    "discord",
    "teams",
    "zoom",
    "spotify",
    "chrome",
    "firefox",
    "edge",
    "brave",
    "arc",
    "opera",
    "safari",
    "outlook",
    "explorer",
    "files",
    "settings",
    "powershell",
    "cmd",
    "terminal",
    // TaskFlow's own window appears in every user's captures; it must never be
    // auto-inferred as a project. A user who IS developing TaskFlow gets the
    // project via `wiki_known_projects`, which is matched before this list applies.
    "taskflow",
    "github",
    "gitlab",
    "jira",
    "linear",
    "notion",
    "figma",
    "vercel",
    "preview",
    "localhost",
];

/// Infer a project name from captured editor window titles.
///
/// **What changed (Aug 2026):** The original heuristic took the **first** segment
/// before the dash separator — the file name in every editor (`Array.md`, `Usage.md`,
/// `App.js.md`). The workspace or project folder sits next to the app name
/// (`TaskFlow`, `SkillForge`, `datavex3`), which is what this function now reads.
/// Segment selection is delegated to [`workspace_from_window_title`] so this
/// fallback and the structured editor signal always read a title the same way.
pub fn infer_project_from_events(events: &[Event]) -> Option<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for event in events {
        let title = match event.window_title.as_deref() {
            Some(t) if !t.trim().is_empty() => t,
            _ => continue,
        };
        let Some(candidate) = workspace_from_window_title(title) else {
            continue;
        };
        let candidate = candidate.as_str();

        // Multi-word names (e.g. "Signal Flow", "Custom CMS") are counted both
        // ways: the leading word, and the whole segment. Whichever the captures
        // support more consistently wins the tally below.
        let token = candidate.split_whitespace().next().unwrap_or("");
        if !token.is_empty() {
            bump_token(&mut counts, token);
        }
        if candidate.split_whitespace().count() > 1 {
            bump_token(&mut counts, candidate);
        }
    }

    counts
        .into_iter()
        .filter(|(token, _)| {
            let lower = token.to_lowercase();
            !PROJECT_IGNORE.iter().any(|ignore| lower == *ignore)
                && token.chars().any(|c| c.is_alphabetic())
        })
        .max_by_key(|(_, n)| *n)
        .map(|(token, _)| titlecase_token(&token))
}

fn bump_token(counts: &mut BTreeMap<String, usize>, token: &str) {
    let key = token.trim().to_lowercase();
    if key.is_empty() {
        return;
    }
    *counts.entry(key).or_insert(0) += 1;
}

/// Resolve the project(s) a day's events touch, using a curated registry FIRST.
///
/// The old single-project heuristic (`infer_project_from_events`) misfires badly
/// on noise like "Array" / "Usage" / "Registration" — leading tokens of window
/// titles that aren't projects at all. The registry fixes this: when the user has
/// listed their real projects (e.g. TaskFlow, SkillForge, datavex3) we match them
/// by case-insensitive substring against each event's window title / url / content,
/// and only fall back to the heuristic when NO known project is seen all day.
///
/// A day may legitimately touch multiple projects (work on two repos), so this
/// returns a set of slugified hub names rather than a single name. Registry names
/// are slugified with `slugify_hub` so they produce stable `Projects/<slug>` paths.
/// The heuristic fallback still yields at most one project (preserving prior
/// behavior when the registry is empty).
pub fn resolve_projects(task: &Task, events: &[Event], known_projects: &[String]) -> Vec<String> {
    resolve_projects_excluding(task, events, known_projects, &rejected_projects_snapshot())
}

/// Process-wide cache of the names the user rejected in the review queue, held as
/// normalized `project_key`s.
///
/// Why a global rather than a parameter: the veto has to apply inside
/// `hubs_touched` → `update_wiki` and `route_workstream`, which are *synchronous*
/// and several layers below the async code that can touch SQLite. Threading a
/// `&BTreeSet` down would change `update_wiki`'s public signature and every call
/// site plus ~14 tests, to carry a value that is app-wide configuration and never
/// varies per call. The set is small (one short string per rejection) and read far
/// more often than written.
///
/// Writers: `refresh_rejected_projects` on roll-up, and the reject/approve
/// commands so a decision takes effect on the very next ingest without a restart.
/// A read lock poisoned by a panicking writer falls back to "no vetoes", which
/// degrades to the pre-veto behavior instead of dropping every project.
static REJECTED_PROJECTS: std::sync::RwLock<std::collections::BTreeSet<String>> =
    std::sync::RwLock::new(std::collections::BTreeSet::new());

/// Replace the rejected-project cache. Called with the full set (not a delta) so
/// an un-reject in the DB is reflected rather than sticking forever.
pub fn set_rejected_projects(keys: std::collections::BTreeSet<String>) {
    match REJECTED_PROJECTS.write() {
        Ok(mut guard) => *guard = keys,
        Err(_) => eprintln!("[taskflow:wiki] rejected-project cache poisoned; veto not updated"),
    }
}

/// Current rejected-project keys. Empty on a poisoned lock (fail-open).
pub fn rejected_projects_snapshot() -> std::collections::BTreeSet<String> {
    REJECTED_PROJECTS
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

/// [`resolve_projects`], plus a durable veto list.
///
/// `rejected` holds normalized keys (`project_key`) of candidates the user
/// answered "no" to. Without this, "no" would only be durable against the
/// candidate pipeline: the heuristic fallback below could still mint
/// `Projects/Mozilla.md` on any day the registry happens not to match, and the
/// user would have to reject the same misfire again and again.
///
/// The veto applies to the **fallback only**. A name in `known_projects` was
/// explicitly configured, so it wins over a stale rejection.
pub fn resolve_projects_excluding(
    task: &Task,
    events: &[Event],
    known_projects: &[String],
    rejected: &std::collections::BTreeSet<String>,
) -> Vec<String> {
    use std::collections::BTreeSet;

    // 1. Registry-first: collect distinct known projects whose name appears in
    //    any event's window title / url / content, or of the task's own
    //    source_project. Matching is normalization-insensitive (`project_key`):
    //    "Signal Flow" in the registry matches "SignalFlow" in a repo name.
    //    Preserves the registry author's casing by slugifying the original (not
    //    the lowercased) name.
    let mut matched: BTreeSet<String> = BTreeSet::new();
    for known in known_projects {
        let needle = known.trim();
        if needle.is_empty() {
            continue;
        }
        let needle_key = project_key(needle);
        if task
            .source_project
            .as_deref()
            .is_some_and(|p| project_mentioned_in(p, &needle_key))
        {
            matched.insert(slugify_hub(needle));
            continue;
        }
        let found = events.iter().any(|e| {
            let title = e.window_title.as_deref().unwrap_or("");
            let url = e.url.as_deref().unwrap_or("");
            let content = e.content.as_deref().unwrap_or("");
            project_mentioned_in(title, &needle_key)
                || project_mentioned_in(url, &needle_key)
                || project_mentioned_in(content, &needle_key)
        });
        if found {
            matched.insert(slugify_hub(needle));
        }
    }

    if !matched.is_empty() {
        return matched.into_iter().collect();
    }

    // 2. Fallback to the window-title heuristic, but ONLY if the registry had no
    //    hit at all. Keeps old behavior intact for users who haven't configured
    //    projects, and avoids the keyword-dust pages once they have. The result
    //    MUST be slugified: heuristic tokens come from raw window titles and can
    //    carry invisible Unicode (e.g. U+200E LRM — which once minted the junk
    //    page `Projects/?hermes.md`) or filesystem-hostile characters.
    let Some(inferred) = infer_project_from_events(events) else {
        return Vec::new();
    };
    // A name the user rejected in the review queue must not come back in
    // through the guess path.
    if rejected.contains(&project_key(&inferred)) {
        return Vec::new();
    }
    let slug = slugify_hub(&inferred);
    if slug.is_empty() {
        Vec::new()
    } else {
        vec![slug]
    }
}

/// Resolve the project slugs a SINGLE event touches, registry-first. Same
/// matching rule as `resolve_projects` (normalization-insensitive mention in
/// the event's title/url/content), but scoped to one event instead of the whole
/// day — needed by the hub↔hub cross-linker, which joins hubs that co-occur on
/// the SAME event.
///
/// Deliberately does NOT use the leading-word heuristic fallback: a per-event
/// leading-word guess is even noisier than a day-level one, and the cross-linker
/// should only draw edges it's confident about (a registry-named project in the
/// event's own text). Returns an empty Vec when the registry is unconfigured or
/// the event names no known project — a safe "no edge" result.
pub fn resolve_projects_for_event(event: &Event, known_projects: &[String]) -> Vec<String> {
    use std::collections::BTreeSet;

    let title = event.window_title.as_deref().unwrap_or("");
    let url = event.url.as_deref().unwrap_or("");
    let content = event.content.as_deref().unwrap_or("");
    if title.is_empty() && url.is_empty() && content.is_empty() {
        return Vec::new();
    }
    let mut matched: BTreeSet<String> = BTreeSet::new();
    for known in known_projects {
        let needle = known.trim();
        if needle.is_empty() {
            continue;
        }
        let needle_key = project_key(needle);
        if project_mentioned_in(title, &needle_key)
            || project_mentioned_in(url, &needle_key)
            || project_mentioned_in(content, &needle_key)
        {
            matched.insert(slugify_hub(needle));
        }
    }
    matched.into_iter().collect()
}

/// Title-case a single token: keep the casing of any capital letters that already
/// appear in the middle of the token (heuristic for names like `TaskFlow`, `iOS`),
/// otherwise capitalize the first letter and lowercase the rest.
fn titlecase_token(token: &str) -> String {
    let lower = token.to_lowercase();
    if lower.is_empty() {
        return lower;
    }
    // Preserve mixed case if the original had internal caps (camelCase names).
    let has_internal_caps = token
        .char_indices()
        .skip(1)
        .any(|(i, c)| c.is_uppercase() && i < token.len());
    if has_internal_caps {
        token.to_string()
    } else {
        let mut out = String::with_capacity(lower.len());
        for (i, c) in lower.chars().enumerate() {
            if i == 0 {
                out.extend(c.to_uppercase());
            } else {
                out.push(c);
            }
        }
        out
    }
}

/// Case- and separator-insensitive project key: lowercase, alphanumeric only.
/// "Signal Flow", "SignalFlow", and "signal-flow" all map to "signalflow", so a
/// registry name matches however the window title / URL / repo actually spells it.
///
/// This is also the identity used by the candidate staging table, so a renamed
/// candidate keeps matching the activity that produced it.
pub fn project_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

/// Does `haystack` (a window title, URL, or content blob) mention the project
/// named `needle` (a registry entry)? Matching is normalization-insensitive
/// (see `project_key`), with one guard: short names (< 4 normalized chars, e.g.
/// "Jan", "Oc", "Uv") require an exact TOKEN match instead of a substring one,
/// otherwise "jan" would false-positive inside "janitor" / "January" / "jan.md"
/// glued to its neighbors.
fn project_mentioned_in(haystack: &str, needle_key: &str) -> bool {
    if needle_key.is_empty() {
        return false;
    }
    if needle_key.chars().count() >= 4 {
        return project_key(haystack).contains(needle_key);
    }
    haystack
        .split(|ch: char| !ch.is_alphanumeric())
        .any(|token| project_key(token) == needle_key)
}

/// Collect distinct terminal commands a day's events captured, in capture order,
/// capped at `max` entries. Terminal events (content_type `TerminalContent`)
/// already carry privacy-sanitized, prompt-prefixed command lines in their
/// `content` field (see `extract_recent_commands` in the screen reader); we just
/// pull those lines back out, drop empties, and dedup while preserving first-seen
/// order so the day's command narrative reads top-to-bottom.
///
/// Returns plain strings — NOT slugified and NOT wrapped in `[[wikilinks]]`. A
/// git command is searchable text (Quick Switcher / full-text find), not a graph
/// node; the plan deliberately keeps commands out of the link graph.
pub fn collect_terminal_commands(events: &[Event]) -> Vec<String> {
    collect_terminal_commands_capped(events, 15)
}

/// Strip a leading shell prompt sigil (and the space after it) from a captured
/// command line so `$ cargo build` and `cargo build` are recognized as the same
/// command for dedupe. Kept exact with the reader's prompt set (`$`/`>`/`#`/`❯`/
/// `→`). Returns the line unchanged when it has no prompt prefix (e.g. a bare
/// `git status` the reader captured via the known-tool rule).
fn strip_prompt_prefix(line: &str) -> String {
    for prompt in ["$ ", "> ", "# ", "❯ ", "→ "] {
        if let Some(rest) = line.strip_prefix(prompt) {
            return rest.to_string();
        }
    }
    line.to_string()
}

/// Same as `collect_terminal_commands` but with a caller-chosen cap. Separated so
/// the vault-wide `Commands.md` index can pull a larger window if it ever needs to.
pub fn collect_terminal_commands_capped(events: &[Event], max: usize) -> Vec<String> {
    use std::collections::BTreeSet;

    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out: Vec<String> = Vec::new();
    for event in events {
        if event.content_type.as_deref() != Some("TerminalContent") {
            continue;
        }
        let Some(content) = event.content.as_deref() else {
            continue;
        };
        for raw in content.lines() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            // The reader already filters to prompt-prefixed / known-tool lines,
            // so trust the stored content. Dedup on the command WITHOUT its
            // leading prompt sigil (the reader keeps `$`/`>`/`#`/etc. verbatim),
            // so `$ cargo build` and `cargo build` collapse to one row — they
            // are the same command captured from two differently-prompted shells.
            // We store the original line for display (preserves the user's
            // actual prompt presentation) but key dedupe on the stripped form.
            let key = strip_prompt_prefix(line);
            if seen.insert(key) {
                out.push(line.to_string());
                if out.len() >= max {
                    return out;
                }
            }
        }
    }
    out
}

/// How a project candidate was detected. Ordered loosely by precision: a repo
/// URL names a project unambiguously, a window-title token is a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalKind {
    /// `tasks.source_project` — came from a Linear/Jira/GitHub ticket. Ground truth.
    SourceProject,
    /// `github.com/<owner>/<repo>` (or GitLab/Bitbucket) seen in an event URL.
    RepoUrl,
    /// The workspace segment of an editor window title (`file.rs - Repo - Cursor`).
    EditorWorkspace,
    /// A repo directory name parsed out of a captured terminal command.
    TerminalRepo,
}

impl SignalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SignalKind::SourceProject => "source_project",
            SignalKind::RepoUrl => "repo_url",
            SignalKind::EditorWorkspace => "editor_workspace",
            SignalKind::TerminalRepo => "terminal_repo",
        }
    }

    /// Human label for the review UI ("Detected from: …").
    pub fn label(self) -> &'static str {
        match self {
            SignalKind::SourceProject => "Linked ticket",
            SignalKind::RepoUrl => "Repository URL",
            SignalKind::EditorWorkspace => "Editor workspace",
            SignalKind::TerminalRepo => "Terminal",
        }
    }
}

/// One detection of a project name, with the raw text that produced it. The
/// `evidence` string is shown verbatim in the review prompt — "Is `Mozilla` a
/// project?" is unanswerable, but "saw it in *Array - JavaScript | MDN - Firefox*"
/// is answerable at a glance.
#[derive(Debug, Clone)]
pub struct ProjectSignal {
    pub name: String,
    pub kind: SignalKind,
    pub evidence: String,
}

/// Directory and host names that are never a project: package dirs, generic
/// parents, and the user's own path prefix. Checked against the *normalized*
/// key (see `project_key`), so `node_modules` matches `nodemodules`.
const PATH_IGNORE: &[&str] = &[
    "users", "home", "documents", "downloads", "desktop", "onedrive", "dropbox",
    "src", "source", "repos", "repositories", "projects", "code", "dev", "git",
    "github", "gitlab", "workspace", "temp", "tmp", "new", "test", "tests",
    "nodemodules", "dist", "build", "target", "out", "bin", "obj", "vendor",
    "venv", "env", "cargo", "appdata", "programfiles", "windows", "system32",
];

/// Editor / IDE process names whose window titles follow the
/// `<file> - <workspace> - <app>` convention.
const EDITOR_APPS: &[&str] = &[
    "code", "cursor", "codium", "vscodium", "windsurf", "zed", "sublime_text",
    "sublime", "atom", "idea", "idea64", "pycharm", "pycharm64", "webstorm",
    "webstorm64", "goland", "goland64", "clion", "clion64", "rider", "rider64",
    "rustrover", "phpstorm", "androidstudio", "studio64", "devenv", "notepad++",
    "nvim", "vim", "emacs",
];

/// Git hosts whose URL path starts with `<owner>/<repo>`.
const REPO_HOSTS: &[&str] = &[
    "github.com", "gitlab.com", "bitbucket.org", "codeberg.org", "gitea.com", "git.sr.ht",
];

/// Non-repo first path segments on git hosts — `github.com/settings/profile`
/// must not mint a project called `settings`.
const REPO_RESERVED: &[&str] = &[
    "settings", "notifications", "explore", "marketplace", "pulls", "issues",
    "orgs", "organizations", "users", "topics", "trending", "sponsors", "apps",
    "features", "pricing", "about", "login", "logout", "signup", "join", "new",
    "search", "codespaces", "dashboard", "account", "help", "support", "docs",
];

/// Is this normalized key too generic / too short to be a project name?
fn is_ignored_project_key(key: &str) -> bool {
    if key.chars().count() < 2 {
        return true;
    }
    PROJECT_IGNORE.iter().any(|ignore| project_key(ignore) == key)
        || PATH_IGNORE.iter().any(|ignore| project_key(ignore) == key)
}

/// Does this app name look like a code editor / IDE?
fn is_editor_app(app_name: &str) -> bool {
    let Some(slug) = hub_slug_for_app(app_name) else {
        return false;
    };
    let normalized = project_key(&slug);
    EDITOR_APPS
        .iter()
        .any(|editor| project_key(editor) == normalized)
}

/// Strip editor decorations from a window-title segment: unsaved markers (`●`,
/// `*`), IntelliJ's `[brackets]`, and a trailing path so `~/dev/TaskFlow` yields
/// `TaskFlow`. Returns None when nothing usable survives.
fn clean_title_segment(segment: &str) -> Option<String> {
    let cleaned = segment
        .trim()
        .trim_start_matches(['●', '•', '*', '◍'])
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();
    // A path — keep only the leaf directory.
    let leaf = cleaned.rsplit(['/', '\\']).next().unwrap_or(cleaned).trim();
    if leaf.is_empty() || !leaf.chars().any(char::is_alphanumeric) {
        return None;
    }
    Some(leaf.to_string())
}

/// Signal 1 — the linked ticket's project field. Highest precision available:
/// an integration told us the name, so no inference is involved.
pub fn signals_from_source_project(task: &Task) -> Vec<ProjectSignal> {
    let Some(project) = task.source_project.as_deref() else {
        return Vec::new();
    };
    let trimmed = project.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    vec![ProjectSignal {
        name: trimmed.to_string(),
        kind: SignalKind::SourceProject,
        evidence: format!("Ticket project: {trimmed}"),
    }]
}

/// Signal 2 — `<host>/<owner>/<repo>` in any captured URL. The repo name is an
/// unambiguous project identifier, which is why this outranks title parsing.
pub fn signals_from_repo_urls(events: &[Event]) -> Vec<ProjectSignal> {
    let mut out = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for event in events {
        let Some(url) = event.url.as_deref() else {
            continue;
        };
        let Some(repo) = repo_from_url(url) else {
            continue;
        };
        if !seen.insert(project_key(&repo)) {
            continue;
        }
        out.push(ProjectSignal {
            name: repo,
            kind: SignalKind::RepoUrl,
            evidence: truncate_evidence(url),
        });
    }
    out
}

/// Parse `<owner>/<repo>` from a git-host URL, rejecting the host's own reserved
/// paths. Returns the repo name with any `.git` suffix removed.
pub fn repo_from_url(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let host = strip_www(parsed.host_str()?);
    if !REPO_HOSTS.iter().any(|known| host == *known) {
        return None;
    }
    let mut segments = parsed
        .path_segments()?
        .filter(|segment| !segment.is_empty());
    let owner = segments.next()?;
    let repo = segments.next()?;
    let owner_key = project_key(owner);
    if REPO_RESERVED
        .iter()
        .any(|reserved| project_key(reserved) == owner_key)
    {
        return None;
    }
    let repo = repo.strip_suffix(".git").unwrap_or(repo).trim();
    if repo.is_empty() || is_ignored_project_key(&project_key(repo)) {
        return None;
    }
    Some(repo.to_string())
}

/// Signal 3 — the workspace segment of an editor window title. This is the
/// signal the old heuristic got wrong: it read segment 0 (the file name), where
/// the workspace lives next to the app name. Scoped to editor apps so browser
/// titles, which follow no such convention, never reach it.
pub fn signals_from_editor_workspace(events: &[Event]) -> Vec<ProjectSignal> {
    let mut out = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for event in events {
        let Some(app) = event.app_name.as_deref() else {
            continue;
        };
        if !is_editor_app(app) {
            continue;
        }
        let Some(title) = event.window_title.as_deref() else {
            continue;
        };
        let Some(workspace) = workspace_from_window_title(title) else {
            continue;
        };
        let key = project_key(&workspace);
        if is_ignored_project_key(&key) || !seen.insert(key) {
            continue;
        }
        out.push(ProjectSignal {
            name: workspace,
            kind: SignalKind::EditorWorkspace,
            evidence: truncate_evidence(title),
        });
    }
    out
}

/// Pull the workspace out of one window title, or None if the title has no
/// workspace in it. Shared by the structured editor signal and the older
/// whole-capture heuristic so the two can't disagree about what a title means.
///
/// Anchors on the app name rather than counting from the end, because editors
/// append status text after it — `… - Visual Studio Code - 1 problem in this
/// file`, `… - Cursor - Modified`. A positional `len - 2` reads the app name
/// itself in those, which is how `Projects/Visual Studio Code` nearly happened.
fn workspace_from_window_title(title: &str) -> Option<String> {
    // The separator is " - ", *with* the spaces. Splitting on a bare hyphen
    // shreds every hyphenated repo name — `Test-Portfolio` became `Portfolio`,
    // `cosc-website` became `website`. En/em dashes are normalized first so the
    // one split covers all three.
    let normalized = title.replace(" – ", " - ").replace(" — ", " - ");
    let segments: Vec<&str> = normalized
        .split(" - ")
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.len() < 2 {
        return None;
    }

    // Last segment that names an editor. Last, not first, because a file or
    // folder may legitimately be called "vim" or "code".
    let app_idx = segments
        .iter()
        .rposition(|segment| is_editor_display_name(segment));

    let (raw, ambiguous) = match app_idx {
        // Nothing precedes the app name, so this is `<x> - <app>`: `x` is the
        // open folder when one is open, and a loose file when none is.
        Some(1) => (segments[0], true),
        Some(idx) if idx >= 2 => (segments[idx - 1], false),
        // The app segment is at 0, or no known editor matched. Fall back to the
        // positional convention rather than guessing.
        _ => {
            if segments.len() >= 3 {
                (segments[segments.len() - 2], false)
            } else {
                (segments[0], true)
            }
        }
    };

    // `Foo::Bar` is a namespace path, not a folder — keep the head, as the
    // heuristic has always done.
    let raw = raw.split("::").next().unwrap_or(raw).trim();

    // Characters Windows forbids in a path can't be in a workspace name, so a
    // segment carrying one is prose — an editor doc tab like
    // `Release Notes: 1.130.0`, not a folder.
    if raw.is_empty() || raw.contains([':', '?', '*', '"', '<', '>', '|']) {
        return None;
    }
    // Only reject file-shaped names in the ambiguous slot. In a three-part title
    // the workspace slot is unambiguous, and a folder there may legitimately
    // carry a dot.
    if ambiguous && looks_like_filename(raw) {
        return None;
    }
    let workspace = clean_title_segment(raw)?;
    if is_editor_display_name(&workspace) {
        return None;
    }
    Some(workspace)
}

/// Does this title segment name an editor, as the editor writes itself in a
/// window title? `EDITOR_APPS` holds *process* names (`code`, `devenv`), which
/// never appear in the title tail — that's why this list exists separately.
fn is_editor_display_name(segment: &str) -> bool {
    const EDITOR_TITLE_NAMES: &[&str] = &[
        "Visual Studio Code",
        "Visual Studio Code - Insiders",
        "Visual Studio",
        "VSCodium",
        "Cursor",
        "Windsurf",
        "Zed",
        "Sublime Text",
        "Atom",
        "IntelliJ IDEA",
        "PyCharm",
        "WebStorm",
        "GoLand",
        "CLion",
        "Rider",
        "RustRover",
        "PhpStorm",
        "Android Studio",
        "Notepad++",
        "Notepad",
        "Neovim",
        "Vim",
        "Emacs",
    ];
    let key = project_key(segment);
    !key.is_empty() && EDITOR_TITLE_NAMES.iter().any(|name| project_key(name) == key)
}

/// Does this segment look like a file rather than a folder? Used only where the
/// two are genuinely ambiguous. Deliberately strict — a short, alphanumeric
/// extension — so `my.project` stays a candidate while `settings.json`,
/// `config.toml`, and `npm.ps1` do not.
fn looks_like_filename(segment: &str) -> bool {
    let Some((stem, ext)) = segment.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && (1..=4).contains(&ext.chars().count())
        && ext.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Signal 4 — a repo directory inferred from captured terminal commands. Reads
/// two shapes the reader already stores: a `cd <path>` argument, and a Windows
/// prompt line that embeds the working directory (`C:\...\TaskFlow> git status`).
pub fn signals_from_terminal(events: &[Event]) -> Vec<ProjectSignal> {
    let mut out = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for command in collect_terminal_commands_capped(events, 60) {
        let Some(dir) = repo_dir_from_command(&command) else {
            continue;
        };
        let key = project_key(&dir);
        if is_ignored_project_key(&key) || !seen.insert(key) {
            continue;
        }
        out.push(ProjectSignal {
            name: dir,
            kind: SignalKind::TerminalRepo,
            evidence: truncate_evidence(&command),
        });
    }
    out
}

/// Pull a repo directory name out of one captured command line.
fn repo_dir_from_command(command: &str) -> Option<String> {
    let line = strip_prompt_prefix(command.trim());
    // Shape A: `cd some/path/Repo` (also `cd /d`, `pushd`).
    let after_cd = line
        .strip_prefix("cd ")
        .or_else(|| line.strip_prefix("pushd "))
        .map(|rest| rest.trim().trim_start_matches("/d ").trim());
    if let Some(path) = after_cd {
        let path = path.trim_matches('"').trim_matches('\'').trim_end_matches(['/', '\\']);
        if !path.is_empty() && path != "~" && !path.starts_with('-') {
            return clean_title_segment(path);
        }
    }
    // Shape B: a Windows prompt that carries the cwd — `C:\dev\TaskFlow> git status`.
    if let Some((prefix, _)) = line.split_once('>') {
        let prefix = prefix.trim();
        if prefix.len() > 3 && (prefix.contains('\\') || prefix.contains('/')) {
            return clean_title_segment(prefix.trim_end_matches(['/', '\\']));
        }
    }
    None
}

/// Cap an evidence string so one pathological window title can't bloat the row.
fn truncate_evidence(value: &str) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= 160 {
        return collapsed;
    }
    let clipped: String = collapsed.chars().take(159).collect();
    format!("{clipped}…")
}

/// Run every extractor over one window of activity and drop anything the user
/// has already ruled on.
///
/// `decided` holds the normalized keys of both confirmed projects and rejected
/// ones — a name in either set is settled and must not be re-proposed. Signals
/// are returned in precision order (see [`SignalKind`]), so a caller merging by
/// name can prefer the most trustworthy spelling.
pub fn detect_project_signals(
    task: &Task,
    events: &[Event],
    decided: &std::collections::BTreeSet<String>,
) -> Vec<ProjectSignal> {
    let mut signals = Vec::new();
    signals.extend(signals_from_source_project(task));
    signals.extend(signals_from_repo_urls(events));
    signals.extend(signals_from_editor_workspace(events));
    signals.extend(signals_from_terminal(events));
    signals.retain(|signal| {
        let key = project_key(&signal.name);
        !key.is_empty() && !is_ignored_project_key(&key) && !decided.contains(&key)
    });
    signals.sort_by(|a, b| a.kind.cmp(&b.kind));
    signals
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tasks::Task;

    fn event_with_title(title: &str) -> Event {
        Event {
            id: "e1".to_string(),
            task_id: "t1".to_string(),
            event_type: "window_switch".to_string(),
            app_name: None,
            window_title: Some(title.to_string()),
            content: None,
            url: None,
            content_type: None,
            capture_method: None,
            is_sanitized: 1,
            chunk_index: 0,
            relevance: 0.0,
            timestamp: "2026-07-27T10:00:00+00:00".to_string(),
            created_at: "2026-07-27T10:00:00+00:00".to_string(),
        }
    }

    fn bare_task() -> Task {
        Task {
            id: "t1".to_string(),
            title: "Memory Capture".to_string(),
            description: None,
            source: "memory".to_string(),
            source_id: None,
            source_url: None,
            source_title: None,
            source_body: None,
            source_labels: None,
            source_assignee: None,
            source_priority: None,
            source_project: None,
            source_branch: None,
            status: "active".to_string(),
            started_at: None,
            ended_at: None,
            created_at: "2026-07-27T10:00:00+00:00".to_string(),
        }
    }

    #[test]
    fn app_slug_strips_extension_case_insensitively() {
        assert_eq!(hub_slug_for_app("Explorer.EXE").as_deref(), Some("explorer"));
        assert_eq!(hub_slug_for_app("explorer.exe").as_deref(), Some("explorer"));
        assert_eq!(hub_slug_for_app("Code.exe").as_deref(), Some("code"));
        assert_eq!(hub_slug_for_app("chrome").as_deref(), Some("chrome"));
    }

    #[test]
    fn project_fallback_is_slugified_and_strips_invisible_unicode() {
        // U+200E LEFT-TO-RIGHT MARK in a window title once minted the junk
        // page `Projects/?hermes.md`; the fallback must slugify it away.
        let events = vec![event_with_title("\u{200E}hermes - chat")];
        let resolved = resolve_projects(&bare_task(), &events, &[]);
        assert_eq!(resolved, vec!["hermes".to_string()]);
    }

    #[test]
    fn project_fallback_empty_when_only_noise() {
        let events = vec![event_with_title("... - ...")];
        let resolved = resolve_projects(&bare_task(), &events, &[]);
        assert!(resolved.is_empty(), "noise titles must not mint projects: {resolved:?}");
    }

    #[test]
    fn registry_match_wins_over_heuristic() {
        let events = vec![event_with_title("TaskFlow - main.rs - Cursor")];
        let resolved = resolve_projects(&bare_task(), &events, &["TaskFlow".to_string()]);
        assert_eq!(resolved, vec!["TaskFlow".to_string()]);
    }

    #[test]
    fn registry_matching_ignores_spaces_and_separators() {
        // The exact user-reported case: registry "Signal Flow" must match the
        // captured "SignalFlow" spelling (repo names, titles, URLs).
        let registry = vec!["Signal Flow".to_string()];
        let events = vec![event_with_title("SignalFlow — App.tsx — Cursor")];
        let resolved = resolve_projects(&bare_task(), &events, &registry);
        assert_eq!(resolved, vec!["Signal-Flow".to_string()], "registry slug: {resolved:?}");

        // And the separator variants match in both directions.
        for variant in ["signal-flow", "Skill Forge", "SKILLFORGE"] {
            let events = vec![event_with_title("SkillForge - README.md")];
            let resolved = resolve_projects(&bare_task(), &events, &[variant.to_string()]);
            assert!(!resolved.is_empty(), "{variant} should match SkillForge");
        }

        // URL matching too (repo slug without the space).
        let event = Event {
            window_title: None,
            url: Some("https://github.com/kaush/SignalFlow".to_string()),
            ..event_with_title("")
        };
        let resolved = resolve_projects_for_event(&event, &registry);
        assert_eq!(resolved, vec!["Signal-Flow".to_string()], "per-event URL match: {resolved:?}");
    }

    #[test]
    fn short_registry_names_require_exact_token() {
        let registry = vec!["Jan".to_string()];
        // Exact token present → match.
        let events = vec![event_with_title("Jan.md - Cursor")];
        assert_eq!(resolve_projects(&bare_task(), &events, &registry), vec!["Jan".to_string()]);
        // Substring of a longer token → the REGISTRY name must not match (a
        // heuristic fallback result like "Janitor" is fine and expected).
        for title in ["janitor config - Cursor", "January notes", "Jansettings tweaks"] {
            let events = vec![event_with_title(title)];
            let resolved = resolve_projects(&bare_task(), &events, &registry);
            assert!(
                !resolved.iter().any(|slug| slug == "Jan"),
                "'Jan' must not match '{title}' (got {resolved:?})"
            );
        }
    }

    // ---------- title-position fix ----------

    /// The bug that minted `Projects/Array.md`, `Projects/Usage.md` and
    /// `Projects/App.js.md`: the heuristic read segment 0 (the FILE) instead of
    /// the workspace at `len - 2`.
    ///
    /// Uses `datavex3` rather than `TaskFlow` as the workspace: `taskflow` is in
    /// `PROJECT_IGNORE` on purpose (the app's own window is in every capture), so
    /// it would assert the denylist rather than the position fix.
    #[test]
    fn heuristic_reads_workspace_not_filename() {
        let events = vec![
            event_with_title("links.rs - datavex3 - Cursor"),
            event_with_title("Array.md - datavex3 - Cursor"),
        ];
        let inferred = infer_project_from_events(&events);
        assert!(
            inferred
                .as_deref()
                .is_some_and(|name| project_key(name) == "datavex3"),
            "workspace segment must win over the filename, got {inferred:?}"
        );
        // And specifically NOT the old first-segment result.
        assert!(
            !inferred
                .as_deref()
                .is_some_and(|name| project_key(name) == "array"),
            "filename must never become the project: {inferred:?}"
        );
    }

    #[test]
    fn heuristic_two_segment_title_uses_first() {
        // No file open: `<workspace> - <app>`.
        let events = vec![
            event_with_title("SkillForge - Cursor"),
            event_with_title("SkillForge - Cursor"),
        ];
        let inferred = infer_project_from_events(&events);
        assert!(
            inferred.as_deref().is_some_and(|n| project_key(n) == "skillforge"),
            "two-segment title reads segment 0, got {inferred:?}"
        );
    }

    #[test]
    fn heuristic_supports_multi_word_workspaces() {
        let events = vec![
            event_with_title("App.tsx - Signal Flow - Visual Studio Code"),
            event_with_title("index.ts - Signal Flow - Visual Studio Code"),
        ];
        let inferred = infer_project_from_events(&events);
        assert!(
            inferred.as_deref().is_some_and(|n| project_key(n) == "signalflow"),
            "multi-word workspace kept whole, got {inferred:?}"
        );
    }

    // ---------- rejection veto ----------

    #[test]
    fn rejected_name_is_vetoed_from_the_fallback() {
        let events = vec![event_with_title("index.html - Mozilla - firefox")];
        // Without a veto the heuristic mints the page.
        let baseline = resolve_projects_excluding(
            &bare_task(),
            &events,
            &[],
            &std::collections::BTreeSet::new(),
        );
        assert_eq!(baseline, vec!["Mozilla".to_string()], "baseline: {baseline:?}");

        // With it, the guess is suppressed — a "no" stays "no".
        let rejected: std::collections::BTreeSet<String> =
            ["mozilla".to_string()].into_iter().collect();
        let vetoed = resolve_projects_excluding(&bare_task(), &events, &[], &rejected);
        assert!(vetoed.is_empty(), "rejected name must not resolve: {vetoed:?}");
    }

    // ---------- structured extractors ----------

    fn editor_event(app: &str, title: &str) -> Event {
        Event {
            app_name: Some(app.to_string()),
            ..event_with_title(title)
        }
    }

    fn url_event(url: &str) -> Event {
        Event {
            url: Some(url.to_string()),
            window_title: None,
            ..event_with_title("")
        }
    }

    fn terminal_event(content: &str) -> Event {
        Event {
            app_name: Some("WindowsTerminal".to_string()),
            content_type: Some("TerminalContent".to_string()),
            content: Some(content.to_string()),
            window_title: None,
            ..event_with_title("")
        }
    }

    #[test]
    fn repo_url_extractor_reads_owner_repo() {
        assert_eq!(
            repo_from_url("https://github.com/kaush/SignalFlow").as_deref(),
            Some("SignalFlow")
        );
        // `.git` suffix stripped; deeper paths still resolve to the repo.
        assert_eq!(
            repo_from_url("https://gitlab.com/acme/datavex3.git").as_deref(),
            Some("datavex3")
        );
        assert_eq!(
            repo_from_url("https://github.com/kaush/datavex3/pull/12/files").as_deref(),
            Some("datavex3")
        );
        // Reserved host paths are not projects.
        assert_eq!(repo_from_url("https://github.com/settings/profile"), None);
        assert_eq!(repo_from_url("https://github.com/explore"), None);
        // Non-git hosts contribute nothing.
        assert_eq!(repo_from_url("https://news.ycombinator.com/a/b"), None);
    }

    #[test]
    fn repo_url_signals_dedupe_by_normalized_name() {
        let events = vec![
            url_event("https://github.com/kaush/SignalFlow"),
            url_event("https://github.com/kaush/signal-flow/issues/4"),
        ];
        let signals = signals_from_repo_urls(&events);
        assert_eq!(signals.len(), 1, "same project normalized once: {signals:?}");
        assert!(matches!(signals[0].kind, SignalKind::RepoUrl));
    }

    #[test]
    fn editor_workspace_signal_is_scoped_to_editors() {
        let events = vec![editor_event("Code.exe", "App.tsx - datavex3 - Visual Studio Code")];
        let signals = signals_from_editor_workspace(&events);
        assert_eq!(signals.len(), 1, "editor title yields a signal: {signals:?}");
        assert_eq!(project_key(&signals[0].name), "datavex3");

        // A browser follows no such convention — the middle segment is prose.
        let browser = vec![editor_event("firefox", "Array - JavaScript - MDN - Mozilla Firefox")];
        assert!(
            signals_from_editor_workspace(&browser).is_empty(),
            "browser titles must not produce workspace signals"
        );
    }

    #[test]
    fn editor_workspace_strips_dirty_markers_and_paths() {
        let events = vec![editor_event(
            "Cursor",
            "● links.rs - ~/dev/datavex3 - Cursor",
        )];
        let signals = signals_from_editor_workspace(&events);
        assert_eq!(signals.len(), 1, "got {signals:?}");
        assert_eq!(
            signals[0].name, "datavex3",
            "unsaved marker + path stripped to the leaf: {:?}",
            signals[0].name
        );
    }

    // The four tests below all come from titles the dry-run harness pulled out of
    // real captures, where the old parser produced junk candidates.

    #[test]
    fn hyphenated_workspace_survives_the_split() {
        // Splitting on a bare '-' turned these into "Portfolio", "website", "AI",
        // "Project" and "Matcher" — plausible-looking names that were all wrong.
        for (title, expected) in [
            ("layout.tsx - Test-Portfolio - Visual Studio Code", "testportfolio"),
            ("pnpm-lock.yaml - cosc-website - Cursor", "coscwebsite"),
            ("server.mjs - SME-AI - Visual Studio Code", "smeai"),
            ("opencode.json - DK24-Project - Visual Studio Code", "dk24project"),
        ] {
            let signals = signals_from_editor_workspace(&[editor_event("Code.exe", title)]);
            assert_eq!(signals.len(), 1, "{title} yielded {signals:?}");
            assert_eq!(
                project_key(&signals[0].name),
                expected,
                "{title} -> {:?}",
                signals[0].name
            );
        }
    }

    #[test]
    fn trailing_status_text_does_not_shift_the_workspace() {
        // Editors append status after their own name; reading `len - 2` blindly
        // picked up the editor itself and would have minted `Projects/Cursor`.
        for title in [
            "layout.tsx - Test-Portfolio - Visual Studio Code - 1 problem in this file",
            "layout.tsx - Test-Portfolio - Cursor - Modified",
        ] {
            let signals = signals_from_editor_workspace(&[editor_event("Code.exe", title)]);
            assert_eq!(signals.len(), 1, "{title} yielded {signals:?}");
            assert_eq!(project_key(&signals[0].name), "testportfolio", "{title}");
        }
    }

    #[test]
    fn loose_file_without_a_workspace_yields_nothing() {
        // `<file> - <app>` means no folder is open. The old rule read the file
        // name as the project, staging candidates called `settings.json`.
        for title in [
            "settings.json - Visual Studio Code",
            "auth.json - Visual Studio Code",
            "config.toml - Visual Studio Code",
            "npm.ps1 - Notepad",
        ] {
            assert!(
                signals_from_editor_workspace(&[editor_event("Code.exe", title)]).is_empty(),
                "{title} must not produce a workspace signal"
            );
        }

        // A folder open with no file looks the same structurally, and must survive.
        let signals = signals_from_editor_workspace(&[editor_event("Code.exe", "backend - Visual Studio Code")]);
        assert_eq!(signals.len(), 1, "got {signals:?}");
        assert_eq!(project_key(&signals[0].name), "backend");
    }

    #[test]
    fn editor_doc_tabs_are_not_workspaces() {
        // `Release Notes: 1.130.0 - Visual Studio Code` is a tab inside the editor.
        assert!(
            signals_from_editor_workspace(&[editor_event(
                "Code.exe",
                "Release Notes: 1.130.0 - Visual Studio Code"
            )])
            .is_empty(),
            "a colon means prose, not a folder name"
        );
    }

    #[test]
    fn terminal_signal_reads_cd_and_prompt_cwd() {
        let events = vec![terminal_event("cd C:\\dev\\datavex3\n$ cargo build")];
        let signals = signals_from_terminal(&events);
        assert!(
            signals.iter().any(|s| project_key(&s.name) == "datavex3"),
            "cd path leaf becomes the project: {signals:?}"
        );

        // Generic parents in PATH_IGNORE must not become projects.
        let noise = vec![terminal_event("cd ~/Documents\n$ ls")];
        assert!(
            signals_from_terminal(&noise).is_empty(),
            "generic directories are ignored: {:?}",
            signals_from_terminal(&noise)
        );
    }

    #[test]
    fn source_project_signal_is_verbatim() {
        let task = Task {
            source_project: Some("Signal Flow".to_string()),
            ..bare_task()
        };
        let signals = signals_from_source_project(&task);
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].name, "Signal Flow", "integration name kept as-is");
        assert!(matches!(signals[0].kind, SignalKind::SourceProject));
    }

    #[test]
    fn detect_skips_names_the_user_already_decided() {
        let events = vec![
            url_event("https://github.com/kaush/datavex3"),
            editor_event("Cursor", "main.rs - SkillForge - Cursor"),
        ];
        let none_decided = detect_project_signals(&bare_task(), &events, &Default::default());
        assert!(
            none_decided.iter().any(|s| project_key(&s.name) == "datavex3"),
            "baseline detects datavex3: {none_decided:?}"
        );

        // Approved or rejected → never proposed again.
        let decided: std::collections::BTreeSet<String> =
            ["datavex3".to_string()].into_iter().collect();
        let filtered = detect_project_signals(&bare_task(), &events, &decided);
        assert!(
            !filtered.iter().any(|s| project_key(&s.name) == "datavex3"),
            "decided name suppressed: {filtered:?}"
        );
        assert!(
            filtered.iter().any(|s| project_key(&s.name) == "skillforge"),
            "other candidates still surface: {filtered:?}"
        );
    }

    #[test]
    fn detect_orders_signals_by_precision() {
        let task = Task {
            source_project: Some("Ticketed".to_string()),
            ..bare_task()
        };
        let events = vec![
            terminal_event("cd /home/k/TermRepo\n$ git status"),
            url_event("https://github.com/kaush/UrlRepo"),
        ];
        let signals = detect_project_signals(&task, &events, &Default::default());
        let kinds: Vec<&str> = signals.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(
            kinds,
            vec!["source_project", "repo_url", "terminal_repo"],
            "most trustworthy signal first: {kinds:?}"
        );
    }

    #[test]
    fn veto_does_not_override_the_registry() {
        // An explicitly configured project outranks a stale rejection.
        let events = vec![event_with_title("main.rs - TaskFlow - Cursor")];
        let rejected: std::collections::BTreeSet<String> =
            ["taskflow".to_string()].into_iter().collect();
        let resolved = resolve_projects_excluding(
            &bare_task(),
            &events,
            &["TaskFlow".to_string()],
            &rejected,
        );
        assert_eq!(resolved, vec!["TaskFlow".to_string()], "registry wins: {resolved:?}");
    }
}

