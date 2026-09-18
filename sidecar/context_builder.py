import re
from datetime import datetime
from urllib.parse import urlparse


class ContextBuilder:
    SKIP_APPS = [
        "taskflow",
        "searchhost",
        "searchapp",
        "textinputhost",
        "shellexperiencehost",
        "startmenuexperiencehost",
        "lockapp",
    ]

    NOISE_PATTERNS = [
        r"^(file|edit|view|insert|format|tools|help|window)\s",
        r"^(ok|cancel|yes|no|close|minimize|maximize)$",
        r"^\s*[-_|•●○◆]\s*$",
        r"^(\d+)\s*$",
        r"^[^a-zA-Z]*$",
    ]

    def build(self, events: list, task_title: str, task_description: str = None) -> dict:
        context = {
            "task_title": task_title,
            "task_description": task_description or "",
            "clean_chunks": [],
            "signals": [],
            "timeline": [],
            "resources": [],
            "duration_secs": self._calc_duration(events),
            "apps_used": [],
            "clipboard_items": [],
            "raw_text_by_source": {},
        }
        seen_apps = []
        seen_pages = set()
        seen_urls = set()
        seen_content_hashes = set()

        for event in events:
            app = (event.app_name or "").strip()
            if any(skip in app.lower() for skip in self.SKIP_APPS):
                continue
            if app and app not in seen_apps:
                seen_apps.append(app)

            if getattr(event, "event_type", None) == "clipboard" and event.content:
                content = event.content.strip()
                if len(content) > 10 and not content.startswith("http") and content not in context["clipboard_items"]:
                    context["clipboard_items"].append(content[:300])

            if event.url and event.url.startswith("http"):
                try:
                    parsed = urlparse(event.url)
                    url_key = parsed.netloc + parsed.path[:50]
                    if url_key not in seen_urls:
                        seen_urls.add(url_key)
                        context["resources"].append(event.url)
                except Exception:
                    pass

            if event.window_title:
                title = self._clean_title(event.window_title, app)
                page_key = f"{app}:{title}"
                if title and page_key not in seen_pages:
                    seen_pages.add(page_key)
                    context["timeline"].append({"app": app, "title": title, "url": event.url})

            if event.content and len(event.content.strip()) > 30:
                clean = self._clean_content(event.content)
                if not clean:
                    continue
                fingerprint = clean[:100].lower()
                if fingerprint in seen_content_hashes:
                    continue
                seen_content_hashes.add(fingerprint)

                for chunk in self._chunk_content(clean, app):
                    context["clean_chunks"].append({
                        "text": chunk,
                        "source_app": app,
                        "source_title": event.window_title or "",
                        "url": event.url,
                    })

                source_key = self._clean_title(event.window_title or app, app)
                context["raw_text_by_source"].setdefault(source_key, []).append(clean)

        context["apps_used"] = seen_apps
        context["signals"] = self._extract_signals(context["clean_chunks"])
        return context

    def _clean_content(self, raw: str) -> str:
        clean = re.sub(r"[\ufffc\ufffe\uffff]", "", raw)
        clean = re.sub(r"[\ue000-\uf8ff]", "", clean)
        clean = re.sub(r"[\ufe00-\ufe0f]", "", clean)
        lines = []
        for line in clean.split("\n"):
            stripped = line.strip()
            if not stripped or not any(c.isalpha() for c in stripped) or len(stripped) < 4:
                continue
            if any(re.match(pattern, stripped, re.IGNORECASE) for pattern in self.NOISE_PATTERNS):
                continue
            lines.append(stripped)
        result = "\n".join(lines)
        result = re.sub(r"\n{3,}", "\n\n", result)
        result = re.sub(r" {2,}", " ", result)
        return result.strip()

    def _clean_title(self, title: str, app: str) -> str:
        for suffix in [
            " - Google Chrome",
            " - Mozilla Firefox",
            " - Microsoft Edge",
            " - Safari",
            " - Brave",
            " – Google Chrome",
            " - Arc",
        ]:
            title = title.replace(suffix, "")
        app_clean = app.replace(".exe", "").strip()
        if title.endswith(f" - {app_clean}"):
            title = title[:-len(f" - {app_clean}")]
        return title.strip()

    def _chunk_content(self, text: str, app: str) -> list:
        if any(token in app.lower() for token in ["code", "vim", "nvim", "terminal", "cmd", "powershell"]):
            return [block.strip()[:500] for block in re.split(r"\n\n+", text) if len(block.strip()) > 20]

        chunks = []
        current = []
        current_len = 0
        for sentence in re.split(r"(?<=[.!?])\s+", text):
            sentence = sentence.strip()
            if len(sentence) < 10:
                continue
            if current and current_len + len(sentence) > 400:
                chunks.append(" ".join(current))
                current = [sentence]
                current_len = len(sentence)
            else:
                current.append(sentence)
                current_len += len(sentence)
        if current:
            chunks.append(" ".join(current))
        return [chunk for chunk in chunks if len(chunk) > 20]

    def _extract_signals(self, chunks: list) -> list:
        signals = []
        all_text = " ".join(chunk["text"] for chunk in chunks)

        counts = re.findall(r"(\d+)\s*responses?", all_text, re.IGNORECASE)
        if counts:
            signals.append(f"Reviewed form with {max(int(value) for value in counts)} responses")

        emails = set(re.findall(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b", all_text))
        if len(emails) >= 2:
            signals.append(f"Reviewed {len(emails)} email addresses")

        github_profiles = set(re.findall(r"github\.com/([\w\-\.]+)(?:/[\w\-\.]+)?", all_text))
        if len(github_profiles) >= 2:
            signals.append(f"Reviewed {len(github_profiles)} GitHub profiles")

        code_files = list(dict.fromkeys(match[0] for match in re.findall(
            r"\b([\w\-]+\.(ts|tsx|js|jsx|py|rs|go|java|cpp|c|rb|md))\b",
            all_text,
        )))
        if code_files:
            signals.append(f"Worked on: {', '.join(code_files[:5])}")

        commands = re.findall(r"(?:^|\n)\s*(?:\$|>|❯|#)\s*([^\n]{3,60})", all_text, re.MULTILINE)
        clean_cmds = list(dict.fromkeys(command.strip() for command in commands if len(command.strip()) > 3))[:5]
        if clean_cmds:
            signals.append(f"Ran commands: {'; '.join(clean_cmds)}")

        for chunk in chunks:
            if len(chunk["text"].split()) > 150:
                title = self._clean_title(chunk.get("source_title", ""), "")
                if title and len(title) > 5:
                    signals.append(f"Read: {title[:60]}")
                break

        errors = re.findall(
            r"\b(Error|Exception|TypeError|ValueError|SyntaxError|Failed|undefined|null pointer)[:\s]([^\n]{5,60})",
            all_text,
            re.IGNORECASE,
        )
        if errors:
            signals.append(f"Encountered error: {errors[0][1][:60]}")

        return list(dict.fromkeys(signals))[:6]

    def _calc_duration(self, events: list) -> int:
        try:
            times = [datetime.fromisoformat(event.timestamp.replace("Z", "+00:00")) for event in events if event.timestamp]
            if len(times) >= 2:
                return int((max(times) - min(times)).total_seconds())
        except Exception:
            pass
        return 0
