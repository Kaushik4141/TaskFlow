//! Pure-Rust inference engine for TaskFlow-TinyNet v1 (`TFCL` binary format).
//!
//! Zero external ML runtime crates (no ONNX Runtime DLLs, no heavy monomorphized
//! tensor graphs): loads an embedded ~3.4 MB SHA-256-verified INT8 weight file
//! (with optional OTA override from `<app_data_dir>/models/activity_classifier_override.bin`)
//! and executes a full multi-head forward pass in ~80-150 microseconds on CPU.

use std::path::Path;

use sha2::{Digest, Sha256};

use super::features::{
    compute_lexical_context_overlap, extract_context_feature_ids, extract_field_features,
    extract_structural_features, ContextFeatureIds, FieldIds, APP_DIM, CTX_FEAT_DIM,
    CTX_HADAMARD_DIM, EMBED_DIM, HIDDEN_DIM, INPUT_DIM, NUM_CATEGORIES, STRUCT_FEAT_DIM,
};

const MAGIC: &[u8; 4] = b"TFCL";
const EXPECTED_VERSION: u32 = 1;

/// Embedded default v1 model weights — zero disk I/O at boot.
pub static EMBEDDED_MODEL_BYTES: &[u8] = include_bytes!("../../models/activity_classifier_v1.bin");

/// Cached semantic representation of the user's active task + known projects.
#[derive(Debug, Clone)]
pub struct CachedContext {
    pub has_context: bool,
    pub vector: [f32; EMBED_DIM],
    pub sig_tokens: Vec<String>,
    pub known_projects_lower: Vec<String>,
}

impl Default for CachedContext {
    fn default() -> Self {
        Self {
            has_context: false,
            vector: [0.0; EMBED_DIM],
            sig_tokens: Vec::new(),
            known_projects_lower: Vec::new(),
        }
    }
}

/// Raw unadapted forward pass output, including the 128-D penultimate hidden vector
/// used by the on-device online personalization adapter.
#[derive(Debug, Clone)]
pub struct ForwardOutput {
    pub prod_logit: f32,
    pub cat_logits: [f32; NUM_CATEGORIES],
    pub rel_logit: f32,
    pub hidden: [f32; HIDDEN_DIM],
}

pub struct TinyNetModel {
    pub version: u32,
    pub vocab_size: usize,
    pub hash_buckets: usize,
    pub temp_prod: f32,
    pub temp_cat: f32,
    pub prod_drop_threshold: f32,
    pub prod_pass_threshold: f32,
    pub sha256_hex: String,

    /// Lexicographically sorted vocabulary tokens for zero-HashMap binary search.
    vocab: Vec<String>,
    /// INT8 quantized vocabulary embeddings: length = vocab_size * EMBED_DIM.
    vocab_q: Vec<i8>,
    vocab_scales: Vec<f32>,
    /// INT8 quantized hashed n-gram embeddings: length = hash_buckets * EMBED_DIM.
    hash_q: Vec<i8>,
    hash_scales: Vec<f32>,

    // FP32 Residual MLP trunk + 3 heads
    fc1_w: Vec<f32>, // [HIDDEN_DIM * INPUT_DIM]
    fc1_b: Vec<f32>, // [HIDDEN_DIM]
    ln1_w: Vec<f32>,
    ln1_b: Vec<f32>,
    res1_w: Vec<f32>, // [HIDDEN_DIM * HIDDEN_DIM]
    res1_b: Vec<f32>,
    ln2_w: Vec<f32>,
    ln2_b: Vec<f32>,
    res2_w: Vec<f32>, // [HIDDEN_DIM * HIDDEN_DIM]
    res2_b: Vec<f32>,
    ln3_w: Vec<f32>,
    ln3_b: Vec<f32>,

    head_prod_w: Vec<f32>, // [1 * HIDDEN_DIM]
    head_prod_b: f32,
    head_cat_w: Vec<f32>,  // [NUM_CATEGORIES * HIDDEN_DIM]
    head_cat_b: Vec<f32>,  // [NUM_CATEGORIES]
    head_rel_w: Vec<f32>,  // [1 * HIDDEN_DIM]
    head_rel_b: f32,
}

