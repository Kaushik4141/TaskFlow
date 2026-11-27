//! Per-user on-device personalization for TaskFlow-TinyNet.
//!
//! Combines:
//! 1. Explicit per-user app/domain pin rules stored in SQLite `classifier_overrides`.
//! 2. A tiny (~8.8 KB) L2-bounded online linear adapter `(dW_prod, db_prod, dW_cat, db_cat)`
//!    operating over the frozen 128-D penultimate hidden vector `h`.
//!    Adapts in < 50 microseconds on user corrections without ever uploading screen data.

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

use super::features::{normalize_app_stem, parse_url_parts, HIDDEN_DIM, NUM_CATEGORIES};
use super::ActivityCategory;

pub const ADAPTER_SETTING_KEY: &str = "classifier_adapter_v1";

const MAX_PROD_WEIGHT_NORM: f32 = 5.0;
const MAX_CAT_ROW_NORM: f32 = 4.0;
const MAX_BIAS_ABS: f32 = 3.0;
const PROD_TARGET_MARGIN: f32 = 3.0;
const CAT_TARGET_MARGIN: f32 = 2.5;
const L2_DECAY: f32 = 0.015;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct UserOverrideRule {
    pub id: String,
    pub match_kind: String, // "domain" | "app"
    pub pattern: String,
    pub is_productive: i64,
    pub category: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct OnlineAdapter {
    pub prod_w: [f32; HIDDEN_DIM],
    pub prod_b: f32,
    pub cat_w: [[f32; HIDDEN_DIM]; NUM_CATEGORIES],
    pub cat_b: [f32; NUM_CATEGORIES],
    pub update_count: u64,
}

impl Default for OnlineAdapter {
    fn default() -> Self {
        Self {
            prod_w: [0.0; HIDDEN_DIM],
            prod_b: 0.0,
            cat_w: [[0.0; HIDDEN_DIM]; NUM_CATEGORIES],
            cat_b: [0.0; NUM_CATEGORIES],
            update_count: 0,
        }
    }
}

impl OnlineAdapter {
    /// Apply the learned user deltas directly onto the base model's raw logits.
    #[inline]
    pub fn apply(
        &self,
        hidden: &[f32; HIDDEN_DIM],
        prod_logit: &mut f32,
        cat_logits: &mut [f32; NUM_CATEGORIES],
    ) {
        if self.update_count == 0 {
            return;
        }
        let mut dp = self.prod_b;
        for i in 0..HIDDEN_DIM {
            dp += self.prod_w[i] * hidden[i];
        }
        *prod_logit += dp;

        for c in 0..NUM_CATEGORIES {
            let mut dc = self.cat_b[c];
            let row = &self.cat_w[c];
            for i in 0..HIDDEN_DIM {
                dc += row[i] * hidden[i];
            }
            cat_logits[c] += dc;
        }
    }

    /// Perform a single L2-regularized Passive-Aggressive update from a user correction.
    /// Runs in < 50 microseconds on CPU and keeps weights norm-bounded to prevent drift.
    pub fn update_from_feedback(
        &mut self,
        hidden: &[f32; HIDDEN_DIM],
        base_prod_logit: f32,
        base_cat_logits: &[f32; NUM_CATEGORIES],
        target_productive: bool,
        target_category: ActivityCategory,
    ) {
        let mut h_norm_sq = 0.0f32;
        for &v in hidden.iter() {
            h_norm_sq += v * v;
        }
        let denom = h_norm_sq + 1.0;

        // 1. Productivity head Passive-Aggressive update (margin loss)
        let mut cur_prod = base_prod_logit;
        let mut cur_cats = *base_cat_logits;
        self.apply(hidden, &mut cur_prod, &mut cur_cats);

        let y = if target_productive { 1.0f32 } else { -1.0f32 };
        let loss_prod = (PROD_TARGET_MARGIN - y * cur_prod).max(0.0);
        if loss_prod > 0.0 {
            // PA-I step capped for stability
            let tau = (loss_prod / denom).min(0.65);
            for i in 0..HIDDEN_DIM {
                self.prod_w[i] = self.prod_w[i] * (1.0 - L2_DECAY) + tau * y * hidden[i];
            }
            self.prod_b = (self.prod_b + tau * y * 0.35).clamp(-MAX_BIAS_ABS, MAX_BIAS_ABS);
            clamp_l2_norm(&mut self.prod_w, MAX_PROD_WEIGHT_NORM);
        }

        // 2. Category head multi-class Crammer-Singer margin update
        let target_idx = target_category.as_index();
        let mut best_other_idx = if target_idx == 0 { 1 } else { 0 };
        for c in 0..NUM_CATEGORIES {
            if c != target_idx && cur_cats[c] > cur_cats[best_other_idx] {
                best_other_idx = c;
            }
        }
        let loss_cat = (CAT_TARGET_MARGIN - (cur_cats[target_idx] - cur_cats[best_other_idx])).max(0.0);
        if loss_cat > 0.0 {
            let tau = (loss_cat / (2.0 * denom)).min(0.50);
            for i in 0..HIDDEN_DIM {
                self.cat_w[target_idx][i] =
                    self.cat_w[target_idx][i] * (1.0 - L2_DECAY) + tau * hidden[i];
                self.cat_w[best_other_idx][i] =
                    self.cat_w[best_other_idx][i] * (1.0 - L2_DECAY) - tau * hidden[i];
            }
            self.cat_b[target_idx] =
                (self.cat_b[target_idx] + tau * 0.30).clamp(-MAX_BIAS_ABS, MAX_BIAS_ABS);
            self.cat_b[best_other_idx] =
                (self.cat_b[best_other_idx] - tau * 0.30).clamp(-MAX_BIAS_ABS, MAX_BIAS_ABS);
            clamp_l2_norm(&mut self.cat_w[target_idx], MAX_CAT_ROW_NORM);
            clamp_l2_norm(&mut self.cat_w[best_other_idx], MAX_CAT_ROW_NORM);
        }

        self.update_count = self.update_count.saturating_add(1);
    }

    /// Compact hex serialization (~17.6 KB hex / ~8.8 KB binary) for SQLite `settings`.
    pub fn to_hex(&self) -> String {
        let total_bytes = 8 + (HIDDEN_DIM + 1 + NUM_CATEGORIES * HIDDEN_DIM + NUM_CATEGORIES) * 4;
        let mut buf = Vec::with_capacity(total_bytes);
        buf.extend_from_slice(&self.update_count.to_le_bytes());
        for &v in &self.prod_w {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        buf.extend_from_slice(&self.prod_b.to_le_bytes());
        for row in &self.cat_w {
            for &v in row {
                buf.extend_from_slice(&v.to_le_bytes());
            }
        }
        for &v in &self.cat_b {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        hex::encode(buf)
    }

    pub fn from_hex(encoded: &str) -> Result<Self, String> {
        let bytes = hex::decode(encoded.trim()).map_err(|e| format!("Invalid adapter hex: {e}"))?;
        let expected = 8 + (HIDDEN_DIM + 1 + NUM_CATEGORIES * HIDDEN_DIM + NUM_CATEGORIES) * 4;
        if bytes.len() != expected {
            return Err(format!(
                "Adapter size mismatch: got {}, expected {expected}",
                bytes.len()
            ));
        }
        let mut pos = 0usize;
        let update_count = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
        pos += 8;

        let mut read_f32 = || -> f32 {
            let v = f32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap());
            pos += 4;
            v
        };

        let mut prod_w = [0.0f32; HIDDEN_DIM];
        for v in &mut prod_w {
            *v = read_f32();
        }
        let prod_b = read_f32();
        let mut cat_w = [[0.0f32; HIDDEN_DIM]; NUM_CATEGORIES];
        for row in &mut cat_w {
            for v in row.iter_mut() {
                *v = read_f32();
            }
        }
        let mut cat_b = [0.0f32; NUM_CATEGORIES];
        for v in &mut cat_b {
            *v = read_f32();
        }

        Ok(Self {
            prod_w,
            prod_b,
            cat_w,
            cat_b,
            update_count,
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct PersonalizationState {
    pub rules: Vec<UserOverrideRule>,
    pub adapter: OnlineAdapter,
}

impl PersonalizationState {
    /// Look up an explicit user rule matching the URL domain (highest priority) or app name.
    pub fn match_rule(&self, app_name: &str, url: Option<&str>) -> Option<&UserOverrideRule> {
        let (host, _) = parse_url_parts(url);
        if !host.is_empty() {
            if let Some(rule) = self.rules.iter().find(|r| {
                r.match_kind == "domain"
                    && (host == r.pattern || host.ends_with(&format!(".{}", r.pattern)))
            }) {
                return Some(rule);
            }
        }
        let app_stem = normalize_app_stem(app_name);
        if !app_stem.is_empty() {
            if let Some(rule) = self
                .rules
                .iter()
                .find(|r| r.match_kind == "app" && app_stem == r.pattern)
            {
                return Some(rule);
            }
        }
        None
    }

    pub fn upsert_local_rule(&mut self, rule: UserOverrideRule) {
        if let Some(existing) = self
            .rules
            .iter_mut()
            .find(|r| r.match_kind == rule.match_kind && r.pattern == rule.pattern)
        {
            *existing = rule;
        } else {
            self.rules.push(rule);
        }
    }
}

fn clamp_l2_norm(v: &mut [f32; HIDDEN_DIM], max_norm: f32) {
    let mut sq = 0.0f32;
    for &x in v.iter() {
        sq += x * x;
    }
    let norm = sq.sqrt();
    if norm > max_norm && norm > 1e-8 {
        let scale = max_norm / norm;
        for x in v.iter_mut() {
            *x *= scale;
        }
    }
}

/// Load persisted override rules and online adapter weights from SQLite.
pub async fn load_personalization(db: &SqlitePool) -> PersonalizationState {
    let rules = sqlx::query_as::<_, UserOverrideRule>(
        "SELECT id, match_kind, pattern, is_productive, category, updated_at FROM classifier_overrides ORDER BY updated_at DESC",
    )
    .fetch_all(db)
    .await
    .unwrap_or_default();

    let adapter = sqlx::query_scalar::<_, String>(
        "SELECT value FROM settings WHERE key = ?1",
    )
    .bind(ADAPTER_SETTING_KEY)
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
    .and_then(|hex_str| OnlineAdapter::from_hex(&hex_str).ok())
    .unwrap_or_default();

    PersonalizationState { rules, adapter }
}

pub async fn save_adapter(db: &SqlitePool, adapter: &OnlineAdapter) -> Result<(), sqlx::Error> {
    let encoded = adapter.to_hex();
    sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)")
        .bind(ADAPTER_SETTING_KEY)
        .bind(encoded)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn upsert_override_rule(
    db: &SqlitePool,
    match_kind: &str,
    pattern: &str,
    is_productive: bool,
    category: Option<ActivityCategory>,
) -> Result<UserOverrideRule, sqlx::Error> {
    let id = format!("{match_kind}:{pattern}");
    let now = chrono::Utc::now().to_rfc3339();
    let cat_str = category.map(|c| c.as_str().to_string());
    let prod_int = if is_productive { 1i64 } else { 0i64 };

    sqlx::query(
        r#"
        INSERT INTO classifier_overrides (id, match_kind, pattern, is_productive, category, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(match_kind, pattern) DO UPDATE SET
            is_productive = excluded.is_productive,
            category = excluded.category,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&id)
    .bind(match_kind)
    .bind(pattern)
    .bind(prod_int)
    .bind(&cat_str)
    .bind(&now)
    .execute(db)
    .await?;

    Ok(UserOverrideRule {
        id,
        match_kind: match_kind.to_string(),
        pattern: pattern.to_string(),
        is_productive: prod_int,
        category: cat_str,
        updated_at: now,
    })
}
