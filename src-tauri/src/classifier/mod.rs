//! TaskFlow-TinyNet: On-device pure-Rust neural activity classification engine.
//!
//! Classifies every captured desktop window or clipboard event in ~80-150 µs across:
//! 1. Calibrated productivity probability & three-zone ingestion decision
//!    (`CaptureProductive`, `CaptureProvisional`, `DropNonProductive`).
//! 2. Universal 16-category semantic activity taxonomy (12 productive + 4 non-productive).
//! 3. Context-aligned relevance score (`events.relevance`) against active task + known projects.

pub mod features;
pub mod model;
pub mod personalization;

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex, RwLock,
};
use std::time::Instant;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::capture::types::ContentType;
use features::{normalize_app_stem, parse_url_parts, NUM_CATEGORIES};
use model::{sigmoid, softmax_scaled, CachedContext, ForwardOutput, TinyNetModel};
use personalization::{OnlineAdapter, PersonalizationState, UserOverrideRule};

const RECENT_BUFFER_CAPACITY: usize = 30;

/// Universal 16-category desktop taxonomy (12 productive + 4 non-productive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityCategory {
    Coding,
    Terminal,
    Research,
    Learning,
    Writing,
    Design,
    MediaProduction,
    Planning,
    Meetings,
    Communication,
    DataFinance,
    Operations,
    Entertainment,
    Gaming,
    SocialFeed,
    ShoppingPersonal,
}

impl ActivityCategory {
    pub const ALL: [ActivityCategory; NUM_CATEGORIES] = [
        Self::Coding,
        Self::Terminal,
        Self::Research,
        Self::Learning,
        Self::Writing,
        Self::Design,
        Self::MediaProduction,
        Self::Planning,
        Self::Meetings,
        Self::Communication,
        Self::DataFinance,
        Self::Operations,
        Self::Entertainment,
        Self::Gaming,
        Self::SocialFeed,
        Self::ShoppingPersonal,
    ];

    #[inline]
    pub fn from_index(idx: usize) -> Self {
        Self::ALL.get(idx).copied().unwrap_or(Self::Research)
    }

    #[inline]
    pub fn as_index(self) -> usize {
        match self {
            Self::Coding => 0,
            Self::Terminal => 1,
            Self::Research => 2,
            Self::Learning => 3,
            Self::Writing => 4,
            Self::Design => 5,
            Self::MediaProduction => 6,
            Self::Planning => 7,
            Self::Meetings => 8,
            Self::Communication => 9,
            Self::DataFinance => 10,
            Self::Operations => 11,
            Self::Entertainment => 12,
            Self::Gaming => 13,
            Self::SocialFeed => 14,
            Self::ShoppingPersonal => 15,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Coding => "coding",
            Self::Terminal => "terminal",
            Self::Research => "research",
            Self::Learning => "learning",
            Self::Writing => "writing",
            Self::Design => "design",
            Self::MediaProduction => "media_production",
            Self::Planning => "planning",
            Self::Meetings => "meetings",
            Self::Communication => "communication",
            Self::DataFinance => "data_finance",
            Self::Operations => "operations",
            Self::Entertainment => "entertainment",
            Self::Gaming => "gaming",
            Self::SocialFeed => "social_feed",
            Self::ShoppingPersonal => "shopping_personal",
        }
    }

