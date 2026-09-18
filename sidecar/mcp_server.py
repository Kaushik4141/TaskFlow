#!/usr/bin/env python3
"""TaskFlow MCP Server — Model Context Protocol bridge for TaskFlow & Obsidian.

Enables external AI agents (Hermes, OpenClaw, Claude Desktop, Cursor) to interact
with the TaskFlow Obsidian vault, workstreams, daily notes, and SQLite memory engine.

Supports:
- stdio transport (default): for local agents running as subprocesses
- sse transport: for network/remote agents (Hermes, OpenClaw) over HTTP/SSE

Usage:
    python mcp_server.py                        # runs stdio transport
    python mcp_server.py --transport sse --port 8765  # runs SSE on http://127.0.0.1:8765
"""
from __future__ import annotations

import argparse
import datetime
import json
import os
import re
import sqlite3
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional

from mcp.server.mcpserver import MCPServer
import supabase_client

# Initialize MCP Server
server = MCPServer("taskflow-brain")



def get_default_db_path() -> Path:
    """Resolve the default TaskFlow SQLite database path."""
    if "TASKFLOW_DB" in os.environ:
        return Path(os.environ["TASKFLOW_DB"])
    if sys.platform == "win32":
        appdata = os.environ.get("APPDATA")
        if appdata:
            return Path(appdata) / "com.taskflow.desktop" / "taskflow.sqlite"
    else:
        home = Path.home()
        return home / ".local" / "share" / "com.taskflow.desktop" / "taskflow.sqlite"
    return Path("taskflow.sqlite")


def get_vault_path() -> Optional[Path]:
    """Resolve the Obsidian vault path from environment or TaskFlow SQLite settings."""
    if "TASKFLOW_VAULT" in os.environ:
        vault = Path(os.environ["TASKFLOW_VAULT"])
        if vault.exists():
            return vault

    db_path = get_default_db_path()
    if db_path.exists():
        try:
            con = sqlite3.connect(str(db_path))
            cur = con.cursor()
            cur.execute("SELECT value FROM settings WHERE key = 'obsidian_vault_path'")
            row = cur.fetchone()
            con.close()
            if row and row[0]:
                vault = Path(row[0])
                if vault.exists():
                    return vault
        except Exception:
            pass

    return None


def get_taskflow_root() -> Optional[Path]:
    """Resolve the root TaskFlow folder within the Obsidian vault (<vault>/TaskFlow)."""
    vault = get_vault_path()
    if not vault:
        return None
    tf_root = vault / "TaskFlow"
    return tf_root if tf_root.exists() else vault


def sanitize_filename(name: str) -> str:
    """Sanitize string to a safe filename."""
    name = re.sub(r'[\\/*?:"<>|]', "", name)
    name = re.sub(r"\s+", "-", name.strip())
    return name.lower() or "untitled"


# =====================================================================
# Tools: Vault Reading
# =====================================================================

@server.tool()
def list_projects() -> List[Dict[str, Any]]:
    """List all tracked projects and workstreams in the TaskFlow vault and SQLite database.
    
    Returns a list of project summaries with slug, title, file path, and last modified timestamp.
    """
    projects = []
    seen_slugs = set()
    tf_root = get_taskflow_root()

    # 1. Check <vault>/TaskFlow/Projects/*.md
    if tf_root and (tf_root / "Projects").exists():
        for file in (tf_root / "Projects").glob("*.md"):
            slug = file.stem
            seen_slugs.add(slug.lower())
            stat = file.stat()
            projects.append({
                "slug": slug,
                "title": slug.replace("-", " ").title(),
                "path": str(file),
                "modified_at": datetime.datetime.fromtimestamp(stat.st_mtime).isoformat(),
                "size_bytes": stat.st_size,
                "source": "vault"
            })

    # 2. Check SQLite database for recent workstream slugs
    db_path = get_default_db_path()
    if db_path.exists():
        try:
            con = sqlite3.connect(str(db_path))
            cur = con.cursor()
            cur.execute("""
                SELECT DISTINCT workstream_slug, max(created_at) as last_seen 
                FROM rollups 
                WHERE workstream_slug IS NOT NULL AND workstream_slug != ''
                GROUP BY workstream_slug
            """)
            for slug, last_seen in cur.fetchall():
                if slug.lower() not in seen_slugs:
                    seen_slugs.add(slug.lower())
                    projects.append({
                        "slug": slug,
                        "title": slug.replace("-", " ").title(),
                        "path": None,
                        "modified_at": last_seen,
                        "size_bytes": 0,
                        "source": "sqlite"
                    })
            con.close()
        except Exception:
            pass

    return sorted(projects, key=lambda x: x["slug"])


