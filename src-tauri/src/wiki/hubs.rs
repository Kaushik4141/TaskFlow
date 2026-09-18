use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::Utc;

/// Which kind of hub a page is. The folder name lives under `TaskFlow/` in the vault.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HubKind {
    App,
    Site,
    Activity,
    Project,
}

impl HubKind {
    pub fn folder_name(self) -> &'static str {
        match self {
            HubKind::App => "Apps",
            HubKind::Site => "Sites",
            HubKind::Activity => "Activity",
            HubKind::Project => "Projects",
        }
    }
}

/// One-line description of what a hub page collects, keyed by kind. Written into
/// the bootstrap body so a fresh hub page is self-explanatory instead of showing
/// the same "Apps in this hub." placeholder on every page.
fn hub_description(kind: HubKind) -> &'static str {
    match kind {
        HubKind::App => "Daily captures that involved this application.",
        HubKind::Site => "Daily captures that referenced this site.",
        HubKind::Activity => "Daily captures containing this kind of activity.",
        HubKind::Project => "Daily captures related to this project.",
    }
}

/// Heading of the LLM-maintained running synthesis on a hub page. Everything
/// between this heading and the next `## ` heading is owned by the LLM and is
/// replaced wholesale whenever a fresh synthesis is supplied.
pub const SYNTHESIS_HEADING: &str = "## Synthesis";
/// Heading of the mechanical, append-only dated backlink list.
const RECENT_HEADING: &str = "## Recent Daily Notes";

/// Upsert a hub page. Two sections are maintained independently:
///
/// - `## Synthesis` — an LLM-written running description of what the user has been
///   doing in this app/project over time. When `synthesis` is `Some`, this whole
///   section is replaced with the new text. When `None` (sidecar offline, or a hub
///   the LLM didn't synthesize), any existing synthesis is preserved untouched.
/// - `## Recent Daily Notes` — a mechanical, deduped list of `[[Memory/Daily/<date>]]`
///   backlinks, each annotated with a one-line `detail`. Re-running the same day
///   replaces that day's row rather than duplicating it.
///
/// This split is deliberate: the mechanical backlinks always work even when the LLM
/// is down, and the synthesis compounds across days (the LLM reads the prior
/// synthesis back in via `load_prior_context`). Read+patch+write.
pub fn upsert_hub_page(
    vault: &Path,
    kind: HubKind,
    slug: &str,
    today_date: &str,
    detail: &str,
    synthesis: Option<&str>,
) -> io::Result<PathBuf> {
    let folder = vault.join("TaskFlow").join(kind.folder_name());
    fs::create_dir_all(&folder)?;
    let path = folder.join(format!("{slug}.md"));

    // The `[[Memory/Daily/<date>]]` link is the stable identity of a day's row;
    // we match on it for dedupe and append the human-readable detail after it.
    let link = format!("[[Memory/Daily/{today_date}]]");
    let detail = detail.trim();
    let backlink = if detail.is_empty() {
        format!("- {link}")
    } else {
        format!("- {link} — {detail}")
    };
    let heading = match kind {
        HubKind::App => format!("# Apps/{slug}"),
        HubKind::Site => format!("# Sites/{slug}"),
        HubKind::Activity => format!("# Activity/{slug}"),
        HubKind::Project => format!("# Projects/{slug}"),
    };

    let existing = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            // Bootstrap a new hub page. Project pages carry a `last_touched` date
            // so Dataview/queries can find recently-active projects; the LLM
            // maintains the running narrative in `## Status` (via the synthesis
            // block), so we deliberately do NOT freeze `status:` into frontmatter
            // here — it would drift from the LLM narrative.
            let frontmatter = match kind {
                HubKind::Project => format!(
                    "---\ntype: hub_page\nkind: {}\nslug: {slug}\nlast_touched: {today_date}\n---\n",
                    kind.folder_name(),
                ),
                _ => format!(
                    "---\ntype: hub_page\nkind: {}\nslug: {slug}\n---\n",
                    kind.folder_name(),
                ),
            };
            let synthesis_block = match synthesis.map(str::trim) {
                Some(text) if !text.is_empty() => format!("{SYNTHESIS_HEADING}\n{text}\n\n"),
                _ => String::new(),
            };
            let body = format!(
                "{frontmatter}\n{heading}\n\n{}\n\n{synthesis_block}{RECENT_HEADING}\n{backlink}\n",
                hub_description(kind),
            );
            fs::write(&path, body)?;
            return Ok(path);
        }
        Err(err) => return Err(err),
    };

    let mut lines: Vec<String> = existing.lines().map(ToString::to_string).collect();

    // Step 1 (projects only): advance `last_touched` frontmatter to today, so the
    // field reflects the most recent day this project was active. Harmless no-op
    // for other kinds; preserves any other frontmatter verbatim.
    if kind == HubKind::Project {
        touch_last_touched(&mut lines, today_date);
    }

    // Step 2: refresh the LLM synthesis section, if one was supplied.
    if let Some(text) = synthesis.map(str::trim) {
        if !text.is_empty() {
            replace_section(&mut lines, SYNTHESIS_HEADING, text, RECENT_HEADING);
        }
    }

    // Step 3: patch the "## Recent Daily Notes" section: replace today's row if it
    // already exists (refreshing the detail), else insert it at the top.
    upsert_backlink(&mut lines, &link, &backlink);

    let mut out = lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    fs::write(&path, out)?;

    Ok(path)
}

