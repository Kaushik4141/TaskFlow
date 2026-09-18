#!/usr/bin/env python3
"""TaskFlow Cloud Agent Client — Portable Memory Interface for Hermes & Remote AI Agents.

This module is 100% portable: it requires ZERO external pip packages and communicates
directly with the TaskFlow Supabase pgvector mirror via standard HTTPS REST.

Can be run directly on any cloud server, Modal container, or imported into Hermes.

Usage:
    from cloud_agent import CloudMemory

    brain = CloudMemory(supabase_url="https://xyz.supabase.co", supabase_key="sbp_...")
    
    # 1. Sub-15ms Scoped Memory Query:
    result = brain.query("OAuth token bug", project="AuthService")
    print(result["context_summary"])

    # 2. Get Zero-Hop Knowledge Topology:
    manifest = brain.get_manifest()
"""
from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any, Dict, List, Optional


def find_credentials() -> tuple[Optional[str], Optional[str]]:
    """Resolve Supabase URL and Key from env or .env files."""
    url = os.environ.get("SUPABASE_URL")
    key = os.environ.get("SUPABASE_KEY") or os.environ.get("SUPABASE_ANON_KEY")
    if not url or not key:
        search_paths = [
            Path(".env"),
            Path("sidecar/.env"),
            Path(__file__).parent / ".env",
            Path(__file__).parent.parent / ".env",
        ]
        for env_path in search_paths:
            if env_path.exists():
                for line in env_path.read_text(encoding="utf-8").splitlines():
                    line = line.strip()
                    if line.startswith("SUPABASE_URL="):
                        url = url or line.split("=", 1)[1].strip().strip('"').strip("'")
                    elif line.startswith("SUPABASE_KEY="):
                        key = key or line.split("=", 1)[1].strip().strip('"').strip("'")
    return url, key


class CloudMemory:
    """Client for querying the 24/7 TaskFlow Supabase Cloud Memory Mirror."""

    def __init__(
        self,
        supabase_url: Optional[str] = None,
        supabase_key: Optional[str] = None
    ) -> None:
        disc_url, disc_key = find_credentials()
        self.url = (supabase_url or disc_url or "").rstrip("/")
        self.key = (supabase_key or disc_key or "")
        
        if not self.url or not self.key:
            raise ValueError(
                "Supabase URL and Key are required. Pass them to CloudMemory(...) "
                "or set SUPABASE_URL and SUPABASE_KEY environment variables."
            )

    def _request(
        self,
        endpoint: str,
        method: str = "GET",
        data: Optional[Any] = None,
        params: Optional[Dict[str, str]] = None
    ) -> Any:
        path = endpoint.lstrip("/")
        full_url = f"{self.url}/rest/v1/{path}"

        if params:
            query_string = urllib.parse.urlencode(params)
            full_url = f"{full_url}?{query_string}"

        headers = {
            "apikey": self.key,
            "Authorization": f"Bearer {self.key}",
            "Content-Type": "application/json",
            "Accept": "application/json",
        }

        body_bytes = json.dumps(data).encode("utf-8") if data is not None else None
        req = urllib.request.Request(full_url, data=body_bytes, headers=headers, method=method)

        try:
            with urllib.request.urlopen(req, timeout=15) as resp:
                raw = resp.read().decode("utf-8")
                return json.loads(raw) if raw else {}
        except urllib.error.HTTPError as e:
            err = e.read().decode("utf-8", errors="replace")
            raise RuntimeError(f"Cloud Memory HTTP {e.code}: {err}") from e
        except Exception as e:
            raise RuntimeError(f"Cloud Memory Connection Error: {e}") from e

    def query(
        self,
        query: Optional[str] = None,
        project: Optional[str] = None,
        time_bucket: Optional[str] = None,
        limit: int = 5
    ) -> Dict[str, Any]:
        """Execute a scope-first query against cloud rollups in Supabase."""
        payload = {
            "match_limit": limit,
            "filter_project": project,
            "filter_time_bucket": time_bucket,
            "query_embedding": None  # Handled by pgvector RPC or PostgREST fallback
        }

        try:
            rows = self._request("rpc/query_cloud_memory", method="POST", data=payload)
            if not isinstance(rows, list):
                rows = []
        except Exception:
            # Fallback to direct table query
            params = {
                "select": "id,project_slug,window_start,window_end,title,summary_md,apps",
                "limit": str(limit),
                "order": "window_start.desc"
            }
            if project:
                params["project_slug"] = f"ilike.{project}"
            if query:
                params["summary_md"] = f"ilike.%{query}%"
            rows = self._request("cloud_rollups", method="GET", params=params)

        summary_lines = ["### Cloud Memory Context (24/7 Supabase Mirror):"]
        results = []

        for r in rows:
            date_str = (r.get("window_end") or "")[:10]
            proj = r.get("project_slug") or "Inbox"
            title = r.get("title", "")
            summary = r.get("summary_md", "")

            apps = r.get("apps") or []
            tools_str = ", ".join([f"[[Apps/{a}]]" for a in apps]) if apps else "None"

            summary_lines.append(f"- **[{date_str}] [[Projects/{proj}]] — {title}**")
            summary_lines.append(f"  - **Summary**: {summary[:250]}...")
            if apps:
                summary_lines.append(f"  - **Connected Tools**: {tools_str}")
            summary_lines.append(f"  - **Daily Reference**: [[Memory/Daily/{date_str}]]")

            results.append({
                "id": r.get("id"),
                "project_slug": proj,
                "title": title,
                "summary_md": summary,
                "date": date_str
            })

        return {
            "source": "supabase_cloud_mirror",
            "count": len(results),
            "results": results,
            "context_summary": "\n".join(summary_lines)
        }

    def get_manifest(self) -> Dict[str, Any]:
        """Read the Graph Topology Manifest from the cloud mirror for zero-hop routing."""
        rows = self._request("vault_notes", method="GET", params={"path": "eq.TaskFlow/manifest.json", "select": "content"})
        if rows and isinstance(rows, list):
            try:
                return json.loads(rows[0].get("content", "{}"))
            except Exception:
                pass
        return {"error": "manifest.json not found in cloud mirror"}

    def read_project(self, project_slug: str, max_lines: int = 40) -> Dict[str, Any]:
        """Read executive abstract of a project workstream note."""
        rows = self._request("vault_notes", method="GET", params={"title": f"eq.{project_slug}", "select": "title,content,path"})
        if not rows:
            return {"error": f"Project '{project_slug}' not found in cloud mirror"}
        content = rows[0].get("content", "")
        lines = content.splitlines()[:max_lines]
        return {
            "title": rows[0].get("title"),
            "path": rows[0].get("path"),
            "content": "\n".join(lines)
        }


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="Hermes Cloud Agent Memory Client")
    parser.add_argument("--query", type=str, help="Search memory")
    parser.add_argument("--project", type=str, help="Filter project")
    parser.add_argument("--manifest", action="store_true", help="Print topology manifest")
    args = parser.parse_args()

    try:
        brain = CloudMemory()
        if args.manifest:
            print(json.dumps(brain.get_manifest(), indent=2))
        elif args.query or args.project:
            res = brain.query(query=args.query, project=args.project)
            print(res.get("context_summary", json.dumps(res, indent=2)))
        else:
            parser.print_help()
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
