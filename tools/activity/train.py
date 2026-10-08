#!/usr/bin/env python3
"""Reproducible offline stdlib sparse multiclass softmax SGD training."""

import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import struct

from corpus import BROWSERS, EDITORS, TERMINALS, examples
from features import CATEGORIES, DIMENSIONS, indices

ROOT = Path(__file__).resolve().parents[2]
DESTINATION = ROOT / "models" / "activity"
VERSION = "synthetic-bootstrap-v2"


def softmax(logits):
    maximum = max(logits)
    exps = [math.exp(value - maximum) for value in logits]
    total = sum(exps)
    return [value / total for value in exps]


def score(weights, biases, active):
    scale = 1.0 / math.sqrt(len(active)) if active else 0.0
    return softmax([biases[c] + scale * sum(weights[c * DIMENSIONS + i] for i in active)
                    for c in range(len(CATEGORIES))])


def prediction(weights, biases, active):
    """Mirror Rust uncertainty gates; never replace the raw winning category."""
    if not active:
        return None
    probs = score(weights, biases, active)
    ranking = sorted(range(8), key=lambda c: (-probs[c], c))
    winner = ranking[0]
    supported = sum(max(abs(weights[c * DIMENSIONS + i]) for c in range(8)) >= 0.10
                    for i in active)
    uncertain = (supported < 4 or supported / len(active) < 0.35
                 or probs[winner] < 0.65 or probs[winner] - probs[ranking[1]] < 0.20)
    return {"category": CATEGORIES[winner], "confidence": probs[winner],
            "uncertain": uncertain, "model_version": VERSION}


def evaluate(weights, biases, rows):
    confusion = [[0] * 8 for _ in range(8)]
    accepted = correct_accepted = correct = 0
    for row in rows:
        active = indices(row)
        result = prediction(weights, biases, active)
        predicted = CATEGORIES.index(result["category"])
        target = CATEGORIES.index(row["category"])
        confusion[target][predicted] += 1
        correct += predicted == target
        if not result["uncertain"]:
            accepted += 1
            correct_accepted += predicted == target
    return {"examples": len(rows), "correct_top1": correct, "top1_fraction": correct / len(rows),
            "accepted": accepted, "accepted_correct": correct_accepted,
            "accepted_fraction": accepted / len(rows), "confusion_actual_rows_predicted_columns": confusion}