/// Anchor heading cross-link sections are inserted before. Cross-link sections
/// (e.g. `## Sites used here`) live between the LLM Synthesis and the mechanical
/// Recent Daily Notes list, so they read as "what this hub relates to" before the
/// raw day-by-day backlinks.
pub const CROSS_LINK_ANCHOR: &str = RECENT_HEADING;

/// Recompute-and-replace a cross-link section on a hub page from a set of
/// `[[Folder/slug]]` links, idempotent per ingest.
///
/// `heading` is the section heading (e.g. `"## Sites used here"`); `links` are the
/// rendered bullets to place under it (already sorted/deduped by the caller, each
/// a `- [[Apps/Notion]]`-style line). When `links` is empty the section is removed
/// entirely (a hub with no cross-links this day shouldn't keep a stale section).
/// The section is fully replaced each call, so re-ingesting a day never duplicates.
///
/// Reads, patches `replace_section`-style, and writes back. The hub page is
/// assumed to already exist (it's upserted earlier in the same `update_wiki`);
/// if it doesn't, this returns Ok without writing — the next ingest will create
/// it and the cross-link will land then.
pub fn write_cross_link_section(
    vault: &Path,
    kind: HubKind,
    slug: &str,
    heading: &str,
    links: &[String],
) -> io::Result<()> {
    let path = vault
        .join("TaskFlow")
        .join(kind.folder_name())
        .join(format!("{slug}.md"));
    let existing = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };
    let mut lines: Vec<String> = existing.lines().map(ToString::to_string).collect();

    let body = links.join("\n");
    if body.is_empty() {
        // Remove the section wholesale if present.
        remove_section(&mut lines, heading);
    } else {
        replace_section(&mut lines, heading, &body, CROSS_LINK_ANCHOR);
    }

    let mut out = lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    fs::write(&path, out)
}

/// Delete the section headed `heading` (the heading line and everything up to the
/// next `## ` heading, plus the trailing blank line) from a hub page. Idempotent:
/// a no-op when the heading is absent.
#[allow(dead_code)]
pub(crate) fn remove_section(lines: &mut Vec<String>, heading: &str) {
    let Some(idx) = lines.iter().position(|l| l.trim() == heading) else {
        return;
    };
    let end = lines
        .iter()
        .enumerate()
        .skip(idx + 1)
        .find(|(_, l)| l.trim_start().starts_with("## "))
        .map(|(i, _)| i)
        .unwrap_or(lines.len());
    // Remove the heading, its body, and a single trailing blank line if present.
    let mut remove_end = end;
    if remove_end < lines.len() && lines[remove_end].trim().is_empty() {
        remove_end += 1;
    }
    lines.splice(idx..remove_end, std::iter::empty());
}

