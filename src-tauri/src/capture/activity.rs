//! Offline synthetic-bootstrap activity classifier. See models/activity/README.md.
//! Only consumes already available text; never captures, persists, or contacts a service.

use super::privacy::PrivacyFilter;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, VecDeque};
use std::sync::{Mutex, OnceLock};

const DIMENSIONS: usize = 8192;
const CATEGORIES: [&str; 8] = [
    "coding_debugging", "research_learning", "writing_documentation", "communication",
    "planning_administration", "entertainment_media", "system_navigation", "other",
];
const VERSION: &str = "synthetic-bootstrap-v2";
const PARAMETERS: usize = DIMENSIONS * CATEGORIES.len() + CATEGORIES.len();
const MAX_FEATURES: usize = 2048;
const CACHE_LIMIT: usize = 256;
const FIELDS: [(&str, usize, usize); 4] = [
    ("app", 16, 256), ("title", 64, 512), ("url", 64, 2048), ("content", 128, 4096),
];
const WEIGHTS: &[u8] = include_bytes!("../../../models/activity/weights.f32le");
const MANIFEST: &str = include_str!("../../../models/activity/manifest.json");

/// Confidence is the winning *uncalibrated* softmax score, not a correctness probability.
/// Uncertain predictions retain the raw winner; empty/private input returns None.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ActivityPrediction {
    pub category: String,
    pub confidence: f32,
    pub uncertain: bool,
    pub model_version: String,
}

#[derive(Deserialize)]
struct Manifest {
    format_version: u32,
    feature_version: u32,
    model_version: String,
    dimensions: usize,
    categories: Vec<String>,
    parameter_count: usize,
    artifact_bytes: usize,
    layout: String,
    sha256: String,
}

struct Model {
    parameters: Vec<f32>,
    version: String,
}

impl Model {
    fn load(bytes: &[u8], manifest: &str) -> Option<Self> {
        let manifest: Manifest = serde_json::from_str(manifest).ok()?;
        if manifest.format_version != 1
            || manifest.feature_version != 1
            || manifest.model_version != VERSION
            || manifest.dimensions != DIMENSIONS
            || manifest.categories.iter().map(String::as_str).ne(CATEGORIES)
            || manifest.parameter_count != PARAMETERS
            || manifest.artifact_bytes != PARAMETERS * 4
            || manifest.layout != "class_major_weights_then_biases_f32le"
            || bytes.len() != PARAMETERS * 4
            || hex::encode(Sha256::digest(bytes)) != manifest.sha256
        {
            return None;
        }
        let parameters: Vec<f32> = bytes.chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        if parameters.iter().any(|value| !value.is_finite()) {
            return None;
        }
        Some(Self { parameters, version: manifest.model_version })
    }

    fn infer(&self, active: &[u16]) -> Option<ActivityPrediction> {
        if active.is_empty() {
            return None;
        }
        let scale = 1.0 / (active.len() as f32).sqrt();
        let mut logits = [0.0f32; 8];
        let mut supported = 0usize;
        for &index in active {
            let mut support = 0.0f32;
            for (category, logit) in logits.iter_mut().enumerate() {
                let weight = *self.parameters.get(category * DIMENSIONS + index as usize)?;
                *logit += weight * scale;
                support = support.max(weight.abs());
            }
            supported += usize::from(support >= 0.10);
        }
        for (category, logit) in logits.iter_mut().enumerate() {
            *logit += self.parameters[DIMENSIONS * 8 + category];
        }
        if logits.iter().any(|logit| !logit.is_finite()) {
            return None;
        }
        let maximum = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mut scores = logits.map(|logit| (logit - maximum).exp());
        let denominator: f32 = scores.iter().sum();
        for score in &mut scores {
            *score /= denominator;
        }
        let mut ranking = [0usize, 1, 2, 3, 4, 5, 6, 7];
        ranking.sort_by(|&a, &b| scores[b].total_cmp(&scores[a]).then(a.cmp(&b)));
        let winner = ranking[0];
        let confidence = scores[winner];
        let uncertain = supported < 4
            || supported as f32 / (active.len() as f32) < 0.35
            || confidence < 0.65
            || confidence - scores[ranking[1]] < 0.20;
        Some(ActivityPrediction {
            category: CATEGORIES[winner].to_string(),
            confidence,
            uncertain,
            model_version: self.version.clone(),
        })
    }
}

