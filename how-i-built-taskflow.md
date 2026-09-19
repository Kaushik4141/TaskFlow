# How I Built TaskFlow: An Auto-Wiki Second Brain, the Hard Problems I Faced, and How I Solved Them

*By Kaushik H S*

---

## 1. The Genesis: Why I Needed a Second Brain That Thinks Like an Engineer

Like many developers, my daily workflow is wonderfully chaotic. On any given afternoon, I’m jumping between **Ghostty** and **Foot** terminals, editing code in **Cursor** and **VSCodium**, testing models on **Ollama** or **NVIDIA NIM**, browsing documentation on **Helium** and **Firefox**, and hacking on multiple projects simultaneously—from hackathon builds to AI agent frameworks.

In this constant flow state, context is both everything and the first thing you lose. 

You take a break for dinner, or switch tasks to fix an urgent bug, and when you return:
* *What was the exact prompt technique that stabilized that LLM response an hour ago?*
* *Which files did I edit before I got sidetracked by that CORS error?*
* *Why did I configure that specific PostgreSQL connection string?*

When I looked at existing tools to solve this, I hit three brick walls:

1. **The Friction Trap (Notion, Obsidian manual logging, Toggl, Clockify):**  
   Manual logging kills flow state. Nobody wants to interrupt their coding rhythm to fill out a timesheet or type markdown bullet points about what they *intend* to do. The moment a tool asks for manual input, it fails.
2. **The Sterile Charts Trap (ActivityWatch, RescueTime):**  
   Activity monitors give you cold, sterile pie charts: *"You spent 4.2 hours in VS Code and 1.1 hours in Firefox."* That tells me virtually nothing. Did I fix the authentication bug? Did I refactor the database schema? Numbers without narrative are useless.
3. **The Creepy Surveillance Trap (Microsoft Recall):**  
   Recall’s approach was brute-force screenshot hoarding—dumping continuous raw bitmaps onto disk, capturing banking sessions, passwords, and private messages with zero user agency or transparent data ownership.

I wanted something fundamentally different. My manifesto for **TaskFlow** became:

> **"Your work, auto-wiki'd. No logging. No setup. Just the next session."**  
> *Invisible. Faithful. Interlinked. Narrative over numbers. Local-first, user-owned.*

I envisioned an app that watches what you do, understands the engineering narrative, organizes it into 10-minute atomic rollups, builds a living, interconnected **Obsidian knowledge graph**, and exposes the entire brain to external AI agents via the **Model Context Protocol (MCP)**.

Building it, however, meant running headfirst into some of the nastiest systems-level, privacy, database, and concurrency problems I've ever tackled.

Here is how I architected TaskFlow, the roadblocks that almost derailed it, and how I solved each one.

---

## 2. The High-Level Architecture

TaskFlow is designed as a hybrid desktop application combining native performance, local AI intelligence, and a cloud-mirrored knowledge graph.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                             DESKTOP ENVIRONMENT                             │
│                                                                             │
│   Linux (Hyprland / X11)                      Windows (UI Automation)       │
│   Active window titles & in-memory OCR         UIA Tree walking + WinRT OCR  │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │ Raw Events (Title, App, Process, Text)
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                    TASKFLOW CORE ENGINE (Rust / Tauri 2)                    │
│                                                                             │
│   ┌──────────────────────────────────────────────────────────────────────┐  │
│   │ 5-Pass Privacy Filter (privacy.rs)                                   │  │
│   │ App exclusions ➔ Private keys ➔ URIs ➔ Tokens ➔ Shannon Entropy      │  │
│   └──────────────────────────────────┬───────────────────────────────────┘  │
│                                      │ Sanitized Events                     │
│                                      ▼                                      │
│   ┌──────────────────────────┐             ┌─────────────────────────────┐  │
│   │ Local SQLite Engine      │             │ Rollup Scheduler (10 min)   │  │
│   │ events, rollups, graph   │◄────────────┤ adaptive summary windows    │  │
│   └──────────────────────────┘             └──────────────┬──────────────┘  │
│                                                           │                 │
│                                                           ▼                 │
│                                            ┌─────────────────────────────┐  │
│                                            │ Wiki Writer & Daily Indexer │  │
│                                            │ Projects/<slug>.md          │  │
│                                            │ Memory/Daily/<date>.md      │  │
│                                            └──────────────┬──────────────┘  │
└───────────────────────────────────────────────────────────┼─────────────────┘
                                                            │ Markdown Nodes
                                                            ▼
                                             ┌─────────────────────────────┐
                                             │ User's Local Obsidian Vault │
                                             │ Apps/, Sites/, Projects/    │
                                             └──────────────┬──────────────┘
                                                            │
                     ┌──────────────────────────────────────┴──────────────────────────────────┐
                     ▼                                                                         ▼
