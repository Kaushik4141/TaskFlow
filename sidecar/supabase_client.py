#!/usr/bin/env python3
"""Supabase client and synchronization engine for TaskFlow MCP.

Manages connection to Supabase, push synchronization of Obsidian vault notes,
vector embeddings generation, and cloud queries for external AI agents.
"""
from __future__ import annotations

import hashlib
import json
import os
import sqlite3
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional

try:
    from supabase import Client, create_client
    SUPABASE_AVAILABLE = True
except ImportError:
    SUPABASE_AVAILABLE = False
    Client = Any  # type: ignore


def get_default_db_path() -> Path:
    """Resolve default SQLite DB path."""
    if "TASKFLOW_DB" in os.environ:
        return Path(os.environ["TASKFLOW_DB"])
    if sys.platform == "win32":
        appdata = os.environ.get("APPDATA")
        if appdata:
            return Path(appdata) / "com.taskflow.desktop" / "taskflow.sqlite"
    else:
        return Path.home() / ".local" / "share" / "com.taskflow.desktop" / "taskflow.sqlite"
    return Path("taskflow.sqlite")


def get_supabase_credentials() -> tuple[Optional[str], Optional[str]]:
    """Resolve Supabase URL and Key from environment, .env file, or SQLite settings."""
    url = os.environ.get("SUPABASE_URL")
    key = os.environ.get("SUPABASE_KEY") or os.environ.get("SUPABASE_ANON_KEY") or os.environ.get("SUPABASE_SERVICE_ROLE_KEY")

    # If not in env, check sidecar/.env or repo-root .env
    if not url or not key:
        for env_file in [Path(".env"), Path("sidecar/.env"), Path("../.env")]:
            if env_file.exists():
                for line in env_file.read_text(encoding="utf-8").splitlines():
                    line = line.strip()
                    if line.startswith("SUPABASE_URL="):
                        url = url or line.split("=", 1)[1].strip().strip('"').strip("'")
                    elif line.startswith("SUPABASE_KEY="):
                        key = key or line.split("=", 1)[1].strip().strip('"').strip("'")

    # If still not found, check SQLite settings table
    if not url or not key:
        db_path = get_default_db_path()
        if db_path.exists():
            try:
                con = sqlite3.connect(str(db_path))
                cur = con.cursor()
                cur.execute("SELECT key, value FROM settings WHERE key IN ('supabase_url', 'supabase_key')")
                rows = dict(cur.fetchall())
                con.close()
                url = url or rows.get("supabase_url")
                key = key or rows.get("supabase_key")
            except Exception:
                pass

    return url, key


_client_instance: Optional[Client] = None


def get_supabase_client() -> Optional[Client]:
    """Get or create the Supabase client instance."""
    global _client_instance
    if not SUPABASE_AVAILABLE:
        return None

    if _client_instance is not None:
        return _client_instance

    url, key = get_supabase_credentials()
    if not url or not key:
        return None

    try:
        _client_instance = create_client(url, key)
        return _client_instance
    except Exception as e:
        print(f"[Supabase] Connection initialization failed: {e}", file=sys.stderr)
        return None


def is_supabase_configured() -> bool:
    """Check if Supabase credentials are configured."""
    url, key = get_supabase_credentials()
    return bool(url and key)


def compute_file_hash(text: str) -> str:
    """Compute SHA256 of text content."""
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def extract_frontmatter(text: str) -> tuple[dict, str]:
    """Parse YAML frontmatter from markdown text."""
    if not text.startswith("---"):
        return {}, text

    parts = text.split("---", 2)
    if len(parts) >= 3:
        front_str = parts[1].strip()
        body = parts[2].strip()
        meta = {}
        for line in front_str.splitlines():
            if ":" in line:
                k, v = line.split(":", 1)
                k = k.strip()
                v = v.strip().strip('"').strip("'")
                if v.startswith("[") and v.endswith("]"):
                    items = [x.strip().strip('"').strip("'") for x in v[1:-1].split(",") if x.strip()]
                    meta[k] = items
                else:
                    meta[k] = v
        return meta, body
    return {}, text


