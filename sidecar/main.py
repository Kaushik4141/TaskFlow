import asyncio
import datetime
import json
import re
import sqlite3
import urllib.error
import urllib.request
from contextlib import asynccontextmanager
from typing import Optional

import httpx
import uvicorn
from fastapi import FastAPI, HTTPException
from fastapi.responses import HTMLResponse

from basic_summarizer import BasicSummarizer
from context_builder import ContextBuilder
from embedder import Embedder
from filter import TaskRelevanceFilter
from llm_summarizer import LLMSummarizer
from models import (
    EmbedRequest,
    EmbedResponse,
    FilterRequest,
    FilterResponse,
    SummarizeRequest,
    SummarizeResponse,
    QueryMemoryRequest,
    SessionRequest,
)

embedder = None
task_filter = None
context_builder = None
basic_summarizer = None
llm_summarizer = None


async def background_sync_loop():
    """Automatic Background Cloud Sync (Set-and-Forget 24/7) — checks every 10 minutes."""
    while True:
        try:
            await asyncio.sleep(600)  # 10 minutes
            import supabase_client
            if not supabase_client.is_supabase_configured():
                continue

            db_path = supabase_client.get_default_db_path()
            auto_sync = True
            if db_path.exists():
                try:
                    con = sqlite3.connect(str(db_path))
                    cur = con.cursor()
                    cur.execute("SELECT value FROM settings WHERE key = 'supabase_auto_sync'")
                    row = cur.fetchone()
                    con.close()
                    if row and row[0] == "false":
                        auto_sync = False
                except Exception:
                    pass

            if auto_sync:
                print("[TaskFlow Auto-Sync] Triggering background cloud memory delta sync...", flush=True)
                res = await asyncio.to_thread(supabase_client.sync_to_supabase)
                print(
                    f"[TaskFlow Auto-Sync] Done: {res.get('vault_notes_uploaded', 0)} notes, "
                    f"{res.get('rollups_uploaded', 0)} rollups, {res.get('graph_edges_uploaded', 0)} edges.",
                    flush=True
                )
        except asyncio.CancelledError:
            break
        except Exception as e:
            print(f"[TaskFlow Auto-Sync] Background sync error: {e}", flush=True)


@asynccontextmanager
async def lifespan(app: FastAPI):
    global context_builder, basic_summarizer, llm_summarizer
    context_builder = ContextBuilder()
    basic_summarizer = BasicSummarizer()
    llm_summarizer = LLMSummarizer()
    print("TaskFlow AI sidecar ready on port 7878", flush=True)
    sync_task = asyncio.create_task(background_sync_loop())
    yield
    sync_task.cancel()


app = FastAPI(lifespan=lifespan)


@app.get("/health")
async def health():
    return {"status": "ready", "version": "3.0"}


@app.post("/embed", response_model=EmbedResponse)
async def embed(request: EmbedRequest):
    global embedder
    if embedder is None:
        print("Loading embedding model...", flush=True)
        embedder = Embedder()
    vector = embedder.embed(request.text)
    return {"vector": vector, "dimensions": len(vector)}


@app.post("/filter", response_model=FilterResponse)
async def filter_events(request: FilterRequest):
    global embedder, task_filter
    if embedder is None:
        print("Loading embedding model...", flush=True)
        embedder = Embedder()
    if task_filter is None:
        task_filter = TaskRelevanceFilter(embedder)
    return task_filter.filter(request)


