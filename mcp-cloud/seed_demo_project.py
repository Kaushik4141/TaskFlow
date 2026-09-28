#!/usr/bin/env python3
"""seed_demo_project.py — Injects 3-day-old paused project into TaskFlow (SQLite, Vault, and Supabase)."""
import datetime
import json
import sqlite3
import sys
import uuid
from pathlib import Path

# 1. Resolve paths
DB_PATH = Path.home() / ".local/share/com.taskflow.desktop/taskflow.sqlite"
if not DB_PATH.exists():
    DB_PATH = Path("taskflow.sqlite")

VAULT_DIR = Path.home() / "Documents/Vault"
PROJECTS_DIR = VAULT_DIR / "TaskFlow/Projects"
PROJECTS_DIR.mkdir(parents=True, exist_ok=True)

project_slug = "PaymentRelay"
timestamp_3_days_ago_start = "2026-09-16T18:10:00Z"
timestamp_3_days_ago_end = "2026-09-16T18:22:00Z"
date_str = "2026-09-16"

rollup_id = str(uuid.uuid4())
task_id = str(uuid.uuid4())

summary_body = """The user worked on the PaymentRelay microservice, completing Stripe webhook HMAC-SHA256 signature verification and idempotency caching. The session was interrupted during the implementation of failure retry mechanisms.

#### Completed Milestones
- Verified Stripe webhook signatures in `src/webhook.py`.
- Built SQLite idempotency cache to eliminate duplicate payment processing.

#### Interrupted / Blockers
- **Unfinished:** `invoice.payment_failed` requires an exponential backoff retry worker with Redis/queue.
- **Pending Tests:** Need unit tests for timestamp replay attack prevention in `tests/test_replay.py`.

#### Immediate Next Action
Create `src/retry_worker.py` implementing `retry_payment_failure()` with 3-attempt exponential backoff, and write replay attack test cases in `tests/test_replay.py`."""

vault_markdown = f"""---
type: hub_page
kind: Projects
slug: {project_slug}
tags:
  - taskflow/project
last_touched: {date_str}
---

# Projects/{project_slug}

Daily captures related to this project.

## Recent Daily Notes
- [[Memory/Daily/{date_str}]] — Stripe Webhook Engine & Failure Retries; Ghostty; Cursor

## Activity Timeline
<!-- rollup:{rollup_id} -->
### {date_str} 18:10–18:22 — Stripe Webhook Signature Verification & Idempotency (Paused on Retry Worker)

> **Date**: [[Memory/Daily/{date_str}]] · **Tools**: [[Apps/cursor]], [[Apps/ghostty]], [[Apps/postman]]

#### Summary
{summary_body}
"""

# 2. Write Obsidian Vault note
vault_file = PROJECTS_DIR / f"{project_slug}.md"
vault_file.write_text(vault_markdown, encoding="utf-8")
print(f"✅ 1. Created Obsidian workstream note at: {vault_file}")

# 3. Write into local TaskFlow SQLite
con = sqlite3.connect(str(DB_PATH))
cur = con.cursor()

# Insert task
cur.execute("""
    INSERT OR REPLACE INTO tasks (id, title, description, source, status, created_at)
    VALUES (?, ?, ?, 'manual', 'completed', ?)
""", (task_id, "PaymentRelay Webhook & Retries", "Stripe payment retry microservice", timestamp_3_days_ago_start))

# Insert rollup
cur.execute("""
    INSERT OR REPLACE INTO rollups (id, task_id, window_start, window_end, title, summary_md, apps, workstream_slug, created_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
""", (
    rollup_id,
    task_id,
    timestamp_3_days_ago_start,
    timestamp_3_days_ago_end,
    "Stripe Webhook Signature Verification & Idempotency (Paused on Retry Worker)",
    summary_body,
    '["cursor", "ghostty", "postman"]',
    project_slug,
    timestamp_3_days_ago_end
))

# Insert graph edges
edges = [
    (project_slug, "cursor", "tools", 10.0, date_str, timestamp_3_days_ago_end),
    (project_slug, "ghostty", "tools", 10.0, date_str, timestamp_3_days_ago_end),
    (project_slug, "postman", "tools", 8.0, date_str, timestamp_3_days_ago_end),
    (project_slug, date_str, "daily", 15.0, date_str, timestamp_3_days_ago_end),
]

for src, tgt, rel, weight, tb, seen in edges:
    cur.execute("""
        INSERT INTO graph_edges (source_entity, target_entity, relation_type, weight, time_bucket, last_seen)
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(source_entity, target_entity, relation_type, time_bucket)
        DO UPDATE SET weight = weight + excluded.weight, last_seen = excluded.last_seen
    """, (src, tgt, rel, weight, tb, seen))

con.commit()
con.close()
print(f"✅ 2. Injected '{project_slug}' task, rollup, and graph edges into SQLite DB.")

# 4. Sync to Supabase Cloud Mirror
sys.path.insert(0, str(Path(__file__).parent))
try:
    import supabase_client
    print("⏳ 3. Triggering sync to Supabase Cloud Mirror...")
    report = supabase_client.sync_to_supabase()
    print(f"✅ 3. Supabase Cloud Mirror sync complete!")
    print(f"   - Vault notes uploaded: {report.get('vault_notes_uploaded', 0)}")
    print(f"   - Rollups uploaded: {report.get('rollups_uploaded', 0)}")
    print(f"   - Graph edges uploaded: {report.get('graph_edges_uploaded', 0)}")
    if report.get("errors"):
        print(f"   - Warnings/Errors: {report['errors']}")
except Exception as e:
    print(f"⚠️ Supabase sync exception: {e}")

print(f"\n🎉 Step 1 Complete! Project '{project_slug}' is seeded in Local Desktop + Vault + Cloud MCP.")