┌──────────────────────────────────────────────┐              ┌──────────────────────────────────────────────┐
│       FASTAPI SIDECAR (:7878 / Python)       │              │         CLOUD GRAPH MIRROR (Supabase)        │
│                                              │              │                                              │
│ • Local Embeddings (all-MiniLM-L6-v2)        │              │ • 24/7 Always-On pgvector store              │
│ • Semantic Relevance Filtering               │              │ • Vault Notes, Rollups & Graph Edges         │
│ • Local / Cloud LLM Summarization            │              │ • Sub-15ms PostgreSQL RPC search             │
└──────────────────────────────────────────────┘              └──────────────────────┬───────────────────────┘
                                                                                     │
                                                                                     ▼
                                                              ┌──────────────────────────────────────────────┐
                                                              │       MODEL CONTEXT PROTOCOL (MCP) GATEWAY   │
                                                              │          (Deployed on Render Cloud)          │
                                                              │                                              │
                                                              │ • Pure Python stdio & SSE JSON-RPC 2.0       │
                                                              │ • Scope-First GraphRAG (<5ms pruning)        │
                                                              │ • Connected to Claude, Cursor, Hermes, Agy   │
                                                              └──────────────────────────────────────────────┘
```

The system breaks down into four core tiers:

1. **Native Rust Core (`src-tauri/`)**: Built on **Tauri 2**. Manages OS hooks, window monitoring, clipboard quarantine, SQLite persistence, the 10-minute rollup scheduler, and the Obsidian wiki writer.
2. **React / Vite Frontend (`src/`)**: A keyboard-first, low-distraction interface (linear-inspired) showing live event streams, activity feeds, and knowledge graph navigation.
3. **Python FastAPI Sidecar (`sidecar/`, Port 7878)**: An isolated Python process handling heavy NLP tasks: local embeddings (`all-MiniLM-L6-v2`), task relevance classification, and LLM summarization (Ollama locally or cloud models like NVIDIA NIM / OpenRouter).
4. **Universal MCP Brain & Supabase Mirror**: An always-on bridge exposing the second brain over the **Model Context Protocol (MCP)** to external AI coding agents (Claude Desktop, Cursor, Hermes Agent, Antigravity CLI).

---

## 3. The War Stories: Hard Problems & How I Solved Them

### Problem 1: The Screen-Scraping Dilemma & The Anti-Recall Stance

#### The Problem
How do you capture meaningful desktop context across different operating systems without turning your app into invasive spyware that eats 100GB of disk space with screenshots?

Microsoft Recall’s approach of snapping screenshots every few seconds was an architectural non-starter for me:
1. It records everything blindly (passwords, bank accounts, private messages).
2. Bitmaps on disk are massive and slow down disk I/O.
3. OCR over full-screen 4K bitmaps consumes ridiculous CPU/GPU cycles.

#### The Solution: Capability-Based Multi-Tier Capture
I implemented a capability-based capture hierarchy in `src-tauri/src/capture/`:

* **Tier 1 (Zero-Cost Metadata):** On Linux, active window titles and process classes are fetched directly from Wayland compositor IPC (Hyprland via `hyprctl activewindow -j`) or X11 via `xprop -id <window_id> WM_NAME`. On Windows, the app registers OS window focus hooks.
* **Tier 2 (Accessibility Tree Walking):** On Windows, before even thinking about pixels, TaskFlow walks the **UI Automation (UIA)** accessibility tree to read text elements directly from the DOM/control hierarchy of the active application.
* **Tier 3 (In-Memory Ephemeral OCR):** When an app renders custom canvas elements (like an IDE or PDF viewer) where accessibility trees return nothing, OCR activates strictly as a last resort:
  * On Windows: `PrintWindow` copies the window pixels directly into memory, downscales buffers larger than 2560px using `StretchBlt` with `HALFTONE`, and feeds the in-memory buffer to WinRT `Windows.Media.Ocr`.
  * On Linux: Screen grabbing utilities (`grim` on Hyprland, `maim`/`import` on X11) capture the target window geometry and pipe raw PNG bytes through stdout directly into `tesseract` stdin.
  * **Crucial Rule:** **Bitmaps NEVER touch the filesystem.** The pixel buffer lives in RAM for milliseconds and is immediately freed after text extraction.
* **Circuit-Breaker Backoff:** If an app repeatedly fails text extraction (e.g., heavily obfuscated or continuously re-painting windows), an exponential backoff breaker (`window_monitor.rs::backoff_delay`) doubles capture intervals from 30 seconds up to 15 minutes, preventing CPU thrashing.

---

### Problem 2: The 5-Pass Zero-Disk-Leak Secrets Redaction Pipeline

#### The Problem
As developers, our screens are drenched in secrets: `.env` files, GitHub Personal Access Tokens (`ghp_...`), OpenAI API keys (`sk-...`), AWS access keys (`AKIA...`), database connection strings (`postgres://user:pass@host`), and private SSH keys.

