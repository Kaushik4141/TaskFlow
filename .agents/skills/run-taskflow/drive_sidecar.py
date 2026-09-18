#!/usr/bin/env python3
"""TaskFlow sidecar driver — the lean agent path (no GUI).

Drives the FastAPI sidecar at http://127.0.0.1:7878 the same way the Tauri
app does, but from the command line. It reads the REAL configured AI settings
mode / cloud provider / model / key, decrypting the XOR-obfuscated API key with
the same hostname-SHA256 scheme the Rust side uses (commands.rs `machine_key`),
so it exercises the genuine summarize path, not a synthetic stub.

Expects the sidecar already running: `python -m uvicorn main:app --port 7878`
from sidecar/. Expects TaskFlow to have been launched at least once so the
SQLite settings DB at APPDATA\\com.taskflow.desktop\\taskflow.sqlite exists.

Usage:
    python drive_sidecar.py Health
    python drive_sidecar.py Summarize --mode cloud_ai
    python drive_sidecar.py Summarize --mode basic
    python drive_sidecar.py Summarize --mode local_ai

    python drive_sidecar.py Config         # print the real settings (key redacted)
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import sqlite3
import sys
from pathlib import Path
from typing import Any

import httpx

SIDECAR = os.environ.get("TASKFLOW_SIDECAR", "http://127.0.0.1:7878")
DB_PATH = Path(os.environ.get(
    "TASKFLOW_DB",
    Path(os.environ["APPDATA"]) / "com.taskflow.desktop" / "taskflow.sqlite",
))


def machine_key() -> bytes:
    """Mirror of Rust `machine_key()` (commands.rs): SHA-256 of the hostname,
    hex-encoded, first 32 bytes used as the XOR key."""
    hostname = os.environ.get("COMPUTERNAME") or os.environ.get("HOSTNAME") or "taskflow-local-machine"
    digest = hashlib.sha256(hostname.encode()).hexdigest()
    return digest[:32].encode()


def decrypt_token(token: str) -> str:
    if not token:
        return ""
    try:
        raw = bytes.fromhex(token)
    except ValueError:
        return token  # not hex: treat as plaintext (mirrors Rust fallback)
    key = machine_key()
    return bytes(b ^ key[i % len(key)] for i, b in enumerate(raw)).decode("utf-8", "replace")


def load_settings() -> dict[str, str]:
    """Read the summary settings keys the Rust `load_summary_settings` reads."""
    if not DB_PATH.exists():
        raise SystemExit(
            f"TaskFlow DB not found at {DB_PATH}. Launch the app once so it "
            f"initializes the settings table."
        )
    con = sqlite3.connect(str(DB_PATH))
    rows = con.execute("SELECT key, value FROM settings").fetchall()
    con.close()
    s = dict(rows)
    return {
        "mode": s.get("summary_mode", "basic"),
        "cloud_base_url": s.get("summary_cloud_base_url", ""),
        "cloud_api_key_encrypted": s.get("summary_cloud_api_key", ""),
        "cloud_model": s.get("summary_cloud_model", ""),
        "ollama_url": s.get("summary_ollama_url", "http://localhost:11434"),
        "ollama_model": s.get("summary_ollama_model", "llama3.1:8b"),
    }


def redacted(settings: dict[str, str]) -> dict[str, str]:
    out = dict(settings)
    key = decrypt_token(settings["cloud_api_key_encrypted"])
    out["cloud_api_key"] = "***" + key[-4:] if len(key) > 4 else ("set" if key else "none")
    out.pop("cloud_api_key_encrypted", None)
    return out


def cmd_config(_args: argparse.Namespace) -> int:
    print(json.dumps(redacted(load_settings()), indent=2))
    return 0


def cmd_health(_args: argparse.Namespace) -> int:
    with httpx.Client(timeout=10.0) as c:
        try:
            r = c.get(f"{SIDECAR}/health")
            print(f"status={r.status_code}")
            print(r.text)
            return 0 if r.is_success else 1
        except httpx.RequestError as e:
            print(f"sidecar unreachable at {SIDECAR}: {e}", file=sys.stderr)
            print(" Boot it first from sidecar/:  uvicorn main:app --port 7878", file=sys.stderr)
            return 2


# A realistic synthetic event batch: the shape ScoredEvent expects on the wire.
# (The Tauri app sends events-through-filter; here we send them directly to
# /summarize as already-scored events, which is what `basic` mode does.)
SAMPLE_EVENTS: list[dict[str, Any]] = [
    {
        "id": "drv-1", "app_name": "Cursor.exe",
        "window_title": "embedder.py - TaskFlow",
        "content": "def embed_text(self, text): ... sentence-transformers all-MiniLM-L6-v2",
        "url": None, "content_type": "CodeContent", "capture_method": "uitautomation",
        "event_type": "capture", "timestamp": "2026-07-10T10:00:00+00:00",
        "relevance_score": 0.9, "reason": "synthetic", "included": True,
    },
    {
        "id": "drv-2", "app_name": "firefox.exe",
        "window_title": "LLM Wiki knowledge base - Mozilla Firefox",
        "content": "Andrej Karpathy: using LLMs to build personal knowledge bases.",
        "url": "https://x.com/karpathy", "content_type": "BrowserContent",
        "capture_method": "title_only", "event_type": "capture",
        "timestamp": "2026-07-10T10:05:00+00:00",
        "relevance_score": 0.8, "reason": "synthetic", "included": True,
    },
    {
        "id": "drv-3", "app_name": "Notion.exe",
        "window_title": "linear search",
        "content": "reviewing Java linear search and ARRAY concepts from personal knowledge base",
        "url": "https://linear.app", "content_type": "DocumentationContent",
        "capture_method": "uitautomation", "event_type": "capture",
        "timestamp": "2026-07-10T10:10:00+00:00",
        "relevance_score": 0.7, "reason": "synthetic", "included": True,
    },
]


def cmd_summarize(args: argparse.Namespace) -> int:
    s = load_settings()
    mode = args.mode or ("cloud_ai" if s["mode"] == "cloud_ai" and decrypt_token(s["cloud_api_key_encrypted"])
                         else "basic")
    req = {
        "task_title": "Memory Capture - 2026-07-10",
        "task_description": "Synthetic drive of the summarize path via the lean agent skill.",
        "relevant_events": SAMPLE_EVENTS,
        "mode": mode,
        "cloud_base_url": s["cloud_base_url"] or None,
        "cloud_api_key": decrypt_token(s["cloud_api_key_encrypted"]) or None,
        "cloud_model": s["cloud_model"] or None,
        "ollama_url": s["ollama_url"] or None,
        "ollama_model": s["ollama_model"] or None,
    }
    # sidecar SummarizeRequest has prior_context optional; omit for a clean synthetic run
    print(f"POST {SIDECAR}/summarize  mode={mode}")
    with httpx.Client(timeout=120.0) as c:
        try:
            r = c.post(f"{SIDECAR}/summarize", json=req)
        except httpx.RequestError as e:
            print(f"sidecar unreachable: {e}", file=sys.stderr)
            return 2
    print(f"status={r.status_code}")
    if not r.is_success:
        print(r.text[:800])
        return 1
    body = r.json()
    md = body.get("markdown", "")
    print("\n=== summary ===\n" + body.get("summary", ""))
    print(f"\nmethod={body.get('method', 'not specified')}")
    print(f"key_points={body.get('key_points')} resources={body.get('resources')}")
    print(f"generated_locally={body.get('generated_locally')}")
    print("\n=== markdown (first 1200 chars) ===")
    print(md[:1200])
    if "```hub:" in md:
        print("\n=== hub synthesis blocks present ===")
        for line in md.splitlines():
            if line.strip().startswith("```hub:"):
                print("  " + line.strip())
    else:
        print("\n(no hub synthesis blocks in response)")
    return 0


def cmd_filter(_args: argparse.Namespace) -> int:
    """Drive /filter to confirm the relevance pipeline runs."""
    req = {
        "task_title": "Memory Capture - 2026-07-10",
        "task_description": "Synthetic filter check via the lean agent skill.",
        "events": [
            {k: v for k, v in e.items() if k not in ("relevance_score", "reason", "included")}
            for e in SAMPLE_EVENTS
        ],
    }
    print(f"POST {SIDECAR}/filter")
    with httpx.Client(timeout=60.0) as c:
        r = c.post(f"{SIDECAR}/filter", json=req)
    print(f"status={r.status_code}")
    if r.is_success:
        b = r.json()
        print(f"total={b.get('total_events')} relevant={b.get('relevant_count')} threshold={b.get('filter_threshold')}")
        for ev in b.get("scored_events", []):
            print(f"  [{ev.get('included')}] {ev.get('app_name')}: {ev.get('reason')} ({ev.get('relevance_score')})")
        return 0
    print(r.text[:600])
    return 1


def main() -> int:
    p = argparse.ArgumentParser(description="Lean agent driver for the TaskFlow sidecar.")
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser("Config", help="print the real TaskFlow AI settings (key redacted)").set_defaults(func=cmd_config)
    sub.add_parser("Health", help="curl the sidecar /health endpoint").set_defaults(func=cmd_health)
    sub.add_parser("Filter", help="drive /filter with synthetic events").set_defaults(func=cmd_filter)
    su = sub.add_parser("Summarize", help="drive /summarize; reads real cloud config, hub blocks included")
    su.add_argument("--mode", choices=["basic", "local_ai", "cloud_ai"],
                    help="override; default = cloud_ai if a key is configured else basic")
    su.set_defaults(func=cmd_summarize)
    args = p.parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
