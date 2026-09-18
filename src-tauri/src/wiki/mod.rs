pub mod daily_index;
pub mod hubs;
pub mod links;
pub mod lint;
pub mod monthly_digest;
pub mod workstream;

use std::path::{Path, PathBuf};

use crate::database::events::Event;
use crate::database::tasks::Task;

#[allow(unused_imports)]
pub use links::{
    activity_slug_for, collect_terminal_commands, detect_project_signals, domain_from_url,
    hub_slug_for_app, infer_project_from_events, project_key, rejected_projects_snapshot,
    resolve_projects, resolve_projects_excluding, resolve_projects_for_event,
    set_rejected_projects, ProjectSignal, SignalKind,
};
#[allow(unused_imports)]
pub use hubs::{
    append_log, update_command_index, update_index, upsert_hub_page, write_cross_link_section,
    HubKind,
};
#[allow(unused_imports)]
pub use lint::{lint_vault, LintReport};
/// A complete description of one hub the current daily note touches. `update_wiki`
/// uses these to know which hub pages to upsert.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiHubRef {
    pub kind: hubs::HubKind,
    pub slug: String,
}

/// Compute the full set of hubs a daily note touches, given the task + events.
/// Deduplicated and sorted for stable output.
///
/// `known_projects` is the user's curated project list (from the
/// `wiki_known_projects` setting). When non-empty, projects are resolved
/// registry-FIRST via `resolve_projects` so the wiki never sprouts keyword-dust
/// hub pages like `Projects/Array`. A day may touch multiple real projects.
pub fn hubs_touched(task: &Task, events: &[Event], known_projects: &[String]) -> Vec<WikiHubRef> {
    use std::collections::BTreeSet;

    let mut set: BTreeSet<(hubs::HubKind, String)> = BTreeSet::new();

    for app in events.iter().filter_map(|e| e.app_name.as_deref()) {
        if app.trim().is_empty() {
            continue;
        }
        if let Some(slug) = hub_slug_for_app(app) {
            set.insert((hubs::HubKind::App, slug));
        }
    }
    for url in events.iter().filter_map(|e| e.url.as_deref()) {
        if !url.starts_with("http") {
            continue;
        }
        if let Some(domain) = domain_from_url(url) {
            set.insert((hubs::HubKind::Site, domain));
        }
    }
    for ct in events.iter().filter_map(|e| e.content_type.as_deref()) {
        if let Some(slug) = activity_slug_for(ct) {
            set.insert((hubs::HubKind::Activity, slug));
        }
    }
    for project in resolve_projects(task, events, known_projects) {
        set.insert((hubs::HubKind::Project, project));
    }

    set.into_iter()
        .map(|(kind, slug)| WikiHubRef { kind, slug })
        .collect()
}

/// Build a short, human-readable detail describing what happened in one hub on a
/// given day, derived purely from the day's events (no LLM). This is what turns a
/// hub page's "Recent Daily Notes" list from bare backlinks into something you can
/// actually skim — e.g. the window titles seen in an app, or the pages visited on
/// a site. Returns an empty string when there is nothing worth showing.
pub fn hub_detail(kind: hubs::HubKind, slug: &str, events: &[Event]) -> String {
    use std::collections::BTreeSet;

    // Collect the events that belong to this specific hub.
    let matching: Vec<&Event> = events
        .iter()
        .filter(|e| match kind {
            hubs::HubKind::App => e
                .app_name
                .as_deref()
                .and_then(hub_slug_for_app)
                .is_some_and(|s| s == slug),
            hubs::HubKind::Site => e
                .url
                .as_deref()
                .filter(|u| u.starts_with("http"))
                .and_then(domain_from_url)
                .is_some_and(|d| d == slug),
            hubs::HubKind::Activity => e
                .content_type
                .as_deref()
                .and_then(activity_slug_for)
                .is_some_and(|s| s == slug),
            // Project membership is inferred at the day level, not per-event, so
            // we describe the whole day's window titles below.
            hubs::HubKind::Project => true,
        })
        .collect();

    if matching.is_empty() {
        return String::new();
    }

    // Prefer concrete, distinct window titles; fall back to URLs. Keep it to a
    // handful so the hub row stays a one-liner.
    let mut labels: BTreeSet<String> = BTreeSet::new();
    for event in &matching {
        let label = event
            .window_title
            .as_deref()
            .filter(|t| !t.trim().is_empty())
            .or_else(|| event.url.as_deref().filter(|u| u.starts_with("http")))
            .map(|s| clean_detail(s));
        if let Some(label) = label {
            if !label.is_empty() {
                labels.insert(label);
            }
        }
        if labels.len() >= 3 {
            break;
        }
    }

    let count = matching.len();
    let noun = if count == 1 { "capture" } else { "captures" };
    if labels.is_empty() {
        format!("{count} {noun}")
    } else {
        let joined = labels.into_iter().collect::<Vec<_>>().join("; ");
        format!("{count} {noun}: {joined}")
    }
}