If a desktop memory recorder captures an API key from my editor and writes it to an unencrypted SQLite database or syncs it to an Obsidian vault, that’s an catastrophic security vulnerability. Furthermore, regexes alone miss random, bespoke passwords or newly invented token formats.

#### The Solution: Shannon Entropy + 5-Pass In-Memory Sanitization
I built a multi-stage defense-in-depth sanitization pipeline in `src-tauri/src/capture/privacy.rs`. It runs **synchronously in memory before any event is ever committed to SQLite or written to disk**:

```rust
pub fn sanitize_content(&self, content: &str) -> String {
    // Pass 1: Redact full private key blocks (RSA, ED25519, etc.)
    let step1 = self.private_key_regex.replace_all(content, "[REDACTED_PRIVATE_KEY]");

    // Pass 2: Redact URI embedded credentials (postgres://user:pass@host)
    let step2 = self.uri_credentials_regex.replace_all(&step1, "${1}[REDACTED]:[REDACTED]@");

    // Pass 3: Redact key-value credential assignments (api_key = "...", token: "...")
    let step3 = self.assignment_regex.replace_all(&step2, "${1}${2}[REDACTED]");

    // Pass 4: Redact known vendor token signatures
    let mut step4 = step3.to_string();
    for re in &self.secret_regexes {
        step4 = re.replace_all(&step4, "[REDACTED]").to_string();
    }

    // Pass 5: Line-by-line Shannon entropy analysis to catch unknown random secrets
    // ...
}
```

#### The Secret Weapon: Shannon Entropy
To catch custom tokens and passwords that don't match known regex patterns, I implemented Shannon entropy analysis ($H(X) = -\sum P(x) \log_2 P(x)$):