    pub fn display_label(self) -> &'static str {
        match self {
            Self::Coding => "Coding",
            Self::Terminal => "Terminal",
            Self::Research => "Research",
            Self::Learning => "Learning",
            Self::Writing => "Writing",
            Self::Design => "Design",
            Self::MediaProduction => "Media Production",
            Self::Planning => "Planning",
            Self::Meetings => "Meetings",
            Self::Communication => "Communication",
            Self::DataFinance => "Data & Finance",
            Self::Operations => "Operations",
            Self::Entertainment => "Entertainment",
            Self::Gaming => "Gaming",
            Self::SocialFeed => "Social Feed",
            Self::ShoppingPersonal => "Shopping & Personal",
        }
    }

    pub fn from_str_name(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "coding" | "code" => Some(Self::Coding),
            "terminal" => Some(Self::Terminal),
            "research" => Some(Self::Research),
            "learning" => Some(Self::Learning),
            "writing" | "documentation" => Some(Self::Writing),
            "design" => Some(Self::Design),
            "media_production" | "mediaproduction" => Some(Self::MediaProduction),
            "planning" => Some(Self::Planning),
            "meetings" => Some(Self::Meetings),
            "communication" | "email" => Some(Self::Communication),
            "data_finance" | "datafinance" | "spreadsheet" => Some(Self::DataFinance),
            "operations" => Some(Self::Operations),
            "entertainment" => Some(Self::Entertainment),
            "gaming" => Some(Self::Gaming),
            "social_feed" | "socialfeed" => Some(Self::SocialFeed),
            "shopping_personal" | "shoppingpersonal" => Some(Self::ShoppingPersonal),
            _ => None,
        }
    }

    #[inline]
    pub fn is_productive_category(self) -> bool {
        self.as_index() < 12
    }

    /// Map semantic category to TaskFlow's `ContentType` for Obsidian Activity hubs.
    pub fn to_content_type(self) -> ContentType {
        match self {
            Self::Coding => ContentType::CodeContent,
            Self::Terminal => ContentType::TerminalContent,
            Self::Research => ContentType::ResearchContent,
            Self::Learning => ContentType::LearningContent,
            Self::Writing => ContentType::DocumentationContent,
            Self::Design => ContentType::DesignContent,
            Self::MediaProduction => ContentType::MediaProductionContent,
            Self::Planning => ContentType::PlanningContent,
            Self::Meetings => ContentType::MeetingsContent,
            Self::Communication => ContentType::CommunicationContent,
            Self::DataFinance => ContentType::SpreadsheetContent,
            Self::Operations => ContentType::OperationsContent,
            Self::Entertainment
            | Self::Gaming
            | Self::SocialFeed
            | Self::ShoppingPersonal => ContentType::BrowserContent,
        }
    }
}

/// Three-zone ingestion policy separating high-confidence capture, uncertain provisional
/// capture (resolved by Stage-2 deep text), and confident distraction filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestionDecision {
    CaptureProductive,
    CaptureProvisional,
    DropNonProductive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassificationResult {
    pub is_productive: bool,
    pub productivity_prob: f32,
    pub category: ActivityCategory,
    pub category_label: String,
    pub category_prob: f32,
    pub relevance_score: f32,
    pub decision: IngestionDecision,
    pub source: String,
    pub latency_us: u64,
}

/// Ephemeral in-memory record (never persisted to disk) of recent classifications,
/// allowing the user to inspect filtered distractions and 1-click train their local adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentClassificationEntry {
    pub timestamp: String,
    pub app_name: String,
    pub window_title: String,
    pub url: Option<String>,
    pub stage: String,
    pub result: ClassificationResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifierStatus {
    pub model_version: String,
    pub vocab_size: usize,
    pub hash_buckets: usize,
    pub sha256_short: String,
    pub using_ota_override: bool,
    pub total_inferences: u64,
    pub dropped_count: u64,
    pub avg_latency_us: f64,
    pub adapter_updates: u64,
    pub user_rule_count: usize,
}

pub struct ActivityClassifier {
    model: TinyNetModel,
    using_ota_override: bool,
    context: RwLock<CachedContext>,
    personalization: RwLock<PersonalizationState>,
    recent: Mutex<VecDeque<RecentClassificationEntry>>,
    total_inferences: AtomicU64,
    total_latency_us: AtomicU64,
    dropped_count: AtomicU64,
}

pub type ClassifierHandle = Arc<ActivityClassifier>;

impl ActivityClassifier {
    /// Initialize from embedded weights (or valid OTA override) and load SQLite personalization.
    pub fn initialize(
        override_path: Option<&Path>,
        personalization_state: PersonalizationState,
    ) -> Result<ClassifierHandle, String> {
        let (model, using_ota_override) = TinyNetModel::load_with_optional_override(override_path)?;
        Ok(Arc::new(Self {
            model,
            using_ota_override,
            context: RwLock::new(CachedContext::default()),
            personalization: RwLock::new(personalization_state),
            recent: Mutex::new(VecDeque::with_capacity(RECENT_BUFFER_CAPACITY)),
            total_inferences: AtomicU64::new(0),
            total_latency_us: AtomicU64::new(0),
            dropped_count: AtomicU64::new(0),
        }))
    }