def train(epochs=65):
    train_rows = examples("train")
    holdout_rows = examples("holdout")
    assert not ({r["template"] for r in train_rows} & {r["template"] for r in holdout_rows})
    assert not ({r["session"] for r in train_rows} & {r["session"] for r in holdout_rows})
    prepared = [(CATEGORIES.index(row["category"]), indices(row)) for row in train_rows]
    # Generic app identity cannot establish activity. Uniform-target loss for
    # app-only contexts suppresses shortcuts without inventing an Other label.
    # These regularization inputs are not counted as labelled evaluation rows.
    app_only = [indices({"app": app}) for app in BROWSERS + EDITORS + TERMINALS]
    weights = [0.0] * (DIMENSIONS * 8)
    biases = [0.0] * 8
    rng = random.Random(4141)
    for epoch in range(epochs):
        rng.shuffle(prepared)
        learning_rate = 0.75 / (1.0 + epoch * 0.025)
        # Dense epoch-boundary L2 shrink; sparse per-example gradients.
        weights = [w * 0.998 for w in weights]
        for target, active in prepared:
            probs = score(weights, biases, active)
            scale = 1.0 / math.sqrt(len(active))
            for category in range(8):
                gradient = learning_rate * (probs[category] - (category == target))
                biases[category] -= gradient * 0.05
                delta = gradient * scale
                offset = category * DIMENSIONS
                for index in active:
                    weights[offset + index] -= delta
        for _ in range(4):
            for active in app_only:
                probs = score(weights, biases, active)
                scale = 1.0 / math.sqrt(len(active))
                for category in range(8):
                    delta = learning_rate * (probs[category] - 0.125) * scale
                    offset = category * DIMENSIONS
                    for index in active:
                        weights[offset + index] -= delta
    # Export f32 LE class-major dense matrix followed by the eight biases.
    parameters = weights + biases
    payload = b"".join(struct.pack("<f", value) for value in parameters)
    rounded = list(struct.unpack("<" + "f" * len(parameters), payload))
    weights, biases = rounded[:-8], rounded[-8:]
    DESTINATION.mkdir(parents=True, exist_ok=True)
    (DESTINATION / "weights.f32le").write_bytes(payload)
    manifest = {
        "format_version": 1, "feature_version": 1, "model_version": VERSION,
        "dimensions": DIMENSIONS, "categories": CATEGORIES,
        "parameter_count": len(parameters), "artifact_bytes": len(payload),
        "layout": "class_major_weights_then_biases_f32le", "sha256": hashlib.sha256(payload).hexdigest(),
        "provenance": "Entirely curated synthetic nonprivate examples; no captured activity or private data.",
        "training": {"algorithm": "sparse multiclass softmax SGD with epoch L2 shrink", "seed": 4141,
                     "epochs": epochs, "learning_rate": "0.75/(1+epoch*0.025)", "epoch_weight_decay": 0.998,
                      "bias_learning_rate_multiplier": 0.05, "examples": len(train_rows), "per_category": len(train_rows) // 8,
                      "app_only_regularization": "Uniform-target cross-entropy on 15 generic browser/editor/terminal identities, four passes per epoch; not labelled Other or included in evaluation."},
        "split": "Authored scenario-template/session/project-disjoint holdout; all app/metadata/OCR variants grouped, no random row split.",
        "confidence": "Uncalibrated bootstrap softmax score, not a calibrated probability of correctness.",
        "abstention": {"min_confidence": 0.65, "min_margin": 0.20, "min_supported_features": 4,
                       "min_support_fraction": 0.35, "support_weight_threshold": 0.10},
        "synthetic_training_evaluation": evaluate(weights, biases, train_rows),
        "synthetic_holdout_evaluation": evaluate(weights, biases, holdout_rows),
        "synthetic_holdout_slices": {
            "metadata_only": evaluate(weights, biases, [r for r in holdout_rows if r["variant"] == 0]),
            "noisy_ocr": evaluate(weights, biases, [r for r in holdout_rows if r["variant"] in (2, 3)]),
            "chrome_only_content": evaluate(weights, biases, [r for r in holdout_rows if r["variant"] == 4]),
        },
        "limitations": "Synthetic-only evaluation is not real-world accuracy; English/ASCII features; ambiguous/mixed activities and hash collisions remain. Holdout shares category vocabulary; no calibration or independent external validation.",
    }
    (DESTINATION / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    golden_fields = [
        {"app": "Code", "title": "main.rs DEBUG", "url": "github.com/example/pull", "content": "Fix failing unit test"},
        {"app": "", "title": "CAFÉ 中文 alpha__BETA 123", "url": "", "content": ""},
        {"app": "a" * 100, "title": "x " * 80, "url": "", "content": "redacted note"},
    ]
    golden_rng = random.Random(1618)
    golden_fields.append({"app": "", "title": "", "url": "", "content": " ".join(
        "".join(golden_rng.choice("abcdefghijklmnopqrstuvwxyz") for _ in range(24))
        for _ in range(128))})
    goldens = [{"fields": fields, "indices": indices(fields)} for fields in golden_fields]
    (DESTINATION / "feature-golden.json").write_text(json.dumps(goldens, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps({"artifact_bytes": len(payload), "sha256": manifest["sha256"],
                      "synthetic_holdout_evaluation": manifest["synthetic_holdout_evaluation"]}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--epochs", type=int, default=65)
    args = parser.parse_args()
    train(args.epochs)
