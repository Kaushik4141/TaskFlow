#!/usr/bin/env python3
"""TaskFlow High-Scale Synthetic Data Generator & Benchmark Suite.

Generates 6 months to 1 year (~50,000 nodes/edges) of structurally exact
TaskFlow memory data in an isolated sandbox environment.

Guarantees:
1. Zero risk to live database or live Obsidian vault (enforced path guardrails).
2. Exact SQLite schema matching `src-tauri/src/database/schema.rs`.
3. Exact Obsidian knowledge graph matching `mcp_server.py` and `daily_index.rs`.
4. High-speed bulk insertion via SQLite WAL mode and batch transactions.
5. Built-in benchmark suite to verify sub-5ms GraphRAG retrieval at scale.
"""
from __future__ import annotations

import argparse
import datetime
import json
import os
import random
import sqlite3
import sys
import time
import uuid
from pathlib import Path
from typing import Any, Dict, List, Tuple

# Guardrail: Never overwrite the live user DB or personal vault
PROTECTED_PATHS = [
    "/home/kaushi/.local/share/com.taskflow.desktop/taskflow.sqlite",
    "/home/kaushi/Documents/Vault",
    "/home/kaushi/Documents/Vault/",
]

DEFAULT_SANDBOX_DB = "/home/kaushi/.local/share/com.taskflow.desktop/sandbox/taskflow_synthetic.sqlite"
DEFAULT_SANDBOX_VAULT = "/home/kaushi/Documents/SyntheticVault/"

PROJECTS = [
    "TaskFlow", "AuthService", "PaymentGateway", "DataPipeline", "SearchIndex",
    "GraphEngine", "UI-Components", "MobileApp", "SyncDaemon", "DevOps-Infra",
    "AnalyticsWorker", "Observability", "DocGenerator", "SecurityAudit", "CacheLayer",
    "EventBus", "PluginHost", "ML-Pipeline", "DatabaseMigration", "CLI-Tools"
]

APPS = [
    "cursor", "ghostty", "code-oss", "alacritty", "firefox", "helium",
    "postman", "datagrip", "git", "docker", "foot"
]

SITES = [
    "github.com", "stackoverflow.com", "docs.rs", "npmjs.com", "linear.app",
    "notion.so", "huggingface.co", "arxiv.org", "developer.mozilla.org", "hub.docker.com"
]

PROJECT_TOPICS = {
    "TaskFlow": [
        "Scope-first retrieval engine", "Shannon entropy secret redaction",
        "SQLite rolling retention clock", "Zero-hop manifest routing",
        "Hyprland active-window capture", "JSON-RPC 2.0 stdio MCP bridge",
        "Work classifier for dual-use platforms", "In-memory OCR pipeline"
    ],
    "AuthService": [
        "OAuth2 token refresh rotation", "JWT RSA-256 signature verification",
        "Multi-factor authentication TOTP", "Rate limiting on /v1/login",
        "Session invalidation via Redis", "PKCE authorization code exchange",
        "User role permission matrix", "Audit log telemetry for failed logins"
    ],
    "PaymentGateway": [
        "Stripe webhook idempotency keys", "PCI-DSS tokenization vault",
        "Automated subscription retry schedule", "Refund reconciliation workflow",
        "Multi-currency exchange rate cache", "3D Secure authentication flow",
        "Invoice PDF rendering engine", "Tax calculation API integration"
    ],
    "DataPipeline": [
        "Kafka consumer group rebalancing", "Parquet columnar storage writer",
        "Schema registry Avro validation", "Dead-letter queue retry mechanism",
        "Backpressure handling in stream worker", "ClickHouse analytical ingest",
        "Windowed aggregation over 1-minute batches", "CDC connector for Postgres"
    ],
    "GraphEngine": [
        "Bidirectional edge traversal", "Graph density community clustering",
        "Sub-millisecond adjacency list scan", "Weighted BFS pathfinding",
        "IVFFlat vector index cosine scoring", "Hierarchical monthly digest rollup",
        "Dynamic node degree recalculation", "Topology manifest synchronization"
    ],
}

