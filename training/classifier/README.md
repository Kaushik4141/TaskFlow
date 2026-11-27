# TaskFlow-TinyNet v1 — Offline Distillation & Training Pipeline

This directory contains the **developer-side offline training pipeline** for TaskFlow's on-device neural activity classifier (`TaskFlow-TinyNet`).

> **Zero Python Runtime in Desktop App**: End users never install Python or PyTorch. This pipeline compiles and quantizes the neural network into a single SHA-256-verified binary (`src-tauri/models/activity_classifier_v1.bin`, ~3.5 MB) executed natively in pure Rust (`src-tauri/src/classifier/`).

## Architecture Summary

- **Pretrained Backbone Distillation**: Extracts ~20,000 clean standalone words from `minishlab/potion-base-8M` and projects 256-D → 96-D via Matryoshka-ordered SVD/PCA, pre-quantized to symmetric per-row `INT8` (zero catastrophic forgetting on unseen apps/domains).
- **Subword + Field-Prefixed Hashing**: 12,288 FNV-1a 64-bit buckets (`app:`, `dom:`, `path:`, `ttl:`, `txt:`, bigrams, char 3-grams) quantized to per-row `INT8` with MLP fine-tuning on exact dequantized weights for `< 1e-4` PyTorch↔Rust numerical parity.
- **Context Cross-Tower**: Computes cosine similarities, lexical project/task overlap, and 40-D Hadamard interactions against a cached 96-D `(active_task + wiki_known_projects)` embedding.
- **Residual Multi-Head MLP**: `328 → 128 → 128` (`LayerNorm` + `GELU(tanh)` + skip connection) → 3 calibrated heads (`productivity`, `16-way category`, `relevance`), ~77k MACs, **< 0.2 ms CPU latency**.
- **On-Device Personalization**: A ~8.8 KB L2-bounded Passive-Aggressive linear adapter in SQLite learns from single-click user corrections in `< 50 µs` without uploading screen telemetry.

## Reproducing Training

```bash
python3 -m venv training/.venv
training/.venv/bin/pip install --index-url https://download.pytorch.org/whl/cpu torch
training/.venv/bin/pip install numpy scikit-learn model2vec safetensors tokenizers huggingface_hub

training/.venv/bin/python training/classifier/generate_dataset.py
training/.venv/bin/python training/classifier/train.py
```

Outputs:
- `src-tauri/models/activity_classifier_v1.bin` — embedded default model binary (`TFCL` v1)
- `src-tauri/src/classifier/testdata/golden_parity.json` — 50 golden PyTorch parity vectors asserted by `cargo test --lib`
- `training/classifier/metrics_report.json` — evaluation across domain-disjoint validation, dual-use counterfactual pairs, and strictly zero-overlap unseen apps/domains

## OTA Model Updates (Without Reinstalling the App)

Any valid SHA-256-verified `TFCL` v1 binary placed at:
- **Linux**: `~/.local/share/com.taskflow.desktop/models/activity_classifier_override.bin`
- **Windows**: `%APPDATA%\com.taskflow.desktop\models\activity_classifier_override.bin`

is automatically verified and loaded at startup in place of the embedded weights (falling back cleanly to embedded weights on checksum or schema mismatch).
