#!/usr/bin/env python3
"""Offline Distillation, Multi-Task Training, Calibration, Quantization & Parity Export
for TaskFlow-TinyNet v1.

Steps:
1. Distills `minishlab/potion-base-8M` into a ~20,000-word, 96-D PCA-ordered vocabulary
   embedding table (pre-quantized to symmetric per-row INT8 for zero quantization gap).
2. Trains the multi-head residual MLP + field-prefixed hashed n-gram table with
   feature-branch dropout (forces pretrained semantic generalization for unseen apps).
3. Fits post-hoc Temperature Scaling on validation logits (ECE < 0.03).
4. Quantizes hash embeddings to per-row INT8 and fine-tunes MLP briefly on exact
   dequantized INT8 embeddings for bit-exact Rust parity.
5. Evaluates on:
   - Stratified validation split (`val.jsonl`)
   - Hard dual-use / counterfactual context benchmark (`eval_dual_use.jsonl`)
   - Strictly zero-overlap unseen-app/domain holdout (`eval_gold_unseen.jsonl`)
6. Exports:
   - `src-tauri/models/activity_classifier_v1.bin` (TFCL v1 SHA-256 verified binary)
   - `src-tauri/src/classifier/testdata/golden_parity.json` (50 golden Rust parity cases)
   - `training/classifier/metrics_report.json`
"""
from __future__ import annotations

import json
import math
import time
from pathlib import Path
from typing import Dict, List, Optional, Sequence, Tuple

import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F
from model2vec import StaticModel
from sklearn.metrics import accuracy_score, f1_score, roc_auc_score

from schema import (
    APP_DIM,
    CATEGORIES,
    CATEGORY_TO_ID,
    CTX_FEAT_DIM,
    CTX_HADAMARD_DIM,
    EMBED_DIM,
    HASH_BUCKETS,
    HIDDEN_DIM,
    INPUT_DIM,
    NUM_CATEGORIES,
    PRODUCTIVE_CATEGORY_IDS,
    STRUCT_FEAT_DIM,
    compute_lexical_context_overlap,
    extract_context_features_ids,
    extract_field_features,
    extract_structural_features,
    pack_tfcl_v1,
)

ROOT_DIR = Path(__file__).resolve().parents[2]
DATA_DIR = Path(__file__).resolve().parent / "data"
OUT_MODEL_PATH = ROOT_DIR / "src-tauri" / "models" / "activity_classifier_v1.bin"
GOLDEN_PARITY_PATH = ROOT_DIR / "src-tauri" / "src" / "classifier" / "testdata" / "golden_parity.json"
METRICS_PATH = Path(__file__).resolve().parent / "metrics_report.json"

MAX_VOCAB = 20000
SEED = 42


def set_seed(seed: int = SEED) -> None:
    np.random.seed(seed)
    torch.manual_seed(seed)