/// Flatten a captured string into a single clean line safe for a markdown bullet:
/// collapse whitespace, drop characters that would break the list layout, and cap
/// the length so one noisy title can't blow out the hub row.
fn clean_detail(value: &str) -> String {
    let collapsed = value
        .replace(['\n', '\r', '|', '[', ']'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    const MAX: usize = 80;
    if collapsed.chars().count() > MAX {
        let truncated: String = collapsed.chars().take(MAX).collect();
        format!("{}…", truncated.trim_end())
    } else {
        collapsed
    }
}

/// Path of the daily note inside the vault, relative form (e.g. `TaskFlow/Memory/Daily/2026-07-05.md`).
pub fn daily_note_relative_path(date: &str) -> PathBuf {
    PathBuf::from("TaskFlow")
        .join("Memory")
        .join("Daily")
        .join(format!("{date}.md"))
}

/// Path of a hub page inside the vault, relative form.
pub fn hub_relative_path(kind: hubs::HubKind, slug: &str) -> PathBuf {
    PathBuf::from("TaskFlow")
        .join(kind.folder_name())
        .join(format!("{slug}.md"))
}

/// Convert an arbitrary string into a hub-safe slug (alphanumeric + dashes).
/// Reused for project inference where the input is a free-form window-title token.
pub fn slugify_hub(value: &str) -> String {
    let cleaned: String = value
        .trim()
        .chars()
        .map(|ch| if ch.is_alphanumeric() { ch } else { '-' })
        .collect();
    let collapsed: String = cleaned
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    collapsed.trim_matches('-').to_string()
}

/// Strip the leading `www.` from a hostname for friendly hub slugs.
pub fn strip_www(host: &str) -> String {
    host.strip_prefix("www.")
        .unwrap_or(host)
        .to_lowercase()
}

/// Orchestrator called from `summarize_task` after the daily note markdown has been
/// assembled. Writes/patches all hub pages touched by today's events, updates the
/// content catalog (`index.md`), and appends to the chronological `log.md`.
///
/// This is the compounding step that makes the wiki accumulate memory rather than
/// re-deriving it from scratch on every ingest. It is purely mechanical — no LLM
/// call — so it works even when the sidecar is offline.
pub fn update_wiki(
    vault: &Path,
    today_date: &str,
    task: &Task,
    events: &[Event],
    summary: &str,
    hub_synthesis: &std::collections::HashMap<String, String>,
    known_projects: &[String],
) -> Result<WikiUpdateReport, String> {
    let hubs = hubs_touched(task, events, known_projects);
    let mut hub_paths = Vec::with_capacity(hubs.len());
    for hub in &hubs {
        let detail = hub_detail(hub.kind, &hub.slug, events);
        // The LLM keys its synthesis by "<Folder>/<slug>" (e.g. "Apps/Notion"),
        // matching the sentinel it emits. Absent → mechanical-only (no synthesis
        // section touched), which is exactly the sidecar-offline behavior.
        let key = format!("{}/{}", hub.kind.folder_name(), hub.slug);
        let synthesis = hub_synthesis
            .get(&key)
            .map(String::as_str)
            .filter(|s| !s.trim().is_empty());
        let path =
            hubs::upsert_hub_page(vault, hub.kind, &hub.slug, today_date, &detail, synthesis)
                .map_err(|err| format!("Failed to upsert hub page: {err}"))?;
        hub_paths.push(path);
    }

    // One-time-per-registry cleanup: once the user configures real projects,
    // remove the old keyword-dust stubs the leading-word heuristic created
    // (e.g. Projects/Array.md, Projects/Usage.md) — including dust workstream
    // pages the roll-up router minted. Only deletes pages that are pure
    // machine-generated output (no Synthesis, no hand edits). Deleted slugs are
    // reported so the async caller can reassign their roll-ups to Inbox.
    let mut deleted_project_slugs = Vec::new();
    if !known_projects.is_empty() {
        let allowed: std::collections::BTreeSet<String> =
            known_projects.iter().map(|p| slugify_hub(p)).collect();
        match cleanup_junk_project_pages(vault, &allowed) {
            Ok(slugs) => deleted_project_slugs = slugs,
            Err(err) => eprintln!("[taskflow:wiki] junk-project cleanup failed: {err}"),
        }
        if let Err(err) = normalize_app_hub_filenames(vault) {
            eprintln!("[taskflow:wiki] app-hub filename normalization failed: {err}");
        }
    }

    // Phase 3 cross-links: draw hub↔hub edges from co-occurrence on the same
    // day. For every hub touched today, compute the OTHER hubs it appeared with
    // (app+site / app+project / site+project on the same event) and write a
    // cross-link section onto each side, symmetric. Fully recomputed per hub so
    // a re-ingest replaces (never duplicates); a hub with no neighbors this day
    // gets its section removed. Cross-links land AFTER upsert so the pages exist.
    apply_cross_links(vault, events, &hubs, known_projects);

    let index_path = hubs::update_index(vault, today_date, summary)
        .map_err(|err| format!("Failed to update index: {err}"))?;

    // Vault-wide chronological command index. Idempotent per (day+command); a
    // re-ingest of a day replaces that day's rows in place. No-op + no file
    // created when nothing terminal was captured that day.
    let commands = links::collect_terminal_commands(events);
    let commands_path = if commands.is_empty() {
        None
    } else {
        match hubs::update_command_index(vault, today_date, &commands) {
            Ok(p) => Some(p),
            Err(err) => {
                eprintln!("[taskflow:wiki] command index update failed: {err}");
                None
            }
        }
    };

    let log_path = hubs::append_log(vault, today_date, &task.title)
        .map_err(|err| format!("Failed to append log: {err}"))?;

    Ok(WikiUpdateReport {
        hub_paths,
        index_path,
        log_path,
        commands_path,
        hubs_touched: hubs,
        deleted_project_slugs,
    })
}

/// A small summary of what the wiki update did. Returned so callers can log it
/// or surface it to the frontend.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiUpdateReport {
    pub hub_paths: Vec<PathBuf>,
    pub index_path: PathBuf,
    pub log_path: PathBuf,
    /// Path to the vault-wide `Commands.md` index, or None if no terminal
    /// commands were captured that day (the index file wasn't touched).
    pub commands_path: Option<PathBuf>,
    pub hubs_touched: Vec<WikiHubRef>,
    /// Dust workstream pages removed by the registry cleanup. The async caller
    /// must reassign these slugs' roll-ups to Inbox and rebuild the affected
    /// daily indexes, or the derived index will link deleted pages.
    pub deleted_project_slugs: Vec<String>,
}

/// Load the markdown of the `n` most recent daily notes preceding `today_date`
/// from the vault, plus the hub pages for the apps/sites the current events touch.
/// Used to inject "prior context" into the summarizer so the synthesis reflects
/// everything captured so far rather than amnesia.
///
/// Returns None if the vault is missing, no prior notes exist, or all sources are
/// empty. Caps the total returned text at ~8KB so we don't blow the LLM context.
pub fn load_prior_context(
    vault: &Path,
    today_date: &str,
    task: &Task,
    events: &[Event],
    prior_dates: &[String],
    known_projects: &[String],
) -> Option<String> {
    const BUDGET: usize = 8 * 1024;

    let mut sections: Vec<String> = Vec::new();
    let mut used = 0usize;

    for date in prior_dates {
        if used >= BUDGET {
            break;
        }
        if date == today_date {
            continue;
        }
        let path = daily_note_relative_path(date);
        let abs = vault.join(&path);
        match std::fs::read_to_string(&abs) {
            Ok(content) => {
                let trimmed = content.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let remaining = BUDGET.saturating_sub(used);
                let slice = truncate_to_char_boundary(trimmed, remaining);
                let section = format!("### Memory/Daily/{date}\n{slice}\n");
                used += section.len();
                sections.push(section);
            }
            Err(_) => continue,
        }
    }

    let hubs = hubs_touched(task, events, known_projects);
    for hub in hubs {
        if used >= BUDGET {
            break;
        }
        let path = hub_relative_path(hub.kind, &hub.slug);
        let abs = vault.join(&path);
        match std::fs::read_to_string(&abs) {
            Ok(content) => {
                let trimmed = content.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let remaining = BUDGET.saturating_sub(used);
                let slice = truncate_to_char_boundary(trimmed, remaining);
                let section = format!(
                    "### {}/{}\n{slice}\n",
                    hub.kind.folder_name(),
                    hub.slug
                );
                used += section.len();
                sections.push(section);
            }
            Err(_) => continue,
        }
    }

    if sections.is_empty() {
        return None;
    }
    let mut out = String::from("# Prior wiki context\n\n");
    out.push_str(
        "Below is the accumulated Memory Tree state from recently captured daily notes\n",
    );
    out.push_str(
        "and the hub pages today's activity touches. Use this to maintain continuity\n",
    );
    out.push_str("with prior work — reference prior days or hub trends where relevant.\n\n---\n\n");
    out.push_str(&sections.join("\n"));
    Some(out)
}

fn truncate_to_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut idx = max_bytes;
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    &s[..idx]
}