impl TinyNetModel {
    /// Load the embedded model binary compiled into the executable.
    pub fn load_embedded() -> Result<Self, String> {
        Self::from_bytes(EMBEDDED_MODEL_BYTES)
    }

    /// Attempt to load an OTA model override from disk; fall back cleanly to embedded weights.
    pub fn load_with_optional_override(override_path: Option<&Path>) -> Result<(Self, bool), String> {
        if let Some(path) = override_path {
            if path.exists() {
                match std::fs::read(path).and_then(|bytes| {
                    Self::from_bytes(&bytes)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                }) {
                    Ok(model) => return Ok((model, true)),
                    Err(err) => {
                        eprintln!(
                            "[taskflow:classifier] Ignoring invalid OTA model override {}: {err}",
                            path.display()
                        );
                    }
                }
            }
        }
        Ok((Self::load_embedded()?, false))
    }

    /// Parse and SHA-256-verify a `TFCL` v1 binary buffer.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 36 + 48 {
            return Err("TFCL model buffer too small".to_string());
        }
        if &bytes[0..4] != MAGIC {
            return Err("Invalid TFCL magic header".to_string());
        }
        let expected_digest = &bytes[4..36];
        let payload = &bytes[36..];
        let actual_digest = Sha256::digest(payload);
        if actual_digest.as_slice() != expected_digest {
            return Err("TFCL SHA-256 checksum mismatch (corrupted model file)".to_string());
        }
        let sha256_hex = hex::encode(actual_digest);

        let mut cur = Cursor::new(payload);
        let version = cur.read_u32()?;
        if version != EXPECTED_VERSION {
            return Err(format!("Unsupported TFCL model version {version}"));
        }
        let vocab_size = cur.read_u32()? as usize;
        let hash_buckets = cur.read_u32()? as usize;
        let embed_dim = cur.read_u32()? as usize;
        let hidden_dim = cur.read_u32()? as usize;
        let num_cats = cur.read_u32()? as usize;
        let struct_dim = cur.read_u32()? as usize;
        let ctx_dim = cur.read_u32()? as usize;

        if embed_dim != EMBED_DIM
            || hidden_dim != HIDDEN_DIM
            || num_cats != NUM_CATEGORIES
            || struct_dim != STRUCT_FEAT_DIM
            || ctx_dim != CTX_FEAT_DIM
        {
            return Err(format!(
                "TFCL dimension mismatch: embed={embed_dim} hidden={hidden_dim} cats={num_cats} struct={struct_dim} ctx={ctx_dim}"
            ));
        }

        let temp_prod = cur.read_f32()?.max(0.05);
        let temp_cat = cur.read_f32()?.max(0.05);
        let prod_drop_threshold = cur.read_f32()?;
        let prod_pass_threshold = cur.read_f32()?;

        // Read sorted vocabulary tokens
        let mut vocab = Vec::with_capacity(vocab_size);
        for _ in 0..vocab_size {
            let len = cur.read_u16()? as usize;
            let s = cur.read_utf8(len)?;
            vocab.push(s);
        }

        let vocab_q = cur.read_i8_vec(vocab_size * EMBED_DIM)?;
        let vocab_scales = cur.read_f32_vec(vocab_size)?;
        let hash_q = cur.read_i8_vec(hash_buckets * EMBED_DIM)?;
        let hash_scales = cur.read_f32_vec(hash_buckets)?;

        let fc1_w = cur.read_f32_vec(HIDDEN_DIM * INPUT_DIM)?;
        let fc1_b = cur.read_f32_vec(HIDDEN_DIM)?;
        let ln1_w = cur.read_f32_vec(HIDDEN_DIM)?;
        let ln1_b = cur.read_f32_vec(HIDDEN_DIM)?;
        let res1_w = cur.read_f32_vec(HIDDEN_DIM * HIDDEN_DIM)?;
        let res1_b = cur.read_f32_vec(HIDDEN_DIM)?;
        let ln2_w = cur.read_f32_vec(HIDDEN_DIM)?;
        let ln2_b = cur.read_f32_vec(HIDDEN_DIM)?;
        let res2_w = cur.read_f32_vec(HIDDEN_DIM * HIDDEN_DIM)?;
        let res2_b = cur.read_f32_vec(HIDDEN_DIM)?;
        let ln3_w = cur.read_f32_vec(HIDDEN_DIM)?;
        let ln3_b = cur.read_f32_vec(HIDDEN_DIM)?;

        let head_prod_w = cur.read_f32_vec(HIDDEN_DIM)?;
        let head_prod_b = cur.read_f32()?;
        let head_cat_w = cur.read_f32_vec(NUM_CATEGORIES * HIDDEN_DIM)?;
        let head_cat_b = cur.read_f32_vec(NUM_CATEGORIES)?;
        let head_rel_w = cur.read_f32_vec(HIDDEN_DIM)?;
        let head_rel_b = cur.read_f32()?;

        if cur.remaining() != 0 {
            return Err(format!("Unexpected trailing bytes in TFCL binary: {}", cur.remaining()));
        }

        Ok(Self {
            version,
            vocab_size,
            hash_buckets,
            temp_prod,
            temp_cat,
            prod_drop_threshold,
            prod_pass_threshold,
            sha256_hex,
            vocab,
            vocab_q,
            vocab_scales,
            hash_q,
            hash_scales,
            fc1_w,
            fc1_b,
            ln1_w,
            ln1_b,
            res1_w,
            res1_b,
            ln2_w,
            ln2_b,
            res2_w,
            res2_b,
            ln3_w,
            ln3_b,
            head_prod_w,
            head_prod_b,
            head_cat_w,
            head_cat_b,
            head_rel_w,
            head_rel_b,
        })
    }

    #[inline]
    pub fn lookup_vocab(&self, token: &str) -> Option<usize> {
        self.vocab
            .binary_search_by(|probe| probe.as_bytes().cmp(token.as_bytes()))
            .ok()
    }

    /// Precompute and cache the 96-D semantic context vector for active task + known projects.
    pub fn build_context(
        &self,
        task_title: Option<&str>,
        task_description: Option<&str>,
        known_projects: &[String],
    ) -> CachedContext {
        let ContextFeatureIds {
            vocab_ids,
            hash_ids,
            sig_tokens,
            known_projects_lower,
        } = extract_context_feature_ids(
            task_title,
            task_description,
            known_projects,
            self.hash_buckets,
            |tok| self.lookup_vocab(tok),
        );
        let has_context =
            !vocab_ids.is_empty() || !hash_ids.is_empty() || !known_projects_lower.is_empty();
        let vector = self.pool_blended_field(&vocab_ids, &hash_ids);
        CachedContext {
            has_context,
            vector,
            sig_tokens,
            known_projects_lower,
        }
    }

    /// Execute full forward pass for a single desktop capture event.
    pub fn forward(
        &self,
        app_name: &str,
        window_title: &str,
        url: Option<&str>,
        content: Option<&str>,
        ctx: &CachedContext,
    ) -> ForwardOutput {
        let (app_ids, tu_ids, cnt_ids) = extract_field_features(
            app_name,
            window_title,
            url,
            content,
            self.hash_buckets,
            |tok| self.lookup_vocab(tok),
        );

        let e_app_full = self.pool_field(&app_ids);
        let e_tu = self.pool_field(&tu_ids);
        let e_cnt = self.pool_field(&cnt_ids);
        let e_ctx = &ctx.vector;

        // Context Cross-Tower (48 dims)
        let cos_tu_ctx = dot(&e_tu, e_ctx);
        let cos_cnt_ctx = dot(&e_cnt, e_ctx);
        let cos_app_ctx = dot(&e_app_full[..APP_DIM], &e_ctx[..APP_DIM]);
        let max_cos = cos_tu_ctx.max(cos_cnt_ctx);

        let mut e_ev = [0.0f32; EMBED_DIM];
        for i in 0..EMBED_DIM {
            e_ev[i] = e_tu[i] + 0.5 * e_cnt[i];
        }
        l2_normalize_in_place(&mut e_ev);
        let cos_ev_ctx = dot(&e_ev, e_ctx);

        let (lex_proj, lex_tok) = compute_lexical_context_overlap(
            window_title,
            url,
            content,
            &ctx.sig_tokens,
            &ctx.known_projects_lower,
        );
        let struct_feats = extract_structural_features(app_name, window_title, url, content);

        // Assemble 328-D concatenated input vector
        let mut x = [0.0f32; INPUT_DIM];
        let mut offset = 0usize;
        x[offset..offset + APP_DIM].copy_from_slice(&e_app_full[..APP_DIM]);
        offset += APP_DIM;
        x[offset..offset + EMBED_DIM].copy_from_slice(&e_tu);
        offset += EMBED_DIM;
        x[offset..offset + EMBED_DIM].copy_from_slice(&e_cnt);
        offset += EMBED_DIM;

        // 8 scalar context features
        x[offset] = if ctx.has_context { 1.0 } else { 0.0 };
        x[offset + 1] = cos_tu_ctx;
        x[offset + 2] = cos_cnt_ctx;
        x[offset + 3] = cos_app_ctx;
        x[offset + 4] = lex_proj;
        x[offset + 5] = lex_tok;
        x[offset + 6] = max_cos;
        x[offset + 7] = cos_ev_ctx;
        offset += 8;

        // 40 Hadamard interaction features
        for i in 0..CTX_HADAMARD_DIM {
            x[offset + i] = e_ev[i] * e_ctx[i];
        }
        offset += CTX_HADAMARD_DIM;

        // 24 structural features
        x[offset..offset + STRUCT_FEAT_DIM].copy_from_slice(&struct_feats);

        // Layer 1: Linear(328 -> 128) -> LayerNorm -> GELU(tanh)
        let mut h1 = [0.0f32; HIDDEN_DIM];
        linear_forward(&x, &self.fc1_w, &self.fc1_b, &mut h1);
        layer_norm_in_place(&mut h1, &self.ln1_w, &self.ln1_b);
        gelu_tanh_in_place(&mut h1);

        // Residual Block: Linear(128 -> 128) -> LayerNorm -> GELU -> Linear(128 -> 128) + h1 -> LayerNorm
        let mut r1 = [0.0f32; HIDDEN_DIM];
        linear_forward(&h1, &self.res1_w, &self.res1_b, &mut r1);
        layer_norm_in_place(&mut r1, &self.ln2_w, &self.ln2_b);
        gelu_tanh_in_place(&mut r1);

        let mut h_out = [0.0f32; HIDDEN_DIM];
        linear_forward(&r1, &self.res2_w, &self.res2_b, &mut h_out);
        for i in 0..HIDDEN_DIM {
            h_out[i] += h1[i];
        }
        layer_norm_in_place(&mut h_out, &self.ln3_w, &self.ln3_b);

        // 3 Linear Heads
        let prod_logit = dot(&h_out, &self.head_prod_w) + self.head_prod_b;
        let mut cat_logits = [0.0f32; NUM_CATEGORIES];
        linear_forward(&h_out, &self.head_cat_w, &self.head_cat_b, &mut cat_logits);
        let rel_logit = dot(&h_out, &self.head_rel_w) + self.head_rel_b;

        ForwardOutput {
            prod_logit,
            cat_logits,
            rel_logit,
            hidden: h_out,
        }
    }

    #[inline]
    fn pool_field(&self, ids: &FieldIds) -> [f32; EMBED_DIM] {
        self.pool_blended_field(&ids.vocab_ids, &ids.hash_ids)
    }

    fn pool_blended_field(&self, vocab_ids: &[usize], hash_ids: &[usize]) -> [f32; EMBED_DIM] {
        let has_v = !vocab_ids.is_empty();
        let has_h = !hash_ids.is_empty();
        if !has_v && !has_h {
            return [0.0f32; EMBED_DIM];
        }

        let mut v_vec = [0.0f32; EMBED_DIM];
        if has_v {
            pool_int8_rows(&self.vocab_q, &self.vocab_scales, vocab_ids, &mut v_vec);
            l2_normalize_in_place(&mut v_vec);
        }

        let mut h_vec = [0.0f32; EMBED_DIM];
        if has_h {
            pool_int8_rows(&self.hash_q, &self.hash_scales, hash_ids, &mut h_vec);
            l2_normalize_in_place(&mut h_vec);
        }

        let mut out = [0.0f32; EMBED_DIM];
        if has_v && has_h {
            for i in 0..EMBED_DIM {
                out[i] = 0.65 * v_vec[i] + 0.35 * h_vec[i];
            }
            l2_normalize_in_place(&mut out);
        } else if has_v {
            out = v_vec;
        } else {
            out = h_vec;
        }
        out
    }
}