```rust
pub fn shannon_entropy(s: &str) -> f32 {
    let mut counts = std::collections::HashMap::new();
    let mut total = 0usize;
    for ch in s.chars() {
        *counts.entry(ch).or_insert(0usize) += 1;
        total += 1;
    }
    let total_f = total as f32;
    let mut entropy = 0.0;
    for &count in counts.values() {
        let p = count as f32 / total_f;
        entropy -= p * p.log2();
    }
    entropy
}

pub fn is_high_entropy_secret(token: &str) -> bool {
    let len = token.chars().count();
    if len < 18 || len > 256 {
        return false;
    }
    // Exclude URLs and file paths
    if token.starts_with("http") || token.contains("://") || token.contains('/') || token.contains('\\') {
        return false;
    }
    // Must contain typical secret character sets (letters + digits/symbols)
    let has_letters = token.chars().any(|c| c.is_alphabetic());
    let has_digits_or_symbols = token.chars().any(|c| c.is_numeric() || "-_!@#$%^&*+=~".contains(c));
    if !has_letters || !has_digits_or_symbols {
        return false;
    }
    shannon_entropy(token) >= 4.0
}
```

Any token with high randomness ($\ge 4.0$ bits/character) that looks like an encoded secret is immediately replaced with `[REDACTED]`.

Additionally:
* **Window Exclusion & Self-Exclusion:** Password managers (Bitwarden, 1Password, KeePass) and incognito windows are dropped at the gate. Crucially, **Obsidian itself is excluded**—preventing an infinite recursive loop where TaskFlow captures itself reading the vault!
* **Clipboard Quarantine:** When clipboard capture is enabled, TaskFlow checks copied text against the secret regexes and entropy calculations. If you copy an API key or password, TaskFlow immediately quarantines it so it never touches the activity log.

---

### Problem 3: The Two-Writers Race Condition & Vault Sprawl

#### The Problem
Early in TaskFlow's development, I had two different systems updating the Obsidian vault:
1. An on-demand documentation engine (`generate_documentation`) triggered by task stops.
2. A background periodic daily indexer.

This dual-writer pattern was a recipe for disaster. File lock contentions on Windows, overwritten notes, and desynchronized state plagued the vault.

Worse, my initial naive entity extraction logic created a new project page for every distinct window title it saw. If I opened a random Google search tab titled `"How to center a div - Google Chrome"`, TaskFlow would mint `Projects/How-to-center-a-div.md`! Within two days, the Obsidian graph was cluttered with hundreds of junk "dust pages."

#### The Solution: The Rollup Spine & The Single-Writer Principle
I completely re-architected the vault ingestion pipeline around two strict rules:

1. **The Rollup Spine is the Single Source of Truth:**  
   The background scheduler (`src-tauri/src/rollup/scheduler.rs`) runs every ~10 minutes. It takes the pending window of events, passes it to the sidecar for summarization, and writes the summary to a workstream node (`TaskFlow/Projects/<slug>.md`) under its `## Activity Timeline`. Workstream files are the primary record of what happened.
2. **Daily Notes are Derived Read-Only Indexes:**  
   The daily note (`Memory/Daily/<date>.md`) was refactored so that it is never written directly by arbitrary tasks. Instead, `wiki/daily_index.rs` acts as the **sole writer** of daily notes. It queries the SQLite `rollups` table and deterministically compiles a chronological index linking back to the workstream hubs.
3. **Known Projects Registry & Dust Cleanup:**  
   I introduced a `wiki_known_projects` configuration. If an activity does not match a recognized project workstream or heuristic, it routes safely to `Projects/Inbox.md`. An automated dust cleanup pass scans the vault, deletes machine-generated ephemeral project stubs, and reassigns their orphan rollups back to the Inbox.

The result? Clean, structured Obsidian graphs with zero file-write collisions.

---

### Problem 4: The Silent SQLite ISO-8601 Timestamp Bug

#### The Problem
I wanted a data retention policy: raw verbatim screen text should only be kept for 72 hours, after which the text is purged to save space and protect privacy.

However, simply running `DELETE FROM events WHERE timestamp < ...` was unacceptable because deleting rows destroyed the app usage statistics and broke the visual timeline feed in the UI.

I updated the logic to *null* the raw text instead:
```sql
UPDATE events SET content = NULL WHERE timestamp < ?;
```

I deployed this, set retention to 24 hours, and checked back the next day. **Nothing was being purged.** Raw text from days ago was still sitting in the database. No errors were thrown. The query simply affected 0 rows.

