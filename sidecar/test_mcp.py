#!/usr/bin/env python3
"""Test harness for TaskFlow MCP Server & GraphRAG Engine.

Validates that:
1. Direct Python tool dispatch (query_graph_memory, read_manifest, read_project, etc.) works.
2. The JSON-RPC 2.0 stdio MCP protocol engine operates cleanly without external pip packages.
"""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sidecar_dir = Path(__file__).parent.resolve()
if str(sidecar_dir) not in sys.path:
    sys.path.insert(0, str(sidecar_dir))

import mcp_server


def test_mcp_tools():
    print("=== Testing TaskFlow MCP Server Tools ===")

    # 1. Setup temporary test vault
    temp_dir = Path(tempfile.mkdtemp(prefix="taskflow_mcp_test_"))
    vault_dir = temp_dir / "TestVault"
    tf_root = vault_dir / "TaskFlow"
    projects_dir = tf_root / "Projects"
    memory_dir = tf_root / "Memory" / "Daily"
    monthly_dir = tf_root / "Memory" / "2026"

    projects_dir.mkdir(parents=True, exist_ok=True)
    memory_dir.mkdir(parents=True, exist_ok=True)
    monthly_dir.mkdir(parents=True, exist_ok=True)

    # Populate test project
    (projects_dir / "TaskFlow.md").write_text("""# Projects/TaskFlow

> [!abstract] Current Status
> **Latest Activity**: [2026-09-18] MCP Server & GraphRAG Engine
> **Active Tools**: [[Apps/cursor]], [[Apps/ghostty]]

## Activity Timeline
- 10:00 AM: Scope-first retrieval engine
- 11:30 AM: Graph edges table indexing
""", encoding="utf-8")

    # Populate test manifest
    (tf_root / "manifest.json").write_text(json.dumps({
        "version": 1,
        "lastUpdated": "2026-09-18",
        "routing": {
            "projects": ["TaskFlow", "AuthService"],
            "apps": ["cursor", "ghostty"],
            "monthlyArchives": ["2026/09"]
        }
    }), encoding="utf-8")

    # Populate test monthly digest
    (monthly_dir / "09.md").write_text("""---
type: monthly_digest
year: 2026
month: "09"
---
# September 2026 Memory Digest
- [[Projects/TaskFlow]] — 12 rollups
""", encoding="utf-8")

    # Populate test daily note
    import datetime
    today_str = datetime.date.today().isoformat()
    (memory_dir / f"{today_str}.md").write_text(f"""# Daily Notes for {today_str}

## Highlights
- Tested MCP GraphRAG integration
""", encoding="utf-8")

    os.environ["TASKFLOW_VAULT"] = str(vault_dir)

    try:
        # Test 1: list_projects
        print("\n[Test 1] Testing list_projects()...")
        projects = mcp_server.tool_list_projects()
        slugs = [p["slug"] for p in projects]
        assert "TaskFlow" in slugs, f"Expected 'TaskFlow' in {slugs}"
        print(f"  Passed! Projects found: {slugs}")

        # Test 2: read_manifest
        print("\n[Test 2] Testing read_manifest()...")
        manifest = mcp_server.tool_read_manifest()
        assert manifest.get("version") == 1
        assert "TaskFlow" in manifest["routing"]["projects"]
        print(f"  Passed! Manifest topology verified.")

        # Test 3: read_monthly_digest
        print("\n[Test 3] Testing read_monthly_digest('2026-09')...")
        digest = mcp_server.tool_read_monthly_digest("2026-09")
        assert digest.get("exists") is True
        assert "September 2026 Memory Digest" in digest["content"]
        print(f"  Passed! Monthly digest read successfully.")

        # Test 4: read_project (with line capping)
        print("\n[Test 4] Testing read_project('TaskFlow', max_lines=5)...")
        proj = mcp_server.tool_read_project("TaskFlow", max_lines=5)
        assert proj["slug"] == "TaskFlow"
        assert len(proj["content"].splitlines()) <= 5
        print(f"  Passed! Executive abstract read with line capping.")

        # Test 5: read_daily_note
        print("\n[Test 5] Testing read_daily_note('today')...")
        daily = mcp_server.tool_read_daily_note("today")
        assert daily["exists"] is True
        assert "Tested MCP GraphRAG integration" in daily["content"]
        print(f"  Passed! Daily note read successfully.")

        # Test 6: create_inbox_note
        print("\n[Test 6] Testing create_inbox_note()...")
        inbox = mcp_server.tool_create_inbox_note("Judge Finding", "Excellent latency and accuracy.")
        assert inbox["success"] is True
        assert Path(inbox["path"]).exists()
        print(f"  Passed! Inbox note safely written to {inbox['path']}.")

        # Test 7: query_graph_memory against live or test SQLite
        print("\n[Test 7] Testing query_graph_memory()...")
        res = mcp_server.tool_query_graph_memory(query="test", limit=3)
        assert "context_summary" in res
        print(f"  Passed! Scoped retrieval output synthesized successfully.")

    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)


def test_mcp_stdio_protocol():
    print("\n=== Testing MCP JSON-RPC 2.0 Stdio Protocol ===")
    server_script = str(sidecar_dir / "mcp_server.py")

    proc = subprocess.Popen(
        [sys.executable, server_script],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    try:
        # Step 1: Send 'initialize'
        init_req = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"clientInfo": {"name": "test-agent", "version": "1.0"}}
        }
        proc.stdin.write(json.dumps(init_req) + "\n")
        proc.stdin.flush()

        init_res = json.loads(proc.stdout.readline())
        assert init_res["id"] == 1
        assert init_res["result"]["serverInfo"]["name"] == "taskflow-brain"
        print("  [Pass] 'initialize' handshake completed.")

        # Step 2: Send 'tools/list'
        tools_req = {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}
        proc.stdin.write(json.dumps(tools_req) + "\n")
        proc.stdin.flush()

        tools_res = json.loads(proc.stdout.readline())
        assert tools_res["id"] == 2
        tool_names = [t["name"] for t in tools_res["result"]["tools"]]
        assert "query_graph_memory" in tool_names
        assert "read_manifest" in tool_names
        assert "read_monthly_digest" in tool_names
        print(f"  [Pass] 'tools/list' returned {len(tool_names)} tools: {tool_names[:4]}...")

        # Step 3: Send 'tools/call' for 'read_manifest'
        call_req = {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "read_manifest", "arguments": {}}
        }
        proc.stdin.write(json.dumps(call_req) + "\n")
        proc.stdin.flush()

        call_res = json.loads(proc.stdout.readline())
        assert call_res["id"] == 3
        assert "content" in call_res["result"]
        print("  [Pass] 'tools/call' executed and returned valid content block.")

    finally:
        proc.stdin.close()
        proc.terminate()
        proc.wait(timeout=2)


if __name__ == "__main__":
    test_mcp_tools()
    test_mcp_stdio_protocol()
    print("\n🎉 ALL TaskFlow MCP tests passed successfully!")