/// Replace the body of `heading` (everything up to the next `## ` heading) with
/// `body`. If `heading` is absent, insert a fresh section just before `before`
/// (the anchor heading, e.g. Recent Daily Notes), or append it at the end when the
/// anchor is also missing.
pub(crate) fn replace_section(lines: &mut Vec<String>, heading: &str, body: &str, before: &str) {
    let start = lines.iter().position(|l| l.trim() == heading);
    match start {
        Some(idx) => {
            let end = lines
                .iter()
                .enumerate()
                .skip(idx + 1)
                .find(|(_, l)| l.trim_start().starts_with("## "))
                .map(|(i, _)| i)
                .unwrap_or(lines.len());
            let mut replacement = vec![heading.to_string()];
            replacement.extend(body.lines().map(ToString::to_string));
            replacement.push(String::new());
            lines.splice(idx..end, replacement);
        }
        None => {
            let mut block = vec![heading.to_string()];
            block.extend(body.lines().map(ToString::to_string));
            block.push(String::new());
            match lines.iter().position(|l| l.trim() == before) {
                Some(anchor) => {
                    lines.splice(anchor..anchor, block);
                }
                None => {
                    if !lines.is_empty() && !lines.last().map(|s| s.is_empty()).unwrap_or(true) {
                        lines.push(String::new());
                    }
                    lines.extend(block);
                }
            }
        }
    }
}

/// Insert or refresh today's backlink row under "## Recent Daily Notes".
fn upsert_backlink(lines: &mut Vec<String>, link: &str, backlink: &str) {
    let mut existing_row: Option<usize> = None;
    let mut in_section = false;
    let mut section_index: Option<usize> = None;

    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("## ") {
            in_section = line.trim() == RECENT_HEADING;
            if in_section {
                section_index = Some(i);
            }
            continue;
        }
        if in_section {
            if line.contains(link) {
                existing_row = Some(i);
            }
            if !line.trim().is_empty()
                && !line.trim().starts_with('-')
                && !line.trim().starts_with("[[")
            {
                // we walked past the bullet list; turn the section flag off but keep going
                in_section = false;
            }
        }
    }

    match existing_row {
        Some(idx) => {
            // Refresh today's line in place (detail may have changed).
            lines[idx] = backlink.to_string();
        }
        None => match section_index {
            Some(idx) => {
                lines.insert(idx + 1, backlink.to_string());
            }
            None => {
                // No section yet; append it at the end.
                if !lines.is_empty() && !lines.last().map(|s| s.is_empty()).unwrap_or(true) {
                    lines.push(String::new());
                }
                lines.push(RECENT_HEADING.to_string());
                lines.push(backlink.to_string());
                lines.push(String::new());
            }
        },
    }
}

/// Advance the `last_touched: <YYYY-MM-DD>` frontmatter field to `today` on a hub
/// page. The frontmatter lives between the first and second `---` lines; we only
/// rewrite the `last_touched:` row there and leave everything else (and any body
/// outside frontmatter) untouched. If the field is absent we insert it; if the
/// page lacks frontmatter entirely we add a minimal block.
fn touch_last_touched(lines: &mut Vec<String>, today: &str) {
    let new_row = format!("last_touched: {today}");
    // Locate frontmatter fences.
    let open = lines.iter().position(|l| l.trim() == "---");
    let Some(open_idx) = open else {
        // No frontmatter at all — prepend a minimal block.
        let mut block = vec!["---".to_string(), new_row, "---".to_string()];
        // ensure exactly one blank line after the fence before the body
        if !lines.is_empty() && !lines.first().map(|s| s.is_empty()).unwrap_or(true) {
            block.push(String::new());
        }
        lines.splice(0..0, block);
        return;
    };
    let close = lines
        .iter()
        .enumerate()
        .skip(open_idx + 1)
        .find(|(_, l)| l.trim() == "---")
        .map(|(i, _)| i);
    let Some(close_idx) = close else {
        return; // malformed frontmatter; leave it alone
    };

    // Find an existing last_touched row inside the fence.
    let existing = (open_idx + 1..close_idx)
        .find(|&i| lines[i].trim_start().starts_with("last_touched:"));
    if let Some(i) = existing {
        lines[i] = new_row;
    } else {
        // Insert just before the closing fence.
        lines.insert(close_idx, new_row);
    }
}
pub fn update_index(vault: &Path, today_date: &str, summary: &str) -> io::Result<PathBuf> {
    let path = vault.join("TaskFlow").join("index.md");
    let entry = format!(
        "- {today_date} - {} → [[Memory/Daily/{today_date}]]",
        summary.trim(),
    );

    // Only keep the catalog's entry (bullet) lines. The title + preamble are
    // re-emitted below, so reading them back in here would duplicate the header
    // on every ingest.
    let mut lines: Vec<String> = match fs::read_to_string(&path) {
        Ok(content) => content
            .lines()
            .filter(|line| line.trim_start().starts_with("- "))
            .map(ToString::to_string)
            .collect(),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(err) => return Err(err),
    };

    let mut replaced = false;
    for line in lines.iter_mut() {
        if line.ends_with(&format!("[[Memory/Daily/{today_date}]]")) {
            *line = entry.clone();
            replaced = true;
            break;
        }
    }
    if !replaced {
        lines.push(entry);
    }

    let mut out = String::new();
    out.push_str("# TaskFlow Memory Index\n\n");
    out.push_str("A chronological catalog of every daily memory note. Updated on every ingest.\n\n");
    for line in &lines {
        out.push_str(line);
        out.push('\n');
    }
    fs::write(&path, out)?;
    Ok(path)
}

