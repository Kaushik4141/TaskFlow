---
name: run-taskflow
description: Build, run, and drive TaskFlow (a Tauri + Python-sided desktop app that captures screen activity and maintains an Obsidian "second brain" wiki). Use when asked to start TaskFlow, run/check its Rust + Python code, build it, exercise the AI sidecar, or drive the summarize/filter pipeline.
---

TaskFlow is a Windows-first **Tauri** desktop app: a Rust core with a Python **FastAPI sidecar** (port 7878) doing AI summarization/filtering/embedding, writing results to a SQLite DB and an Obsidian vault. The lean agent path here drives the **sidecar directly** and type-checks the Rust — it does NOT launch the GUI window (the full app build hits a Windows PDB linker limit; see Gotchas). All paths are relative to the repo root.

**Documentation is event-driven, not stop-triggered.** A background roll-up scheduler (`src-tauri/src/rollup/scheduler.rs`, spawned in `lib.rs`) flushes the pending event window every ~10 minutes (settings keys `rollup_enabled` default true, `rollup_interval_minutes` default 10), plus early flushes on 5-min idle, workstream switch, and task stop. Each roll-up (`rollup::run_rollup`) summarizes one window adaptively — <3 events get a local template, larger windows use the configured AI mode with the previous roll-up chained as context — persists to the `rollups` table, and is routed to a workstream node (`TaskFlow/Projects/<slug>.md`, `## Activity Timeline` section, writer `wiki/workstream.rs`). The date-based daily note is a **derived index** regenerated from the rollups table (`wiki/daily_index.rs`, its sole writer); workstream nodes are the source of truth. The `generate_documentation` command still produces in-app docs (and `Tasks/<title>.md` for non-memory tasks) but deliberately does NOT write the daily note or hub pages for memory tasks.

## Prerequisites

Windows. Verified present in the authoring environment:

```bash
# Verified working versions (probe with --version):
#   node v24.16.0  | pnpm 10.32.1  | npm 11.12.x
#   rustc 1.94.0 (host x86_64-pc-windows-msvc)  | cargo 1.94.0
#   python 3.11.5  | uv 0.11.14
```

Optional but expected for AI: Ollama (`http://localhost:11434`) for `local_ai` mode; a cloud provider key OpenAI-compatible (OpenRouter was the configured one here) for `cloud_ai`.

The frontend deps (one-time):

```bash
pnpm install
```

The **sidecar Python deps are the real prerequisite** — they are heavy and were NOT installed during skill authoring (see Setup). They are a multi-GB, minutes-long install (torch, transformers, sentence-transformers, sumy, scikit-learn).

## Setup

### Sidecar deps — REQUIRED to boot the sidecar, NOT YET RUN

Importing the sidecar currently fails: `sumy` is missing (`basic_summarizer` imports it at module load), and `sentence-transformers`, `scikit-learn`, `anthropic` are absent. Worse, the **installed numpy is 2.1.3 while `requirements.txt` pins 1.26**, and that mismatch already breaks `torch` at import (`_ARRAY_API not found`). So patch-installing just the missing names is not enough — do a clean pin in an isolated venv:

```bash
# from repo root, one-time:
python -m venv .venv-sidecar
.venv-sidecar\Scripts\activate
uv pip install -r sidecar/requirements.txt
# Verify import works in that venv before trusting it:
python -c "import sumy, sklearn, sentence_transformers, torch; print('sidecar deps OK')"
```

This `.venv-sidecar` is local to the repo and untouched by the generator's run; it is the reproducible prerequisite the skill needs but has not verified end-to-end. **TODO: run `boot_sidecar.sh` after this install to mark it verified.**

## Build

The Rust core type-checks cleanly and is what the agent path uses:

```bash
cd src-tauri && cargo check --lib
```

A **full app build** is NOT part of the agent path — it triggers the Windows `link.exe` PDB limit (`LNK1318`; see Gotchas). If a GUI build is ever needed:

```bash
pnpm tauri dev   # recompiles + launches the window; may hit LNK1318
```

## Run (agent path)

Two commands, both committed in this skill dir. Start the sidecar, then drive it.

### 1. Boot the sidecar

```bash
# requires the sidecar venv from Setup to be active
bash .claude/skills/run-taskflow/boot_sidecar.sh
# → polls http://127.0.0.1:7878/health until ready (≤40s), prints PID + health JSON
#   leaves uvicorn running; stop with: kill $(cat /tmp/taskflow_sidecar.pid)
```

### 2. Drive it (the real spine — no GUI needed)

```bash
# From repo root. Reads the REAL TaskFlow AI config from the live SQLite DB
# (incl. decrypting the XOR-obfuscated cloud key) and exercises the routes.

# Show the configured mode/provider/model (key redacted to last 4):
python .claude/skills/run-taskflow/drive_sidecar.py Config

# Drive /summarize with synthetic events, prints daily markdown + hub blocks:
python .claude/skills/run-taskflow/drive_sidecar.py Summarize --mode cloud_ai

# Drive /filter (relevance pipeline) with synthetic events:
python .claude/skills/run-taskflow/drive_sidecar.py Filter

# Health probe:
python .claude/skills/run-taskflow/drive_sidecar.py Health
```