#### The Root Cause
The culprit was an insidious character encoding mismatch between Rust and SQLite:
* In Rust, `Utc::now().to_rfc3339()` produces timestamps formatted with a `'T'` separator: `2026-09-18T14:30:00Z`.
* In SQLite, the built-in `datetime('now', '-24 hours')` helper returns a space-separated string: `2026-09-18 14:30:00`.

In ASCII, a space (`' '`, byte `0x20`) has a lower numerical value than `'T'` (byte `0x54`). When SQLite executes a string comparison:
```sql
'2026-09-18T14:30:00Z' < '2026-09-18 14:30:00'
```
Because `' '` < `'T'`, any event captured on the same day evaluated to `FALSE`! Same-day events were completely immune to pruning.

#### The Fix
I eliminated SQLite date functions entirely from the application layer. All timestamp bounds are now generated in Rust via `chrono::Utc::now().to_rfc3339()` and passed into SQLite as explicit parameters.

I also added a safety constraint:
$$\text{cutoff} = \min(\text{now} - \text{retention\_hours}, \text{rollup\_watermark})$$
This ensures that raw text is never purged before the 10-minute rollup engine has had a chance to process and summarize it!

---

### Problem 5: Toolchain & Linker Nightmares (MSVC LNK1318 & NumPy 2.x)

#### The Problem
Building a cross-language desktop app (Rust + TypeScript + Python) means dealing with the friction where toolchains collide:

1. **The Windows PDB Limit (`LNK1318`):**  
   During development on Windows, running `cargo build` or `pnpm tauri build` in debug mode suddenly began crashing with:
   `fatal error LNK1318: Unexpected PDB error; LIMIT (23) ''`
   Because Tauri bundles large Rust dependencies (tokio, tauri, windows-rs, rusqlite, uiautomation), the monolithic debug `.pdb` file exceeded the MSVC linker’s 4GB limit.
2. **The NumPy 2.x Torch Breakage:**  
   On my dev machine, the system Python had automatically upgraded to `numpy==2.1.3`. When the sidecar attempted to import PyTorch and `sentence-transformers` for local vector embeddings, it crashed immediately:
   `ImportError: _ARRAY_API not found. A module compiled with NumPy 1.x cannot run in NumPy 2.x.`

#### The Fix
* **Decoupled Rust Dev Workflow:** For everyday coding and CI checks, I stopped running full executable links. I switched to `cd src-tauri && cargo check --lib` for instantaneous type verification, and `cargo test --lib` (which compiles isolated test runners without hitting the 4GB PDB limit). Full builds are reserved for optimized release binaries (`--release`), where symbol striping prevents PDB bloat.
* **Isolated Sidecar Virtualenv:** In `sidecar/setup.sh` and `setup.bat`, I enforced the creation of an isolated `.venv-sidecar` pinned strictly to Python 3.11 with `numpy==1.26.3` and pinned PyTorch wheels. The Tauri app launcher was updated to detect and boot this local `.venv-sidecar` automatically.

---

### Problem 6: The Remote Agent Dilemma — GraphRAG, Cloud Mirroring, and Zero-Dependency MCP

#### The Problem
TaskFlow was working brilliantly on my local machine. My Obsidian vault was growing, and my local SQLite database held rich telemetry.

Then came the next frontier: **AI Agents**.

I wanted my coding assistants—**Claude Desktop**, **Cursor**, **Hermes Agent**, and the **Antigravity CLI (`agy`)**—to be able to query my second brain. When I ask an agent, *"Why did the OAuth token refresh bug happen yesterday?"*, I wanted it to retrieve the exact answer from my TaskFlow timeline.

However:
1. **Local SQLite is Trapped:** If I'm running an agent in a container, a remote VM, or a cloud IDE, it cannot read my laptop's local `~/.local/share/com.taskflow.desktop/taskflow.sqlite` or Obsidian folder.
2. **Vector Flooding:** Standard RAG that dumps top-20 chunk vectors into the prompt causes context explosion (~10,000 tokens) and hallucinates because it lacks structural project context.
3. **MCP Hosting Dependency Bloat:** Most Model Context Protocol servers require heavy node packages or complex python virtualenvs that make remote deployment a headache.