@server.tool()
def read_project(project_slug: str) -> Dict[str, Any]:
    """Read the full workstream document and activity timeline for a project.
    
    Args:
        project_slug: The identifier/slug of the project (e.g. 'taskflow' or 'website').
    """
    tf_root = get_taskflow_root()
    if not tf_root:
        return {"error": "Obsidian vault path not configured. Set TASKFLOW_VAULT or configure in TaskFlow settings."}

    proj_dir = tf_root / "Projects"
    candidates = list(proj_dir.glob(f"{project_slug}.md")) if proj_dir.exists() else []
    if not candidates and proj_dir.exists():
        # Try case-insensitive match
        for f in proj_dir.glob("*.md"):
            if f.stem.lower() == project_slug.lower():
                candidates = [f]
                break

    if not candidates or not candidates[0].exists():
        return {"error": f"Project '{project_slug}' not found in vault under {proj_dir}."}

    file_path = candidates[0]
    content = file_path.read_text(encoding="utf-8", errors="replace")
    stat = file_path.stat()

    return {
        "slug": file_path.stem,
        "path": str(file_path),
        "modified_at": datetime.datetime.fromtimestamp(stat.st_mtime).isoformat(),
        "content": content
    }


@server.tool()
def read_daily_note(date: str = "today") -> Dict[str, Any]:
    """Read the synthesized daily activity note from the vault.
    
    Args:
        date: 'today', 'yesterday', or a specific date in 'YYYY-MM-DD' format.
    """
    tf_root = get_taskflow_root()
    if not tf_root:
        return {"error": "Obsidian vault path not configured."}

    target_date = datetime.date.today()
    if date.lower() == "today":
        pass
    elif date.lower() == "yesterday":
        target_date -= datetime.timedelta(days=1)
    else:
        try:
            target_date = datetime.date.fromisoformat(date)
        except ValueError:
            return {"error": f"Invalid date format '{date}'. Expected 'YYYY-MM-DD', 'today', or 'yesterday'."}

    date_str = target_date.isoformat()
    note_path = tf_root / "Memory" / "Daily" / f"{date_str}.md"

    if not note_path.exists():
        return {
            "date": date_str,
            "exists": False,
            "message": f"No daily note found for {date_str} at {note_path}."
        }

    content = note_path.read_text(encoding="utf-8", errors="replace")
    return {
        "date": date_str,
        "exists": True,
        "path": str(note_path),
        "content": content
    }


@server.tool()
def list_vault_tree(subfolder: str = "") -> Dict[str, Any]:
    """Get the file hierarchy of the TaskFlow Obsidian vault.
    
    Args:
        subfolder: Optional subfolder to restrict listing (e.g. 'Projects', 'Memory/Daily', 'Apps').
    """
    tf_root = get_taskflow_root()
    if not tf_root:
        return {"error": "Obsidian vault path not configured."}

    base_dir = tf_root / subfolder if subfolder else tf_root
    if not base_dir.exists() or not base_dir.is_dir():
        return {"error": f"Directory '{subfolder}' not found in vault."}

    files_list = []
    for root, _, files in os.walk(base_dir):
        for file in files:
            if file.endswith(".md"):
                full_p = Path(root) / file
                rel_p = full_p.relative_to(tf_root)
                stat = full_p.stat()
                files_list.append({
                    "relative_path": str(rel_p).replace("\\", "/"),
                    "name": file,
                    "size_bytes": stat.st_size,
                    "modified_at": datetime.datetime.fromtimestamp(stat.st_mtime).isoformat()
                })

    return {
        "vault_root": str(tf_root),
        "file_count": len(files_list),
        "files": sorted(files_list, key=lambda x: x["relative_path"])
    }


