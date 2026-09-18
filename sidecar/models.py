from __future__ import annotations

from enum import Enum
from typing import Optional

from pydantic import BaseModel, Field


class SummaryMode(str, Enum):
    BASIC = "basic"
    LOCAL_AI = "local_ai"
    CLOUD_AI = "cloud_ai"


class EmbedRequest(BaseModel):
    text: str


class EventItem(BaseModel):
    id: str
    app_name: str | None = None
    window_title: str | None = None
    content: str | None = None
    url: str | None = None
    content_type: str | None = None
    capture_method: str | None = None
    event_type: str
    timestamp: str


class FilterRequest(BaseModel):
    task_title: str
    task_description: str
    source_labels: str | None = None
    source_priority: str | None = None
    source_project: str | None = None
    source_body: str | None = None
    events: list[EventItem]


class ScoredEvent(EventItem):
    relevance_score: float
    reason: str
    included: bool = True


class SummarizeRequest(BaseModel):
    task_title: str
    task_description: str
    relevant_events: list[ScoredEvent]
    mode: SummaryMode = SummaryMode.BASIC
    cloud_base_url: Optional[str] = None
    cloud_api_key: Optional[str] = None
    cloud_model: Optional[str] = None
    ollama_url: Optional[str] = "http://localhost:11434"
    ollama_model: Optional[str] = "llama3.1:8b"
    prior_context: Optional[str] = None


class EmbedResponse(BaseModel):
    vector: list[float]
    dimensions: int


class FilterResponse(BaseModel):
    scored_events: list[ScoredEvent]
    total_events: int
    relevant_count: int
    filter_threshold: float


class SummarizeResponse(BaseModel):
    markdown: str
    summary: str
    key_points: list[str]
    resources: list[str]
    duration_seconds: int | None = None
    generated_locally: bool = Field(default=True)
    method: Optional[str] = None