#### The Solution: 24/7 Supabase Mirror + Pure Python MCP on Render

```
                                  CLOUD ARCHITECTURE
                                  
  Local Machine                                               Render Cloud Web Service
┌────────────────────────────┐                               ┌─────────────────────────────────────────┐
│ TaskFlow Desktop           │                               │ Taskflow MCP Brain (Python HTTP / SSE)  │
│                            │                               │                                         │
│ • Local SQLite & Vault     │                               │ • /sse ➔ Server-Sent Events Stream      │
│ • Periodic Background Sync ├─────────────┐                 │ • /messages ➔ JSON-RPC 2.0 Dispatch     │
└────────────────────────────┘             │                 │ • /health ➔ 200 OK Readiness            │
                                           │                 │ • Pure stdlib (zero pip deps on Render) │
                                           ▼                 └───────────────────▲─────────────────────┘
                             ┌───────────────────────────┐                       │
                             │ Supabase Cloud Mirror     │                       │
                             │                           │                       │
                             │ • cloud_rollups (pgvector)│◄──────────────────────┘
                             │ • cloud_graph_edges       │   High-Speed PostgreSQL RPC
                             │ • vault_notes (Markdown)  │   query_cloud_memory() (<15ms)
                             └───────────────────────────┘
                                           ▲
                                           │
                               ┌───────────┴───────────┐
                               │ External AI Clients   │
                               │ Cursor, Claude, Hermes│
                               └───────────────────────┘
```

#### Step 1: Scope-First, Search-Second GraphRAG
Instead of brute-force similarity search over all raw events, I designed a **Scope-First** retrieval engine:
* The knowledge graph tracks relationships (`Apps/codium` ➔ `Projects/TaskFlow`, `Sites/supabase.com` ➔ `Projects/TaskFlow`).
* When querying, the engine first filters by the project workstream or month bucket (`2026-09`).
* This prunes 100,000+ nodes down to the relevant subgraph in $< 5\text{ms}$. Only then does semantic vector ranking run, returning high-density ground truth in $\sim 300$ tokens.

#### Step 2: 24/7 Supabase pgvector Cloud Mirror
In `sidecar/supabase_client.py`, I built a bidirectional synchronization engine:
* Syncs markdown vault notes, atomic rollups with vector embeddings, and graph edges to Supabase.
* **PostgREST Upsert Fix:** Early versions threw `400 Bad Request` on sync because PostgREST requires unambiguous unique constraint targets for upserts. I configured explicit `on_conflict` parameters (`on_conflict="user_id,path"` for vault notes and `on_conflict="id"` for rollups) with `Prefer: resolution=merge-duplicates`.
* **Multi-Tenant Isolation:** Added automatic data claiming on user login so unassigned legacy notes are seamlessly bound to the user's authenticated Supabase UID.

#### Step 3: Zero-Dependency Pure Python MCP Server
I wanted the MCP server to deploy effortlessly on cloud hosting without waiting for multi-gigabyte PyTorch wheel installs.

In `sidecar/mcp_server.py`, I wrote a complete **Model Context Protocol (2024-11-05 spec) engine from scratch using only Python standard library**:
* Standard library `http.server.BaseHTTPRequestHandler` handles both direct JSON-RPC 2.0 (`/mcp`) and Server-Sent Events (`/sse` + `/messages`).
* Maintains active SSE queues with thread-safe keepalive pings (`: ping\r\n\r\n`).
* If local SQLite is not found (which is true in the cloud on Render), it automatically falls back to querying the Supabase pgvector mirror via standard `urllib.request` REST calls!