#[inline]
fn pool_int8_rows(table: &[i8], scales: &[f32], ids: &[usize], out: &mut [f32; EMBED_DIM]) {
    let inv_len = 1.0 / (ids.len() as f32);
    for &id in ids {
        let scale = scales[id];
        let row_start = id * EMBED_DIM;
        let row = &table[row_start..row_start + EMBED_DIM];
        for i in 0..EMBED_DIM {
            out[i] += (row[i] as f32) * scale;
        }
    }
    for val in out.iter_mut() {
        *val *= inv_len;
    }
}

#[inline]
fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        acc += x * y;
    }
    acc
}

#[inline]
fn l2_normalize_in_place(v: &mut [f32]) {
    let norm = dot(v, v).sqrt();
    if norm > 1e-8 {
        let inv = 1.0 / norm;
        for x in v.iter_mut() {
            *x *= inv;
        }
    } else {
        v.fill(0.0);
    }
}

#[inline]
fn linear_forward(input: &[f32], weight: &[f32], bias: &[f32], output: &mut [f32]) {
    let in_dim = input.len();
    for (o, out_val) in output.iter_mut().enumerate() {
        let row = &weight[o * in_dim..(o + 1) * in_dim];
        *out_val = dot(input, row) + bias[o];
    }
}

#[inline]
fn layer_norm_in_place(x: &mut [f32], weight: &[f32], bias: &[f32]) {
    let n = x.len() as f32;
    let mean = x.iter().copied().sum::<f32>() / n;
    let mut var = 0.0f32;
    for &v in x.iter() {
        let d = v - mean;
        var += d * d;
    }
    let inv_std = 1.0 / (var / n + 1e-5f32).sqrt();
    for i in 0..x.len() {
        x[i] = (x[i] - mean) * inv_std * weight[i] + bias[i];
    }
}