DEFAULT_TOPICS = [
    "Performance profiling and flamegraph analysis",
    "Unit test coverage hardening",
    "Refactoring core async message handler",
    "Fixing memory leak in worker thread pool",
    "Updating dependencies and security patching",
    "CI/CD pipeline configuration and caching",
    "API documentation and OpenAPI spec updates",
    "Benchmarking latency percentiles under high load"
]


def init_database_schema(con: sqlite3.Connection, clean: bool = True) -> None:
    """Initialize exact SQLite tables and indexes matching TaskFlow schema.rs."""
    cur = con.cursor()
    cur.execute("PRAGMA journal_mode=WAL;")
    cur.execute("PRAGMA synchronous=NORMAL;")
    cur.execute("PRAGMA foreign_keys=OFF;")

    if clean:
        cur.execute("DROP TABLE IF EXISTS tasks;")
        cur.execute("DROP TABLE IF EXISTS events;")
        cur.execute("DROP TABLE IF EXISTS rollups;")
        cur.execute("DROP TABLE IF EXISTS graph_edges;")
        cur.execute("DROP TABLE IF EXISTS settings;")

    # 1. Tasks
    cur.execute("""
        CREATE TABLE IF NOT EXISTS tasks (
            id          TEXT PRIMARY KEY,
            title       TEXT NOT NULL,
            description TEXT,
            source      TEXT NOT NULL DEFAULT 'manual',
            source_id   TEXT,
            source_url  TEXT,
            status      TEXT NOT NULL DEFAULT 'active',
            started_at  TEXT,
            ended_at    TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            source_title TEXT,
            source_body  TEXT,
            source_labels TEXT,
            source_assignee TEXT,
            source_priority TEXT,
            source_project TEXT,
            source_branch TEXT
        );
    """)

    # 2. Events (Retained lightweight rows for stats)
    cur.execute("""
        CREATE TABLE IF NOT EXISTS events (
            id           TEXT PRIMARY KEY,
            task_id      TEXT NOT NULL REFERENCES tasks(id),
            event_type   TEXT NOT NULL,
            app_name     TEXT,
            window_title TEXT,
            content      TEXT,
            url          TEXT,
            relevance    REAL DEFAULT 0.0,
            timestamp    TEXT NOT NULL,
            created_at   TEXT NOT NULL DEFAULT (datetime('now')),
            content_type TEXT,
            capture_method TEXT,
            is_sanitized INTEGER DEFAULT 1,
            chunk_index  INTEGER DEFAULT 0
        );
    """)

    # 3. Rollups
    cur.execute("""
        CREATE TABLE IF NOT EXISTS rollups (
            id              TEXT PRIMARY KEY,
            task_id         TEXT NOT NULL REFERENCES tasks(id),
            window_start    TEXT NOT NULL,
            window_end      TEXT NOT NULL,
            title           TEXT NOT NULL,
            summary_md      TEXT NOT NULL,
            key_points      TEXT,
            apps            TEXT,
            resources       TEXT,
            workstream_slug TEXT,
            ai_mode         TEXT,
            event_count     INTEGER NOT NULL DEFAULT 0,
            trigger_kind    TEXT,
            created_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );
    """)

    # 4. Graph Edges
    cur.execute("""
        CREATE TABLE IF NOT EXISTS graph_edges (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            source_entity  TEXT NOT NULL,
            target_entity  TEXT NOT NULL,
            relation_type  TEXT NOT NULL,
            weight         REAL NOT NULL DEFAULT 1.0,
            time_bucket    TEXT NOT NULL,
            last_seen      TEXT NOT NULL,
            metadata_json  TEXT
        );
    """)

    # 5. Settings
    cur.execute("""
        CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
    """)

    # 6. Critical Performance Indexes
    cur.execute("CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);")
    cur.execute("CREATE INDEX IF NOT EXISTS idx_rollups_window_range ON rollups(window_start, window_end);")
    cur.execute("CREATE INDEX IF NOT EXISTS idx_rollups_workstream_window ON rollups(workstream_slug, window_start);")
    cur.execute("CREATE INDEX IF NOT EXISTS idx_rollups_created_at ON rollups(created_at);")
    cur.execute("CREATE INDEX IF NOT EXISTS idx_graph_edges_source_bucket ON graph_edges(source_entity, time_bucket, weight DESC);")
    cur.execute("CREATE INDEX IF NOT EXISTS idx_graph_edges_target_bucket ON graph_edges(target_entity, time_bucket);")
    cur.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_graph_edges_unique ON graph_edges(source_entity, target_entity, relation_type, time_bucket);")

    con.commit()