static MODEL: OnceLock<Option<Model>> = OnceLock::new();
static PRIVACY: OnceLock<PrivacyFilter> = OnceLock::new();
type PredictionCache = VecDeque<(Vec<u16>, ActivityPrediction)>;
static CACHE: OnceLock<Mutex<PredictionCache>> = OnceLock::new();

/// Infer from existing capture metadata and optional already-captured content only.
/// Caller must also honor its configured user exclusions before calling this function.
/// Built-in PrivacyFilter exclusions are enforced here even for direct callers.
pub fn predict(
    app: Option<&str>, title: Option<&str>, url: Option<&str>, content: Option<&str>,
) -> Option<ActivityPrediction> {
    let fields = prepare(app, title, url, content)?;
    let active = feature_indices(&fields);
    if active.is_empty() {
        return None;
    }
    let model = MODEL.get_or_init(|| Model::load(WEIGHTS, MANIFEST)).as_ref()?;
    let cache = CACHE.get_or_init(|| Mutex::new(VecDeque::new()));
    // Only bounded hashed feature IDs are cached. No raw or sanitized source text is retained.
    if let Ok(entries) = cache.lock() {
        if let Some((_, result)) = entries.iter().find(|(key, _)| *key == active) {
            return Some(result.clone());
        }
    }
    let result = model.infer(&active)?;
    if let Ok(mut entries) = cache.lock() {
        // Another capture may have inserted the same key during inference.
        if !entries.iter().any(|(key, _)| *key == active) {
            if entries.len() >= CACHE_LIMIT {
                entries.pop_front();
            }
            entries.push_back((active, result.clone()));
        }
    }
    Some(result)
}

fn prefix(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn bounded_metadata(text: Option<&str>, max_chars: usize) -> Option<String> {
    let text = text.unwrap_or("");
    // Reject oversized app/title instead of hiding a blocked suffix through truncation.
    if text.chars().take(max_chars + 1).count() > max_chars {
        return None;
    }
    Some(text.to_string())
}

fn host_path(text: &str) -> Option<String> {
    if text.chars().take(2049).count() > 2048 {
        return None;
    }
    let parsed = url::Url::parse(text).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    // Deliberately never read userinfo, query, fragment, port or scheme into features.
    Some(format!("{}{}", parsed.host_str()?, parsed.path()))
}

fn strip_embedded_urls(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let lower = rest.to_ascii_lowercase();
        let start = [lower.find("https://"), lower.find("http://")]
            .into_iter().flatten().min();
        let Some(start) = start else { result.push_str(rest); break };
        result.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = rest.find(|c: char| c.is_whitespace() || "<>\"'".contains(c))
            .unwrap_or(rest.len());
        let candidate = rest[..end].trim_end_matches([')', ']', ',', ';']);
        if let Some(safe) = host_path(candidate) {
            result.push_str(&safe);
        }
        result.push(' ');
        rest = &rest[end..];
    }
    result
}

fn sanitized(filter: &PrivacyFilter, text: &str, cap: usize) -> String {
    // A truncated private-key block cannot safely be handed to the complete-block regex.
    if text.contains("BEGIN ") && text.contains("PRIVATE KEY") {
        return String::new();
    }
    prefix(&filter.sanitize_content(&strip_embedded_urls(text)), cap)
}

fn prepare(
    app: Option<&str>, title: Option<&str>, url: Option<&str>, content: Option<&str>,
) -> Option<[String; 4]> {
    let filter = PRIVACY.get_or_init(PrivacyFilter::default);
    let app = bounded_metadata(app, 256)?;
    let title = bounded_metadata(title, 512)?;
    if !filter.should_capture_window(&app, &title) {
        return None;
    }
    let content = if filter.is_sensitive_window_for_deep_capture(&title) {
        String::new()
    } else {
        prefix(content.unwrap_or(""), 4096)
    };
    let url = url.and_then(host_path).unwrap_or_default();
    Some([
        sanitized(filter, &app, 256), sanitized(filter, &title, 512),
        sanitized(filter, &url, 2048), sanitized(filter, &content, 4096),
    ])
}