# =====================================================================
# Tools: TaskFlow Memory Engine & Activity Rollups
# =====================================================================

@server.tool()
def get_recent_activity(hours: int = 4, limit: int = 20) -> List[Dict[str, Any]]:
    """Retrieve recent aggregated work activity rollups directly from the TaskFlow SQLite database.
    
    Args:
        hours: How many hours of past activity to retrieve (default: 4).
        limit: Maximum number of rollup events to return (default: 20).
    """
    db_path = get_default_db_path()
    if not db_path.exists():
        return [{"error": f"TaskFlow SQLite database not found at {db_path}."}]

    cutoff = (datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(hours=hours)).isoformat()

    results = []
    try:
        con = sqlite3.connect(str(db_path))
        con.row_factory = sqlite3.Row
        cur = con.cursor()
        cur.execute("""
            SELECT id, task_id, window_start, window_end, title, summary_md, key_points,
                   apps, workstream_slug, event_count, created_at
            FROM rollups
            WHERE created_at >= ? OR window_start >= ?
            ORDER BY window_start DESC
            LIMIT ?
        """, (cutoff, cutoff, limit))

        for row in cur.fetchall():
            item = dict(row)
            # Parse JSON key_points if available
            if item.get("key_points"):
                try:
                    item["key_points"] = json.loads(item["key_points"])
                except Exception:
                    pass
            # Parse apps if JSON
            if item.get("apps"):
                try:
                    item["apps"] = json.loads(item["apps"])
                except Exception:
                    pass
            results.append(item)
        con.close()
    except Exception as e:
        return [{"error": f"Database query failed: {e}"}]

    return results


# =====================================================================
# Tools: Semantic & Keyword Search
# =====================================================================

@server.tool()
def search_vault(query: str, semantic: bool = True, limit: int = 5) -> List[Dict[str, Any]]:
    """Search through the Obsidian vault using semantic vector similarity or keyword matching.
    
    Args:
        query: The search term or natural language question (e.g. 'Windows linker PDB limit').
        semantic: If True, uses local vector embeddings (SentenceTransformers); if False, keyword match.
        limit: Maximum number of results to return (default: 5).
    """
    tf_root = get_taskflow_root()
    if not tf_root:
        return [{"error": "Obsidian vault path not configured."}]

    # Collect markdown files
    notes = []
    for file in tf_root.rglob("*.md"):
        try:
            text = file.read_text(encoding="utf-8", errors="replace")
            rel_p = str(file.relative_to(tf_root)).replace("\\", "/")
            notes.append((rel_p, file.stem, text))
        except Exception:
            continue

    if not notes:
        return []

    # Semantic search with Embedder if requested
    if semantic:
        try:
            from embedder import Embedder
            emb = Embedder()
            query_vec = emb.embed(query)

            scored = []
            for rel_path, title, text in notes:
                snippet = text[:1000]
                doc_vec = emb.embed(snippet)
                score = emb.cosine_similarity(query_vec, doc_vec)
                scored.append({
                    "path": rel_path,
                    "title": title,
                    "score": round(score, 4),
                    "snippet": snippet[:300] + ("..." if len(snippet) > 300 else "")
                })

            scored.sort(key=lambda x: x["score"], reverse=True)
            return scored[:limit]
        except Exception:
            pass

    # Keyword search fallback
    q_lower = query.lower()
    keyword_matches = []
    for rel_path, title, text in notes:
        idx = text.lower().find(q_lower)
        if idx != -1:
            start = max(0, idx - 80)
            end = min(len(text), idx + len(query) + 120)
            snippet = text[start:end].strip()
            keyword_matches.append({
                "path": rel_path,
                "title": title,
                "score": 1.0 if q_lower in title.lower() else 0.5,
                "snippet": f"...{snippet}..."
            })

    keyword_matches.sort(key=lambda x: x["score"], reverse=True)
    return keyword_matches[:limit]


