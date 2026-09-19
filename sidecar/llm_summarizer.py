import httpx
from typing import Optional


def normalize_base_url(url: str) -> str:
    return url.rstrip("/")


SUMMARY_PROMPT = """You are a work documentation assistant.
A user just completed a work session. Based on their captured screen activity, generate comprehensive documentation.

Task: {task_title}
Duration: {duration}
Apps used: {apps}
{prior_block}
Captured activity by source:
{activity_context}

Generate documentation using EXACTLY these section headings and structure.
Use GitHub-flavored Markdown. Be concise and scannable.

# {task_title}

## Summary
2-3 sentences. What did the user work on overall?
Be specific — use actual names, tools, and sites from the data.
Where useful, connect today's activity to the prior context.

## Activity Timeline
Group by application. Use a `### {{App Name}}` subheading for each app, then
list the key pages/actions as bullets. Include a backtick-quoted domain badge
when a URL was visited. Skip the TaskFlow app itself. Max 5 bullets per app.

## Key Points
Bullet list of the most important actions and takeaways.
Use action verbs: reviewed, wrote, debugged, researched, shipped, etc.
Reference concrete details from the activity data. Max 6 bullets.

## Resources Referenced
List specific URLs, files, or tools that were actually referenced.
One per line as a bullet. Only include what appears in the activity data.
If none, write: "- No external resources captured."

## Next Steps
Optional. What would logically come next based on this session? Max 3 bullets.
If nothing clearly follows, omit this entire section.

## Hub Synthesis  (machine-readable — do NOT render as prose)
After the sections above, append ONE fenced code block for each app AND project the
user touched today. These maintain TaskFlow's long-term "second brain" pages — they
are NOT shown to the human in the daily note, they are parsed and filed onto entity
pages. This is the only place your prior-context memory of the apps/projects
matters: integrate today's activity into a running description of what the user has
been doing with that app/project over time.

For each app in "Apps used" above, AND for each project implied by the activity
(typically the leading token of editor window titles, e.g. "TaskFlow", "Next.js"),
emit a block in EXACTLY this fenced format — the info string is the marker:

```hub:Apps/<slug>
<slug> is the app name lowercased with non-alphanumerics replaced by "-",
e.g. chrome, Code, Cursor. Keep the existing casing from the app name.
A 1–3 sentence running synthesis: what the user uses this app FOR, the kind of
work they did in it today, and — when prior context for this app exists — how
today continues or shifts that pattern. Be specific (real titles/files/sites).
```

```hub:Projects/<slug>
<slug> is the project name TitleCased as it appears in titles, e.g. TaskFlow.
This body becomes the project page's `## Status` section — its running narrative.
A 1–3 sentence synthesis: the project's current goal/status, what advanced today
(files touched, commands run, decisions made), and continuity with prior days when
context exists. Infer a project ONLY from a clear, repeated proper-noun signal in
window titles/URLs/paths (a repo or app name appearing in multiple events) — never
from a single generic word like "Array", "Usage", or "Registration", and never
from an app name (Chrome/Firefox/Notion are apps, not projects).
```

Rules for Hub Synthesis:
- Emit a block for each app that had real activity (skip Noise/OS apps like
  Explorer, SearchHost, ShellHost, LockApp, SnippingTool, PickerHost). If a
  project can't be inferred, emit no Projects block — do not invent one.
- One fenced block per app/project. Info string must be exactly `hub:<Folder>/<slug>`.
- The block body is plain text (no markdown fences inside it). Keep each body ≤ 60
  words. This is the compounding memory layer — where prior context exists, fold
  today in rather than restating from scratch.
- Never invent facts not in the activity or prior context.
- If there is nothing to synthesize (no meaningful activity), emit no block.

Rules:
- Use ONLY the section headings above, in that order. No extras — except the
  `hub:...` fenced blocks, which always come last.
- Be factual. Only use information from the activity data or prior context.
- Never invent or hallucinate details, names, URLs, or numbers.
- If activity data is sparse, say so honestly and keep sections short.
- The prior context is for continuity only — do not plagiarize it; cite it
  only when today's activity genuinely builds on it.
- Output a single blank line between sections, no trailing blank lines.
"""

PRIOR_BLOCK_HEADER = """
Prior Memory Tree context (accumulated from previous daily notes and hub pages):
{prior_context}
"""


def build_activity_context(context: dict) -> str:
    sections = []
    char_budget = 12000
    used = 0
    for source, texts in context.get("raw_text_by_source", {}).items():
        if used >= char_budget:
            break
        combined = "\n".join(texts)
        section = f"### {source}\n{combined[:2000]}\n"
        remaining = char_budget - used
        sections.append(section[:remaining])
        used += len(section)
    if context.get("clipboard_items"):
        clipboard = "### Clipboard (user copied)\n"
        for item in context["clipboard_items"][:3]:
            clipboard += f"- {item[:200]}\n"
        sections.append(clipboard)
    return "\n".join(sections) if sections else "No content captured."


