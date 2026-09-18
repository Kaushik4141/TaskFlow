import re
from contextlib import asynccontextmanager

import httpx
import uvicorn
from fastapi import FastAPI, HTTPException

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
)

embedder = None
task_filter = None
context_builder = None
basic_summarizer = None
llm_summarizer = None


@asynccontextmanager
async def lifespan(app: FastAPI):
    global context_builder, basic_summarizer, llm_summarizer
    context_builder = ContextBuilder()
    basic_summarizer = BasicSummarizer()
    llm_summarizer = LLMSummarizer()
    print("TaskFlow AI sidecar ready on port 7878", flush=True)
    yield


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


if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port=7878)
