use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

use super::hubs::HubKind;

/// Result of one wiki lint pass. Mechanical, no LLM call.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LintReport {
    pub vault_path: String,
    pub total_pages: usize,
    pub total_links: usize,
    pub orphan_pages: Vec<String>,
    pub dangling_links: Vec<DanglingLink>,
    pub hub_pages_missing_backlinks: Vec<HubPageBacklinkGap>,
}

/// A `[[wikilink]]` whose target page does not exist in the vault.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DanglingLink {
    pub source: String,
    pub target: String,
}

/// A hub page that exists but is missing backlinks to one or more daily notes
/// that are known to mention it (according to the daily note's link block).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubPageBacklinkGap {
    pub hub_page: String,
    pub missing_backlinks: Vec<String>,
}

/// Walk the TaskFlow-owned section of the Obsidian vault and produce a mechanical
/// health report. v1 surfaces three issues only:
///  - `orphan_pages`: pages no inbound `[[link]]` references.
///  - `dangling_links`: `[[links]]` whose targets don't exist.
///  - `hub_pages_missing_backlinks`: hub pages that should link back to one or
///    more daily notes (because that daily note's link block points at them) but
///    don't yet have the backlink entry.
///
/// LLM-driven contradiction detection and stale-claim detection are deferred.
pub fn lint_vault(vault: &Path) -> Result<LintReport, String> {
    let wiki_root = vault.join("TaskFlow");
    if !wiki_root.is_dir() {
        return Err("TaskFlow wiki directory does not exist in the configured vault.".to_string());
    }

    let link_re = Regex::new(r"\[\[([^\]|]+)(?:\|[^\]]*)?\]\]").map_err(|e| e.to_string())?;

    // Collect every .md page under wiki_root.
    let mut pages: Vec<PathBuf> = Vec::new();
    walk_md(&wiki_root, &mut pages);

    // Resolve each page into a slug (its path relative to wiki_root, without the .md)
    // e.g. Memory/Daily/2026-07-05. We use forward-slash joined slugs so the same
    // format works on Windows.
    let page_slugs: Vec<String> = pages
        .iter()
        .filter_map(|p| p.strip_prefix(&wiki_root).ok())
        .filter_map(|rel| {
            rel.with_extension("")
                .to_string_lossy()
                .replace('\\', "/")
                .into()
        })
        .collect();

    // For each page, collect outbound link target slugs.
    let mut outbound: Vec<(String, Vec<String>)> = Vec::with_capacity(pages.len());
    let mut total_links = 0usize;
    for (page_path, slug) in pages.iter().zip(page_slugs.iter()) {
        let content = fs::read_to_string(page_path).unwrap_or_default();
        let targets: Vec<String> = link_re
            .captures_iter(&content)
            .filter_map(|c| {
                let raw = c.get(1)?.as_str().trim().trim_start_matches('/');
                Some(raw.to_string())
            })
            .collect();
        total_links += targets.len();
        outbound.push((slug.clone(), targets));
    }

    // Build set of existing page slugs for fast lookup. We also normalize the
    // lookup by comparing against the slash-joined lowercase slugs so casing
    // differences don't yield false-positive dangling links.
    let exists: std::collections::HashSet<String> = page_slugs
        .iter()
        .map(|s| s.to_lowercase())
        .collect();

    // Dangling links: outbound target not in `exists`. Ignore known hub "index"
    // pseudo-slugs like `Memory/Daily` (the daily-note index) which intentionally
    // has no single behind-page.
    let pseudo_targets: &[&str] = &["Memory/Daily"];
    let mut dangling_links: Vec<DanglingLink> = Vec::new();
    for (src, targets) in &outbound {
        for t in targets {
            if pseudo_targets.contains(&t.as_str()) {
                continue;
            }
            if exists.contains(&t.to_lowercase()) {
                continue;
            }
            dangling_links.push(DanglingLink {
                source: src.clone(),
                target: t.clone(),
            });
        }
    }

    // Inbound graph: for every page slug S, the set of source slugs that link to S.
    let mut inbound: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (src, targets) in &outbound {
        for t in targets {
            inbound
                .entry(t.to_lowercase())
                .or_default()
                .push(src.clone());
        }
    }

    let orphan_pages: Vec<String> = page_slugs
        .iter()
        .filter(|s| {
            // A page is an orphan if no inbound links target it (case-insensitive match).
            // The Daily Index pseudo-target `Memory/Daily` is given a virtual inbound
            // by every daily note's Navigation section, so daily notes are never orphans.
            let key = s.to_lowercase();
            !inbound.contains_key(&key)
                && !s.starts_with("Memory/Daily/")
                && *s != "index"
                && *s != "log"
        })
        .cloned()
        .collect();

    // Hub backlink gaps: cross-reference daily notes against hub pages.
    // For every `[[Apps/<slug>]]`, `[[Sites/<slug>]]`, `[[Activity/<slug>]]`,
    // `[[Projects/<slug>]]` found in a daily note's link block, the corresponding
    // hub page (folder/<slug>.md) should have `- [[Memory/Daily/<YYYY-MM-DD>]]`
    // somewhere in its body. If it doesn't, report a gap.
    let hub_pages_missing_backlinks = find_hub_backlink_gaps(&wiki_root, &outbound, &link_re);

    Ok(LintReport {
        vault_path: wiki_root.to_string_lossy().into_owned(),
        total_pages: page_slugs.len(),
        total_links,
        orphan_pages,
        dangling_links,
        hub_pages_missing_backlinks,
    })
}