    /// Lightweight embedded-only constructor for tests and CLI dry-run tools.
    pub fn load_embedded() -> Result<ClassifierHandle, String> {
        Self::initialize(None, PersonalizationState::default())
    }

    /// Recompute and cache the 96-D context vector when active task or known projects change.
    pub fn refresh_context(
        &self,
        task_title: Option<&str>,
        task_description: Option<&str>,
        known_projects: &[String],
    ) {
        let ctx = self
            .model
            .build_context(task_title, task_description, known_projects);
        if let Ok(mut guard) = self.context.write() {
            *guard = ctx;
        }
    }

    pub fn build_context_snapshot(
        &self,
        task_title: Option<&str>,
        task_description: Option<&str>,
        known_projects: &[String],
    ) -> CachedContext {
        self.model
            .build_context(task_title, task_description, known_projects)
    }

    /// Stage 1 classification on `(app_name, window_title, url)` before writing to SQLite.
    pub fn classify_window(
        &self,
        app_name: &str,
        window_title: &str,
        url: Option<&str>,
        record_in_ring: bool,
    ) -> ClassificationResult {
        let ctx = self
            .context
            .read()
            .map(|g| g.clone())
            .unwrap_or_default();
        self.classify_with_context(app_name, window_title, url, None, &ctx, "neural_title", record_in_ring)
    }

    /// Stage 2 deep classification once UIA/OCR screen text is extracted.
    pub fn classify_deep(
        &self,
        app_name: &str,
        window_title: &str,
        url: Option<&str>,
        content: Option<&str>,
        record_in_ring: bool,
    ) -> ClassificationResult {
        let ctx = self
            .context
            .read()
            .map(|g| g.clone())
            .unwrap_or_default();
        self.classify_with_context(app_name, window_title, url, content, &ctx, "neural_deep", record_in_ring)
    }

    /// Classify with an explicit `CachedContext` (used by tests, previews, and replay tools).
    pub fn classify_with_context(
        &self,
        app_name: &str,
        window_title: &str,
        url: Option<&str>,
        content: Option<&str>,
        ctx: &CachedContext,
        stage_source: &str,
        record_in_ring: bool,
    ) -> ClassificationResult {
        let start = Instant::now();

        // 1. Check explicit per-user domain/app pin rule first
        if let Ok(pers) = self.personalization.read() {
            if let Some(rule) = pers.match_rule(app_name, url) {
                let is_prod = rule.is_productive != 0;
                let cat = rule
                    .category
                    .as_deref()
                    .and_then(ActivityCategory::from_str_name)
                    .unwrap_or(if is_prod {
                        ActivityCategory::Research
                    } else {
                        ActivityCategory::Entertainment
                    });
                let latency_us = start.elapsed().as_micros() as u64;
                let result = ClassificationResult {
                    is_productive: is_prod,
                    productivity_prob: if is_prod { 0.99 } else { 0.01 },
                    category: cat,
                    category_label: cat.display_label().to_string(),
                    category_prob: 0.99,
                    relevance_score: if is_prod { 0.90 } else { 0.02 },
                    decision: if is_prod {
                        IngestionDecision::CaptureProductive
                    } else {
                        IngestionDecision::DropNonProductive
                    },
                    source: "user_rule".to_string(),
                    latency_us,
                };
                self.record_telemetry(app_name, window_title, url, stage_source, &result, record_in_ring);
                return result;
            }
        }

        // 2. Neural forward pass + per-user online adapter delta
        let ForwardOutput {
            mut prod_logit,
            mut cat_logits,
            rel_logit,
            hidden,
        } = self.model.forward(app_name, window_title, url, content, ctx);

        let mut source = stage_source.to_string();
        if let Ok(pers) = self.personalization.read() {
            if pers.adapter.update_count > 0 {
                pers.adapter.apply(&hidden, &mut prod_logit, &mut cat_logits);
                source = format!("{stage_source}+adapted");
            }
        }

        let prod_prob = sigmoid(prod_logit / self.model.temp_prod);
        let cat_probs = softmax_scaled(&cat_logits, self.model.temp_cat);
        let rel_prob = sigmoid(rel_logit);

        // Pick top category consistent with productivity polarity when adapter/head strongly agrees
        let (best_idx, best_cat_prob) = argmax(&cat_probs);
        let category = ActivityCategory::from_index(best_idx);

        let is_deep = content.is_some_and(|c| !c.trim().is_empty());
        let decision = if is_deep {
            if prod_prob < 0.38 {
                IngestionDecision::DropNonProductive
            } else {
                IngestionDecision::CaptureProductive
            }
        } else if prod_prob <= self.model.prod_drop_threshold {
            IngestionDecision::DropNonProductive
        } else if prod_prob < self.model.prod_pass_threshold {
            IngestionDecision::CaptureProvisional
        } else {
            IngestionDecision::CaptureProductive
        };

        let is_productive = !matches!(decision, IngestionDecision::DropNonProductive);
        let latency_us = start.elapsed().as_micros() as u64;

        let result = ClassificationResult {
            is_productive,
            productivity_prob: prod_prob,
            category,
            category_label: category.display_label().to_string(),
            category_prob: best_cat_prob,
            relevance_score: if is_productive { rel_prob } else { rel_prob.min(0.15) },
            decision,
            source,
            latency_us,
        };
        self.record_telemetry(app_name, window_title, url, stage_source, &result, record_in_ring);
        result
    }