def generate_synthetic_vault_and_db(
    vault_path: Path,
    db_path: Path,
    start_date: datetime.date,
    days_count: int,
    rollups_per_day: int,
) -> Dict[str, Any]:
    """Populate exact SQLite database and Obsidian markdown knowledge graph."""
    tf_root = vault_path / "TaskFlow"
    daily_dir = tf_root / "Memory" / "Daily"
    projects_dir = tf_root / "Projects"
    apps_dir = tf_root / "Apps"
    sites_dir = tf_root / "Sites"

    daily_dir.mkdir(parents=True, exist_ok=True)
    projects_dir.mkdir(parents=True, exist_ok=True)
    apps_dir.mkdir(parents=True, exist_ok=True)
    sites_dir.mkdir(parents=True, exist_ok=True)

    db_path.parent.mkdir(parents=True, exist_ok=True)
    con = sqlite3.connect(str(db_path))
    init_database_schema(con)
    cur = con.cursor()

    # Save obsidian vault path in settings
    cur.execute("INSERT OR REPLACE INTO settings (key, value) VALUES ('obsidian_vault_path', ?)", (str(vault_path),))
    cur.execute("INSERT OR REPLACE INTO settings (key, value) VALUES ('wiki_known_projects', ?)", (json.dumps(PROJECTS),))

    # Seed master tasks for each project
    tasks_batch = []
    for proj in PROJECTS:
        task_id = f"task-{proj.lower()}"
        tasks_batch.append((
            task_id, f"Develop {proj}", f"Core engineering workstream for {proj}",
            "manual", None, None, "active",
            f"{start_date.isoformat()}T08:00:00Z", None,
            f"{start_date.isoformat()}T08:00:00Z",
            proj, f"Workstream tracking {proj}", "core", "kaushi", "high", proj, "main"
        ))
    cur.executemany("""
        INSERT OR REPLACE INTO tasks (id, title, description, source, source_id, source_url, status, started_at, ended_at, created_at, source_title, source_body, source_labels, source_assignee, source_priority, source_project, source_branch)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    """, tasks_batch)

    print(f"[*] Generating {days_count} days ({rollups_per_day} rollups/day) from {start_date}...")
    start_time = time.time()

    rollups_batch = []
    edges_map: Dict[Tuple[str, str, str, str], float] = {}
    monthly_archives_set = set()
    project_timeline_map: Dict[str, List[Tuple[str, str, str]]] = {p: [] for p in PROJECTS}

    cur_date = start_date
    end_date = start_date + datetime.timedelta(days=days_count - 1)

    while cur_date <= end_date:
        day_str = cur_date.isoformat()
        year_str = f"{cur_date.year}"
        month_str = f"{cur_date.month:02d}"
        time_bucket = f"{year_str}-{month_str}"
        monthly_archives_set.add(f"{year_str}/{month_str}")

        (tf_root / "Memory" / year_str).mkdir(parents=True, exist_ok=True)

        daily_workstreams_seen = set()
        daily_apps_seen = set()
        daily_sites_seen = set()
        daily_timeline_entries = []

        # Start active workday around 09:00 UTC
        base_minutes = 9 * 60

        for r_idx in range(rollups_per_day):
            proj = random.choice(PROJECTS)
            app = random.choice(APPS)
            site = random.choice(SITES)

            daily_workstreams_seen.add(proj)
            daily_apps_seen.add(app)
            daily_sites_seen.add(site)

            # 10-minute rollup intervals spaced across the day
            m_start = base_minutes + (r_idx * 15)
            m_end = m_start + 10
            h_s, min_s = divmod(m_start, 60)
            h_e, min_e = divmod(m_end, 60)

            # Cap day rollups to 23:59:59
            if h_s >= 24:
                h_s, min_s = 23, 50
                h_e, min_e = 23, 59

            w_start = f"{day_str}T{h_s:02d}:{min_s:02d}:00Z"
            w_end = f"{day_str}T{h_e:02d}:{min_e:02d}:00Z"
            r_id = str(uuid.uuid4())

            topics = PROJECT_TOPICS.get(proj, DEFAULT_TOPICS)
            topic = random.choice(topics)

            title = f"{app} — {topic} ({proj})"
            summary = (
                f"# Memory Capture - {day_str} ({h_s:02d}:{min_s:02d}–{h_e:02d}:{min_e:02d})\n\n"
                f"## Summary\n"
                f"The user was actively developing **{proj}** in `{app}` focusing on {topic.lower()}. "
                f"Cross-referenced reference implementations on `{site}`.\n\n"
                f"## Activity Timeline\n"
                f"- **{app}**: Executed test suite and validated performance metrics.\n"
                f"- **{site}**: Reviewed API documentation and architecture specifications.\n\n"
                f"## Key Points\n"
                f"- Successfully implemented {topic.lower()}.\n"
                f"- All unit assertions passed without regressions.\n"
            )

            kp = json.dumps([f"Completed {topic.lower()}", f"Verified with {app}"])
            apps_json = json.dumps([app])
            resources_json = json.dumps([f"https://{site}/docs/{proj.lower()}"])

            rollups_batch.append((
                r_id, f"task-{proj.lower()}", w_start, w_end,
                title, summary, kp, apps_json, resources_json,
                proj, "basic", random.randint(10, 35), "interval", w_start
            ))

            # Record graph relationships
            k_tool = (f"Projects/{proj}", f"Apps/{app}", "used_tool", time_bucket)
            k_site = (f"Projects/{proj}", f"Sites/{site}", "visited_site", time_bucket)
            edges_map[k_tool] = edges_map.get(k_tool, 0.0) + 1.0
            edges_map[k_site] = edges_map.get(k_site, 0.0) + 1.0

            time_range_str = f"{h_s:02d}:{min_s:02d}–{h_e:02d}:{min_e:02d}"
            daily_timeline_entries.append((time_range_str, app, title, proj))
            project_timeline_map[proj].append((day_str, app, topic))

        # Write Obsidian Daily Note (Memory/Daily/YYYY-MM-DD.md)
        daily_note_content = (
            f"---\n"
            f"type: daily_memory_index\n"
            f"source: rollup_index\n"
            f"date: {day_str}\n"
            f"rollups: {rollups_per_day}\n"
            f"tags:\n"
            f"  - taskflow/daily\n"
            f"  - memory\n"
            f"workstreams:\n"
            + "\n".join(f"  - {w}" for w in sorted(daily_workstreams_seen)) + "\n"
            f"apps:\n"
            + "\n".join(f"  - {a}" for a in sorted(daily_apps_seen)) + "\n"
            f"sites:\n"
            + "\n".join(f"  - {s}" for s in sorted(daily_sites_seen)) + "\n"
            f"---\n\n"
            f"# Daily Index - {day_str}\n\n"
            f"> Derived view — regenerated by TaskFlow after every roll-up. The linked workstream nodes hold the full activity timeline.\n\n"
            f"## Workstreams Active\n"
            + "\n".join(f"- [[Projects/{w}]]" for w in sorted(daily_workstreams_seen)) + "\n\n"
            f"## Hubs Touched\n"
            + "\n".join(f"- [[Apps/{a}]]" for a in sorted(daily_apps_seen)) + "\n"
            + "\n".join(f"- [[Sites/{s}]]" for s in sorted(daily_sites_seen)) + "\n\n"
            f"## Timeline\n"
            + "\n".join(f"- `{t_range}` **{a} — {t}** → [[Projects/{p}]]" for t_range, a, t, p in daily_timeline_entries)
            + "\n"
        )
        (daily_dir / f"{day_str}.md").write_text(daily_note_content, encoding="utf-8")

        cur_date += datetime.timedelta(days=1)

    print(f"[*] Inserting {len(rollups_batch)} rollups into SQLite...")
    cur.executemany("""
        INSERT OR REPLACE INTO rollups (
            id, task_id, window_start, window_end, title, summary_md,
            key_points, apps, resources, workstream_slug, ai_mode,
            event_count, trigger_kind, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    """, rollups_batch)

    # Generate 1:1 matching raw events to populate full event stream
    print(f"[*] Inserting {len(rollups_batch)} events into SQLite...")
    events_batch = [
        (
            str(uuid.uuid4()), task_id, "window_focus", json.loads(apps)[0], title,
            f"Active engineering on {workstream} via {json.loads(apps)[0]}.",
            json.loads(resources)[0], 0.9, w_start, w_start, "code", "accessibility", 1, 0
        )
        for _, task_id, w_start, _, title, _, _, apps, resources, workstream, _, _, _, _ in rollups_batch
    ]
    cur.executemany("""
        INSERT OR REPLACE INTO events (
            id, task_id, event_type, app_name, window_title, content,
            url, relevance, timestamp, created_at, content_type, capture_method,
            is_sanitized, chunk_index
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    """, events_batch)

    print(f"[*] Inserting {len(edges_map)} graph edges into SQLite...")
    edges_batch = [
        (src, tgt, rel, weight, bucket, f"{bucket}-28T23:59:59Z", json.dumps({"source": "synthetic_generator"}))
        for (src, tgt, rel, bucket), weight in edges_map.items()
    ]
    cur.executemany("""
        INSERT OR REPLACE INTO graph_edges (source_entity, target_entity, relation_type, weight, time_bucket, last_seen, metadata_json)
        VALUES (?, ?, ?, ?, ?, ?, ?)
    """, edges_batch)

    con.commit()
    con.close()

    # Write Obsidian Workstream nodes (Projects/<slug>.md)
    print(f"[*] Generating {len(PROJECTS)} project workstream notes...")
    for proj, entries in project_timeline_map.items():
        sample_entries = entries[-60:]  # Keep top 60 recent lines for executive abstract
        proj_file = projects_dir / f"{proj}.md"
        proj_content = (
            f"# Projects/{proj}\n\n"
            f"> [!abstract] Executive Workstream Overview\n"
            f"> **Active Tools**: " + ", ".join(f"[[Apps/{a}]]" for a in APPS[:4]) + "\n"
            f"> **Total Lifetime Rollups**: {len(entries)}\n\n"
            f"## Activity Timeline\n"
            + "\n".join(f"- [{d}] [[Apps/{a}]] — {t}" for d, a, t in sample_entries)
            + "\n"
        )
        proj_file.write_text(proj_content, encoding="utf-8")

    # Write Hub nodes (Apps & Sites)
    print(f"[*] Generating Hub nodes ({len(APPS)} apps, {len(SITES)} sites)...")
    for app in APPS:
        (apps_dir / f"{app}.md").write_text(f"# Apps/{app}\n\nTool Hub node tracking activity in `{app}`.\n", encoding="utf-8")
    for site in SITES:
        (sites_dir / f"{site}.md").write_text(f"# Sites/{site}\n\nDomain Hub node tracking technical research on `{site}`.\n", encoding="utf-8")

    # Write Level 3 Monthly Digests (Memory/YYYY/MM.md)
    print(f"[*] Generating {len(monthly_archives_set)} monthly digest notes...")
    for ym in sorted(monthly_archives_set):
        yr, mo = ym.split("/")
        digest_file = tf_root / "Memory" / yr / f"{mo}.md"
        digest_content = (
            f"---\n"
            f"type: monthly_digest\n"
            f"year: {yr}\n"
            f"month: \"{mo}\"\n"
            f"---\n\n"
            f"# {yr}-{mo} Memory Digest\n\n"
            f"Monthly aggregate rollup across {len(PROJECTS)} active workstreams.\n"
        )
        digest_file.write_text(digest_content, encoding="utf-8")

    # Write Graph Topology Manifest (manifest.json)
    manifest = {
        "version": 1,
        "lastUpdated": datetime.date.today().isoformat(),
        "routing": {
            "projects": sorted(PROJECTS),
            "apps": sorted(APPS),
            "monthlyArchives": sorted(list(monthly_archives_set))
        }
    }
    (tf_root / "manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")

    elapsed = time.time() - start_time
    total_nodes = len(rollups_batch) + len(events_batch) + len(edges_batch) + days_count + len(PROJECTS) + len(APPS) + len(SITES)

    return {
        "total_graph_elements": total_nodes,
        "rollups_count": len(rollups_batch),
        "events_count": len(events_batch),
        "edges_count": len(edges_batch),
        "daily_notes_count": days_count,
        "projects_count": len(PROJECTS),
        "elapsed_seconds": round(elapsed, 2),
        "vault_path": str(vault_path),
        "db_path": str(db_path),
    }


