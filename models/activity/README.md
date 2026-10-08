# Offline activity classifier — synthetic bootstrap v2

This is a **real trained sparse linear softmax model**, bootstrapped exclusively
from curated, invented, nonprivate examples. It is not validated on real user
activity. Confidence is an **uncalibrated softmax score**, not a calibrated
probability that a label is correct. Activity labels describe context, not
productivity, importance, or task completion.

## Shipping and reproduction

Rust embeds `weights.f32le` and `manifest.json` using `include_bytes!` /
`include_str!`. Shipping inference is pure Rust CPU using existing dependencies;
no Python runtime, install, model download, OCR, extra capture, or network.

From the repository root, training/reproduction requires only Python 3 stdlib:

```sh
python3 tools/activity/train.py
python3 tools/activity/self_test.py
```

The default trainer uses seed 4141 and 65 SGD epochs. It overwrites only these
model artifacts. `tools/activity/corpus.py` is the source dataset, and
`tools/activity/features.py` is the shared feature contract. Python's RNG and
math implementations can differ between Python/platform releases; artifact
SHA-256 identifies the exact exported revision. No generated corpus contains
captured telemetry. Do not replace synthetic data with private captures.

V2 fixes an output bug: uncertainty no longer replaces the model's raw winning
category with `other`. It also replaces the keyword-rich v1 corpus with plausible
metadata and screen-text fragments, mixed with shared UI chrome/neutral noise.
The feature contract and acceptance thresholds remain v1/unchanged. No user
database, activity history, or private inputs were used to develop or train v2.

## Model and artifact contract

- 8 stable snake-case IDs, in order: `coding_debugging`, `research_learning`,
  `writing_documentation`, `communication`, `planning_administration`,
  `entertainment_media`, `system_navigation`, `other`.
- 8192 hashed feature dimensions × 8 classes, plus 8 biases = **65,544 f32
  parameters**. `weights.f32le` is **262,176 bytes (256.03 KiB)**.
- Layout: class-major contiguous 8192-element weight rows, then 8 biases;
  little-endian IEEE-754 f32, no header. Exact size, all-parameter finiteness,
  SHA-256, model/feature/format versions, category ordering and dimensions are
  validated once. Invalid artifacts result in `None`, never panic or a label.
- SGD updates actual weights from normalized sparse features, with epoch L2
  decay; no hand-authored inference rules select categories.
- Uniform-target cross-entropy regularizes **15 generic browser/editor/terminal
  app-only identities**, four passes per epoch. This discourages learning activity
  from app identity alone. These inputs have no hard category (especially not
  `other`) and are not counted in labelled evaluation. This is not calibration.
- The cached decoded matrix costs 262,176 bytes, in addition to embedded bytes.
  A FIFO prediction cache holds at most 256 entries. Keys contain only up to
  2048 hashed u16 feature indices (at most 1 MiB feature payload total); no raw
  or sanitized source text is retained in this cache. Cache eviction and locks
  do not affect model output. Inference allocates bounded temporary strings.

## Privacy and limits

`capture::activity::predict(app, title, url, content)` only consumes existing
metadata and optional existing captured text. A cached default `PrivacyFilter`
rejects blocked apps and private windows and sanitizes every input field.
The capture caller must still honor configured **user exclusions** before
inference; the pure function has no DB access. Sensitive-file windows never
contribute content. Complete or truncated private-key blocks are omitted.

- App: at most 256 Unicode characters; title: 512. Overlong metadata is rejected
  rather than truncating away a potentially privacy-blocked suffix.
- URL: 2048 characters maximum, must parse as HTTP(S); only parsed host and path
  are used. Credentials, scheme, port, query, and fragment are never features.
  Invalid/overlong URLs contribute nothing. Embedded HTTP(S) URLs in other
  fields get the same stripping before privacy sanitization. A URL host/path
  can still reveal context; feature hashing is not anonymization.
- Existing content: first 4096 Unicode characters, sanitized, never fetched or
  deep-captured by this module. The privacy sanitizer is defense in depth, not
  a claim that arbitrary private text can be made anonymous.
- Per-field token limits: app 16, title 64, URL 64, content 128. Token length 32
  ASCII characters; non-ASCII characters are delimiters. English/ASCII-focused
  bootstrap does not infer activity reliably in other languages.

## Exact feature contract v1

Emit fields in app/title/url/content order after the above privacy preparation.
ASCII letters/digits form words; fold ASCII uppercase to lowercase. Keep only
the first 32 characters of each token, and stop at each field's token limit.
For each word, emit in this order:

1. Shared word `w:<word>`.
2. Field word `<field>:w:<word>`.
3. Field bigram `<field>:b:<previous> <word>` if there is a previous word.
4. Every sliding 3-byte window of `^<word>$`, as `c:<trigram>`.

Hash UTF-8 bytes with **FNV-1a u32**: start 2166136261, then for each byte XOR
and multiply by 16777619 with wrapping modulo 2³²; index = hash modulo 8192.
Keep binary presence, the first 2048 distinct indices in emission order, then
sort indices numerically. Every active feature has value `1/sqrt(active_count)`.
No randomness or process-dependent hash seed participates in inference.
`feature-golden.json` records exact integer indices, including Unicode, repeated
and long-token edge cases and feature-cap saturation; Rust and Python tests
assert the same goldens.
Rust f32 accumulation can differ slightly from Python's f64 score arithmetic.

## Abstention and synthetic evaluation

Empty/unsupported-script-only inputs, privacy exclusions, and model failure
return `None`. Otherwise the category **always retains the raw winning class**;
weak evidence sets `uncertain=true`. `other` now means an actual Other winner,
which can itself be certain or uncertain. The confidence is the original winning
softmax score even when uncertain. Consumers must never treat a retained uncertain
candidate as an accepted prediction.

Accept only if the winning score ≥0.65, top-two margin ≥0.20, at least 4 active
indices have a learned absolute weight ≥0.10 in any class, and ≥35% of indices
have that support. Support coverage is a coarse OOD heuristic; hash collisions
and lexical overlap can defeat it. Mixed activities can still be wrong with
high confidence. These conservative bootstrap thresholds are **not calibrated**.

Training uses **1,080 balanced labelled synthetic rows** (135/class), authored
from 72 scenario templates and 216 sessions. The fixed holdout has **360 rows**
(45/class), from 24 independent scenario templates and 72 sessions. Each session
includes metadata-only, clean content, chrome before content, chrome after content,
and chrome-only content variants. App identities include Windows/Linux/macOS forms
and generic browsers (`chromium`, `helium`) across multiple activities; editors
include both source code and documentation, terminals include code, administration,
and text editing, and communication includes desktop/direct-message/browser mail.

Entire authored scenario templates, sessions and invented project slots remain
disjoint across splits; there is no random near-duplicate row split. Reserved
public synthetic terminal/source-title/mail/mixed-domain regression rows are not
in training. The holdout still shares category vocabulary, app names and public
sites; its rows are correlated session variants, not independent observations.
Dataset design and app-only regularization are bootstrap assumptions, not evidence
of generalization to unseen real screen text.

`manifest.json` records the evaluation of exported f32 weights. For this
artifact: synthetic holdout top-1 **328/360**, accepted **278/360**, and accepted
correct **278/278**. These are checks of invented scenarios, **not real-world
accuracy** or proof that accepted predictions will be correct outside this set.

| Synthetic heldout slice | Top-1 correct | Accepted | Accepted correct |
| --- | ---: | ---: | ---: |
| Metadata only | 70/72 | 51/72 | 51/51 |
| Signal plus unfamiliar OCR chrome | 138/144 | 138/144 | 138/138 |
| Chrome-only content plus metadata | 50/72 | 20/72 | 20/20 |

Short meaningful public synthetic repros improve: the Ghostty title
`cargo test --lib` wins Coding with score **0.997**, and
`main.rs - ExampleProject - Visual Studio Code` wins Coding with **0.725**.
Adding unfamiliar chrome-only OCR to the latter still wins Coding but is
**uncertain at 0.253**. These are narrow regression observations, not calibrated
probabilities. The unchanged L2 normalization can still dilute short title
evidence when many unrelated OCR features are active. V2 accepts that tradeoff
rather than lowering the 0.65 threshold or claiming every opaque window is known.
Most holdout errors are unfamiliar coding fragments or metadata obscured by
chrome-only text. Mixed-domain regression examples distinguish educational video
from entertainment and email discussing code from active code editing, but mixed
activities can still be wrong with high confidence. No independent real-data
evaluation or probability calibration has been performed.

## Checks and inference-only benchmark

Rust unit tests cover exact Python/Rust feature parity, privacy, field/cache
caps, invalid-model failure, canonical examples, public synthetic metadata/noisy
and mixed-domain regressions, OOD, winner-preserving uncertainty, and URL
invariance. Python self-tests validate the exported artifact, grouped corpus,
recorded evaluation, app-only regularization, unchanged goldens, and regressions.
The ignored `inference_cpu_memory_benchmark` measures 10,000 uncached sparse
matrix inferences only. On Linux it reports process CPU ticks converted via
`getconf CLK_TCK`, RSS/HWM before/after, wall time and matrix/cache sizes;
on other platforms only wall time and allocation bounds are available.
Model loading/feature construction are outside the timed loop. RSS/HWM are
process-level observations and not precise per-model allocation accounting.