def sync_vault_to_supabase(vault_root: Path, embedder: Any = None) -> Dict[str, Any]:
    """Scan the local Obsidian vault and upsert notes and vector embeddings into Supabase.
    
    Returns a summary of uploaded, skipped, and errored files.
    """
    client = get_supabase_client()
    if not client:
        return {
            "success": False,
            "error": "Supabase client not initialized. Set SUPABASE_URL and SUPABASE_KEY in environment."
        }

    # Lazy-load embedder if not passed
    if embedder is None:
        try:
            from embedder import Embedder
            embedder = Embedder()
        except Exception:
            embedder = None

    uploaded = 0
    skipped = 0
    errors = []

    for file_path in vault_root.rglob("*.md"):
        try:
            rel_path = str(file_path.relative_to(vault_root)).replace("\\", "/")
            raw_text = file_path.read_text(encoding="utf-8", errors="replace")
            current_hash = compute_file_hash(raw_text)
            title = file_path.stem
            metadata, body = extract_frontmatter(raw_text)

            # Generate embedding vector
            embedding = None
            if embedder:
                snippet = (title + " " + body)[:1000]
                embedding = embedder.embed(snippet)

            row: Dict[str, Any] = {
                "path": rel_path,
                "title": title,
                "content": raw_text,
                "metadata": metadata,
                "file_hash": current_hash,
            }
            if embedding:
                row["embedding"] = embedding

            # Upsert into Supabase table vault_notes on conflict (path)
            res = client.table("vault_notes").upsert(row, on_conflict="path").execute()
            if hasattr(res, "data"):
                uploaded += 1
            else:
                skipped += 1
        except Exception as e:
            errors.append(f"{file_path.name}: {e}")

    return {
        "success": len(errors) == 0,
        "uploaded_count": uploaded,
        "skipped_count": skipped,
        "errors": errors
    }


def query_cloud_vault(query: str, semantic: bool = True, limit: int = 5, embedder: Any = None) -> List[Dict[str, Any]]:
    """Query the Supabase vault using vector similarity search (RPC) or full-text filtering.
    
    Enables remote agents to access the user's knowledge base even when the local laptop is off.
    """
    client = get_supabase_client()
    if not client:
        return [{"error": "Supabase client not initialized."}]

    if semantic:
        if embedder is None:
            try:
                from embedder import Embedder
                embedder = Embedder()
            except Exception:
                embedder = None

        if embedder:
            try:
                query_vec = embedder.embed(query)
                # Call stored procedure search_vault_notes
                rpc_res = client.rpc("search_vault_notes", {
                    "query_embedding": query_vec,
                    "match_threshold": 0.1,
                    "match_count": limit,
                    "filter_prefix": ""
                }).execute()

                if hasattr(rpc_res, "data") and rpc_res.data:
                    results = []
                    for row in rpc_res.data:
                        content = row.get("content", "")
                        results.append({
                            "path": row.get("path"),
                            "title": row.get("title"),
                            "similarity": round(row.get("similarity", 0.0), 4),
                            "snippet": content[:300] + ("..." if len(content) > 300 else ""),
                            "source": "supabase_cloud"
                        })
                    return results
            except Exception as e:
                # Fallback to standard table query if RPC is not installed
                print(f"[Supabase] Semantic RPC search error: {e}", file=sys.stderr)

    # Standard ILIKE search on table
    try:
        res = client.table("vault_notes").select("path, title, content").ilike("content", f"%{query}%").limit(limit).execute()
        if hasattr(res, "data") and res.data:
            results = []
            for row in res.data:
                content = row.get("content", "")
                idx = content.lower().find(query.lower())
                start = max(0, idx - 60)
                end = min(len(content), idx + len(query) + 100)
                results.append({
                    "path": row.get("path"),
                    "title": row.get("title"),
                    "similarity": 1.0,
                    "snippet": f"...{content[start:end]}...",
                    "source": "supabase_cloud"
                })
            return results
    except Exception as e:
        return [{"error": f"Cloud search query failed: {e}"}]

    return []