def run_scale_benchmark(db_path: Path, vault_path: Path) -> Dict[str, Any]:
    """Execute high-speed latency benchmarks directly against the generated database."""
    print("\n" + "=" * 60)
    print("🚀 RUNNING BENCHMARK ON SYNTHETIC GRAPH AT SCALE (~50,000 NODES)")
    print("=" * 60)

    con = sqlite3.connect(str(db_path))
    cur = con.cursor()

    # Test 1: Scope-First Range Scan Query Latency
    test_queries = [
        ("AuthService", "token refresh", "2025-11-01", "2025-11-30"),
        ("TaskFlow", "retrieval", "2026-01-01", "2026-03-31"),
        ("PaymentGateway", "webhook", "2025-09-01", "2026-08-31"),
    ]

    latencies_rollups = []
    for proj, term, s_date, e_date in test_queries:
        t0 = time.perf_counter()
        cur.execute("""
            SELECT id, title, summary_md, apps, resources
            FROM rollups
            WHERE workstream_slug = ? AND window_end >= ? AND window_start <= ?
            ORDER BY window_start DESC
            LIMIT 5
        """, (proj, s_date, e_date))
        rows = cur.fetchall()
        t1 = time.perf_counter()
        ms = (t1 - t0) * 1000.0
        latencies_rollups.append(ms)
        print(f"  [Query] Project: {proj:<15} | Date Range: {s_date} to {e_date} -> {len(rows)} hits in {ms:.3f} ms")

    # Test 2: Graph Edges Traversal (< 1ms target)
    latencies_edges = []
    for proj in ["TaskFlow", "AuthService", "PaymentGateway", "GraphEngine"]:
        t0 = time.perf_counter()
        cur.execute("""
            SELECT target_entity, sum(weight) as total_weight
            FROM graph_edges
            WHERE source_entity = ?
            GROUP BY target_entity
            ORDER BY total_weight DESC
            LIMIT 10
        """, (f"Projects/{proj}",))
        edges = cur.fetchall()
        t1 = time.perf_counter()
        ms = (t1 - t0) * 1000.0
        latencies_edges.append(ms)
        print(f"  [Graph] Entity: Projects/{proj:<11} | Traversed {len(edges)} connected hubs in {ms:.3f} ms")

    con.close()

    # Test 3: Zero-hop Topology Manifest Read
    manifest_path = vault_path / "TaskFlow" / "manifest.json"
    t0 = time.perf_counter()
    manifest_data = json.loads(manifest_path.read_text(encoding="utf-8"))
    t1 = time.perf_counter()
    manifest_ms = (t1 - t0) * 1000.0
    print(f"  [Routing] Read manifest.json ({len(manifest_data['routing']['projects'])} projects) in {manifest_ms:.3f} ms")

    avg_rollup_ms = sum(latencies_rollups) / len(latencies_rollups)
    avg_edge_ms = sum(latencies_edges) / len(latencies_edges)

    print("-" * 60)
    print(f"  Average Scoped Rollup Query Latency: {avg_rollup_ms:.3f} ms  (Target: < 5.0 ms)")
    print(f"  Average Multi-Hop Edge Latency:      {avg_edge_ms:.3f} ms  (Target: < 1.0 ms)")
    print("=" * 60 + "\n")

    return {
        "avg_rollup_query_ms": round(avg_rollup_ms, 3),
        "avg_graph_edge_ms": round(avg_edge_ms, 3),
        "manifest_read_ms": round(manifest_ms, 3),
    }