def build_prior_block(context: dict) -> str:
    prior = context.get("prior_context")
    if not prior or not prior.strip():
        return ""
    return PRIOR_BLOCK_HEADER.format(prior_context=prior.strip())


class LLMSummarizer:
    async def summarize(
        self,
        context: dict,
        mode: str,
        cloud_base_url: Optional[str] = None,
        cloud_api_key: Optional[str] = None,
        cloud_model: Optional[str] = None,
        ollama_url: str = "http://localhost:11434",
        ollama_model: str = "llama3.1:8b",
    ) -> dict:
        prompt = SUMMARY_PROMPT.format(
            task_title=context["task_title"],
            duration=self._format_dur(context["duration_secs"]),
            apps=", ".join(app.replace(".exe", "") for app in context["apps_used"][:5]),
            prior_block=build_prior_block(context),
            activity_context=build_activity_context(context),
        )
        if mode == "local_ai":
            method = f"local_ai (Ollama: {ollama_model} @ {ollama_url})"
            print(f"SUMMARIZE: LLMSummarizer using method '{method}'", flush=True)
            markdown = await self._call_ollama(prompt, ollama_url, ollama_model)
        elif mode == "cloud_ai":
            method = f"cloud_ai (Model: {cloud_model} @ {cloud_base_url})"
            print(f"SUMMARIZE: LLMSummarizer using method '{method}'", flush=True)
            markdown = await self._call_cloud(prompt, cloud_base_url, cloud_api_key, cloud_model)
        else:
            raise ValueError(f"Unknown mode: {mode}")
        return {
            "markdown": markdown,
            "summary": self._extract_tldr(markdown),
            "key_points": self._extract_key_points(markdown),
            "resources": context["resources"][:10],
            "generated_locally": mode == "local_ai",
            "mode": mode,
            "method": method,
        }

    async def _call_ollama(self, prompt: str, url: str, model: str) -> str:
        base_url = (url or "http://localhost:11434").rstrip("/")
        try:
            async with httpx.AsyncClient(timeout=300.0) as client:
                response = await client.post(
                    f"{base_url}/api/generate",
                    json={
                        "model": model,
                        "prompt": prompt,
                        "stream": False,
                        "options": {"temperature": 0.3, "num_predict": 1000},
                    },
                )
                response.raise_for_status()
                return response.json().get("response", "")
        except httpx.TimeoutException:
            raise Exception("Ollama timeout. Try a smaller model like qwen2.5:3b")
        except Exception as error:
            raise Exception(f"Ollama error: {str(error)}")

    async def _call_cloud(self, prompt: str, base_url: Optional[str], api_key: str, model: Optional[str]) -> str:
        if not api_key:
            raise ValueError("Cloud API key is required")
        if not base_url:
            raise ValueError("Cloud base URL is required")
        if not model:
            raise ValueError("Cloud model is required")
        url = f"{normalize_base_url(base_url)}/chat/completions"
        async with httpx.AsyncClient(timeout=60.0) as client:
            response = await client.post(
                url,
                headers={"Authorization": f"Bearer {api_key}", "Content-Type": "application/json"},
                json={
                    "model": model,
                    "max_tokens": 1500,
                    "temperature": 0.3,
                    "messages": [{"role": "user", "content": prompt}],
                },
            )
            if response.status_code != 200:
                raise Exception(f"Cloud API error {response.status_code}: {response.text[:200]}")
            return response.json()["choices"][0]["message"]["content"]

    def _extract_tldr(self, markdown: str) -> str:
        in_tldr = False
        for line in markdown.split("\n"):
            if "## TLDR" in line or "## Summary" in line:
                in_tldr = True
                continue
            if in_tldr and line.startswith("##"):
                break
            if in_tldr and line.strip():
                return line.strip()
        return ""

    def _extract_key_points(self, markdown: str) -> list[str]:
        points = []
        in_kp = False
        for line in markdown.split("\n"):
            trimmed = line.strip()
            if "## Key Points" in trimmed or "## Key Highlights" in trimmed:
                in_kp = True
                continue
            if in_kp and trimmed.startswith("##"):
                break
            if in_kp and (trimmed.startswith("-") or trimmed.startswith("*") or trimmed.startswith("•")):
                cleaned = trimmed.lstrip("-*• ").strip()
                if cleaned and len(cleaned) >= 8:
                    points.append(cleaned)
        return points

    def _format_dur(self, seconds: int) -> str:
        if seconds < 60:
            return f"{seconds}s"
        if seconds < 3600:
            minutes, secs = divmod(seconds, 60)
            return f"{minutes}m {secs}s" if secs else f"{minutes}m"
        hours, remainder = divmod(seconds, 3600)
        return f"{hours}h {remainder // 60}m"