/// Append a single Karpathy-style entry to `TaskFlow/log.md`.
/// Format: `## [YYYY-MM-DD] ingest | {title}`
/// Always appends; the log is append-only by design.
pub fn append_log(vault: &Path, today_date: &str, title: &str) -> io::Result<PathBuf> {
    let path = vault.join("TaskFlow").join("log.md");
    let now = Utc::now().to_rfc3339();
    let entry = format!("## [{today_date}] ingest | {title} | captured_at={now}\n");

    let existing = fs::read_to_string(&path).unwrap_or_default();
    let mut out = existing;
    if out.is_empty() {
        out.push_str("# TaskFlow Memory Log\n\nAppend-only timeline of wiki operations.\n\n");
    }
    out.push_str(&entry);
    fs::write(&path, out)?;
    Ok(path)
}

/// Rewrite `TaskFlow/Commands.md`, an append-only chronological index of every
/// distinct terminal command captured, tagged with the day it ran. Each line is
/// `- [YYYY-MM-DD] {command}` (plain text — commands are searchable, not link
/// nodes, mirroring the daily-note `## Commands Run` section).
///
/// Idempotent across re-ingests of the same day: a day's block is identified by
/// the `[YYYY-MM-DD]` of its lines, so re-running for a day refreshes that day's
/// command lines in place rather than duplicating them. Other days' lines are
/// preserved verbatim and kept in their existing order (chronological by
/// insertion). The day being written is appended after the last entry of any
/// later-ordered day so a re-ingest of an old day still lands in order, but the
/// common case (ingest today, the latest day) simply appends.
pub fn update_command_index(
    vault: &Path,
    today_date: &str,
    commands: &[String],
) -> io::Result<PathBuf> {
    let path = vault.join("TaskFlow").join("Commands.md");

    // Parse the existing file into ordered (date, command) pairs, dropping the
    // header preamble (we re-emit it) and any non-bullet noise.
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut header = String::new();
    if let Ok(content) = fs::read_to_string(&path) {
        let mut in_header = true;
        for raw in content.lines() {
            let line = raw.trim_end();
            if let Some(rest) = line.strip_prefix("- [") {
                in_header = false;
                if let Some(close) = rest.find(']') {
                    let date = rest[..close].to_string();
                    let cmd = rest[close + 1..].trim_start().to_string();
                    if !cmd.is_empty() {
                        entries.push((date, cmd));
                    }
                }
                continue;
            }
            if in_header {
                if !header.is_empty() {
                    header.push('\n');
                }
                header.push_str(line);
            }
        }
    }
    if header.is_empty() {
        header = "# TaskFlow Commands Index\n\n\
A chronological catalog of terminal commands captured from terminal-content events.\n\
Updated on every ingest; re-ingesting a day replaces its rows in place.\n"
            .to_string();
    }

    // Drop any existing rows for today (the block we own), then append today's.
    entries.retain(|(d, _)| d != today_date);
    for cmd in commands {
        entries.push((today_date.to_string(), cmd.clone()));
    }

    // Sort stable-by-date keeps prior insertion order within a day; across days
    // we want chronological order so the index reads top-to-bottom by time.
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = String::new();
    out.push_str(&header);
    out.push('\n');
    for (date, cmd) in &entries {
        out.push_str(&format!("- [{date}] {cmd}\n"));
    }
    fs::write(&path, out)?;
    Ok(path)
}