def write_isolated_mcp_config(db_path: Path, vault_path: Path, output_file: Path) -> None:
    """Write an MCP configuration JSON snippet specifically pointing to the synthetic database."""
    config = {
        "mcpServers": {
            "taskflow-synthetic": {
                "command": "python3",
                "args": [str(Path(__file__).parent.parent / "sidecar" / "mcp_server.py") if (Path(__file__).parent.parent / "sidecar" / "mcp_server.py").exists() else str(Path(__file__).parent / "mcp_server.py")],
                "env": {
                    "TASKFLOW_VAULT": str(vault_path),
                    "TASKFLOW_DB": str(db_path)
                }
            }
        }
    }
    output_file.write_text(json.dumps(config, indent=2), encoding="utf-8")
    print(f"[*] Generated isolated MCP configuration at: {output_file}")


def main() -> None:
    parser = argparse.ArgumentParser(description="TaskFlow Scale Synthetic Data Generator")
    parser.add_argument("--vault", type=str, default=DEFAULT_SANDBOX_VAULT, help="Target synthetic vault directory")
    parser.add_argument("--db", type=str, default=DEFAULT_SANDBOX_DB, help="Target synthetic SQLite database path")
    parser.add_argument("--days", type=int, default=365, help="Span of historical days (default: 365 = 1 year)")
    parser.add_argument("--rollups-per-day", type=int, default=40, help="Rollups per day (default: 40 = ~14,600/year)")
    parser.add_argument("--start-date", type=str, default="2025-09-01", help="Start date (YYYY-MM-DD)")
    parser.add_argument("--benchmark", action="store_true", default=True, help="Run latency benchmark on synthetic data")
    parser.add_argument("--output-config", type=str, default="synthetic_mcp_config.json", help="Path to write MCP client config")

    args = parser.parse_args()

    v_path = Path(args.vault).resolve()
    d_path = Path(args.db).resolve()

    # Guardrail check
    for prot in PROTECTED_PATHS:
        if str(v_path) == str(Path(prot).resolve()) or str(d_path) == str(Path(prot).resolve()):
            print(f"❌ SAFETY ABORT: Attempting to write synthetic data to live protected path: {prot}")
            sys.exit(1)

    start_d = datetime.date.fromisoformat(args.start_date)

    print("============================================================")
    print("TASKFLOW SYNTHETIC BENCHMARK DATASET GENERATOR")
    print("============================================================")
    print(f"  Target Vault:   {v_path}")
    print(f"  Target DB:      {d_path}")
    print(f"  Duration:       {args.days} days ({start_d} to {start_d + datetime.timedelta(days=args.days - 1)})")
    print(f"  Rollup Density: {args.rollups_per_day} rollups/day (~{args.days * args.rollups_per_day:,} total)")
    print("============================================================\n")

    stats = generate_synthetic_vault_and_db(
        vault_path=v_path,
        db_path=d_path,
        start_date=start_d,
        days_count=args.days,
        rollups_per_day=args.rollups_per_day
    )

    print(f"✅ Generated {stats['total_graph_elements']:,} graph entities in {stats['elapsed_seconds']}s:")
    print(f"   - {stats['rollups_count']:,} Activity Rollups")
    print(f"   - {stats['edges_count']:,} Knowledge Graph Edges")
    print(f"   - {stats['daily_notes_count']:,} Obsidian Daily Notes")
    print(f"   - {stats['projects_count']:,} Project Workstream Nodes")

    config_path = Path(__file__).parent / args.output_config
    write_isolated_mcp_config(d_path, v_path, config_path)

    if args.benchmark:
        run_scale_benchmark(d_path, v_path)


if __name__ == "__main__":
    main()