The normal integration checks are run by the coordinator (`cargo check --lib`
and appropriate Rust tests); this component's trainer does not build the app.
UI label behavior can be tested without launching a browser using
`node --test tools/activity/ui-labels.test.mjs` (uses the existing TypeScript
development dependency, no additional packages).

## TaskFlow integration

Newly persisted events are annotated locally in `activity_annotations` with
category, uncalibrated confidence, uncertainty and model version. Enrichment
reclassifies the event using its new sanitized text. Legacy events remain
unannotated; reading history does not train, backfill or classify it. Annotation
write failure never fails a committed capture. Retention clears raw content
without deleting the original prediction.

Read predictions in Settings' developer raw event stream or a rollup's Activity
rows. The interface uses "Predicted activity" or "uncertain prediction" rather
than presenting labels as facts. Existing privacy and capture filtering stay
unchanged: this model does not decide what to capture, delete, summarize, or send
to any existing configured service. No automatic productivity judgement or task
alignment is enabled.

V2 UI consumers can display the winning label as an **uncertain candidate**.
Legacy `synthetic-bootstrap-v1` annotations with `category=other` and
`uncertain=true` erased the original winner: display **Activity unclear**, not
an Other candidate. Do not relabel or backfill those records from this model
upgrade. A confident v1 Other and a v2 actual Other remain Other.

### Historical v1 development-build latency

The following measurements predate v2 and are retained as historical v1 results.
V2 has the same feature contract, parameter count, cache and inference algorithm,
but these numbers have **not been remeasured for v2**. They were measured locally
on Linux using synthetic short inputs (not private activity):

- Cold model/privacy initialization plus first prediction: **10.75 ms**.
- Complete prediction pipeline, 1,000 varied inputs: mean **162.39 us**, median
  **159.79 us**, p95 **173.17 us**; includes sanitation/features/cache/inference.
- Repeated-input pipeline: mean **139.80 us**; privacy/features still run.
- Sparse matrix inference only, 10,000 iterations: **12.30 us/call**; sampled
  process CPU time **0.12 seconds** across that entire loop.
- Inference-only test process RSS: **12,916 -> 13,208 KiB**. This is a test
  executable's resident memory, not TaskFlow RAM or an isolated model footprint.

Reproduce with:

```sh
cd src-tauri
CARGO_BUILD_JOBS=1 cargo test --lib capture::activity::tests::end_to_end_prediction_benchmark -- --ignored --nocapture --test-threads=1
CARGO_BUILD_JOBS=1 cargo test --lib capture::activity::tests::inference_cpu_memory_benchmark -- --ignored --nocapture --test-threads=1
```

These are debug-build microbenchmarks, not whole-app CPU utilization or latency
guarantees for long inputs or other operating systems. CPU duty cycle depends
on classification frequency; do not infer application-wide resource savings
from these numbers. Native Windows/macOS runtime testing is still outstanding.

## Diagnosing "everything is Other"

v1 replaced every abstained winner with `other`. That conflated uncertainty with
an actual Other prediction. v2 retains the winning candidate and `uncertain`
separately; the UI renders tentative candidates as "Activity unclear · possible
...". Old v1 uncertain labels show "Activity unclear · insufficient evidence";
their original candidate cannot be recovered from the annotation alone. No
history rewrite is performed.

The v1 training language was also too repetitive and unlike real titles/OCR.
v2 broadens synthetic scenarios and shared OCR chrome without lowering the
confidence threshold. It does not solve general semantic understanding. Long
unfamiliar OCR, mixed contexts, and opaque titles can still be uncertain.

An explicit read-only diagnostic uses the production Rust privacy/features/
prediction pipeline on up to 50 recent events and prints only aggregate category
counts, never captured text. It does not update annotations or train on captures:

```sh
cd src-tauri
TASKFLOW_ACTIVITY_DIAGNOSTIC_DB="/path/to/taskflow.sqlite" CARGO_BUILD_JOBS=1 cargo test --lib capture::activity::tests::recent_annotation_diagnostic -- --ignored --nocapture --test-threads=1
```

v2 short-input debug pipeline benchmark measured mean **191.03 us**, median
**190.67 us**, p95 **197.24 us**, with **13.89 ms** cold initialization and
**166.27 us** repeated-input mean on this Linux machine. These measurements
include privacy preparation and features, but not capture/OCR or DB persistence.