/// Parse the per-hub synthesis blocks the LLM appends to its markdown. The agreed
/// protocol is a fenced code block whose info string is `hub:<Folder>/<slug>`:
///
/// ```text
/// ```hub:Apps/Notion
/// The user uses Notion as their primary knowledge base, mostly for...
/// ```
/// ```
///
/// Returns a map keyed by `"<Folder>/<slug>"` (e.g. `"Apps/Notion"`) → the block
/// body. Malformed or unterminated blocks are skipped rather than erroring, so a
/// sloppy model response degrades to "no synthesis for that hub" instead of
/// breaking the ingest. The blocks are also stripped from the daily-note markdown
/// by `strip_hub_blocks` so they don't leak into the human-facing Timeline Note.
pub fn parse_hub_synthesis(markdown: &str) -> std::collections::HashMap<String, String> {
    use std::collections::HashMap;

    let mut out: HashMap<String, String> = HashMap::new();
    let mut lines = markdown.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        let key = trimmed
            .strip_prefix("```hub:")
            .or_else(|| trimmed.strip_prefix("~~~hub:"))
            .map(str::trim)
            .filter(|k| !k.is_empty());
        let Some(key) = key else { continue };

        let mut body: Vec<String> = Vec::new();
        let mut closed = false;
        for inner in lines.by_ref() {
            let it = inner.trim_start();
            if it.starts_with("```") || it.starts_with("~~~") {
                closed = true;
                break;
            }
            body.push(inner.to_string());
        }
        if !closed {
            // Unterminated fence — discard and stop; the rest is likely truncated.
            break;
        }
        let text = body.join("\n").trim().to_string();
        if !text.is_empty() {
            out.insert(key.to_string(), text);
        }
    }

    out
}

/// Remove the `hub:<...>` fenced blocks from markdown so the daily note's Timeline
/// Note shows only the human-facing summary, not the machine-readable hub payloads.
/// Anything that isn't a well-formed hub block is left untouched.
pub fn strip_hub_blocks(markdown: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut lines = markdown.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        let is_hub_open = trimmed.starts_with("```hub:") || trimmed.starts_with("~~~hub:");
        if !is_hub_open {
            out.push(line.to_string());
            continue;
        }
        // Skip through the closing fence (or to EOF if unterminated).
        for inner in lines.by_ref() {
            let it = inner.trim_start();
            if it.starts_with("```") || it.starts_with("~~~") {
                break;
            }
        }
    }

    // Collapse any run of blank lines left where blocks were removed.
    let mut cleaned: Vec<String> = Vec::with_capacity(out.len());
    let mut prev_blank = false;
    for line in out {
        let blank = line.trim().is_empty();
        if blank && prev_blank {
            continue;
        }
        prev_blank = blank;
        cleaned.push(line);
    }
    let mut result = cleaned.join("\n");
    result = result.trim_end().to_string();
    result.push('\n');
    result
}

