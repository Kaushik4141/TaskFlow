#!/usr/bin/env python3
"""Offline artifact/data/feature regression checks; stdlib only, no installation."""

import hashlib
import json
import math
from pathlib import Path
import struct
import unittest
from urllib.parse import urlsplit

from corpus import examples
from features import CATEGORIES, DIMENSIONS, MAX_FEATURES, fnv1a, indices, tokens
from train import VERSION, evaluate, prediction, score

ROOT = Path(__file__).resolve().parents[2]
MODEL = ROOT / "models" / "activity"


class ActivityTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.manifest = json.loads((MODEL / "manifest.json").read_text())
        cls.payload = (MODEL / "weights.f32le").read_bytes()
        parameters = list(struct.unpack("<65544f", cls.payload))
        cls.weights, cls.biases = parameters[:-8], parameters[-8:]

    def test_artifact_contract(self):
        self.assertEqual(len(self.payload), 262176)
        self.assertEqual(self.manifest["dimensions"], DIMENSIONS)
        self.assertEqual(self.manifest["categories"], CATEGORIES)
        self.assertEqual(self.manifest["parameter_count"], 65544)
        self.assertEqual(self.manifest["model_version"], VERSION)
        self.assertEqual(self.manifest["feature_version"], 1)
        self.assertEqual(self.manifest["abstention"]["min_confidence"], 0.65)
        self.assertEqual(hashlib.sha256(self.payload).hexdigest(), self.manifest["sha256"])
        self.assertTrue(all(math.isfinite(w) for w in self.weights + self.biases))
        self.assertGreater(sum(abs(w) > 0.001 for w in self.weights), 1000)

    def test_balanced_disjoint_synthetic_data(self):
        train, holdout = examples("train"), examples("holdout")
        self.assertEqual(len(train), 1080)
        self.assertEqual(len(holdout), 360)
        for category in CATEGORIES:
            self.assertEqual(sum(row["category"] == category for row in train), 135)
            self.assertEqual(sum(row["category"] == category for row in holdout), 45)
        for key in ("session", "template"):
            self.assertFalse({r[key] for r in train} & {r[key] for r in holdout})
        # Exact text rows are also distinct; metadata/deep variants belong to one session.
        text = lambda r: tuple(r[field] for field in ("app", "title", "url", "content"))
        self.assertFalse({text(r) for r in train} & {text(r) for r in holdout})
        for rows in (train, holdout):
            for row in rows:
                self.assertFalse(any(part in row["url"] for part in ("://", "?", "#")))
                if row["url"]:
                    self.assertIsNone(urlsplit("https://" + row["url"]).username)
            for session in {r["session"] for r in rows}:
                self.assertEqual({r["variant"] for r in rows if r["session"] == session}, set(range(5)))

    def test_golden_and_limits(self):
        self.assertEqual(fnv1a("hello"), 0x4f9f2cab)
        goldens = json.loads((MODEL / "feature-golden.json").read_text())
        self.assertEqual(len(goldens[-1]["indices"]), MAX_FEATURES)
        for golden in goldens:
            self.assertEqual(indices(golden["fields"]), golden["indices"])
        self.assertEqual(tokens("CAFÉ 中文 alpha__BETA 123", 10), ["caf", "alpha", "beta", "123"])
        self.assertEqual(tokens("a" * 100, 10), ["a" * 32])
        self.assertEqual(len(tokens("word " * 1000, 128)), 128)
        active = indices({"content": " ".join("word" + str(i) for i in range(1000))})
        self.assertLessEqual(len(active), MAX_FEATURES)
        self.assertEqual(active, sorted(set(active)))
        self.assertTrue(all(0 <= index < DIMENSIONS for index in active))
        self.assertEqual(indices({}), [])

    def test_recorded_evaluation_matches_exported_f32(self):
        self.assertEqual(evaluate(self.weights, self.biases, examples("holdout")),
                         self.manifest["synthetic_holdout_evaluation"])

    def test_canonical_examples_and_ood(self):
        from corpus import TRAIN
        for category in CATEGORIES:
            apps, title, url, content = TRAIN[category][0]
            active = indices({"app": apps[0], "title": title.format(project=""), "url": url, "content": content})
            scores = score(self.weights, self.biases, active)
            winner = max(range(8), key=scores.__getitem__)
            self.assertEqual(CATEGORIES[winner], category)
            self.assertGreaterEqual(scores[winner], 0.65)
        unknown = score(self.weights, self.biases, indices({"app": "xyzq", "title": "qzxv jxqz vqxj"}))
        self.assertLess(max(unknown), 0.65)

    def test_uncertainty_retains_raw_winner(self):
        weights, biases = [0.0] * (8 * DIMENSIONS), [0.0] * 8
        weights[42] = 5.0
        result = prediction(weights, biases, [42])
        self.assertGreater(result["confidence"], 0.65)
        self.assertTrue(result["uncertain"])  # support gate, not score gate
        self.assertEqual(result["category"], "coding_debugging")
        self.assertIsNone(prediction(weights, biases, []))
        weights[42] = 0.0
        weights[7 * DIMENSIONS + 42] = 5.0
        self.assertEqual(prediction(weights, biases, [42])["category"], "other")

    def test_public_synthetic_regressions_not_in_training(self):
        # Hand-authored regressions, never imported into corpus/train. Not an
        # independent benchmark: category vocabulary is intentionally shared.
        cases = [
            ({"app": "com.mitchellh.ghostty", "title": "cargo test --lib"}, "coding_debugging"),
            ({"app": "code", "title": "main.rs - ExampleProject - Visual Studio Code"}, "coding_debugging"),
            ({"app": "helium", "title": "Inbox (12) - Gmail"}, "communication"),
            ({"app": "chromium", "title": "Inbox - Outlook", "url": "outlook.live.com/mail/0/inbox"}, "communication"),
            ({"app": "helium", "title": "Direct messages - Slack"}, "communication"),
            ({"app": "com.mitchellh.ghostty", "title": "sudo apt upgrade"}, "system_navigation"),
            ({"app": "code", "title": "README.md - ExampleProject - Visual Studio Code"}, "writing_documentation"),
            ({"app": "firefox", "title": "YouTube lecture on Rust ownership", "url": "youtube.com/watch",
              "content": "Course lesson Learn about memory allocation and references Read tutorial examples"}, "research_learning"),
            ({"app": "firefox", "title": "Rust compiler error - Gmail", "url": "mail.google.com/mail/u/0/inbox",
              "content": "Inbox Sent Drafts Reply Forward Subject Rust build failed please review the code"}, "communication"),
        ]
        train = examples("train")
        for fields, expected in cases:
            self.assertFalse(any(all(row[field] == fields.get(field, "")
                                     for field in ("app", "title", "url", "content")) for row in train))
            result = prediction(self.weights, self.biases, indices(fields))
            self.assertEqual(result["category"], expected, (fields, result))
        # Meaningful source title survives unfamiliar OCR chrome as a candidate.
        chrome = "Menu Toolbar Navigation pane Find text Toggle sidebar Fit page Backward Forward Reload Options Help center 09:52 Tuesday Connected English US Search everywhere Open tab Recent items Bookmarks View full screen This panel shows additional information. Select an entry to continue. The current view has been updated."
        for fields, expected in cases[:3]:  # cargo, source editor, browser mail
            noisy_fields = dict(fields, content=chrome)
            result = prediction(self.weights, self.biases, indices(noisy_fields))
            self.assertEqual(result["category"], expected, (noisy_fields, result))
        noisy = {"app": "code", "title": "main.rs - ExampleProject - Visual Studio Code", "content": chrome}
        result = prediction(self.weights, self.biases, indices(noisy))
        self.assertEqual(result["category"], "coding_debugging")
        self.assertTrue(result["uncertain"])  # no forcing opaque context over .65
        for fields in ({"app": "xyzq", "title": "qzxv jxqz vqxj"},
                       {"app": "helium", "title": "New Tab"},
                       {"app": "chromium", "title": "Loading..."}):
            result = prediction(self.weights, self.biases, indices(fields))
            self.assertTrue(result["uncertain"], (fields, result))
            self.assertIn(result["category"], CATEGORIES)

    def test_generic_app_identity_regularization(self):
        # This verifies the trained regularization contract, not heldout quality.
        from corpus import BROWSERS, EDITORS, TERMINALS
        for app in BROWSERS + EDITORS + TERMINALS:
            result = prediction(self.weights, self.biases, indices({"app": app}))
            self.assertTrue(result["uncertain"], (app, result))


if __name__ == "__main__":
    unittest.main(verbosity=2)