# =====================================================================
# Tools: Agent Safe Writing & Actions
# =====================================================================

@server.tool()
def create_inbox_note(title: str, content: str, tags: Optional[List[str]] = None) -> Dict[str, Any]:
    """Safely create a new note in the vault under TaskFlow/Inbox/.
    
    Guards ensure agents can only write within the Inbox sandbox.
    
    Args:
        title: Title of the note (will be sanitized to a safe filename).
        content: The Markdown body content.
        tags: Optional list of tags to place in the frontmatter.
    """
    tf_root = get_taskflow_root()
    if not tf_root:
        return {"error": "Obsidian vault path not configured."}

    inbox_dir = tf_root / "Inbox"
    inbox_dir.mkdir(parents=True, exist_ok=True)

    safe_title = sanitize_filename(title)
    file_path = inbox_dir / f"{safe_title}.md"

    # Avoid overwriting existing note; append timestamp suffix if needed
    if file_path.exists():
        timestamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
        file_path = inbox_dir / f"{safe_title}-{timestamp}.md"

    # Assemble YAML frontmatter
    tag_list = tags or ["agent-note"]
    if "agent-note" not in tag_list:
        tag_list.append("agent-note")

    now_iso = datetime.datetime.now().isoformat()
    doc = f"""---
title: "{title}"
created_at: {now_iso}
tags: [{', '.join(tag_list)}]
author: agent
---

{content}
"""
    file_path.write_text(doc, encoding="utf-8")

    return {
        "success": True,
        "path": str(file_path),
        "title": title,
        "created_at": now_iso
    }


@server.tool()
def append_project_note(project_slug: str, note: str) -> Dict[str, Any]:
    """Append a note or finding under the '## Agent Notes' section of an existing project node.
    
    Guards ensure this never modifies or corrupts the automated '## Activity Timeline' section.
    
    Args:
        project_slug: The project slug (e.g. 'taskflow').
        note: The note content to append.
    """
    tf_root = get_taskflow_root()
    if not tf_root:
        return {"error": "Obsidian vault path not configured."}

    proj_file = tf_root / "Projects" / f"{project_slug}.md"
    if not proj_file.exists():
        return {"error": f"Project file '{proj_file}' does not exist."}

    content = proj_file.read_text(encoding="utf-8", errors="replace")
    timestamp = datetime.datetime.now().strftime("%Y-%m-%d %H:%M")
    entry = f"\n- **[{timestamp}] Agent Note**: {note}\n"

    if "## Agent Notes" in content:
        # Append right after ## Agent Notes header
        idx = content.find("## Agent Notes") + len("## Agent Notes")
        new_content = content[:idx] + "\n" + entry + content[idx:]
    else:
        # Add ## Agent Notes at the end of the file
        new_content = content.rstrip() + "\n\n## Agent Notes\n" + entry

    proj_file.write_text(new_content, encoding="utf-8")

    return {
        "success": True,
        "project_slug": project_slug,
        "appended_at": timestamp
    }


# =====================================================================
# Tools: Supabase Cloud Sync & Remote Query
# =====================================================================