#[inline]
fn gelu_tanh_in_place(x: &mut [f32]) {
    // Standard tanh approximation matching torch.nn.functional.gelu(..., approximate="tanh")
    const SQRT_2_OVER_PI: f32 = 0.797_884_6;
    for v in x.iter_mut() {
        let val = *v;
        let inner = SQRT_2_OVER_PI * (val + 0.044_715 * val * val * val);
        *v = 0.5 * val * (1.0 + inner.tanh());
    }
}

#[inline]
pub fn sigmoid(x: f32) -> f32 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let ex = x.exp();
        ex / (1.0 + ex)
    }
}

pub fn softmax_scaled(logits: &[f32; NUM_CATEGORIES], temperature: f32) -> [f32; NUM_CATEGORIES] {
    let inv_t = 1.0 / temperature.max(0.05);
    let mut scaled = [0.0f32; NUM_CATEGORIES];
    let mut max_val = f32::NEG_INFINITY;
    for i in 0..NUM_CATEGORIES {
        let v = logits[i] * inv_t;
        scaled[i] = v;
        if v > max_val {
            max_val = v;
        }
    }
    let mut sum = 0.0f32;
    for v in scaled.iter_mut() {
        *v = (*v - max_val).exp();
        sum += *v;
    }
    let inv_sum = 1.0 / sum.max(1e-12);
    for v in scaled.iter_mut() {
        *v *= inv_sum;
    }
    scaled
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.pos + n > self.bytes.len() {
            return Err("Unexpected EOF while parsing TFCL binary".to_string());
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn read_u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_f32(&mut self) -> Result<f32, String> {
        let b = self.take(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_utf8(&mut self, n: usize) -> Result<String, String> {
        let b = self.take(n)?;
        std::str::from_utf8(b)
            .map(ToString::to_string)
            .map_err(|e| format!("Invalid UTF-8 in TFCL vocab: {e}"))
    }

    fn read_i8_vec(&mut self, count: usize) -> Result<Vec<i8>, String> {
        let b = self.take(count)?;
        Ok(b.iter().map(|&u| u as i8).collect())
    }

    fn read_f32_vec(&mut self, count: usize) -> Result<Vec<f32>, String> {
        let b = self.take(count * 4)?;
        let mut out = Vec::with_capacity(count);
        for chunk in b.chunks_exact(4) {
            out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        Ok(out)
    }
}