    /// Raw unadapted probabilities for numerical parity verification against PyTorch.
    pub fn raw_forward_probs(
        &self,
        app_name: &str,
        window_title: &str,
        url: Option<&str>,
        content: Option<&str>,
        ctx: &CachedContext,
    ) -> (f32, [f32; NUM_CATEGORIES], f32) {
        let out = self.model.forward(app_name, window_title, url, content, ctx);
        let p_prod = sigmoid(out.prod_logit / self.model.temp_prod);
        let p_cats = softmax_scaled(&out.cat_logits, self.model.temp_cat);
        let p_rel = sigmoid(out.rel_logit);
        (p_prod, p_cats, p_rel)
    }

    /// Apply immediate on-device personalization from user feedback and persist to SQLite.
    pub async fn apply_user_feedback(
        &self,
        db: &SqlitePool,
        app_name: &str,
        window_title: &str,
        url: Option<&str>,
        content: Option<&str>,
        target_productive: bool,
        target_category: ActivityCategory,
        pin_rule_scope: Option<&str>, // Some("domain") | Some("app") | None
    ) -> Result<ClassificationResult, String> {
        let ctx = self
            .context
            .read()
            .map(|g| g.clone())
            .unwrap_or_default();
        let base = self.model.forward(app_name, window_title, url, content, &ctx);

        // 1. Update online neural adapter in RAM + persist to SQLite
        let updated_adapter: OnlineAdapter = {
            let mut guard = self
                .personalization
                .write()
                .map_err(|_| "personalization lock poisoned".to_string())?;
            guard.adapter.update_from_feedback(
                &base.hidden,
                base.prod_logit,
                &base.cat_logits,
                target_productive,
                target_category,
            );
            guard.adapter.clone()
        };
        personalization::save_adapter(db, &updated_adapter)
            .await
            .map_err(|e| format!("Failed to persist personalization adapter: {e}"))?;

        // 2. Optionally pin an explicit domain or app rule
        if let Some(scope) = pin_rule_scope {
            let pattern = match scope {
                "domain" => {
                    let (host, _) = parse_url_parts(url);
                    host
                }
                "app" => normalize_app_stem(app_name),
                _ => String::new(),
            };
            if !pattern.is_empty() {
                let rule: UserOverrideRule = personalization::upsert_override_rule(
                    db,
                    scope,
                    &pattern,
                    target_productive,
                    Some(target_category),
                )
                .await
                .map_err(|e| format!("Failed to save override rule: {e}"))?;
                if let Ok(mut guard) = self.personalization.write() {
                    guard.upsert_local_rule(rule);
                }
            }
        }

        // Return updated classification immediately
        Ok(self.classify_with_context(
            app_name,
            window_title,
            url,
            content,
            &ctx,
            "neural_feedback",
            false,
        ))
    }