@app.post("/summarize", response_model=SummarizeResponse)
async def summarize(request: SummarizeRequest):
    if context_builder is None or basic_summarizer is None:
        raise HTTPException(status_code=503, detail="Summarizer is not ready")

    mode = getattr(request.mode, "value", request.mode)

    if mode == "basic":
        method_desc = "basic (extractive)"
    elif mode == "local_ai":
        ollama_model = request.ollama_model or "llama3.1:8b"
        ollama_url = request.ollama_url or "http://localhost:11434"
        method_desc = f"local_ai (Ollama: {ollama_model} @ {ollama_url})"
    elif mode == "cloud_ai":
        cloud_model = request.cloud_model or "default"
        cloud_url = request.cloud_base_url or "unknown"
        method_desc = f"cloud_ai (Model: {cloud_model} @ {cloud_url})"
    else:
        method_desc = f"unknown ({mode})"

    print(
        f"SUMMARIZE: using method '{method_desc}' (mode={mode}, task='{request.task_title}', "
        f"events={len(request.relevant_events)})",
        flush=True,
    )

    context = context_builder.build(
        events=request.relevant_events,
        task_title=request.task_title,
        task_description=request.task_description or "",
    )
    context["event_count"] = len(request.relevant_events)
    if request.prior_context:
        context["prior_context"] = request.prior_context

    print(
        f"CONTEXT: chunks={len(context['clean_chunks'])} "
        f"signals={len(context['signals'])} pages={len(context['timeline'])}",
        flush=True,
    )

    try:
        if mode == "basic":
            result = basic_summarizer.summarize(context)
        elif mode in ("local_ai", "cloud_ai"):
            if llm_summarizer is None:
                raise RuntimeError("LLM summarizer is not ready")
            result = await llm_summarizer.summarize(
                context=context,
                mode=mode,
                cloud_base_url=request.cloud_base_url,
                cloud_api_key=request.cloud_api_key,
                cloud_model=request.cloud_model,
                ollama_url=request.ollama_url or "http://localhost:11434",
                ollama_model=request.ollama_model or "llama3.1:8b",
            )
        else:
            result = basic_summarizer.summarize(context)
    except Exception as error:
        fallback_method = "basic (fallback after error)"
        print(
            f"SUMMARIZE ERROR: {_safe_error(error, request.cloud_api_key)} - falling back to method '{fallback_method}'",
            flush=True,
        )
        result = basic_summarizer.summarize(context)
        result["method"] = fallback_method
        result["markdown"] = "> AI summary unavailable, showing basic summary.\n\n" + result["markdown"]

    actual_method = result.get("method", method_desc)
    print(f"DONE: method='{actual_method}' summary='{result['summary'][:80]}'", flush=True)
    return result


@app.get("/ollama/status")
async def ollama_status(url: str = "http://localhost:11434"):
    try:
        base_url = (url or "http://localhost:11434").rstrip("/")
        async with httpx.AsyncClient(timeout=3.0) as client:
            response = await client.get(f"{base_url}/api/tags")
            response.raise_for_status()
            models = [m["name"] for m in response.json().get("models", [])]
            has_recommended = any(
                any(token in model.lower() for token in ["llama", "mistral", "qwen"])
                for model in models
            )
            return {
                "is_running": True,
                "available_models": models,
                "has_recommended_model": has_recommended,
            }
    except Exception:
        return {
            "is_running": False,
            "available_models": [],
            "has_recommended_model": False,
        }


@app.post("/query_memory")
async def api_query_memory(request: QueryMemoryRequest):
    """Scope-first, search-second graph memory retrieval endpoint for remote AI agents."""
    from mcp_server import tool_query_graph_memory
    return tool_query_graph_memory(
        query=request.query,
        project=request.project,
        time_bucket=request.time_bucket,
        start_date=request.start_date,
        end_date=request.end_date,
        limit=request.limit,
    )


@app.get("/manifest")
async def api_get_manifest():
    """Read the TaskFlow Graph Topology Manifest for zero-hop machine routing."""
    from mcp_server import tool_read_manifest
    return tool_read_manifest()


def _safe_error(error: Exception, api_key: str | None = None) -> str:
    message = str(error)
    if "api_key" in message.lower():
        return "Provider request failed."
    if api_key:
        message = message.replace(api_key, "[redacted]")
    return re.sub(r"key=[^&\s]+", "key=[redacted]", message)


# =====================================================================
# Supabase Auth & Google OAuth Receiver
# =====================================================================