// FNV-1a 32-bit over UTF-8, explicitly wrapping; exactly mirrored by features.py.
fn fnv1a(text: &str) -> u32 {
    text.bytes().fold(2166136261u32, |hash, byte| {
        (hash ^ byte as u32).wrapping_mul(16777619)
    })
}

fn tokens(text: &str, limit: usize) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            if current.len() < 32 {
                current.push(c.to_ascii_lowercase());
            }
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
            if words.len() == limit {
                return words;
            }
        }
    }
    if !current.is_empty() && words.len() < limit {
        words.push(current);
    }
    words
}

fn feature_indices(fields: &[String; 4]) -> Vec<u16> {
    let mut active = BTreeSet::new();
    let mut emit = |feature: &str| {
        if active.len() < MAX_FEATURES {
            active.insert((fnv1a(feature) as usize % DIMENSIONS) as u16);
        }
    };
    for (position, &(name, limit, cap)) in FIELDS.iter().enumerate() {
        let words = tokens(&prefix(&fields[position], cap), limit);
        let mut previous: Option<&str> = None;
        for word in &words {
            emit(&format!("w:{word}"));
            emit(&format!("{name}:w:{word}"));
            if let Some(previous) = previous {
                emit(&format!("{name}:b:{previous} {word}"));
            }
            let marked = format!("^{word}$");
            for trigram in marked.as_bytes().windows(3) {
                // Tokens are strictly ASCII, so every window is valid UTF-8.
                emit(&format!("c:{}", std::str::from_utf8(trigram).expect("ASCII tokens")));
            }
            previous = Some(word);
        }
    }
    active.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_rust_feature_golden_parity() {
        #[derive(Deserialize)]
        struct Golden { fields: std::collections::HashMap<String, String>, indices: Vec<u16> }
        let goldens: Vec<Golden> = serde_json::from_str(include_str!(
            "../../../models/activity/feature-golden.json")).unwrap();
        assert_eq!(fnv1a("hello"), 0x4f9f2cab);
        for golden in goldens {
            let fields = FIELDS.map(|(name, _, _)| golden.fields[name].clone());
            assert_eq!(feature_indices(&fields), golden.indices);
        }
    }

    #[test]
    fn privacy_exclusions_and_sanitization() {
        for (app, title) in [("bitwarden", "vault"), ("obsidian", "notes"),
                             ("firefox", "Incognito"), ("taskflow", "Home")] {
            assert!(predict(Some(app), Some(title), None, None).is_none());
        }
        let fields = prepare(Some("code"), Some("debug api_key=do-not-retain"),
            Some("https://alice:do-not-retain@example.org/docs?token=do-not-retain#private"),
            Some("password=hunter2\nread https://user:pass@example.org/learn?q=hidden#hidden\n-----BEGIN PRIVATE KEY-----\ntruncated")).unwrap();
        assert!(!fields.join(" ").contains("do-not-retain"));
        assert_eq!(fields[2], "example.org/docs");
        assert!(fields[3].is_empty());
        let fields = prepare(Some("code"), Some(".env - editor"), None,
                             Some("private content must never be used")).unwrap();
        assert!(fields[3].is_empty());
        let fields = prepare(None, None, None,
            Some("password=hunter2\nread https://user:pass@example.org/docs?secret=hidden#hidden")).unwrap();
        assert!(!fields[3].contains("hunter2"));
        assert!(!fields[3].contains("user:pass"));
        assert!(!fields[3].contains("hidden"));
        assert!(fields[3].contains("example.org/docs"));
    }

    #[test]
    fn input_feature_and_cache_caps() {
        assert!(predict(Some(&"x".repeat(257)), None, None, None).is_none());
        assert!(predict(None, Some(&format!("{} Incognito", "x".repeat(513))), None, None).is_none());
        let content = "alpha ".repeat(10000);
        let fields = prepare(None, Some("file manager"), Some(&"x".repeat(2049)), Some(&content)).unwrap();
        assert!(fields[2].is_empty());
        assert!(fields[3].chars().count() <= 4096);
        assert!(tokens(&fields[3], 128).len() <= 128);
        let active = feature_indices(&fields);
        assert!(active.len() <= MAX_FEATURES);
        assert!(active.iter().all(|&index| (index as usize) < DIMENSIONS));
        for n in 0..300 {
            predict(Some("settings"), Some(&format!("display preferences {n}")), None, None);
        }
        assert!(CACHE.get().unwrap().lock().unwrap().len() <= CACHE_LIMIT);
    }

    #[test]
    fn invalid_model_fails_closed() {
        assert!(Model::load(&WEIGHTS[..100], MANIFEST).is_none());
        assert!(Model::load(WEIGHTS, "{}").is_none());
        assert!(Model::load(WEIGHTS, &MANIFEST.replace(VERSION, "unknown-v9")).is_none());
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut corrupt = WEIGHTS.to_vec();
            corrupt[0..4].copy_from_slice(&bad.to_le_bytes());
            let mut manifest: serde_json::Value = serde_json::from_str(MANIFEST).unwrap();
            // Update digest to ensure finiteness is independently validated.
            manifest["sha256"] = hex::encode(Sha256::digest(&corrupt)).into();
            assert!(Model::load(&corrupt, &manifest.to_string()).is_none());
        }
        let mut corrupt = WEIGHTS.to_vec();
        corrupt[0] ^= 1;
        assert!(Model::load(&corrupt, MANIFEST).is_none());
        assert!(Model::load(WEIGHTS, MANIFEST).is_some());
    }

    #[test]
    fn known_examples_and_conservative_abstention() {
        let examples = [
            ("code", "main.rs compiler error", "debug Rust function fix failing unit test cargo check borrow checker", "coding_debugging"),
            ("firefox", "tutorial and guide", "read documentation learn concepts examples reference explanation study", "research_learning"),
            ("writer", "report draft", "write introduction edit paragraphs conclusion proofread document prose", "writing_documentation"),
            ("slack", "team channel", "chat message reply colleague discussion thread conversation meeting", "communication"),
            ("calendar", "weekly schedule", "organize agenda appointment meeting calendar deadlines time planning", "planning_administration"),
            ("steam", "game library", "play game launch multiplayer achievement match level gaming", "entertainment_media"),
            ("explorer", "Downloads folder", "files directories copy rename move search file manager browse folder", "system_navigation"),
            ("drawing", "illustration canvas", "paint sketch brush layers artwork shapes design creative", "other"),
        ];
        for (app, title, content, expected) in examples {
            let result = predict(Some(app), Some(title), None, Some(content)).unwrap();
            assert_eq!(result.category, expected, "{result:?}");
            assert!(!result.uncertain, "{result:?}");
            assert!((0.0..=1.0).contains(&result.confidence));
        }
        assert!(predict(None, None, None, None).is_none());
        assert!(predict(None, Some("中文"), None, None).is_none());
        let unknown = predict(Some("xyzq"), Some("qzxv jxqz vqxj"), None, None).unwrap();
        assert!(unknown.uncertain, "{unknown:?}");
        assert!(CATEGORIES.contains(&unknown.category.as_str()));
    }

    #[test]
    fn uncertainty_never_erases_the_winning_category() {
        let mut parameters = vec![0.0; PARAMETERS];
        // A high winner, but only one supported feature: force abstention.
        parameters[42] = 5.0;
        let model = Model { parameters, version: VERSION.to_string() };
        let result = model.infer(&[42]).unwrap();
        assert!(result.confidence > 0.65, "{result:?}");
        assert!(result.uncertain);
        assert_eq!(result.category, "coding_debugging");
        assert!(model.infer(&[]).is_none());
        // A genuine Other winner is still Other, independently of uncertainty.
        let mut parameters = vec![0.0; PARAMETERS];
        parameters[7 * DIMENSIONS + 42] = 5.0;
        let result = Model { parameters, version: VERSION.to_string() }.infer(&[42]).unwrap();
        assert_eq!(result.category, "other");
        assert!(result.uncertain);
    }

    #[test]
    fn public_synthetic_metadata_and_mixed_domain_regressions() {
        // None of these exact rows is in training; no capture/history access.
        let cases = [
            ("com.mitchellh.ghostty", "cargo test --lib", None, None, "coding_debugging"),
            ("code", "main.rs - ExampleProject - Visual Studio Code", None, None, "coding_debugging"),
            ("helium", "Inbox (12) - Gmail", None, None, "communication"),
            ("chromium", "Inbox - Outlook", Some("https://outlook.live.com/mail/0/inbox"), None, "communication"),
            ("helium", "Direct messages - Slack", None, None, "communication"),
            ("com.mitchellh.ghostty", "sudo apt upgrade", None, None, "system_navigation"),
            ("code", "README.md - ExampleProject - Visual Studio Code", None, None, "writing_documentation"),
            ("firefox", "YouTube lecture on Rust ownership", Some("https://youtube.com/watch"),
             Some("Course lesson Learn about memory allocation and references Read tutorial examples"), "research_learning"),
            ("firefox", "Rust compiler error - Gmail", Some("https://mail.google.com/mail/u/0/inbox"),
             Some("Inbox Sent Drafts Reply Forward Subject Rust build failed please review the code"), "communication"),
        ];
        for (app, title, url, content, expected) in cases {
            let result = predict(Some(app), Some(title), url, content).unwrap();
            assert_eq!(result.category, expected, "{app} {title}: {result:?}");
        }
        let result = predict(Some("code"), Some("main.rs - ExampleProject - Visual Studio Code"), None,
            Some("Menu Toolbar Navigation pane Find text Toggle sidebar Fit page Backward Forward Reload Options Help center 09:52 Tuesday Connected English US Search everywhere Open tab Recent items Bookmarks View full screen This panel shows additional information. Select an entry to continue. The current view has been updated.")).unwrap();
        assert_eq!(result.category, "coding_debugging", "{result:?}");
        assert!(result.uncertain, "{result:?}");
        for (app, title) in [("xyzq", "qzxv jxqz vqxj"), ("helium", "New Tab"), ("chromium", "Loading...")] {
            assert!(predict(Some(app), Some(title), None, None).unwrap().uncertain);
        }
        for app in ["chromium", "helium", "firefox", "code", "com.mitchellh.ghostty"] {
            assert!(predict(Some(app), None, None, None).unwrap().uncertain);
        }
    }

    #[test]
    fn urls_have_no_query_fragment_or_credentials_signal() {
        let clean = predict(Some("browser"), Some("tutorial guide"),
                            Some("https://developer.mozilla.org/docs"), None);
        let dirty = predict(Some("browser"), Some("tutorial guide"),
            Some("https://alice:secret@developer.mozilla.org/docs?query=game#chat"), None);
        assert_eq!(clean, dirty);
        assert!(host_path("javascript:alert(1)").is_none());
        assert!(host_path("not a parsed URL").is_none());
    }

    /// Opt-in, read-only diagnostic. Never prints captured text or changes annotations.
    /// Requires TASKFLOW_ACTIVITY_DIAGNOSTIC_DB pointing to the database to inspect.
    #[tokio::test]
    #[ignore]
    async fn recent_annotation_diagnostic() {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
        let path = std::env::var_os("TASKFLOW_ACTIVITY_DIAGNOSTIC_DB")
            .expect("set TASKFLOW_ACTIVITY_DIAGNOSTIC_DB explicitly for this read-only diagnostic");
        let db = SqlitePoolOptions::new().max_connections(1).connect_with(
            SqliteConnectOptions::new().filename(std::path::PathBuf::from(path))
                .read_only(true).create_if_missing(false)
        ).await.expect("open read-only database");
        sqlx::query("PRAGMA query_only = ON").execute(&db).await.unwrap();
        let rows = sqlx::query_as::<_, (
            Option<String>, Option<String>, Option<String>, Option<String>, i64,
            Option<String>, Option<bool>,
        )>("SELECT e.app_name, e.window_title, e.url, e.content, e.is_sanitized, \
                  a.category, a.uncertain \
           FROM events e LEFT JOIN activity_annotations a ON a.event_id = e.id \
           ORDER BY e.timestamp DESC LIMIT 50")
            .fetch_all(&db).await.expect("read recent inputs and annotation state");
        let mut candidates = std::collections::BTreeMap::<String, (usize, usize)>::new();
        let mut none = 0;
        let mut legacy_other_uncertain = 0;
        for (app, title, url, content, sanitized, stored_category, stored_uncertain) in &rows {
            legacy_other_uncertain += usize::from(stored_category.as_deref() == Some("other")
                && *stored_uncertain == Some(true));
            match predict(app.as_deref(), title.as_deref(), url.as_deref(),
                if *sanitized == 1 { content.as_deref() } else { None }) {
                Some(result) => {
                    let entry = candidates.entry(result.category).or_default();
                    entry.0 += 1;
                    entry.1 += usize::from(!result.uncertain);
                }
                None => none += 1,
            }
        }
        db.close().await;
        eprintln!("read-only replay: rows={}, stored Other+uncertain={}, model={VERSION}, no prediction={none}",
            rows.len(), legacy_other_uncertain);
        for (category, (count, accepted)) in candidates {
            eprintln!("candidate={category}: total={count}, accepted={accepted}, uncertain={}", count - accepted);
        }
    }

    /// Includes privacy preparation, feature extraction, cache lookup and inference.
    /// Synthetic inputs only; never accesses the user's activity or launches the app.
    #[test]
    #[ignore]
    fn end_to_end_prediction_benchmark() {
        let first = std::time::Instant::now();
        assert!(predict(Some("code"), Some("compiler error"), None, None).is_some());
        eprintln!("cold model/privacy initialization plus prediction: {:?}", first.elapsed());
        let inputs: Vec<_> = (0..1000).map(|n| (
            format!("main.rs compiler error synthetic session {n}"),
            format!("debug Rust function fix failing unit test cargo check borrow checker {n}"),
        )).collect();
        let mut timings = Vec::with_capacity(inputs.len());
        let start = std::time::Instant::now();
        for (title, content) in &inputs {
            let call = std::time::Instant::now();
            std::hint::black_box(predict(Some("code"), Some(title),
                Some("https://example.test/repository"), Some(content)).unwrap());
            timings.push(call.elapsed().as_secs_f64() * 1e6);
        }
        let elapsed = start.elapsed();
        timings.sort_by(f64::total_cmp);
        eprintln!("full prediction pipeline: 1000 varied short inputs; mean={:.2} us median={:.2} us p95={:.2} us elapsed={:?}",
            elapsed.as_secs_f64() * 1e3, timings[500], timings[950], elapsed);
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            std::hint::black_box(predict(Some("code"), Some(&inputs[999].0),
                Some("https://example.test/repository"), Some(&inputs[999].1)).unwrap());
        }
        eprintln!("cached full pipeline mean={:.2} us (privacy/features still included)",
            start.elapsed().as_secs_f64() * 1e3);
    }

    /// Inference only, no captures/OCR/DB/network. Linux /proc CPU ticks and RSS/HWM;
    /// elapsed time fallback on other platforms. Run explicitly with --ignored --nocapture.
    #[test]
    #[ignore]
    fn inference_cpu_memory_benchmark() {
        fn resources() -> Option<(u64, String)> {
            let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
            let fields: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
            let ticks = fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?;
            let status = std::fs::read_to_string("/proc/self/status").ok()?;
            let memory = status.lines().filter(|line| line.starts_with("VmRSS:") || line.starts_with("VmHWM:"))
                .collect::<Vec<_>>().join(", ");
            Some((ticks, memory))
        }
        let model = Model::load(WEIGHTS, MANIFEST).unwrap();
        let fields = prepare(Some("code"), Some("main.rs compiler error"), None,
            Some("debug Rust function fix failing unit test cargo check borrow checker")).unwrap();
        let active = feature_indices(&fields);
        let hz = std::process::Command::new("getconf").arg("CLK_TCK").output().ok()
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .and_then(|text| text.trim().parse::<f64>().ok());
        let iterations = 10000;
        let before = resources();
        let start = std::time::Instant::now();
        for _ in 0..iterations {
            std::hint::black_box(model.infer(std::hint::black_box(&active)).unwrap());
        }
        let elapsed = start.elapsed();
        let after = resources();
        eprintln!("uncached sparse inference: {iterations} iterations, {:?} elapsed, {:.2} us/call, decoded model={} bytes, max cache feature payload={} bytes",
            elapsed, elapsed.as_secs_f64() * 1e6 / iterations as f64,
            model.parameters.len() * 4, CACHE_LIMIT * MAX_FEATURES * 2);
        if let (Some((before, initial)), Some((after, final_memory)), Some(hz)) = (before, after, hz) {
            eprintln!("process CPU={:.6}s ({hz} ticks/s); memory before={initial}; after={final_memory}",
                      after.saturating_sub(before) as f64 / hz);
        }
    }
}