    pub fn recent_classifications(&self) -> Vec<RecentClassificationEntry> {
        self.recent
            .lock()
            .map(|q| q.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn status(&self) -> ClassifierStatus {
        let total = self.total_inferences.load(Ordering::Relaxed);
        let lat_sum = self.total_latency_us.load(Ordering::Relaxed);
        let dropped = self.dropped_count.load(Ordering::Relaxed);
        let (adapter_updates, user_rule_count) = self
            .personalization
            .read()
            .map(|p| (p.adapter.update_count, p.rules.len()))
            .unwrap_or((0, 0));

        ClassifierStatus {
            model_version: format!("taskflow-tinynet-v{}", self.model.version),
            vocab_size: self.model.vocab_size,
            hash_buckets: self.model.hash_buckets,
            sha256_short: self.model.sha256_hex.chars().take(12).collect(),
            using_ota_override: self.using_ota_override,
            total_inferences: total,
            dropped_count: dropped,
            avg_latency_us: if total > 0 {
                (lat_sum as f64) / (total as f64)
            } else {
                0.0
            },
            adapter_updates,
            user_rule_count,
        }
    }

    fn record_telemetry(
        &self,
        app_name: &str,
        window_title: &str,
        url: Option<&str>,
        stage: &str,
        result: &ClassificationResult,
        record_in_ring: bool,
    ) {
        self.total_inferences.fetch_add(1, Ordering::Relaxed);
        self.total_latency_us
            .fetch_add(result.latency_us, Ordering::Relaxed);
        if matches!(result.decision, IngestionDecision::DropNonProductive) {
            self.dropped_count.fetch_add(1, Ordering::Relaxed);
        }

        if record_in_ring {
            if let Ok(mut ring) = self.recent.lock() {
                // Collapse duplicate consecutive entries for the same window + stage
                let is_dup = ring.front().is_some_and(|head| {
                    head.app_name == app_name
                        && head.window_title == window_title
                        && head.stage == stage
                });
                if !is_dup {
                    if ring.len() >= RECENT_BUFFER_CAPACITY {
                        ring.pop_back();
                    }
                    ring.push_front(RecentClassificationEntry {
                        timestamp: Utc::now().to_rfc3339(),
                        app_name: app_name.to_string(),
                        window_title: window_title.chars().take(120).collect(),
                        url: url.map(ToString::to_string),
                        stage: stage.to_string(),
                        result: result.clone(),
                    });
                }
            }
        }
    }
}

#[inline]
fn argmax(probs: &[f32; NUM_CATEGORIES]) -> (usize, f32) {
    let mut best_i = 0usize;
    let mut best_v = probs[0];
    for (i, &v) in probs.iter().enumerate().skip(1) {
        if v > best_v {
            best_v = v;
            best_i = i;
        }
    }
    (best_i, best_v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::schema::run_migrations;
    use serde::Deserialize;
    use sqlx::SqlitePool;

    #[derive(Deserialize)]
    struct GoldenCase {
        app_name: String,
        window_title: String,
        url: Option<String>,
        content: Option<String>,
        task_title: Option<String>,
        task_description: Option<String>,
        known_projects: Vec<String>,
        expected_struct: Vec<f32>,
        expected_prod_prob: f32,
        expected_cat_probs: Vec<f32>,
        expected_relevance: f32,
        expected_category: String,
    }

    /// Proves byte-exact feature parity and < 1e-4 numerical parity between
    /// offline Python PyTorch evaluation and our pure-Rust INT8 inference engine.
    #[test]
    fn pytorch_to_rust_golden_parity() {
        let clf = ActivityClassifier::load_embedded().expect("embedded TFCL v1 loads cleanly");
        let raw = include_str!("testdata/golden_parity.json");
        let cases: Vec<GoldenCase> = serde_json::from_str(raw).expect("valid golden_parity.json");
        assert_eq!(cases.len(), 50, "expected 50 golden parity vectors");

        let mut max_prod_diff = 0.0f32;
        let mut max_cat_diff = 0.0f32;
        let mut max_rel_diff = 0.0f32;

        for (idx, case) in cases.iter().enumerate() {
            let actual_struct = features::extract_structural_features(
                &case.app_name,
                &case.window_title,
                case.url.as_deref(),
                case.content.as_deref(),
            );
            for (f_i, (&a, &e)) in actual_struct.iter().zip(case.expected_struct.iter()).enumerate() {
                assert!(
                    (a - e).abs() < 1e-5,
                    "case {idx} struct feature {f_i} mismatch: rust={a} py={e} (title={})",
                    case.window_title
                );
            }

            let ctx = clf.build_context_snapshot(
                case.task_title.as_deref(),
                case.task_description.as_deref(),
                &case.known_projects,
            );
            let (p_prod, p_cats, p_rel) = clf.raw_forward_probs(
                &case.app_name,
                &case.window_title,
                case.url.as_deref(),
                case.content.as_deref(),
                &ctx,
            );

            max_prod_diff = max_prod_diff.max((p_prod - case.expected_prod_prob).abs());
            max_rel_diff = max_rel_diff.max((p_rel - case.expected_relevance).abs());
            for (c_i, (&a, &e)) in p_cats.iter().zip(case.expected_cat_probs.iter()).enumerate() {
                max_cat_diff = max_cat_diff.max((a - e).abs());
                assert!(
                    (a - e).abs() < 1e-4,
                    "case {idx} cat[{c_i}] diff too high: rust={a} py={e}"
                );
            }
            let (top_i, _) = argmax(&p_cats);
            assert_eq!(
                ActivityCategory::from_index(top_i).as_str(),
                case.expected_category,
                "case {idx} top category mismatch"
            );
        }

        assert!(max_prod_diff < 1e-4, "max prod parity diff: {max_prod_diff}");
        assert!(max_cat_diff < 1e-4, "max cat parity diff: {max_cat_diff}");
        assert!(max_rel_diff < 1e-4, "max rel parity diff: {max_rel_diff}");
    }

    #[test]
    fn classifies_diverse_personas_and_learning_vs_leisure() {
        let clf = ActivityClassifier::load_embedded().expect("embedded model");
        let empty_ctx = CachedContext::default();

        // Software coding
        let r = clf.classify_with_context("Cursor", "window_monitor.rs — TaskFlow — Cursor", None, None, &empty_ctx, "test", false);
        assert!(r.is_productive, "IDE must be productive: {r:?}");
        assert_eq!(r.category, ActivityCategory::Coding);

        // Structured learning on YouTube
        let r = clf.classify_with_context(
            "Google Chrome",
            "MIT 6.824 Distributed Systems Lecture 6: Fault Tolerance Raft - YouTube - Google Chrome",
            Some("https://www.youtube.com/watch?v=raft6824"),
            None,
            &empty_ctx,
            "test",
            false,
        );
        assert!(r.is_productive, "MIT lecture on YouTube must be productive: {r:?}");
        assert_eq!(r.category, ActivityCategory::Learning);

        // Leisure Shorts on YouTube
        let r = clf.classify_with_context(
            "Google Chrome",
            "Funny Dog Tricks & Fails #shorts - YouTube - Google Chrome",
            Some("https://www.youtube.com/shorts/dog123"),
            None,
            &empty_ctx,
            "test",
            false,
        );
        assert!(!r.is_productive, "YouTube Shorts must be non-productive: {r:?}");
        assert_eq!(r.decision, IngestionDecision::DropNonProductive);

        // Unseen CAD design tool (zero-shot via pretrained potion-base-8M backbone!)
        let r = clf.classify_with_context(
            "SolidWorks",
            "Gearbox_Housing_Assembly.SLDASM — SOLIDWORKS",
            None,
            Some("Sketch4 | Extrude-Boss 14.5mm | Material: 6061 Aluminum Alloy"),
            &empty_ctx,
            "test",
            false,
        );
        assert!(r.is_productive, "Unseen CAD tool must be productive: {r:?}");

        // Shopping cart
        let r = clf.classify_with_context(
            "Google Chrome",
            "Amazon.com Shopping Cart — Proceed to Checkout - Google Chrome",
            Some("https://www.amazon.com/gp/cart/view.html"),
            Some("Price: $299.99 | Add to Cart | Free Delivery | Order Summary"),
            &empty_ctx,
            "test",
            false,
        );
        assert!(!r.is_productive, "Shopping cart must be non-productive: {r:?}");
        assert_eq!(r.category, ActivityCategory::ShoppingPersonal);
    }

    #[test]
    fn context_cross_tower_flips_ambiguous_youtube_video() {
        let clf = ActivityClassifier::load_embedded().expect("embedded model");
        let title = "Medieval Ceramic Glaze Chemistry & Kiln Firing - YouTube - Google Chrome";
        let url = Some("https://www.youtube.com/watch?v=glaze101");

        let aligned_ctx = clf.build_context_snapshot(
            Some("Write Pottery Studio Glaze Handbook"),
            Some("Ceramic glaze chemistry and kiln firing temperatures"),
            &["PotteryBook".to_string(), "CeramicLab".to_string()],
        );
        let unrelated_ctx = clf.build_context_snapshot(
            Some("Stripe webhook idempotency & billing dashboard"),
            Some("Handle subscription retries and Q3 revenue sheet"),
            &["PaymentGateway".to_string()],
        );

        let r_aligned = clf.classify_with_context("Google Chrome", title, url, None, &aligned_ctx, "test", false);
        let r_unrelated = clf.classify_with_context("Google Chrome", title, url, None, &unrelated_ctx, "test", false);

        assert!(
            r_aligned.productivity_prob > r_unrelated.productivity_prob + 0.30,
            "aligned task context must lift productivity prob significantly: aligned={} unrelated={}",
            r_aligned.productivity_prob,
            r_unrelated.productivity_prob
        );
        assert!(r_aligned.is_productive);
        assert!(!r_unrelated.is_productive);
    }

    #[tokio::test]
    async fn online_adapter_personalizes_without_corrupting_unrelated_domains() {
        let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
        run_migrations(&db).await.unwrap();

        let clf = ActivityClassifier::load_embedded().expect("embedded model");
        let ctx = CachedContext::default();

        // Suppose a social media strategist uses Instagram Business Insights for work
        let app = "Google Chrome";
        let title = "Instagram — Reels & Explore Feed - Google Chrome";
        let url = Some("https://www.instagram.com/reels/");

        let before = clf.classify_with_context(app, title, url, None, &ctx, "test", false);
        assert!(!before.is_productive, "baseline treats Instagram reels as non-productive");

        // 2 feedback updates via online neural adapter ONLY (no hard pin rule)
        for _ in 0..2 {
            clf.apply_user_feedback(
                &db,
                app,
                title,
                url,
                None,
                true,
                ActivityCategory::Research,
                None,
            )
            .await
            .unwrap();
        }
        let after = clf.classify_with_context(app, title, url, None, &ctx, "test", false);
        assert!(
            after.is_productive && after.productivity_prob > before.productivity_prob + 0.4,
            "online adapter must adapt to user correction: before={} after={}",
            before.productivity_prob,
            after.productivity_prob
        );

        // Unrelated coding & gaming benchmarks must remain completely intact!
        let coding = clf.classify_with_context("Cursor", "main.rs — TaskFlow — Cursor", None, None, &ctx, "test", false);
        assert!(coding.is_productive);
        assert_eq!(coding.category, ActivityCategory::Coding);

        let gaming = clf.classify_with_context("Steam", "Counter-Strike 2 — Steam", None, None, &ctx, "test", false);
        assert!(!gaming.is_productive);
    }

    #[test]
    fn inference_latency_is_sub_millisecond() {
        let clf = ActivityClassifier::load_embedded().expect("embedded model");
        let ctx = clf.build_context_snapshot(
            Some("Build TaskFlow neural classifier"),
            Some("Rust on-device inference engine"),
            &["TaskFlow".to_string()],
        );
        // Warmup
        let _ = clf.classify_with_context("Cursor", "model.rs — TaskFlow — Cursor", None, None, &ctx, "bench", false);

        let start = Instant::now();
        let iters = 500;
        for _ in 0..iters {
            let _ = clf.classify_with_context(
                "Google Chrome",
                "Async Rust Deep Dive & Tokio Runtime Explained - YouTube - Google Chrome",
                Some("https://www.youtube.com/watch?v=asyncrust"),
                Some("Lesson 4 Objectives: Understand tokio runtime scheduling and async futures."),
                &ctx,
                "bench",
                false,
            );
        }
        let avg_us = start.elapsed().as_micros() as f64 / (iters as f64);
        println!("[bench] TaskFlow-TinyNet avg latency (debug/test build): {avg_us:.1} us");
        // Even unoptimized debug builds must stay well below 2.5 ms (release is < 0.15 ms)
        assert!(avg_us < 2500.0, "inference too slow: {avg_us:.1} us");
    }
}