@server.tool()
def sync_vault_to_supabase() -> Dict[str, Any]:
    """Synchronize local Obsidian vault notes and vector embeddings into Supabase.
    
    Uploads notes into the 'vault_notes' table with full-text content, frontmatter,
    and 384-dimensional semantic embeddings (SentenceTransformers).
    """
    tf_root = get_taskflow_root()
    if not tf_root:
        return {"success": False, "error": "Obsidian vault path not configured."}

    if not supabase_client.is_supabase_configured():
        return {
            "success": False,
            "error": "Supabase not configured. Set SUPABASE_URL and SUPABASE_KEY in environment or .env file."
        }

    return supabase_client.sync_vault_to_supabase(tf_root)


@server.tool()
def query_cloud_vault(query: str, semantic: bool = True, limit: int = 5) -> List[Dict[str, Any]]:
    """Query the always-on Supabase cloud vault using semantic vector similarity or keyword filtering.
    
    Allows external agents to query the user's knowledge base even when the local laptop is off.
    
    Args:
        query: Natural language query or search keyword.
        semantic: If True, calls the 'search_vault_notes' vector similarity RPC function.
        limit: Maximum number of matching notes to return (default: 5).
    """
    if not supabase_client.is_supabase_configured():
        return [{"error": "Supabase not configured. Set SUPABASE_URL and SUPABASE_KEY."}]

    return supabase_client.query_cloud_vault(query=query, semantic=semantic, limit=limit)


# =====================================================================
# Resources (MCP Standard)
# =====================================================================

@server.resource("vault://daily/today")
def resource_daily_today() -> str:
    """Live resource returning today's daily activity note content."""
    res = read_daily_note("today")
    return res.get("content", f"No daily note for today ({res.get('date')}).")


@server.resource("vault://projects")
def resource_projects() -> str:
    """Live resource returning the list of active projects."""
    projects = list_projects()
    return json.dumps(projects, indent=2)


@server.resource("taskflow://activity/recent")
def resource_recent_activity() -> str:
    """Live resource returning the last 4 hours of rollups."""
    rollups = get_recent_activity(hours=4, limit=10)
    return json.dumps(rollups, indent=2)


@server.resource("supabase://status")
def resource_supabase_status() -> str:
    """Live resource reporting Supabase cloud connectivity status."""
    url, key = supabase_client.get_supabase_credentials()
    status = {
        "configured": bool(url and key),
        "url": url or "Not configured",
        "has_key": bool(key)
    }
    return json.dumps(status, indent=2)


# =====================================================================
# Main entrypoint
# =====================================================================

def main() -> None:
    parser = argparse.ArgumentParser(description="TaskFlow MCP Server")
    parser.add_argument(
        "--transport",
        choices=["stdio", "sse"],
        default="stdio",
        help="Transport type: 'stdio' (default) or 'sse' (HTTP server)"
    )
    parser.add_argument("--host", default="127.0.0.1", help="Host for SSE server (default: 127.0.0.1)")
    parser.add_argument("--port", type=int, default=8765, help="Port for SSE server (default: 8765)")
    args = parser.parse_args()

    # Print startup banner to stderr so stdio JSON-RPC remains clean on stdout
    print(f"[TaskFlow MCP] Starting server using transport '{args.transport}'...", file=sys.stderr)
    vault = get_vault_path()
    db = get_default_db_path()
    url, key = supabase_client.get_supabase_credentials()
    print(f"[TaskFlow MCP] Vault Path: {vault or 'Not configured (check TASKFLOW_VAULT)'}", file=sys.stderr)
    print(f"[TaskFlow MCP] SQLite DB: {db if db.exists() else 'Not found'}", file=sys.stderr)
    print(f"[TaskFlow MCP] Supabase: {url if url and key else 'Not configured (set SUPABASE_URL / SUPABASE_KEY)'}", file=sys.stderr)

    if args.transport == "sse":
        print(f"[TaskFlow MCP] Serving SSE on http://{args.host}:{args.port}/sse", file=sys.stderr)
        server.run(transport="sse", host=args.host, port=args.port)
    else:
        server.run(transport="stdio")


if __name__ == "__main__":
    main()

