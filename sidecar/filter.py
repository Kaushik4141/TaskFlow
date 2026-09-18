from __future__ import annotations

import re
from urllib.parse import urlparse

from embedder import Embedder
from models import EventItem, FilterRequest, FilterResponse, ScoredEvent

HIGH_TRUST_APPS = [
    "code", "vscodium", "vim", "nvim", "neovim",
    "pycharm", "intellij", "webstorm", "goland",
    "terminal", "iterm2", "wezterm", "alacritty",
    "postman", "insomnia", "tableplus", "datagrip",
    "github desktop", "sourcetree", "fork",
    "xcode", "android studio", "cursor",
]

MEDIUM_TRUST_APPS = [
    "chrome", "firefox", "safari", "edge",
    "brave", "arc", "opera",
]

LOW_TRUST_APPS = [
    "slack", "teams", "discord", "telegram",
    "whatsapp", "zoom", "meet", "webex",
]

EXCLUDED_APPS = [
    "1password", "bitwarden", "keepass", "lastpass", "dashlane",
    "nordpass", "proton pass", "auth", "authenticator", "duo mobile",
    "yubico", "keychain", "credential manager", "certificate manager",
    "spotify", "music", "vlc", "mpv", "netflix", "youtube", "hulu",
    "prime video", "disney", "obsidian",
]

STOPWORDS = {
    "the", "a", "an", "is", "in", "on", "at", "to",
    "for", "of", "and", "or", "with", "that", "this", "it", "be", "are",
    "was", "were", "has", "have", "had", "not", "but", "by", "from", "as",
}


class TaskRelevanceFilter:
    def __init__(self, embedder: Embedder) -> None:
        self.embedder = embedder

    def filter(self, request: FilterRequest) -> FilterResponse:
        task_context = self.build_task_context(request)
        try:
            task_vector = self.embedder.embed(task_context)
        except Exception:
            print("FILTER FALLBACK: embedding model unavailable", flush=True)
            task_vector = None
        keywords = self.extract_request_keywords(request)
        scored_events = [
            self.evaluate_event(event, task_vector, keywords)
            for event in request.events
        ]
        relevant_count = sum(1 for event in scored_events if event.included)
        return FilterResponse(
            scored_events=scored_events,
            total_events=len(request.events),
            relevant_count=relevant_count,
            filter_threshold=0.35,
        )

    def build_task_context(self, request: FilterRequest) -> str:
        parts: list[str] = []
        parts.append(request.task_title)
        parts.append(request.task_title)
        parts.append(request.task_title)

        if request.task_description:
            parts.append(request.task_description)
        if request.source_body:
            parts.append(request.source_body[:500])
        if request.source_labels:
            parts.append(request.source_labels)
        if request.source_project:
            parts.append(request.source_project)

        return " ".join(parts)

    def extract_request_keywords(self, request: FilterRequest) -> set[str]:
        parts = [
            request.task_title,
            request.task_description,
            request.source_labels or "",
            request.source_project or "",
        ]
        keywords = self.extract_keywords(" ".join(parts))
        if request.source_body:
            keywords.update(self.extract_technical_terms(request.source_body[:500]))
        return keywords

    def extract_keywords(self, text: str) -> set[str]:
        words = re.split(r"[\s\W_]+", text.lower())
        return {word for word in words if len(word) > 3 and word not in STOPWORDS}

    def extract_technical_terms(self, text: str) -> set[str]:
        terms: set[str] = set()
        terms.update(match.lower().lstrip(".") for match in re.findall(r"\.[A-Za-z0-9]{2,8}\b", text))
        terms.update(match.lower() for match in re.findall(r"\b[a-z]+(?:[A-Z][a-z0-9]+)+\b", text))
        terms.update(match.lower() for match in re.findall(r"\b[A-Z][A-Z0-9]+(?:_[A-Z0-9]+)+\b", text))
        return {term for term in terms if len(term) > 2 and term not in STOPWORDS}

    def evaluate_event(
        self,
        event: EventItem,
        task_vector: list[float] | None,
        keywords: set[str],
    ) -> ScoredEvent:
        app_lower = (event.app_name or "").lower()

        for excluded in EXCLUDED_APPS:
            if excluded in app_lower:
                return self.to_scored_event(event, 0.0, "excluded_app", False)

        event_text = self.build_event_text(event)

        if not event_text.strip():
            return self.to_scored_event(event, 0.0, "empty_event", False)

        event_lower = event_text.lower()
        keyword_hits = sum(1 for keyword in keywords if keyword in event_lower)

        if keyword_hits >= 2:
            return self.to_scored_event(event, 0.85, f"keyword_match:{keyword_hits}", True)

        if task_vector is None:
            app_trust = self.get_threshold(app_lower)
            included = keyword_hits >= 1 or app_trust <= 0.30
            score = 0.85 if keyword_hits >= 1 else 0.60 if app_trust <= 0.15 else 0.50
            return self.to_scored_event(
                event,
                score,
                f"keyword_fallback:{keyword_hits}",
                included,
            )

        event_vector = self.embedder.embed(event_text)
        similarity = self.embedder.cosine_similarity(task_vector, event_vector)
        threshold = self.get_threshold(app_lower)
        if event.content:
            threshold = max(0.05, threshold - 0.05)
        if event.capture_method == "title_only":
            threshold += 0.05
        included = similarity >= threshold

        return self.to_scored_event(
            event,
            round(similarity, 4),
            f"semantic:{similarity:.2f}",
            included,
        )

    def build_event_text(self, event: EventItem) -> str:
        parts: list[str] = []
        if event.app_name:
            parts.append(event.app_name)
        if event.window_title:
            parts.append(event.window_title)
        if event.url:
            parts.append(event.url)
            try:
                domain = urlparse(event.url).netloc
                if domain:
                    parts.append(domain)
            except Exception:
                pass
        if event.content:
            content_preview = event.content[:400]
            parts.append(content_preview)
            parts.append(content_preview)
        return " ".join(parts)

    def get_threshold(self, app_lower: str) -> float:
        for app in HIGH_TRUST_APPS:
            if app in app_lower:
                return 0.15
        for app in MEDIUM_TRUST_APPS:
            if app in app_lower:
                return 0.30
        for app in LOW_TRUST_APPS:
            if app in app_lower:
                return 0.50
        return 0.35

    def to_scored_event(
        self,
        event: EventItem,
        score: float,
        reason: str,
        included: bool,
    ) -> ScoredEvent:
        return ScoredEvent(
            id=event.id,
            app_name=event.app_name,
            window_title=event.window_title,
            content=event.content,
            url=event.url,
            event_type=event.event_type,
            timestamp=event.timestamp,
            relevance_score=score,
            reason=reason,
            included=included,
        )