OAUTH_CALLBACK_HTML = """<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>TaskFlow Authentication</title>
  <style>
    body {
      background: #090d16;
      color: #f8fafc;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
      display: flex;
      align-items: center;
      justify-content: center;
      height: 100vh;
      margin: 0;
    }
    .box {
      background: #111827;
      border: 1px solid rgba(255, 255, 255, 0.1);
      border-radius: 18px;
      padding: 36px 40px;
      text-align: center;
      max-width: 420px;
      box-shadow: 0 20px 40px rgba(0, 0, 0, 0.6);
    }
    .icon {
      font-size: 40px;
      margin-bottom: 16px;
      color: #38bdf8;
    }
    h2 {
      margin: 0 0 8px;
      font-size: 22px;
      font-weight: 600;
      color: #fff;
    }
    p {
      color: #94a3b8;
      font-size: 14px;
      line-height: 1.5;
      margin: 0 0 20px;
    }
    .status {
      display: inline-block;
      padding: 8px 16px;
      background: rgba(56, 189, 248, 0.1);
      border: 1px solid rgba(56, 189, 248, 0.25);
      border-radius: 8px;
      font-size: 13px;
      color: #38bdf8;
    }
  </style>
</head>
<body>
  <div class="box">
    <div class="icon">⚡</div>
    <h2>TaskFlow Authenticated</h2>
    <p>Connecting your Google account with TaskFlow Memory Cloud...</p>
    <div id="status" class="status">Verifying session...</div>
  </div>
  <script>
    (function() {
      const hash = window.location.hash.substring(1);
      const params = new URLSearchParams(hash);
      const accessToken = params.get('access_token');
      const refreshToken = params.get('refresh_token');
      const statusEl = document.getElementById('status');

      if (!accessToken) {
        statusEl.innerText = 'No authentication token found in redirect.';
        statusEl.style.color = '#ef4444';
        return;
      }

      fetch('/auth/session', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ access_token: accessToken, refresh_token: refreshToken })
      })
      .then(res => res.json())
      .then(data => {
        if (data.success) {
          statusEl.innerText = '✓ Success! You can close this window.';
          statusEl.style.color = '#34d399';
          setTimeout(() => { window.close(); }, 1500);
        } else {
          statusEl.innerText = 'Error: ' + (data.detail || data.error || 'Failed');
          statusEl.style.color = '#ef4444';
        }
      })
      .catch(err => {
        statusEl.innerText = 'Network error: ' + err;
        statusEl.style.color = '#ef4444';
      });
    })();
  </script>
</body>
</html>
"""


@app.get("/auth/callback", response_class=HTMLResponse)
async def auth_callback():
    """OAuth callback endpoint handling Google OAuth redirect from Supabase."""
    return HTMLResponse(content=OAUTH_CALLBACK_HTML, status_code=200)


@app.get("/auth/login_url")
async def get_login_url():
    """Generate Supabase Google OAuth authorization URL."""
    import supabase_client
    url, key = supabase_client.get_supabase_credentials()
    if not url:
        raise HTTPException(status_code=400, detail="Supabase URL not configured")
    clean_url = url.rstrip("/")
    redirect_uri = "http://localhost:7878/auth/callback"
    oauth_url = f"{clean_url}/auth/v1/authorize?provider=google&redirect_to={redirect_uri}"
    return {"url": oauth_url}


@app.post("/auth/session")
async def save_auth_session(request: SessionRequest):
    """Verify Supabase JWT access token and store user identity in SQLite settings."""
    import supabase_client
    url, key = supabase_client.get_supabase_credentials()
    if not url or not key:
        raise HTTPException(status_code=400, detail="Supabase not configured")

    clean_url = url.rstrip("/")
    user_url = f"{clean_url}/auth/v1/user"
    user_req = urllib.request.Request(
        user_url,
        headers={
            "apikey": key,
            "Authorization": f"Bearer {request.access_token}",
            "Accept": "application/json",
        },
        method="GET"
    )
    try:
        with urllib.request.urlopen(user_req, timeout=10) as resp:
            user_data = json.loads(resp.read().decode("utf-8"))
    except Exception as e:
        raise HTTPException(status_code=401, detail=f"Failed to verify session with Supabase: {e}")

    user_id = user_data.get("id")
    email = user_data.get("email", "")
    metadata = user_data.get("user_metadata", {}) or {}
    full_name = metadata.get("full_name") or metadata.get("name") or email.split("@")[0]
    avatar_url = metadata.get("avatar_url") or metadata.get("picture") or ""

    db_path = supabase_client.get_default_db_path()
    if db_path.exists():
        try:
            con = sqlite3.connect(str(db_path))
            cur = con.cursor()
            settings_map = {
                "supabase_access_token": request.access_token,
                "supabase_refresh_token": request.refresh_token or "",
                "supabase_user_id": user_id,
                "supabase_user_email": email,
                "supabase_user_name": full_name,
                "supabase_user_avatar": avatar_url,
            }
            for k, v in settings_map.items():
                cur.execute("""
                    INSERT INTO settings (key, value) VALUES (?, ?)
                    ON CONFLICT(key) DO UPDATE SET value = excluded.value
                """, (k, v))
            con.commit()
            con.close()
        except Exception as e:
            raise HTTPException(status_code=500, detail=f"Failed to save session to database: {e}")

    # Trigger immediate delta sync in background thread
    asyncio.create_task(asyncio.to_thread(supabase_client.sync_to_supabase))

    return {
        "success": True,
        "user": {
            "id": user_id,
            "email": email,
            "name": full_name,
            "avatar_url": avatar_url,
        }
    }