#### Step 4: Render 1-Click Blueprint
I added a simple `render.yaml` to the repo root:
```yaml
services:
  - type: web
    name: taskflow-mcp-brain
    runtime: python
    plan: free
    buildCommand: "echo 'No external dependencies required for pure Python MCP engine'"
    startCommand: "python sidecar/mcp_server.py"
    healthCheckPath: /health
    envVars:
      - key: PYTHONUNBUFFERED
        value: "1"
      - key: SUPABASE_URL
        sync: false
      - key: SUPABASE_KEY
        sync: false
```

Because there are zero external pip dependencies, the Render deploy builds in **2 seconds**.

Today, the web service runs live at `https://taskflow-mcp-brain.onrender.com`. I wired this endpoint into my `.cursor/mcp.json`, Claude Desktop config, and Hermes Agent. Wherever I am in the world, my AI assistants have direct, real-time access to my development memory.

---

## 4. What It Feels Like Today

Here is what my daily experience looks like with TaskFlow running invisibly in the background:

1. **Zero Logging Friction:** I wake up, open Ghostty, and start hacking. TaskFlow tracks my branch switches, terminal commands, browser documentation visits, and IDE edits without a single prompt, popup, or button press.
2. **Living Obsidian Vault:** When I open Obsidian, my `TaskFlow/` directory is alive with interlinked notes. Every project has an `## Activity Timeline` detailing what I accomplished in 10-minute increments, complete with links to tools used (`[[Apps/codium]]`), sites referenced (`[[Sites/integrate.api.nvidia.com]]`), and daily memory indexes.
3. **Omnipresent AI Context via MCP:** When I open Cursor or Claude and type:
   > *"What was the exact 5-pass secrets redaction algorithm we implemented in privacy.rs?"*
   
   The agent calls `query_graph_memory` through the TaskFlow MCP server. In $< 5\text{ms}$, the MCP server queries the cloud mirror, pulls the exact implementation timeline, and the agent responds with precise formulas, regex patterns, and commit history.
4. **Complete Privacy Peace of Mind:** Not a single raw screenshot exists on my SSD. My API tokens and passwords never leave RAM unredacted. My data belongs to me in open SQLite and Markdown formats.

---

## 5. Key Takeaways for Building AI-Native Developer Tools

If you are building developer tools, second-brain apps, or MCP services, here are the core lessons I learned:

1. **Narrative Over Numbers:** Raw analytics and time counters don't help developers think. Connect actions into human-readable narratives using periodic rollup windows.
2. **Local-First with Cloud Mirroring is the Golden Pattern:** Keep local SQLite and Markdown as the primary interface for zero latency and offline resilience. Mirror to cloud pgvector only for multi-device synchronization and remote agent access.
3. **Scope-First Beats Big Context Windows:** Don't dump thousands of raw events into an LLM context window. Filter by project workstream, graph edges, and time buckets first. 300 tokens of precise, structured ground truth will beat 30,000 tokens of fuzzy search every single time.
4. **Zero-Dependency Core Transports Win:** Writing the MCP server using pure Python stdlib (`http.server`, `urllib`, `sqlite3`) eliminated deployment fragility, made Docker images unnecessary, and allowed sub-second cloud deploys.
5. **Never Compromise on In-Memory Redaction:** If your app captures desktop activity, sanitize *before* disk write. Once a secret hits an SQLite WAL file or a markdown note, it is permanently compromised.

---

### Resources & Stack Summary
* **Frontend:** React 18, TypeScript, Tailwind CSS, Vite, Lucide Icons
* **Desktop Runtime:** Tauri 2 (Rust)
* **Local Storage:** SQLite (rusqlite) + Local Markdown (Obsidian Vault)
* **AI Sidecar:** Python 3.11, FastAPI, Uvicorn, Sentence-Transformers (`all-MiniLM-L6-v2`)
* **Cloud & GraphRAG:** Supabase (PostgreSQL + pgvector), Render (Python Web Service)
* **Protocol:** Model Context Protocol (MCP 2024-11-05 JSON-RPC 2.0 & SSE)
* **Repository:** [Kaushik4141/TaskFlow](https://github.com/Kaushik4141/TaskFlow)