| driver command | what it does |
|---|---|
| `Config` | reads live `%APPDATA%\com.taskflow.desktop\taskflow.sqlite` settings; XOR-decrypts the cloud key via hostname SHA-256, redacts to `***<last4>` |
| `Summarize` | POSTs to `/summarize` with realistic synthetic events; auto-picks `cloud_ai` if a key is configured else `basic`; prints summary, key_points, markdown head, and any ` ```hub:` synthesis blocks |
| `Filter` | POSTs to `/filter`; prints per-event included/reason/score |
| `Health` | GETs `/health`; exits non-zero if sidecar down |

Artifacts/logs: `boot_sidecar.sh` writes the uvicorn log to a `mktemp` `.log` and the PID to `/tmp/taskflow_sidecar.pid`.

### Type-check / test the Rust

```bash
cd src-tauri && cargo check --lib   # verified: compiles with 0 warnings
cargo test --lib                    # 34 tests: wiki ingest, workstream timeline, daily index, dust cleanup, slug/fallback hygiene, reassignment, OCR clamping, capture backoff, retention, snapshot teardown
```

The test suite covers the wiki ingest phases, the roll-up workstream writer (prepend/idempotency/section preservation), the derived daily index (hub links, roll-up-based nav), registry-gated dust cleanup (incl. machine-generated workstream pages), app-slug normalization, the vault-safe fallback reason, and workstream reassignment to Inbox.

## Run (human path)

```bash
pnpm tauri dev      # → compiles Rust + Vite, opens the TaskFlow window. Ctrl-C to stop.
```

Useless headless (needs a visible window for screen-capture). The agent-path harness above mirrors what the app does without the window.

## Test

`cargo test --lib` (34 tests) + `cargo check --lib` (Rust) + `drive_sidecar.py Summarize` (sidecar summarize path) + `drive_sidecar.py Filter` (relevance path) are the verification surface. `pnpm build` type-checks the frontend (tsc + vite).

## Gotchas

- **Full app build hits `LNK1318`** ("Unexpected PDB error; LIMIT") on this machine — `link.exe`'s debug PDB exceeds the limit during the final `.exe` link. `cargo check --lib` is unaffected (no linking). The lean agent path deliberately avoids the full build. To actually launch the GUI you'd need to work around the PDB limit (e.g. split the binary, or build `--release` which uses different PDB handling).
- **Sidecar deps must be a clean pinned venv** — the *system* Python here has numpy 2.x, which silently breaks the `torch` import the embedder needs ("a module compiled with NumPy 1.x cannot be run in NumPy 2.x"). Don't `pip install sumy` into the system Python; use the `.venv-sidecar` from Setup.
- **Cloud API key storage is XOR, not real encryption** — `commands.rs` `machine_key()` derives the key as `SHA256(hostname)[..32]` and XORs the token, hex-stored in `summary_cloud_api_key`. The driver reproduces this exactly so it can read the real key; treat the DB key as only-as-strong-as-hostname-obfuscation (it's the app's existing scheme, not new weakness introduced here).
- **Sidecar runs from source in dev** — `lib.rs` `dev_sidecar_script()` launches `python sidecar/main.py` from the repo, so editing `llm_summarizer.py`/`basic_summarizer.py` takes effect on sidecar restart with no Rust recompile. The bundle step (`externalBin` in `tauri.conf.json`) only matters for `tauri build`.
- **No `cl.exe` on PATH in a plain shell** — Rust still builds via cargo's MSVC toolchain auto-discovery, but verify with `cargo check` rather than assuming a Developer-prefixed shell.

## Troubleshooting

- **`ModuleNotFoundError: No module named 'sumy'` when importing/starting the sidecar**: the venv from Setup wasn't activated, or the pinned install didn't finish. Re-run `uv pip install -r sidecar/requirements.txt` inside `.venv-sidecar`.
- **`sidecar unreachable at http://127.0.0.1:7878: [WinError 10061] ... actively refused it`** (verified symptom): nothing is serving 7878. Boot it first via `boot_sidecar.sh`; if that exits early, the import-time deps are missing (it prints the exact `.log` tail).
- **`_ARRAY_API not found` / "NumPy 1.x cannot run in NumPy 2.x" on torch import**: installed numpy is 2.x. `uv pip install -r sidecar/requirements.txt` pins `numpy==1.26.3` and fixes it — do the install in the isolated venv, not the system Python.
- **`TaskFlow DB not found at ...\com.taskflow.desktop\taskflow.sqlite`**: the app has never run on this machine. Launch once (`pnpm tauri dev` or an installed build) so it initializes the settings table, then retry `Config`.
- **`cargo` rant about `LNK1318`**: you ran a command that triggered the full link (e.g. `cargo build`/`pnpm tauri build`). Use `cargo check --lib` for the agent path; the PDB limit is a build-machine constraint, not a code bug.

---