/// Remove old keyword-dust project hub pages left by the leading-word heuristic
/// once the user has configured real projects. Conservative: a `Projects/<slug>.md`
/// file is deleted only if its slug is NOT in `allowed` AND the page is pure
/// machine-generated output (`is_dust_project_page`). Returns the deleted slugs
/// so the async caller can reassign their roll-ups to Inbox and rebuild the
/// affected daily indexes.
fn cleanup_junk_project_pages(
    vault: &Path,
    allowed: &std::collections::BTreeSet<String>,
) -> std::io::Result<Vec<String>> {
    let mut deleted = Vec::new();
    let dir = vault.join("TaskFlow").join(hubs::HubKind::Project.folder_name());
    if !dir.is_dir() {
        return Ok(deleted);
    }
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if allowed.contains(&stem) {
            continue; // a real configured project — never touch
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if !is_dust_project_page(&content) {
            continue; // has real content (notes, LLM synthesis, hand edits) — preserve
        }
        // Safe to remove. Best-effort; ignore individual delete failures.
        if std::fs::remove_file(&path).is_ok() {
            deleted.push(stem);
        }
    }
    Ok(deleted)
}

/// Is this Projects page pure machine-generated dust? True iff ALL of:
///   1. No `## Synthesis` section (LLM-owned narrative = real memory).
///   2. Outside the `## Activity Timeline` section, the page is nothing but
///      boilerplate + backlinks (the `is_bare_stub` structural check).
///   3. Inside `## Activity Timeline`, every entry carries a `<!-- rollup:id -->`
///      marker — i.e. the whole timeline is roll-up output, with no hand-typed
///      edits. A hand-edited entry breaks this and preserves the page.
/// Dust timelines are safe to delete because the roll-up rows backing them
/// live in SQLite and get reassigned to Inbox by the caller.
fn is_dust_project_page(content: &str) -> bool {
    if !content.contains(workstream::TIMELINE_HEADING) {
        return is_bare_stub(content);
    }
    if content.contains(hubs::SYNTHESIS_HEADING) {
        return false;
    }

    let mut outside: Vec<&str> = Vec::new();
    let mut timeline_entries: Vec<String> = Vec::new();
    let mut current_entry: Vec<&str> = Vec::new();
    let mut in_timeline = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == workstream::TIMELINE_HEADING {
            in_timeline = true;
            outside.push(line);
            continue;
        }
        if in_timeline && trimmed.starts_with("## ") {
            in_timeline = false;
        }
        if in_timeline {
            // A `<!-- rollup:` marker ALWAYS starts a new entry. A bare `### `
            // heading only starts one when the current chunk has no marker —
            // modern entries are "marker line + ### heading + body", and the
            // heading must not split the marker off from its own entry.
            let has_marker = current_entry
                .iter()
                .any(|l| l.trim_start().starts_with("<!-- rollup:"));
            let starts_entry = trimmed.starts_with("<!-- rollup:")
                || (trimmed.starts_with("### ") && !has_marker);
            if starts_entry && !current_entry.is_empty() {
                timeline_entries.push(current_entry.join("\n"));
                current_entry.clear();
            }
            current_entry.push(line);
        } else {
            outside.push(line);
        }
    }
    if !current_entry.is_empty() {
        timeline_entries.push(current_entry.join("\n"));
    }

    // Every non-blank timeline entry must be marker-stamped roll-up output.
    if timeline_entries
        .iter()
        .filter(|entry| !entry.trim().is_empty())
        .any(|entry| !entry.contains("<!-- rollup:"))
    {
        return false;
    }
    is_bare_stub(&outside.join("\n"))
}

/// Heuristic: is this hub page body nothing but boilerplate + per-day backlink
/// bullets? `true` iff there is no `## Synthesis` section and every non-blank
/// line is structural (heading, frontmatter, a yaml `key: value`, a backlink
/// bullet, or one of the known hub-description boilerplate lines). A page with
/// any genuine prose — hand-written notes or LLM synthesis — is NOT a bare stub
/// and is preserved. The boilerplate set includes both the current
/// `hub_description`s and the legacy "Apps in this hub." string written by older
/// versions, so old keyword-dust stubs still qualify for cleanup.
const STUB_BOILERPLATE: &[&str] = &[
    "Daily captures that involved this application.",
    "Daily captures that referenced this site.",
    "Daily captures containing this kind of activity.",
    "Daily captures related to this project.",
    // Boilerplate from the workstream bootstrap page (workstream.rs).
    "Rolling workstream node — TaskFlow appends one entry per activity roll-up.",
    // Legacy boilerplate emitted by older upsert_hub_page versions.
    "Apps in this hub.",
];

fn is_bare_stub(content: &str) -> bool {
    if content.contains(hubs::SYNTHESIS_HEADING) {
        return false;
    }
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        // Headings (# / ##), frontmatter delimiters (---), yaml `key: value`
        // inside the fences, and backlink bullets (`- `) are all structural.
        // The hub-description boilerplate is also structural (it's generated,
        // not user content). Anything else (a real prose sentence, a hand note)
        // means the page has genuine content => keep it.
        let is_structure = line == "---"
            || line.starts_with('#')
            || (line.contains(':') && !line.starts_with('-'))
            || line.starts_with("- ")
            || STUB_BOILERPLATE.iter().any(|b| *b == line);
        if !is_structure {
            return false;
        }
    }
    true
}