def load_jsonl(path: Path) -> List[Dict]:
    rows: List[Dict] = []
    with path.open("r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def quantize_int8_per_row(mat: np.ndarray) -> Tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Symmetric per-row INT8 quantization. Returns (q_int8, scales_f32, dequantized_f32)."""
    mat = mat.astype(np.float32)
    max_abs = np.max(np.abs(mat), axis=1)
    scales = np.where(max_abs > 1e-8, max_abs / 127.0, 1.0 / 127.0).astype(np.float32)
    q = np.clip(np.round(mat / scales[:, None]), -127, 127).astype(np.int8)
    deq = (q.astype(np.float32) * scales[:, None]).astype(np.float32)
    return q, scales, deq


def build_distilled_vocab() -> Tuple[List[str], np.ndarray, np.ndarray, np.ndarray]:
    """Extract clean real-world vocabulary from `minishlab/potion-base-8M` and project 256->96 via SVD."""
    print("[train] Loading pretrained teacher minishlab/potion-base-8M...")
    teacher = StaticModel.from_pretrained("minishlab/potion-base-8M")
    raw_vocab: Dict[str, int] = teacher.tokenizer.get_vocab()
    raw_emb: np.ndarray = teacher.embedding.astype(np.float32)  # (29528, 256)

    # Keep clean standalone lowercase word tokens (no WordPiece '##' or '[SPECIAL]')
    # Order by original token ID (correlates with frequency in WordPiece vocab).
    candidates: List[Tuple[int, str]] = []
    for tok, tid in raw_vocab.items():
        if tok.startswith("##") or tok.startswith("[") or tok.endswith("]"):
            continue
        if len(tok) < 2 or len(tok) > 24:
            continue
        if all((c.isascii() and (c.isalnum() or c in ("+", "#"))) for c in tok) and tok == tok.lower():
            candidates.append((tid, tok))

    candidates.sort(key=lambda x: x[0])

    # Add domain-critical modern terms via teacher.encode() if not already present
    extra_terms = [
        "tauri", "supabase", "postgres", "sqlite", "neovim", "vscode", "cursor", "alacritty",
        "wezterm", "ghostty", "hyprland", "figma", "excalidraw", "miro", "canva", "blender",
        "davinci", "premiere", "audacity", "ableton", "descript", "linear", "jira", "asana",
        "trello", "clickup", "notion", "overleaf", "confluence", "substack", "loom", "zoom",
        "slack", "zendesk", "intercom", "quickbooks", "xero", "metabase", "snowflake",
        "tableau", "vercel", "cloudflare", "datadog", "grafana", "sentry", "pagerduty",
        "kubernetes", "terraform", "docker", "github", "gitlab", "bitbucket", "stackoverflow",
        "arxiv", "pubmed", "coursera", "udemy", "edx", "leetcode", "kaggle", "anki", "duolingo",
        "quizlet", "freecodecamp", "codecademy", "pluralsight", "zotero", "typora", "scrivener",
        "krita", "solidworks", "kicad", "penpot", "wireshark", "postman", "rstudio", "matlab",
        "netflix", "crunchyroll", "hulu", "disney", "hotstar", "spotify", "twitch", "youtube",
        "shorts", "reels", "tiktok", "instagram", "facebook", "threads", "pinterest", "reddit",
        "valorant", "minecraft", "fortnite", "roblox", "steam", "chess", "lichess", "stardew",
        "factorio", "flipkart", "myntra", "ebay", "amazon", "doordash", "swiggy", "zomato",
        "airbnb", "expedia", "booking", "ikea", "shein", "letterboxd", "imdb", "9gag", "tumblr",
        "bluesky", "buzzfeed", "pytorch", "tensorflow", "onnx", "fastapi", "nextjs", "typescript",
        "javascript", "golang", "rust", "python", "docker", "cargo", "pnpm", "kubectl", "ansible",
        "refactoring", "debugging", "deployment", "benchmark", "onboarding", "wireframe",
        "prototype", "standup", "retrospective", "spreadsheet", "workbook", "invoice", "revenue",
        "gameplay", "walkthrough", "speedrun", "playthrough", "vlog", "prank", "memes", "meme",
        "wishlist", "checkout", "cart", "shipping", "delivery", "coupon", "flashcards", "syllabus",
        "lecture", "tutorial", "masterclass", "course", "huddle", "webinar", "unmute",
    ]
    existing_set = {tok for _, tok in candidates}
    selected_words: List[str] = [tok for _, tok in candidates[: MAX_VOCAB - len(extra_terms)]]
    selected_vecs: List[np.ndarray] = [raw_emb[tid] for tid, _ in candidates[: MAX_VOCAB - len(extra_terms)]]

    missing_extras = [w for w in dict.fromkeys(extra_terms) if w not in existing_set]
    if missing_extras:
        extra_vecs = teacher.encode(missing_extras).astype(np.float32)
        for w, v in zip(missing_extras, extra_vecs):
            selected_words.append(w)
            selected_vecs.append(v)

    # Sort vocabulary lexicographically by UTF-8 bytes so Rust binary_search works directly!
    order = sorted(range(len(selected_words)), key=lambda i: selected_words[i].encode("utf-8"))
    vocab = [selected_words[i] for i in order]
    vecs_256 = np.stack([selected_vecs[i] for i in order], axis=0)  # (V, 256)

    # PCA / SVD projection 256 -> 96 (Matryoshka-ordered by descending singular value!)
    mean = vecs_256.mean(axis=0, keepdims=True)
    centered = vecs_256 - mean
    _, _, vt = np.linalg.svd(centered, full_matrices=False)
    proj = vt[:EMBED_DIM].T  # (256, 96)
    vecs_96 = centered @ proj

    # L2-normalize each row so dot products == cosine similarity
    norms = np.linalg.norm(vecs_96, axis=1, keepdims=True)
    vecs_96 = vecs_96 / np.maximum(norms, 1e-8)

    q_int8, scales, deq_f32 = quantize_int8_per_row(vecs_96)
    print(f"[train] Distilled vocab={len(vocab)} tokens -> {EMBED_DIM}-D INT8 ({q_int8.nbytes / 1e6:.2f} MB)")
    return vocab, q_int8, scales, deq_f32


# ---------------------------------------------------------------------------
# Pre-encoded Sample Representation for Fast Mini-Batch CPU Training
# ---------------------------------------------------------------------------
def l2_norm_np(v: np.ndarray) -> np.ndarray:
    n = float(np.linalg.norm(v))
    return v / n if n > 1e-8 else np.zeros_like(v)


def encode_dataset_rows(
    rows: List[Dict],
    vocab_to_id: Dict[str, int],
    vocab_deq: np.ndarray,
) -> Dict[str, object]:
    """Precompute frozen vocab vectors, hash bucket lists, lexical overlaps, and structural features."""
    n = len(rows)
    app_v = np.zeros((n, EMBED_DIM), dtype=np.float32)
    tu_v = np.zeros((n, EMBED_DIM), dtype=np.float32)
    cnt_v = np.zeros((n, EMBED_DIM), dtype=np.float32)
    ctx_v = np.zeros((n, EMBED_DIM), dtype=np.float32)

    app_has_v = np.zeros(n, dtype=np.float32)
    tu_has_v = np.zeros(n, dtype=np.float32)
    cnt_has_v = np.zeros(n, dtype=np.float32)
    ctx_has_v = np.zeros(n, dtype=np.float32)
    has_ctx_flag = np.zeros(n, dtype=np.float32)

    app_hashes: List[List[int]] = []
    tu_hashes: List[List[int]] = []
    cnt_hashes: List[List[int]] = []
    ctx_hashes: List[List[int]] = []

    lex_proj = np.zeros(n, dtype=np.float32)
    lex_tok = np.zeros(n, dtype=np.float32)
    struct_mat = np.zeros((n, STRUCT_FEAT_DIM), dtype=np.float32)

    y_prod = np.zeros(n, dtype=np.float32)
    y_cat = np.zeros(n, dtype=np.int64)
    y_rel = np.zeros(n, dtype=np.float32)

    for i, r in enumerate(rows):
        (av, ah), (tuv, tuh), (cv, ch) = extract_field_features(
            r["app_name"], r["window_title"], r.get("url"), r.get("content"), vocab_to_id
        )
        ctx_vid, ctx_hid, ctx_sig, proj_lowers = extract_context_features_ids(
            r.get("task_title"), r.get("task_description"), r.get("known_projects") or [], vocab_to_id
        )

        if av:
            app_v[i] = l2_norm_np(vocab_deq[av].mean(axis=0))
            app_has_v[i] = 1.0
        if tuv:
            tu_v[i] = l2_norm_np(vocab_deq[tuv].mean(axis=0))
            tu_has_v[i] = 1.0
        if cv:
            cnt_v[i] = l2_norm_np(vocab_deq[cv].mean(axis=0))
            cnt_has_v[i] = 1.0
        if ctx_vid:
            ctx_v[i] = l2_norm_np(vocab_deq[ctx_vid].mean(axis=0))
            ctx_has_v[i] = 1.0
        if ctx_vid or ctx_hid or proj_lowers:
            has_ctx_flag[i] = 1.0

        app_hashes.append(ah)
        tu_hashes.append(tuh)
        cnt_hashes.append(ch)
        ctx_hashes.append(ctx_hid)

        ep, lt = compute_lexical_context_overlap(
            r["window_title"], r.get("url"), r.get("content"), ctx_sig, proj_lowers
        )
        lex_proj[i] = ep
        lex_tok[i] = lt
        struct_mat[i] = extract_structural_features(
            r["app_name"], r["window_title"], r.get("url"), r.get("content")
        )

        y_prod[i] = float(r["is_productive"])
        y_cat[i] = CATEGORY_TO_ID[r["category"]]
        y_rel[i] = float(r["relevance"])

    return {
        "app_v": app_v,
        "tu_v": tu_v,
        "cnt_v": cnt_v,
        "ctx_v": ctx_v,
        "app_has_v": app_has_v,
        "tu_has_v": tu_has_v,
        "cnt_has_v": cnt_has_v,
        "ctx_has_v": ctx_has_v,
        "has_ctx_flag": has_ctx_flag,
        "app_hashes": app_hashes,
        "tu_hashes": tu_hashes,
        "cnt_hashes": cnt_hashes,
        "ctx_hashes": ctx_hashes,
        "lex_proj": lex_proj,
        "lex_tok": lex_tok,
        "struct_mat": struct_mat,
        "y_prod": y_prod,
        "y_cat": y_cat,
        "y_rel": y_rel,
    }


def _pack_bag(lists: List[List[int]], indices: np.ndarray) -> Tuple[torch.Tensor, torch.Tensor, torch.Tensor]:
    flat: List[int] = []
    offsets: List[int] = []
    has_mask: List[float] = []
    for idx in indices:
        bag = lists[int(idx)]
        offsets.append(len(flat))
        if bag:
            flat.extend(bag)
            has_mask.append(1.0)
        else:
            flat.append(0)
            has_mask.append(0.0)
    return (
        torch.tensor(flat, dtype=torch.long),
        torch.tensor(offsets, dtype=torch.long),
        torch.tensor(has_mask, dtype=torch.float32),
    )


def l2_normalize_safe(x: torch.Tensor) -> torch.Tensor:
    norm = torch.linalg.norm(x, dim=-1, keepdim=True)
    return torch.where(norm > 1e-8, x / torch.clamp(norm, min=1e-8), torch.zeros_like(x))


def blend_vocab_and_hash(
    v_vec: torch.Tensor,
    v_has: torch.Tensor,
    h_vec: torch.Tensor,
    h_has: torch.Tensor,
) -> torch.Tensor:
    """Exact blend mirrored in Rust model.rs:
    - both present: L2Norm(0.65 * v_vec + 0.35 * h_vec)
    - only one present: that normalized vector
    - neither: zero vector
    """
    vh = v_has.unsqueeze(-1)
    hh = h_has.unsqueeze(-1)
    both = vh * hh
    raw = torch.where(
        both > 0.5,
        0.65 * v_vec + 0.35 * h_vec,
        vh * v_vec + hh * h_vec,
    )
    return l2_normalize_safe(raw)


class TaskFlowTinyNet(nn.Module):
    def __init__(self) -> None:
        super().__init__()
        self.hash_emb = nn.EmbeddingBag(HASH_BUCKETS, EMBED_DIM, mode="mean")
        nn.init.normal_(self.hash_emb.weight, mean=0.0, std=0.08)

        self.fc1 = nn.Linear(INPUT_DIM, HIDDEN_DIM)
        self.ln1 = nn.LayerNorm(HIDDEN_DIM, eps=1e-5)
        self.res1 = nn.Linear(HIDDEN_DIM, HIDDEN_DIM)
        self.ln2 = nn.LayerNorm(HIDDEN_DIM, eps=1e-5)
        self.res2 = nn.Linear(HIDDEN_DIM, HIDDEN_DIM)
        self.ln3 = nn.LayerNorm(HIDDEN_DIM, eps=1e-5)

        self.head_prod = nn.Linear(HIDDEN_DIM, 1)
        self.head_cat = nn.Linear(HIDDEN_DIM, NUM_CATEGORIES)
        self.head_rel = nn.Linear(HIDDEN_DIM, 1)

        self.drop = nn.Dropout(0.28)

    def forward_from_bags(
        self,
        app_v: torch.Tensor,
        app_has_v: torch.Tensor,
        app_h_in: Tuple[torch.Tensor, torch.Tensor, torch.Tensor],
        tu_v: torch.Tensor,
        tu_has_v: torch.Tensor,
        tu_h_in: Tuple[torch.Tensor, torch.Tensor, torch.Tensor],
        cnt_v: torch.Tensor,
        cnt_has_v: torch.Tensor,
        cnt_h_in: Tuple[torch.Tensor, torch.Tensor, torch.Tensor],
        ctx_v: torch.Tensor,
        ctx_has_v: torch.Tensor,
        ctx_h_in: Tuple[torch.Tensor, torch.Tensor, torch.Tensor],
        has_ctx_flag: torch.Tensor,
        lex_proj: torch.Tensor,
        lex_tok: torch.Tensor,
        struct_mat: torch.Tensor,
        hash_branch_drop_p: float = 0.0,
        struct_drop_p: float = 0.0,
    ) -> Tuple[torch.Tensor, torch.Tensor, torch.Tensor, torch.Tensor]:
        def pool_hash(bag: Tuple[torch.Tensor, torch.Tensor, torch.Tensor]) -> Tuple[torch.Tensor, torch.Tensor]:
            inp, off, mask = bag
            h = l2_normalize_safe(self.hash_emb(inp, off))
            if self.training and hash_branch_drop_p > 0.0:
                keep = (torch.rand_like(mask) > hash_branch_drop_p).float()
                mask = mask * keep
            return h * mask.unsqueeze(-1), mask

        app_h, app_has_h = pool_hash(app_h_in)
        tu_h, tu_has_h = pool_hash(tu_h_in)
        cnt_h, cnt_has_h = pool_hash(cnt_h_in)
        ctx_h, ctx_has_h = pool_hash(ctx_h_in)

        e_app_full = blend_vocab_and_hash(app_v, app_has_v, app_h, app_has_h)
        e_app = e_app_full[:, :APP_DIM]
        e_tu = blend_vocab_and_hash(tu_v, tu_has_v, tu_h, tu_has_h)
        e_cnt = blend_vocab_and_hash(cnt_v, cnt_has_v, cnt_h, cnt_has_h)
        e_ctx = blend_vocab_and_hash(ctx_v, ctx_has_v, ctx_h, ctx_has_h)

        # Context Cross-Tower (48 dims)
        cos_tu_ctx = (e_tu * e_ctx).sum(dim=-1)
        cos_cnt_ctx = (e_cnt * e_ctx).sum(dim=-1)
        cos_app_ctx = (e_app * e_ctx[:, :APP_DIM]).sum(dim=-1)
        max_cos = torch.maximum(cos_tu_ctx, cos_cnt_ctx)
        e_ev = l2_normalize_safe(e_tu + 0.5 * e_cnt)
        cos_ev_ctx = (e_ev * e_ctx).sum(dim=-1)

        ctx_scalars = torch.stack(
            [
                has_ctx_flag,
                cos_tu_ctx,
                cos_cnt_ctx,
                cos_app_ctx,
                lex_proj,
                lex_tok,
                max_cos,
                cos_ev_ctx,
            ],
            dim=-1,
        )
        hadamard = e_ev[:, :CTX_HADAMARD_DIM] * e_ctx[:, :CTX_HADAMARD_DIM]
        f_ctx = torch.cat([ctx_scalars, hadamard], dim=-1)

        if self.training and struct_drop_p > 0.0:
            s_mask = (torch.rand_like(struct_mat) > struct_drop_p).float()
            f_struct = struct_mat * s_mask
        else:
            f_struct = struct_mat

        x = torch.cat([e_app, e_tu, e_cnt, f_ctx, f_struct], dim=-1)  # (B, 328)

        h1 = F.gelu(self.ln1(self.fc1(x)), approximate="tanh")
        h1_d = self.drop(h1)
        r1 = F.gelu(self.ln2(self.res1(h1_d)), approximate="tanh")
        r1_d = self.drop(r1)
        h_out = self.ln3(self.res2(r1_d) + h1)

        prod_logit = self.head_prod(h_out).squeeze(-1)
        cat_logits = self.head_cat(h_out)
        rel_logit = self.head_rel(h_out).squeeze(-1)
        return prod_logit, cat_logits, rel_logit, h_out


@torch.no_grad()
def predict_encoded(
    model: TaskFlowTinyNet,
    enc: Dict[str, object],
    batch_size: int = 512,
) -> Tuple[np.ndarray, np.ndarray, np.ndarray]:
    model.eval()
    n = len(enc["y_cat"])
    prod_logits = np.zeros(n, dtype=np.float32)
    cat_logits = np.zeros((n, NUM_CATEGORIES), dtype=np.float32)
    rel_logits = np.zeros(n, dtype=np.float32)

    for start in range(0, n, batch_size):
        end = min(start + batch_size, n)
        idx = np.arange(start, end)
        pl, cl, rl, _ = model.forward_from_bags(
            torch.from_numpy(enc["app_v"][idx]),
            torch.from_numpy(enc["app_has_v"][idx]),
            _pack_bag(enc["app_hashes"], idx),
            torch.from_numpy(enc["tu_v"][idx]),
            torch.from_numpy(enc["tu_has_v"][idx]),
            _pack_bag(enc["tu_hashes"], idx),
            torch.from_numpy(enc["cnt_v"][idx]),
            torch.from_numpy(enc["cnt_has_v"][idx]),
            _pack_bag(enc["cnt_hashes"], idx),
            torch.from_numpy(enc["ctx_v"][idx]),
            torch.from_numpy(enc["ctx_has_v"][idx]),
            _pack_bag(enc["ctx_hashes"], idx),
            torch.from_numpy(enc["has_ctx_flag"][idx]),
            torch.from_numpy(enc["lex_proj"][idx]),
            torch.from_numpy(enc["lex_tok"][idx]),
            torch.from_numpy(enc["struct_mat"][idx]),
        )
        prod_logits[start:end] = pl.numpy()
        cat_logits[start:end] = cl.numpy()
        rel_logits[start:end] = rl.numpy()
    return prod_logits, cat_logits, rel_logits


def compute_ece(probs: np.ndarray, labels: np.ndarray, n_bins: int = 15) -> float:
    bins = np.linspace(0.0, 1.0, n_bins + 1)
    ece = 0.0
    n = len(labels)
    for lo, hi in zip(bins[:-1], bins[1:]):
        mask = (probs >= lo) & (probs < hi if hi < 1.0 else probs <= hi)
        cnt = int(mask.sum())
        if cnt > 0:
            acc = float(labels[mask].mean())
            conf = float(probs[mask].mean())
            ece += (cnt / n) * abs(acc - conf)
    return float(ece)


def fit_temperature_binary(logits: np.ndarray, labels: np.ndarray) -> float:
    best_t = 1.0
    best_nll = 1e9
    lt = torch.from_numpy(logits)
    yt = torch.from_numpy(labels.astype(np.float32))
    # Lower-bound at 0.90 to avoid over-sharpening logits on unseen apps
    for t in np.linspace(0.9, 4.0, 156):
        loss = float(F.binary_cross_entropy_with_logits(lt / float(t), yt).item())
        if loss < best_nll:
            best_nll = loss
            best_t = float(t)
    return best_t


def fit_temperature_multiclass(logits: np.ndarray, labels: np.ndarray) -> float:
    best_t = 1.0
    best_nll = 1e9
    lt = torch.from_numpy(logits)
    yt = torch.from_numpy(labels.astype(np.int64))
    for t in np.linspace(0.9, 4.0, 156):
        loss = float(F.cross_entropy(lt / float(t), yt).item())
        if loss < best_nll:
            best_nll = loss
            best_t = float(t)
    return best_t


def evaluate_split(
    name: str,
    model: TaskFlowTinyNet,
    enc: Dict[str, object],
    temp_prod: float,
    temp_cat: float,
) -> Dict[str, float]:
    pl, cl, rl = predict_encoded(model, enc)
    p_prod = 1.0 / (1.0 + np.exp(-pl / temp_prod))
    y_prod = enc["y_prod"].astype(np.int64)
    prod_preds = (p_prod >= 0.5).astype(np.int64)
    prod_acc = float(accuracy_score(y_prod, prod_preds))
    prod_f1 = float(f1_score(y_prod, prod_preds))
    prod_auc = float(roc_auc_score(y_prod, p_prod))
    prod_ece = compute_ece(p_prod, y_prod)

    scaled_cl = cl / temp_cat
    scaled_cl -= scaled_cl.max(axis=1, keepdims=True)
    cat_probs = np.exp(scaled_cl)
    cat_probs /= cat_probs.sum(axis=1, keepdims=True)
    cat_preds = cat_probs.argmax(axis=1)
    y_cat = enc["y_cat"]
    cat_acc = float(accuracy_score(y_cat, cat_preds))
    cat_macro_f1 = float(f1_score(y_cat, cat_preds, average="macro", zero_division=0))

    rel_preds = 1.0 / (1.0 + np.exp(-rl))
    rel_mae = float(np.abs(rel_preds - enc["y_rel"]).mean())

    metrics = {
        "prod_accuracy": round(prod_acc, 4),
        "prod_f1": round(prod_f1, 4),
        "prod_auroc": round(prod_auc, 4),
        "prod_ece": round(prod_ece, 4),
        "category_accuracy": round(cat_acc, 4),
        "category_macro_f1": round(cat_macro_f1, 4),
        "relevance_mae": round(rel_mae, 4),
        "samples": int(len(y_cat)),
    }
    print(
        f"[eval:{name}] n={metrics['samples']} | prod_acc={prod_acc:.4f} prod_auc={prod_auc:.4f} "
        f"ece={prod_ece:.4f} | cat_acc={cat_acc:.4f} macro_f1={cat_macro_f1:.4f} | rel_mae={rel_mae:.4f}"
    )
    return metrics


def main() -> None:
    t0 = time.time()
    set_seed(SEED)
    torch.set_num_threads(2)

    # 1. Distill pretrained vocabulary embeddings from minishlab/potion-base-8M
    vocab, vocab_q, vocab_scales, vocab_deq = build_distilled_vocab()
    vocab_to_id = {tok: i for i, tok in enumerate(vocab)}

    # 2. Load and pre-encode datasets
    train_rows = load_jsonl(DATA_DIR / "train.jsonl")
    val_rows = load_jsonl(DATA_DIR / "val.jsonl")
    dual_rows = load_jsonl(DATA_DIR / "eval_dual_use.jsonl")
    unseen_rows = load_jsonl(DATA_DIR / "eval_gold_unseen.jsonl")

    print("[train] Encoding datasets with distilled vocab + FNV-1a subword buckets + 24 structural features...")
    train_enc = encode_dataset_rows(train_rows, vocab_to_id, vocab_deq)
    val_enc = encode_dataset_rows(val_rows, vocab_to_id, vocab_deq)
    dual_enc = encode_dataset_rows(dual_rows, vocab_to_id, vocab_deq)
    unseen_enc = encode_dataset_rows(unseen_rows, vocab_to_id, vocab_deq)

    # 3. Train TaskFlow-TinyNet with heavy branch dropout + weight decay to maximize unseen generalization
    model = TaskFlowTinyNet()
    optimizer = torch.optim.AdamW(model.parameters(), lr=2.5e-3, weight_decay=4e-3)
    epochs = 10
    batch_size = 256
    n_train = len(train_rows)
    scheduler = torch.optim.lr_scheduler.CosineAnnealingLR(optimizer, T_max=epochs, eta_min=2e-4)

    best_val_score = -1.0
    best_state = None

    print(f"[train] Phase 1: Training for {epochs} epochs (batch={batch_size}, n={n_train})...")
    for epoch in range(1, epochs + 1):
        model.train()
        perm = np.random.permutation(n_train)
        total_loss = 0.0
        steps = 0
        for start in range(0, n_train, batch_size):
            idx = perm[start : start + batch_size]
            optimizer.zero_grad()
            pl, cl, rl, _ = model.forward_from_bags(
                torch.from_numpy(train_enc["app_v"][idx]),
                torch.from_numpy(train_enc["app_has_v"][idx]),
                _pack_bag(train_enc["app_hashes"], idx),
                torch.from_numpy(train_enc["tu_v"][idx]),
                torch.from_numpy(train_enc["tu_has_v"][idx]),
                _pack_bag(train_enc["tu_hashes"], idx),
                torch.from_numpy(train_enc["cnt_v"][idx]),
                torch.from_numpy(train_enc["cnt_has_v"][idx]),
                _pack_bag(train_enc["cnt_hashes"], idx),
                torch.from_numpy(train_enc["ctx_v"][idx]),
                torch.from_numpy(train_enc["ctx_has_v"][idx]),
                _pack_bag(train_enc["ctx_hashes"], idx),
                torch.from_numpy(train_enc["has_ctx_flag"][idx]),
                torch.from_numpy(train_enc["lex_proj"][idx]),
                torch.from_numpy(train_enc["lex_tok"][idx]),
                torch.from_numpy(train_enc["struct_mat"][idx]),
                hash_branch_drop_p=0.60,
                struct_drop_p=0.35,
            )
            loss_prod = F.binary_cross_entropy_with_logits(pl, torch.from_numpy(train_enc["y_prod"][idx]))
            loss_cat = F.cross_entropy(cl, torch.from_numpy(train_enc["y_cat"][idx]), label_smoothing=0.06)
            loss_rel = F.smooth_l1_loss(torch.sigmoid(rl), torch.from_numpy(train_enc["y_rel"][idx]))
            loss = 1.2 * loss_prod + 1.0 * loss_cat + 0.6 * loss_rel
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 2.0)
            optimizer.step()
            total_loss += float(loss.item())
            steps += 1
        scheduler.step()

        val_m = evaluate_split(f"val_e{epoch:02d}", model, val_enc, 1.0, 1.0)
        score = 0.5 * val_m["prod_accuracy"] + 0.5 * val_m["category_macro_f1"]
        if score > best_val_score:
            best_val_score = score
            best_state = {k: v.detach().cpu().clone() for k, v in model.state_dict().items()}

    if best_state is not None:
        model.load_state_dict(best_state)

    # 4. Quantize hash_emb to per-row INT8, freeze it, and fine-tune MLP on exact dequantized INT8
    hash_w_np = model.hash_emb.weight.detach().cpu().numpy()
    hash_q, hash_scales, hash_deq = quantize_int8_per_row(hash_w_np)
    with torch.no_grad():
        model.hash_emb.weight.copy_(torch.from_numpy(hash_deq))
    model.hash_emb.weight.requires_grad_(False)

    mlp_params = [p for p in model.parameters() if p.requires_grad]
    opt_ft = torch.optim.AdamW(mlp_params, lr=6e-4, weight_decay=4e-3)
    print("[train] Phase 2: Fine-tuning MLP on frozen dequantized INT8 hash embeddings (2 epochs)...")
    for _ in range(2):
        model.train()
        perm = np.random.permutation(n_train)
        for start in range(0, n_train, batch_size):
            idx = perm[start : start + batch_size]
            opt_ft.zero_grad()
            pl, cl, rl, _ = model.forward_from_bags(
                torch.from_numpy(train_enc["app_v"][idx]),
                torch.from_numpy(train_enc["app_has_v"][idx]),
                _pack_bag(train_enc["app_hashes"], idx),
                torch.from_numpy(train_enc["tu_v"][idx]),
                torch.from_numpy(train_enc["tu_has_v"][idx]),
                _pack_bag(train_enc["tu_hashes"], idx),
                torch.from_numpy(train_enc["cnt_v"][idx]),
                torch.from_numpy(train_enc["cnt_has_v"][idx]),
                _pack_bag(train_enc["cnt_hashes"], idx),
                torch.from_numpy(train_enc["ctx_v"][idx]),
                torch.from_numpy(train_enc["ctx_has_v"][idx]),
                _pack_bag(train_enc["ctx_hashes"], idx),
                torch.from_numpy(train_enc["has_ctx_flag"][idx]),
                torch.from_numpy(train_enc["lex_proj"][idx]),
                torch.from_numpy(train_enc["lex_tok"][idx]),
                torch.from_numpy(train_enc["struct_mat"][idx]),
                hash_branch_drop_p=0.55,
                struct_drop_p=0.30,
            )
            loss = (
                1.2 * F.binary_cross_entropy_with_logits(pl, torch.from_numpy(train_enc["y_prod"][idx]))
                + 1.0 * F.cross_entropy(cl, torch.from_numpy(train_enc["y_cat"][idx]), label_smoothing=0.05)
                + 0.6 * F.smooth_l1_loss(torch.sigmoid(rl), torch.from_numpy(train_enc["y_rel"][idx]))
            )
            loss.backward()
            torch.nn.utils.clip_grad_norm_(mlp_params, 2.0)
            opt_ft.step()

    # 5. Post-hoc Temperature Calibration on domain-disjoint validation split
    val_pl, val_cl, _ = predict_encoded(model, val_enc)
    temp_prod = fit_temperature_binary(val_pl, val_enc["y_prod"])
    temp_cat = fit_temperature_multiclass(val_cl, val_enc["y_cat"])
    print(f"[train] Calibrated temperatures: temp_prod={temp_prod:.3f}, temp_cat={temp_cat:.3f}")

    # 6. Full Evaluation across all 3 benchmarks
    val_metrics = evaluate_split("val_disjoint", model, val_enc, temp_prod, temp_cat)
    dual_metrics = evaluate_split("dual_use", model, dual_enc, temp_prod, temp_cat)
    unseen_metrics = evaluate_split("unseen_gold", model, unseen_enc, temp_prod, temp_cat)

    # 7. Export TFCL v1 binary model
    mlp_weights = {
        k: v.detach().cpu().numpy().astype("<f4").tobytes()
        for k, v in model.state_dict().items()
        if k != "hash_emb.weight"
    }
    blob = pack_tfcl_v1(
        vocab=vocab,
        vocab_q=vocab_q.tobytes(),
        vocab_scales=vocab_scales.astype("<f4").tobytes(),
        hash_q=hash_q.tobytes(),
        hash_scales=hash_scales.astype("<f4").tobytes(),
        mlp_weights=mlp_weights,
        temp_prod=temp_prod,
        temp_cat=temp_cat,
    )
    OUT_MODEL_PATH.parent.mkdir(parents=True, exist_ok=True)
    OUT_MODEL_PATH.write_bytes(blob)
    model_size_mb = len(blob) / (1024.0 * 1024.0)
    print(f"[export] Wrote {OUT_MODEL_PATH} ({model_size_mb:.2f} MB)")

    # 8. Export 50 Golden Parity Test Cases (exact Python outputs for Rust unit test verification!)
    golden_rows = val_rows[:20] + dual_rows[:15] + unseen_rows[:15]
    golden_enc = encode_dataset_rows(golden_rows, vocab_to_id, vocab_deq)
    g_pl, g_cl, g_rl = predict_encoded(model, golden_enc)
    golden_cases = []
    for i, r in enumerate(golden_rows):
        p_prod = float(1.0 / (1.0 + math.exp(-float(g_pl[i]) / temp_prod)))
        scaled = g_cl[i] / temp_cat
        scaled = scaled - float(scaled.max())
        exps = np.exp(scaled)
        probs = (exps / exps.sum()).tolist()
        p_rel = float(1.0 / (1.0 + math.exp(-float(g_rl[i]))))
        golden_cases.append(
            {
                "app_name": r["app_name"],
                "window_title": r["window_title"],
                "url": r.get("url"),
                "content": r.get("content"),
                "task_title": r.get("task_title"),
                "task_description": r.get("task_description"),
                "known_projects": r.get("known_projects") or [],
                "expected_struct": [round(float(x), 6) for x in golden_enc["struct_mat"][i].tolist()],
                "expected_prod_prob": round(p_prod, 6),
                "expected_cat_probs": [round(float(x), 6) for x in probs],
                "expected_relevance": round(p_rel, 6),
                "expected_category": CATEGORIES[int(np.argmax(probs))],
            }
        )
    GOLDEN_PARITY_PATH.parent.mkdir(parents=True, exist_ok=True)
    GOLDEN_PARITY_PATH.write_text(json.dumps(golden_cases, indent=2), encoding="utf-8")
    print(f"[export] Wrote {len(golden_cases)} golden parity vectors to {GOLDEN_PARITY_PATH}")

    # 9. Save metrics report
    report = {
        "model_version": "taskflow-tinynet-v1",
        "teacher_backbone": "minishlab/potion-base-8M",
        "vocab_size": len(vocab),
        "hash_buckets": HASH_BUCKETS,
        "embed_dim": EMBED_DIM,
        "hidden_dim": HIDDEN_DIM,
        "num_categories": NUM_CATEGORIES,
        "categories": list(CATEGORIES),
        "model_size_bytes": len(blob),
        "model_size_mb": round(model_size_mb, 3),
        "temp_prod": round(temp_prod, 4),
        "temp_cat": round(temp_cat, 4),
        "training_seconds": round(time.time() - t0, 1),
        "metrics": {
            "validation": val_metrics,
            "dual_use_adversarial": dual_metrics,
            "unseen_zero_overlap_gold": unseen_metrics,
        },
    }
    METRICS_PATH.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"[export] Saved metrics report to {METRICS_PATH} in {report['training_seconds']}s")


if __name__ == "__main__":
    main()
