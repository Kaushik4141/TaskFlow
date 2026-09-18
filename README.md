# TaskFlow

Desktop app that captures screen activity on Windows and Linux, processes it with AI summarization/filtering, and maintains an Obsidian "second brain" wiki.


<img src="\public\fonts\assets\image.png" >

## Architecture

- **Rust core** (`src-tauri/`): Tauri 2 backend with SQLite storage, rollup engine, wiki writer
- **React/TS frontend** (`src/`): Vite-powered UI with timeline, settings, vault browser
- **Python FastAPI sidecar** (`sidecar/`, port 7878): AI summarize/filter/embed endpoints
- **Storage**: SQLite at `%APPDATA%\com.taskflow.desktop\taskflow.sqlite` on Windows, or `~/.local/share/com.taskflow.desktop/taskflow.sqlite` on Linux
- **Output**: Obsidian vault (user-configured path) under `TaskFlow/` subdirectory

## Getting Started

### Prerequisites

- Rust (stable)
- Node.js with pnpm
- Python 3.11+
- Linux window capture: Hyprland (`hyprctl`) or X11 (`xprop`)
- Optional Linux deep capture OCR: `tesseract`, plus `grim` on Hyprland or `import`/`maim` on X11
- Optional Linux idle detection: `xprintidle` (X11) or logind `IdleHint` (Wayland)

### Setup

1. **Frontend dependencies**
   ```bash
   pnpm install
   ```

2. **Sidecar dependencies** (use isolated venv to avoid numpy 2.x breaking torch)
   ```bash
   bash sidecar/setup.sh
   ```
   On Windows, run `sidecar\setup.bat` instead.
   The script creates `.venv-sidecar` and pins Python 3.11 when `uv` is available.
   First launch downloads the `all-MiniLM-L6-v2` embedding model (~80MB).

3. **Run in development**
   ```bash
   pnpm tauri dev
   ```
   The Python sidecar starts automatically when the Tauri app launches.

## Development Commands

- **Verify Rust code**: `cd src-tauri && cargo check --lib`
- **Run tests**: `cd src-tauri && cargo test --lib` (wiki ingest, rollup engine, OCR, Linux capture parsing, retention, etc.)
- **Build frontend**: `pnpm build` (typecheck + vite)

**⚠️ DO NOT run `cargo build` or `pnpm tauri build`**: Full link hits LNK1318 (Windows PDB limit). Use `cargo check --lib` for verification.

## Key Features

- **Capture**: UI Automation on Windows; Hyprland/X11 window titles on Linux, with in-memory OCR fallback when `tesseract` and a screenshot utility are installed
- **Rollup engine**: ~10-min event windows → workstream nodes (`TaskFlow/Projects/<slug>.md`)
- **Wiki**: Obsidian vault with Apps/Sites/Activity/Projects hubs + daily notes derived from rollups
- **Privacy**: OCR never writes bitmaps to disk on either platform; Obsidian itself is capture-excluded
- **Retention**: Nulls `events.content` after configured hours while keeping row for stats/feed

## Testing & Troubleshooting

See `.agents/skills/run-taskflow/SKILL.md` for:
- Headless sidecar harness (boot, drive `/summarize` and `/filter` without GUI)
- Setup gotchas and debugging tips

## Project Documentation

- `AGENTS.md`: Architecture facts, command reference, conventions
- `PRODUCT.md`: Design philosophy (narrative-over-numbers, keyboard-first, local-first)