/// Normalize `Apps/<name>.md` filenames to the canonical `hub_slug_for_app`
/// slug (lowercased, extension-stripped). Older captures minted case variants
/// (`Explorer-EXE.md`, `PickerHost-Exe.md`) that fork the hub from `explorer.md`.
///   - Bare stubs with a non-canonical name are deleted outright (future
///     captures recreate the canonical page).
///   - Pages with real content are RENAMED to the canonical slug when no
///     canonical file exists yet; if one does, both are left for a manual merge
///     (we never silently merge prose).
fn normalize_app_hub_filenames(vault: &Path) -> std::io::Result<()> {
    let dir = vault.join("TaskFlow").join(hubs::HubKind::App.folder_name());
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(stem) => stem.to_string(),
            None => continue,
        };
        // Legacy captures with an upper-case extension ("Explorer.EXE") slipped
        // past the case-sensitive strip and were slugified to "Explorer-EXE".
        // Recover the intended name by mapping a trailing dash-extension back.
        let base = if stem.len() > 4 && stem.to_lowercase().ends_with("-exe") {
            &stem[..stem.len() - 4]
        } else {
            &stem
        };
        let canonical = match links::hub_slug_for_app(base) {
            Some(slug) => slug,
            None => continue,
        };
        if canonical == stem {
            continue;
        }
        let canonical_path = dir.join(format!("{canonical}.md"));
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        if is_bare_stub(&content) {
            let _ = std::fs::remove_file(&path);
        } else if !canonical_path.exists() {
            let _ = std::fs::rename(&path, &canonical_path);
        }
    }
    Ok(())
}

/// Hub kind this page is (row) × hub kind its neighbors are (column) → the
/// section heading written onto the page under which those neighbor links land.
/// Activity hubs are deliberately NOT cross-linked (an "activity" isn't an entity
/// the graph relates; it's a capture bucket), hence their absence here. The
/// headings are phrased from the perspective of the host page ("Sites used here"
/// on an App page), and the symmetric reverse is its own cell.
fn cross_link_heading(host: hubs::HubKind, neighbor: hubs::HubKind) -> Option<&'static str> {
    use hubs::HubKind::*;
    match (host, neighbor) {
        (App, Site) => Some("## Sites used here"),
        (App, Project) => Some("## Projects touched here"),
        (Site, App) => Some("## Apps that visited"),
        (Site, Project) => Some("## Projects that referenced this"),
        (Project, App) => Some("## Apps used"),
        (Project, Site) => Some("## Sites used"),
        // No self-links, no Activity edges.
        _ => None,
    }
}