fn walk_md(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk_md(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                out.push(path);
            }
        }
    }
}

const HUB_PREFIXES: &[(&str, HubKind)] = &[
    ("Apps/", HubKind::App),
    ("Sites/", HubKind::Site),
    ("Activity/", HubKind::Activity),
    ("Projects/", HubKind::Project),
];

/// Iterate the (daily_note_slug, hub_target_slug) pairs that appear in outbound
/// links of any daily note, then verify each hub page actually backlinks to
/// that daily note.
fn find_hub_backlink_gaps(
    wiki_root: &Path,
    outbound: &[(String, Vec<String>)],
    link_re: &Regex,
) -> Vec<HubPageBacklinkGap> {
    let mut gaps: Vec<HubPageBacklinkGap> = Vec::new();

    for (src, targets) in outbound {
        // We only inspect daily notes for hub mentions — other pages (hubs,
        // index, log) don't contribute to the backlink contract.
        let date_part = match src.strip_prefix("Memory/Daily/") {
            Some(d) if !d.is_empty() => d,
            _ => continue,
        };
        let expected_backlink_target = format!("[[Memory/Daily/{date_part}]]");

        for t in targets {
            let prefix_match = HUB_PREFIXES.iter().find(|(pfx, _)| t.starts_with(pfx));
            let (pfx, kind) = match prefix_match {
                Some((pfx, kind)) => (pfx, *kind),
                None => continue,
            };
            let slug = t[pfx.len()..].to_string();
            if slug.is_empty() {
                continue;
            }
            let hub_page_path = wiki_root
                .join(kind.folder_name())
                .join(format!("{slug}.md"));
            let hub_content = fs::read_to_string(&hub_page_path).unwrap_or_default();
            if hub_content.contains(&expected_backlink_target) {
                continue;
            }
            // Record the gap unless we already have a row for this hub page.
            if let Some(existing) = gaps.iter_mut().find(|g| &g.hub_page == t) {
                if !existing.missing_backlinks.contains(&expected_backlink_target) {
                    existing.missing_backlinks.push(expected_backlink_target.clone());
                }
            } else {
                gaps.push(HubPageBacklinkGap {
                    hub_page: t.clone(),
                    missing_backlinks: vec![expected_backlink_target.clone()],
                });
            }
        }
    }

    // The link regex above isn't used directly here, but lints some Parts
    // in callers that want to blanket-scan content; suppress unused warning.
    let _ = link_re;

    gaps
}
