from collections import defaultdict
from urllib.parse import urlparse

import nltk
from sumy.nlp.stemmers import Stemmer
from sumy.nlp.tokenizers import Tokenizer
from sumy.parsers.plaintext import PlaintextParser
from sumy.summarizers.lex_rank import LexRankSummarizer
from sumy.utils import get_stop_words

LANGUAGE = "english"


class BasicSummarizer:
    def __init__(self):
        self.lexrank_available = self._has_tokenizer_data()
        self.stemmer = Stemmer(LANGUAGE)
        self.summarizer = LexRankSummarizer(self.stemmer)
        self.summarizer.stop_words = get_stop_words(LANGUAGE)

    def summarize(self, context: dict) -> dict:
        title = context["task_title"]
        duration = self._format_dur(context["duration_secs"])
        signals = context["signals"]
        timeline = context["timeline"]
        resources = context["resources"]
        apps = context["apps_used"]
        chunks = context["clean_chunks"]

        summary = self._build_summary(title, duration, apps, signals, resources)
        key_points = signals.copy()
        all_text = " ".join(chunk["text"] for chunk in chunks if len(chunk["text"]) > 30)

        if self.lexrank_available and len(all_text.split()) > 100:
            for point in self._run_lexrank(all_text, 3):
                point = point.strip()
                if len(point) > 20 and point not in key_points and not point.startswith("http") and len(key_points) < 6:
                    key_points.append(point)

        markdown = self._build_markdown(
            title,
            context["task_description"],
            duration,
            context.get("event_count", len(chunks)),
            summary,
            timeline,
            key_points,
            resources,
        )
        return {
            "markdown": markdown,
            "summary": summary,
            "key_points": key_points,
            "resources": resources[:10],
            "generated_locally": True,
            "mode": "basic",
        }

    def _has_tokenizer_data(self) -> bool:
        try:
            nltk.data.find("tokenizers/punkt")
            return True
        except LookupError:
            print("LexRank tokenizer data not found; basic mode will use structured signals only.", flush=True)
            return False

    def _run_lexrank(self, text: str, max_sentences: int) -> list:
        try:
            parser = PlaintextParser.from_string(text, Tokenizer(LANGUAGE))
            return [str(sentence) for sentence in self.summarizer(parser.document, max_sentences)]
        except Exception as error:
            print(f"LexRank error: {error}", flush=True)
            return []

    def _build_summary(self, title, duration, apps, signals, resources) -> str:
        parts = [f"Spent {duration} on '{title}'."]
        if apps:
            parts.append(f"Used {', '.join(app.replace('.exe', '') for app in apps[:3])}.")
        domains = list(dict.fromkeys(urlparse(url).netloc for url in resources if url.startswith("http")))
        if domains:
            parts.append(f"Visited {', '.join(domains[:2])}.")
        if signals:
            parts.append(signals[0] + ".")
        return " ".join(parts)

    def _build_markdown(self, title, description, duration, event_count, summary, timeline, key_points, resources) -> str:
        md = f"# {title}\n\n"
        if description:
            md += f"**Description:** {description}\n"
        md += f"**Duration:** {duration}  \n"
        md += f"**Events captured:** {event_count}\n\n"
        md += f"## Summary\n{summary}\n\n"
        md += "## Activity Timeline\n"
        by_app = defaultdict(list)
        for page in timeline:
            by_app[page["app"]].append(page)
        for app, pages in by_app.items():
            if "taskflow" in app.lower():
                continue
            md += f"\n### {app.replace('.exe', '')}\n"
            for page in pages[:6]:
                badge = ""
                if page.get("url"):
                    badge = f" `{urlparse(page['url']).netloc}`"
                md += f"- {page['title']}{badge}\n"
        if key_points:
            md += "\n## Key Points\n"
            for point in key_points:
                md += f"- {point}\n"
        if resources:
            md += "\n## Resources Referenced\n"
            for url in resources[:8]:
                md += f"- {url}\n"
        else:
            md += "\n## Resources Referenced\n- No external resources captured.\n"
        return md

    def _format_dur(self, seconds: int) -> str:
        if seconds < 60:
            return f"{seconds}s"
        if seconds < 3600:
            minutes, secs = divmod(seconds, 60)
            return f"{minutes}m {secs}s" if secs else f"{minutes}m"
        hours, remainder = divmod(seconds, 3600)
        return f"{hours}h {remainder // 60}m"