@app.get("/auth/user")
async def get_current_user():
    """Get active authenticated Supabase user profile from SQLite settings."""
    import supabase_client
    db_path = supabase_client.get_default_db_path()
    if not db_path.exists():
        return {"authenticated": False, "user": None}

    try:
        con = sqlite3.connect(str(db_path))
        cur = con.cursor()
        cur.execute("SELECT key, value FROM settings WHERE key LIKE 'supabase_user_%' OR key = 'supabase_access_token'")
        rows = dict(cur.fetchall())
        con.close()

        user_id = rows.get("supabase_user_id")
        if user_id:
            return {
                "authenticated": True,
                "user": {
                    "id": user_id,
                    "email": rows.get("supabase_user_email", ""),
                    "name": rows.get("supabase_user_name", ""),
                    "avatar_url": rows.get("supabase_user_avatar", ""),
                }
            }
    except Exception:
        pass
    return {"authenticated": False, "user": None}


@app.post("/auth/signout")
async def signout():
    """Clear Supabase user session and authentication tokens from SQLite settings."""
    import supabase_client
    db_path = supabase_client.get_default_db_path()
    if db_path.exists():
        try:
            con = sqlite3.connect(str(db_path))
            cur = con.cursor()
            cur.execute("DELETE FROM settings WHERE key LIKE 'supabase_user_%' OR key = 'supabase_access_token' OR key = 'supabase_refresh_token'")
            con.commit()
            con.close()
        except Exception:
            pass
    return {"success": True}


# =====================================================================
# 24/7 Cloud Mirror Sync Endpoints
# =====================================================================

@app.post("/sync_cloud")
async def api_sync_cloud():
    """Trigger on-demand 24/7 delta sync to Supabase."""
    import supabase_client
    if not supabase_client.is_supabase_configured():
        raise HTTPException(status_code=400, detail="Supabase not configured in settings")
    res = await asyncio.to_thread(supabase_client.sync_to_supabase)
    return res


@app.get("/sync_status")
async def api_sync_status():
    """Query current cloud sync status, timestamps, and multi-tenant user info."""
    import supabase_client
    db_path = supabase_client.get_default_db_path()
    last_sync = None
    last_stats = None
    auto_sync = True
    url, key = supabase_client.get_supabase_credentials()

    if db_path.exists():
        try:
            con = sqlite3.connect(str(db_path))
            cur = con.cursor()
            cur.execute("SELECT key, value FROM settings WHERE key IN ('supabase_last_sync', 'supabase_last_sync_stats', 'supabase_auto_sync')")
            rows = dict(cur.fetchall())
            con.close()
            last_sync = rows.get("supabase_last_sync")
            if rows.get("supabase_last_sync_stats"):
                try:
                    last_stats = json.loads(rows["supabase_last_sync_stats"])
                except Exception:
                    pass
            if rows.get("supabase_auto_sync") == "false":
                auto_sync = False
        except Exception:
            pass

    return {
        "configured": bool(url and key),
        "supabase_url": url,
        "auto_sync": auto_sync,
        "last_sync": last_sync,
        "last_stats": last_stats,
    }


if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port=7878)