/// Draw the hub↔hub cross-link sections for one day's ingest. For every hub
/// touched today we record, per neighbor kind (Sites/Projects for Apps,
/// Apps/Projects for Sites, Apps/Sites for Projects), the distinct neighbor
/// slugs that co-occur on the SAME event — an App and a Site co-occur when one
/// event carries both that app's name and a URL; a Project co-occurs when the
/// event's text names that project (registry substring). Edges are symmetric:
/// the App lists the Site under "Sites used here" AND the Site lists the App
/// under "Apps that visited".
///
/// Each section is written (or removed) via `write_cross_link_section`, which
/// fully recomputes it per ingest, so re-ingesting a day replaces rows in place
/// and never duplicates. Strictly best-effort: a failed write is logged and
/// skipped rather than failing the whole ingest, since the mechanical backlinks
/// and synthesis are already on disk.
fn apply_cross_links(
    vault: &Path,
    events: &[Event],
    hubs: &[WikiHubRef],
    known_projects: &[String],
) {
    use hubs::HubKind;
    use std::collections::{BTreeMap, BTreeSet};

    // Only App/Site/Project hubs participate; Activity is a capture bucket, not
    // a graph entity. Pre-collect which of each kind the day touched so we only
    // draw edges to hubs that actually have pages this ingest.
    let touched: std::collections::BTreeMap<(HubKind, String), ()> = hubs
        .iter()
        .filter(|h| h.kind != HubKind::Activity)
        .map(|h| ((h.kind, h.slug.clone()), ()))
        .collect();
    if touched.is_empty() {
        return;
    }

    // adjacency[(host_kind, host_slug)][neighbor_kind] = set of neighbor slugs.
    // Built by tagging each event with its app/site/project slug(s) and drawing
    // complete bipartite edges among the tags present on that one event.
    let mut adjacency: BTreeMap<(HubKind, String), BTreeMap<HubKind, BTreeSet<String>>> =
        BTreeMap::new();

    for event in events {
        let app_slug = event
            .app_name
            .as_deref()
            .filter(|a| !a.trim().is_empty())
            .and_then(links::hub_slug_for_app);
        // Site slugs are keyed by the RAW domain (see `hubs_touched`: it inserts
        // `domain_from_url(url)` without slugifying, so the on-disk page is
        // Sites/openrouter.ai.md, not Sites/openrouter-ai.md). Mirror that here
        // exactly or the `touched` membership filter drops every site edge.
        let site_slug = event
            .url
            .as_deref()
            .filter(|u| u.starts_with("http"))
            .and_then(links::domain_from_url);
        let project_slugs = links::resolve_projects_for_event(event, known_projects);

        // Collect the (kind, slug) tags present on this event.
        let tags: Vec<(HubKind, String)> = app_slug
            .into_iter()
            .map(|s| (HubKind::App, s))
            .chain(site_slug.into_iter().map(|s| (HubKind::Site, s)))
            .chain(
                project_slugs
                    .into_iter()
                    .map(|s| (HubKind::Project, s)),
            )
            .filter(|t| touched.contains_key(t))
            .collect();

        // Only draw edges among distinct sorts (App↔Site, App↔Project,
        // Site↔Project); a sort with no partners this event contributes nothing.
        for i in 0..tags.len() {
            for j in 0..tags.len() {
                if i == j {
                    continue;
                }
                let (host_kind, host_slug) = &tags[i];
                let (neigh_kind, neigh_slug) = &tags[j];
                if host_kind == neigh_kind {
                    continue; // no same-kind self edges
                }
                adjacency
                    .entry((*host_kind, host_slug.clone()))
                    .or_default()
                    .entry(*neigh_kind)
                    .or_default()
                    .insert(neigh_slug.clone());
            }
        }
    }

    // Emit one section per (host hub, neighbor kind), phrased via the heading
    // table. Hubs that ended up with NO neighbors this day still need any stale
    // section from a prior ingest removed — so iterate the touched set, not the
    // adjacency map, and write an empty neighbor list (which `write_cross_link_section`
    // turns into a removal).
    for (host_kind, host_slug) in touched.keys() {
        let neighbors = adjacency
            .get(&(*host_kind, host_slug.clone()))
            .cloned()
            .unwrap_or_default();
        for neigh_kind in [HubKind::App, HubKind::Site, HubKind::Project] {
            let Some(heading) = cross_link_heading(*host_kind, neigh_kind) else {
                continue;
            };
            let mut links: Vec<String> = neighbors
                .get(&neigh_kind)
                .map(|s| {
                    let mut v: Vec<String> = s
                        .iter()
                        .map(|slug| format!("- [[{}/{}]]", neigh_kind.folder_name(), slug))
                        .collect();
                    v.sort();
                    v
                })
                .unwrap_or_default();
            links.dedup();
            if let Err(err) =
                hubs::write_cross_link_section(vault, *host_kind, host_slug, heading, &links)
            {
                eprintln!(
                    "[taskflow:wiki] cross-link write failed for {}/{} ({}): {err}",
                    host_kind.folder_name(),
                    host_slug,
                    heading
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! End-to-end tests for the wiki ingest pipeline. These exercise the REAL
    //! `update_wiki` code path (the same one the GUI "Stop / daily-capture" flow
    //! calls) against an isolated temp vault with synthetic events, so the GUI
    //! build (which this machine can't complete — LNK1318 PDB limit) isn't needed
    //! to verify Phases 1–3 land on disk correctly.
    //!
    //! No `tempfile` dev-dependency is used: each test sweeps a uniquely-named
    //! subdir of the OS temp dir (deterministic name + a process-global counter
    //! rather than wall-clock, since `Date::now` is unavailable). Tests also wipe
    //! their subdir on entry so re-runs start clean.

    use super::*;
    use crate::database::events::Event;
    use crate::database::tasks::Task;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_SEQ: AtomicUsize = AtomicUsize::new(0);

    /// Build an isolated temp vault for one test, wiping any prior contents at
    /// the same path so re-runs are clean. Returns the vault root PathBuf.
    fn fresh_vault(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "taskflow-wiki-test-{name}-{}",
            TEST_SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).ok();
        }
        std::fs::create_dir_all(&dir).expect("create temp vault");
        dir
    }

    /// Minimal task wired for a memory daily-capture (the `source` value
    /// `memory_tree_markdown`/`update_wiki` care about). `started_at` drives the
    /// `## Recent Daily Notes` backlink date and is decoupled from `today_date`.
    fn memory_task(started_at: &str, source_project: Option<&str>) -> Task {
        Task {
            id: "task-1".to_string(),
            title: "Daily capture".to_string(),
            description: None,
            source: "memory".to_string(),
            source_id: Some("daily-capture-2026-07-10".to_string()),
            source_url: None,
            source_title: None,
            source_body: None,
            source_labels: None,
            source_assignee: None,
            source_priority: None,
            source_project: source_project.map(str::to_string),
            source_branch: None,
            status: "done".to_string(),
            started_at: Some(started_at.to_string()),
            ended_at: Some(format!("{started_at}+00:01")),
            created_at: started_at.to_string(),
        }
    }

    /// One synthetic event. `app`/`url`/`content_type`/`content` are the real
    /// signal these features read; the string fields are left blank where unused.
    fn ev(app: Option<&str>, url: Option<&str>, ctype: Option<&str>, content: Option<&str>) -> Event {
        Event {
            id: format!("ev-{}", TEST_SEQ.fetch_add(1, Ordering::SeqCst)),
            task_id: "task-1".to_string(),
            event_type: "capture".to_string(),
            app_name: app.map(str::to_string),
            window_title: content.map(str::to_string),
            content: content.map(str::to_string),
            url: url.map(str::to_string),
            content_type: ctype.map(str::to_string),
            capture_method: None,
            is_sanitized: 1,
            chunk_index: 0,
            relevance: 1.0,
            timestamp: "2026-07-10T10:00:00+00:00".to_string(),
            created_at: "2026-07-10T10:00:00+00:00".to_string(),
        }
    }

    #[test]
    fn phase1_registry_resolves_real_projects_and_drops_dust() {
        let vault = fresh_vault("p1");
        let known = vec![
            "TaskFlow".to_string(),
            "datavex3".to_string(),
        ];
        // A day's events touching TaskFlow in two forms: a terminal session
        // and a browser visit to the TaskFlow repo. No "Array"/"Mozilla" noise
        // is intended as a project.
        let events = vec![
            ev(Some("WindowsTerminal"), None, Some("TerminalContent"), Some("$ git commit -m \"feat: hub synthesis\"")),
            ev(Some("chrome"), Some("https://github.com/kaush/TaskFlow"), Some("BrowserContent"), Some("TaskFlow repo")),
        ];
        let task = memory_task("2026-07-10T10:00:00+00:00", None);

        // Supply the LLM synthesis for the project hub, keyed
        // "<Folder>/<slug>" exactly as parse_hub_synthesis would produce it.
        let mut synth = std::collections::HashMap::new();
        synth.insert(
            "Projects/TaskFlow".to_string(),
            "TaskFlow hub synthesis is progressing; today merged the project-resolution registry.".to_string(),
        );

        let report = update_wiki(&vault, "2026-07-10", &task, &events, "synth", &synth, &known)
            .expect("update_wiki ok");

        // The day touches app TaskFlow-adjacent hubs: Apps/github -> dropped to
        // a domain site hub github.com, plus the explicit WindowsTerminal app and
        // the Projects/TaskFlow hub from the registry.
        dbg!(&report.hubs_touched);
        let project_hubs: Vec<&str> = report
            .hubs_touched
            .iter()
            .filter(|h| h.kind == hubs::HubKind::Project)
            .map(|h| h.slug.as_str())
            .collect();
        // Project slugs preserve the registry author's casing (resolve_projects
        // slugifies the original name, not a lowercased copy), so compare CI.
        let lower_hubs: Vec<String> =
            project_hubs.iter().map(|s| s.to_lowercase()).collect();
        assert!(lower_hubs.iter().any(|s| s == "taskflow"),
            "expected Projects/taskflow hub, got {project_hubs:?}");
        assert!(!lower_hubs.iter().any(|s| s == "array" || s == "mozilla"),
            "dust projects must NOT be hubs: {project_hubs:?}");

        // Seed the dust stubs the OLD heuristic would have created, then re-run:
        // cleanup must delete them because they're not in the registry and bare.
        let projects_dir = vault.join("TaskFlow").join("Projects");
        std::fs::create_dir_all(&projects_dir).ok();
        std::fs::write(
            projects_dir.join("Array.md"),
            "---\ntype: hub_page\nkind: Projects\nslug: Array\n---\n\n# Projects/Array\n\nDaily captures related to this project.\n\n## Recent Daily Notes\n- [[Memory/Daily/2026-07-01]]\n",
        ).ok();
        let _ = update_wiki(&vault, "2026-07-10", &task, &events, "synth", &Default::default(), &known).unwrap();
        assert!(!projects_dir.join("Array.md").exists(),
            "dust stub Array.md should be cleaned up once the registry is configured");

        // The real project page carries last_touched + an LLM Synthesis section
        // when synthesis is supplied. (The template renames ## Synthesis to
        // ## Status for readability; the on-disk heading is ## Synthesis.)
        let proj = std::fs::read_to_string(projects_dir.join("TaskFlow.md"))
            .expect("Projects/TaskFlow.md exists");
        assert!(proj.contains("last_touched: 2026-07-10"), "last_touched advanced: {proj}");
        assert!(proj.contains("## Synthesis"), "synthesis section present: {proj}");
    }

    #[test]
    fn phase2_commands_section_and_index() {
        let vault = fresh_vault("p2");
        let known: Vec<String> = Vec::new(); // no registry → heuristic fallback, fine here
        let events = vec![
            ev(Some("WindowsTerminal"), None, Some("TerminalContent"),
               Some("$ cargo build\ngit status\n$ pnpm test")),
            ev(Some("WindowsTerminal"), None, Some("TerminalContent"),
               Some("cargo build\n$ cargo test")),
            ev(Some("Code"), Some("https://docs.rs"), Some("CodeContent"), Some("fn main(){}")),
        ];
        let task = memory_task("2026-07-10T10:00:00+00:00", None);

        // Hub update must produce and populate TaskFlow/Commands.md from the
        // two terminal events, deduped (cargo build appears twice → once).
        let report = update_wiki(&vault, "2026-07-10", &task, &events, "synth", &Default::default(), &known)
            .expect("update_wiki ok");
        assert!(report.commands_path.is_some(), "commands_path set when terminal events exist");
        let commands_md = std::fs::read_to_string(report.commands_path.as_ref().unwrap())
            .expect("Commands.md readable");
        assert!(commands_md.contains("cargo build"), "cargo build present: {commands_md}");
        assert!(commands_md.contains("pnpm test"), "pnpm test present: {commands_md}");
        // Dedupe: cargo build appears in BOTH terminal events but must list once.
        let cargo_count = commands_md.matches("cargo build").count();
        assert_eq!(cargo_count, 1, "cargo build deduped to one row, got {cargo_count}");

        // Commands are plain text bullets in the vault index, never wikilinks
        // (a git command is searchable text, not a graph node).
        assert!(!commands_md.contains("[[cargo"), "commands are plain text: {commands_md}");
    }

    #[test]
    fn phase4_dust_workstreams_cleaned_and_app_names_normalized() {
        let vault = fresh_vault("p4");
        let known = vec!["TaskFlow".to_string()];
        let projects_dir = vault.join("TaskFlow").join("Projects");
        std::fs::create_dir_all(&projects_dir).unwrap();

        // Pure machine-generated dust workstream page (bootstrap format,
        // marker-stamped timeline entry with prose body) → cleanable.
        std::fs::write(
            projects_dir.join("Evil.md"),
            "---\ntype: hub_page\nkind: Projects\nslug: Evil\nlast_touched: 2026-07-27\n---\n\n\
             # Projects/Evil\n\n\
             Rolling workstream node — TaskFlow appends one entry per activity roll-up.\n\n\
             ## Activity Timeline\n\n\
             <!-- rollup:r1 -->\n### 2026-07-27 09:00–09:10 — Evil window\n\n\
             #### Summary\nTemplate prose with real sentences that are not structural.\n",
        ).unwrap();
        // Hand-edited dust page (timeline entry WITHOUT a roll-up marker) → preserved.
        std::fs::write(
            projects_dir.join("Notes.md"),
            "---\ntype: hub_page\nkind: Projects\nslug: Notes\n---\n\n\
             # Projects/Notes\n\nDaily captures related to this project.\n\n\
             ## Activity Timeline\n\n### My own notes\nI wrote this by hand.\n",
        ).unwrap();

        let apps_dir = vault.join("TaskFlow").join("Apps");
        std::fs::create_dir_all(&apps_dir).unwrap();
        // Non-canonical stub → deleted.
        std::fs::write(
            apps_dir.join("PickerHost-Exe.md"),
            "---\ntype: hub_page\nkind: Apps\nslug: PickerHost-Exe\n---\n\n\
             # Apps/PickerHost-Exe\n\nDaily captures that involved this application.\n\n\
             ## Recent Daily Notes\n- [[Memory/Daily/2026-07-01]]\n",
        ).unwrap();
        // Non-canonical page WITH synthesis (extension-case artifact — a genuinely
        // different filename from its canonical slug) → renamed to canonical.
        std::fs::write(
            apps_dir.join("Explorer-EXE.md"),
            "---\ntype: hub_page\nkind: Apps\nslug: Explorer-EXE\n---\n\n\
             # Apps/Explorer-EXE\n\nDaily captures that involved this application.\n\n\
             ## Synthesis\nFile browsing for the hackathon assets.\n",
        ).unwrap();

        let events = vec![ev(Some("Code"), None, Some("CodeContent"), Some("TaskFlow work"))];
        let task = memory_task("2026-07-10T10:00:00+00:00", None);
        let report = update_wiki(&vault, "2026-07-10", &task, &events, "synth", &Default::default(), &known)
            .expect("update_wiki ok");

        assert!(!projects_dir.join("Evil.md").exists(),
            "machine-generated dust workstream page deleted");
        assert!(report.deleted_project_slugs.iter().any(|s| s == "Evil"),
            "deleted slug reported for roll-up reassignment: {:?}", report.deleted_project_slugs);
        assert!(projects_dir.join("Notes.md").exists(),
            "hand-edited page preserved even though not in registry");

        assert!(!apps_dir.join("PickerHost-Exe.md").exists(), "non-canonical stub deleted");
        assert!(apps_dir.join("explorer.md").exists(), "rich page renamed to canonical slug");
        assert!(!apps_dir.join("Explorer-EXE.md").exists(), "old name gone after rename");
    }

    #[test]
    fn phase3_app_to_site_and_back() {
        let vault = fresh_vault("p3");
        // One event with BOTH an app and a URL → App↔Site co-occurrence.
        let events = vec![
            ev(Some("Jan"), Some("https://openrouter.ai/api/v1"), Some("ChatContent"),
               Some("talking about Jan + openrouter")),
            ev(Some("Jan"), Some("https://openrouter.ai/dashboard"), Some("ChatContent"),
               Some("again jan and openrouter.ai")),
        ];
        let task = memory_task("2026-07-10T10:00:00+00:00", None);
        let known = vec!["Jan".to_string()];

        let _ = update_wiki(&vault, "2026-07-10", &task, &events, "synth", &Default::default(), &known)
            .expect("update_wiki ok");

        let app_page = std::fs::read_to_string(
            vault.join("TaskFlow").join("Apps").join("jan.md"),
        ).expect("Apps/jan.md exists");
        // "Jan" is BOTH a registry project and an app here (collision by design
        // to keep the test small); the App page must show the Site it visited.
        assert!(app_page.contains("## Sites used here"), "App page Sites section: {app_page}");
        assert!(app_page.contains("[[Sites/openrouter.ai]]"), "App page links the Site: {app_page}");

        let site_page = std::fs::read_to_string(
            vault.join("TaskFlow").join("Sites").join("openrouter.ai.md"),
        ).expect("Sites/openrouter.ai.md exists");
        assert!(site_page.contains("## Apps that visited"), "Site page Apps section: {site_page}");
        assert!(site_page.contains("[[Apps/jan]]"), "Site page links the App back: {site_page}");

        // Idempotency: re-ingest must not duplicate the cross-link rows.
        let before = app_page.matches("[[Sites/openrouter.ai]]").count();
        let _ = update_wiki(&vault, "2026-07-10", &task, &events, "synth", &Default::default(), &known).unwrap();
        let after = std::fs::read_to_string(vault.join("TaskFlow").join("Apps").join("jan.md"))
            .unwrap()
            .matches("[[Sites/openrouter.ai]]").count();
        assert_eq!(before, after, "re-ingest does not duplicate cross-link rows ({before} -> {after})");

        // A day with NO sites for an app that still has a page must REMOVE the
        // stale section (recompute-and-replace semantics, not append).
        let no_site_events = vec![
            ev(Some("Jan"), None, Some("ChatContent"), Some("offline chat no url")),
        ];
        let _ = update_wiki(&vault, "2026-07-10", &task, &no_site_events, "synth", &Default::default(), &known).unwrap();
        let app_after = std::fs::read_to_string(vault.join("TaskFlow").join("Apps").join("jan.md")).unwrap();
        assert!(!app_after.contains("## Sites used here"),
            "stale cross-link section removed when no site co-occurrs: {app_after}");
    }
}
