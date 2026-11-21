use std::collections::{BTreeMap, BTreeSet};
use serde::{Deserialize, Serialize};

use crate::database::events::Event;
use crate::database::tasks::Task;

#[derive(Clone)]
pub struct AiClient {
    #[allow(dead_code)]
    base_url: String,
    client: reqwest::Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EventPayload {
    pub id: String,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub content: Option<String>,
    pub url: Option<String>,
    pub content_type: Option<String>,
    pub capture_method: Option<String>,
    pub event_type: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ScoredEvent {
    pub id: String,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub content: Option<String>,
    pub url: Option<String>,
    pub event_type: String,
    pub timestamp: String,
    pub relevance_score: f64,
    pub reason: String,
    pub included: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FilterRequest {
    pub task_title: String,
    pub task_description: String,
    pub source_labels: Option<String>,
    pub source_priority: Option<String>,
    pub source_project: Option<String>,
    pub source_body: Option<String>,
    pub events: Vec<EventPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FilterResponse {
    pub scored_events: Vec<ScoredEvent>,
    pub total_events: usize,
    pub relevant_count: usize,
    pub filter_threshold: f64,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SummarizeRequest {
    pub task_title: String,
    pub task_description: String,
    pub relevant_events: Vec<ScoredEvent>,
    pub mode: String,
    pub cloud_base_url: Option<String>,
    pub cloud_api_key: Option<String>,
    pub cloud_model: Option<String>,
    pub ollama_url: Option<String>,
    pub ollama_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SummarizeResponse {
    pub markdown: String,
    pub summary: String,
    pub key_points: Vec<String>,
    pub resources: Vec<String>,
    pub duration_seconds: Option<i64>,
    pub generated_locally: bool,
    #[serde(default)]
    pub method: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AskMemoryRequest {
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ollama_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ollama_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AskMemoryResponse {
    pub query: String,
    pub answer: String,
    pub scoped_count: usize,
    #[serde(default)]
    pub results: Vec<serde_json::Value>,
}

const HIGH_TRUST_APPS: &[&str] = &[
    "code", "vscodium", "vim", "nvim", "neovim",
    "pycharm", "intellij", "webstorm", "goland",
    "terminal", "iterm2", "wezterm", "alacritty",
    "postman", "insomnia", "tableplus", "datagrip",
    "github desktop", "sourcetree", "fork",
    "xcode", "android studio", "cursor",
];

const MEDIUM_TRUST_APPS: &[&str] = &[
    "chrome", "firefox", "safari", "edge",
    "brave", "arc", "opera",
];

const EXCLUDED_APPS: &[&str] = &[
    "1password", "bitwarden", "keepass", "lastpass", "dashlane",
    "nordpass", "proton pass", "auth", "authenticator", "duo mobile",
    "yubico", "keychain", "credential manager", "certificate manager",
    "spotify", "music", "vlc", "mpv", "netflix", "youtube", "hulu",
    "prime video", "disney", "obsidian",
];

const FILTER_STOPWORDS: &[&str] = &[
    "the", "a", "an", "is", "in", "on", "at", "to",
    "for", "of", "and", "or", "with", "that", "this", "it", "be", "are",
    "was", "were", "has", "have", "had", "not", "but", "by", "from", "as",
];

const SUMMARY_PROMPT_TEMPLATE: &str = r#"You are a work documentation assistant.
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
- The block body is plain text (no markdown fences inside it). Keep each body <= 60
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
"#;

impl AiClient {
    pub fn new() -> Self {
        AiClient {
            base_url: String::new(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(45))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    /// Pure-Rust engine is built directly into TaskFlow and is always ready.
    pub async fn is_ready(&self) -> bool {
        true
    }

    /// Native, fast event filtering without Python.
    pub async fn filter_events(
        &self,
        task: &Task,
        task_description: &str,
        events: Vec<EventPayload>,
    ) -> Result<FilterResponse, String> {
        let mut text_for_keywords = format!(
            "{} {} {} {}",
            task.title,
            task_description,
            task.source_labels.as_deref().unwrap_or(""),
            task.source_project.as_deref().unwrap_or("")
        );
        if let Some(ref body) = task.source_body {
            text_for_keywords.push(' ');
            text_for_keywords.push_str(&body.chars().take(500).collect::<String>());
        }

        let keywords = extract_keywords(&text_for_keywords);

        let mut scored_events = Vec::with_capacity(events.len());
        for event in events {
            let app_lower = event.app_name.as_deref().unwrap_or("").to_lowercase();
            if EXCLUDED_APPS.iter().any(|ex| app_lower.contains(ex)) {
                scored_events.push(ScoredEvent {
                    id: event.id,
                    app_name: event.app_name,
                    window_title: event.window_title,
                    content: event.content,
                    url: event.url,
                    event_type: event.event_type,
                    timestamp: event.timestamp,
                    relevance_score: 0.0,
                    reason: "excluded_app".to_string(),
                    included: false,
                });
                continue;
            }

            let mut event_text = String::new();
            if let Some(ref a) = event.app_name {
                event_text.push_str(a);
                event_text.push(' ');
            }
            if let Some(ref w) = event.window_title {
                event_text.push_str(w);
                event_text.push(' ');
            }
            if let Some(ref u) = event.url {
                event_text.push_str(u);
                event_text.push(' ');
            }
            if let Some(ref c) = event.content {
                event_text.push_str(&c.chars().take(400).collect::<String>());
            }

            if event_text.trim().is_empty() {
                scored_events.push(ScoredEvent {
                    id: event.id,
                    app_name: event.app_name,
                    window_title: event.window_title,
                    content: event.content,
                    url: event.url,
                    event_type: event.event_type,
                    timestamp: event.timestamp,
                    relevance_score: 0.0,
                    reason: "empty_event".to_string(),
                    included: false,
                });
                continue;
            }

            let event_lower = event_text.to_lowercase();
            let keyword_hits = keywords.iter().filter(|kw| event_lower.contains(*kw)).count();
            let is_high_trust = HIGH_TRUST_APPS.iter().any(|app| app_lower.contains(app));
            let is_med_trust = MEDIUM_TRUST_APPS.iter().any(|app| app_lower.contains(app));

            let (score, reason, included) = if keyword_hits >= 2 {
                (0.85, format!("keyword_match:{keyword_hits}"), true)
            } else if keyword_hits == 1 {
                (0.70, "keyword_match:1".to_string(), true)
            } else if is_high_trust {
                (0.60, "high_trust_app".to_string(), true)
            } else if is_med_trust && event.content.is_some() {
                (0.40, "medium_trust_content".to_string(), true)
            } else {
                (0.20, "low_relevance".to_string(), false)
            };

            scored_events.push(ScoredEvent {
                id: event.id,
                app_name: event.app_name,
                window_title: event.window_title,
                content: event.content,
                url: event.url,
                event_type: event.event_type,
                timestamp: event.timestamp,
                relevance_score: score,
                reason,
                included,
            });
        }

        let relevant_count = scored_events.iter().filter(|e| e.included).count();
        let total_events = scored_events.len();

        Ok(FilterResponse {
            scored_events,
            total_events,
            relevant_count,
            filter_threshold: 0.35,
        })
    }

    /// Pure-Rust summarization: calls Cloud AI or Ollama directly via HTTP without Python,
    /// or generates an extractive local summary.
    pub async fn summarize(
        &self,
        task_title: &str,
        task_description: &str,
        relevant_events: Vec<ScoredEvent>,
        settings: &crate::commands::SummarySettings,
        prior_context: Option<String>,
    ) -> Result<SummarizeResponse, String> {
        if settings.mode == "cloud_ai"
            && !settings.cloud_base_url.is_empty()
            && !settings.cloud_api_key.is_empty()
        {
            match self
                .summarize_cloud(
                    task_title,
                    task_description,
                    &relevant_events,
                    settings,
                    prior_context.as_deref(),
                )
                .await
            {
                Ok(resp) => return Ok(resp),
                Err(err) => {
                    eprintln!("[taskflow:ai] Cloud AI summarization failed: {err}; falling back to local pure-Rust summary");
                }
            }
        } else if settings.mode == "local_ai" && !settings.ollama_url.is_empty() {
            match self
                .summarize_ollama(
                    task_title,
                    task_description,
                    &relevant_events,
                    settings,
                    prior_context.as_deref(),
                )
                .await
            {
                Ok(resp) => return Ok(resp),
                Err(err) => {
                    eprintln!("[taskflow:ai] Ollama summarization failed: {err}; falling back to local pure-Rust summary");
                }
            }
        }

        Ok(self.summarize_basic(task_title, task_description, &relevant_events))
    }

    async fn summarize_cloud(
        &self,
        task_title: &str,
        task_description: &str,
        relevant_events: &[ScoredEvent],
        settings: &crate::commands::SummarySettings,
        prior_context: Option<&str>,
    ) -> Result<SummarizeResponse, String> {
        let prompt = build_summary_prompt(task_title, task_description, relevant_events, prior_context);
        let url = format!("{}/chat/completions", settings.cloud_base_url.trim_end_matches('/'));

        let body = serde_json::json!({
            "model": settings.cloud_model,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": 1500,
            "temperature": 0.3
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", settings.cloud_api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|err| err.to_string())?;

        if !resp.status().is_success() {
            let error_body = resp.text().await.unwrap_or_default();
            return Err(format!("Cloud API error: {error_body}"));
        }

        let json: serde_json::Value = resp.json().await.map_err(|err| err.to_string())?;
        let markdown = json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| "Missing content in choices".to_string())?
            .to_string();

        let summary = extract_tldr(&markdown);
        let key_points = extract_key_points(&markdown);
        let resources = extract_resources(relevant_events);

        Ok(SummarizeResponse {
            markdown,
            summary,
            key_points,
            resources,
            duration_seconds: None,
            generated_locally: false,
            method: Some(format!("cloud_ai ({})", settings.cloud_model)),
        })
    }

    async fn summarize_ollama(
        &self,
        task_title: &str,
        task_description: &str,
        relevant_events: &[ScoredEvent],
        settings: &crate::commands::SummarySettings,
        prior_context: Option<&str>,
    ) -> Result<SummarizeResponse, String> {
        let prompt = build_summary_prompt(task_title, task_description, relevant_events, prior_context);
        let url = format!("{}/api/generate", settings.ollama_url.trim_end_matches('/'));

        let body = serde_json::json!({
            "model": settings.ollama_model,
            "prompt": prompt,
            "stream": false,
            "options": {"temperature": 0.3, "num_predict": 1000}
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|err| err.to_string())?;

        if !resp.status().is_success() {
            let error_body = resp.text().await.unwrap_or_default();
            return Err(format!("Ollama API error: {error_body}"));
        }

        let json: serde_json::Value = resp.json().await.map_err(|err| err.to_string())?;
        let markdown = json["response"]
            .as_str()
            .ok_or_else(|| "Missing response from Ollama".to_string())?
            .to_string();

        let summary = extract_tldr(&markdown);
        let key_points = extract_key_points(&markdown);
        let resources = extract_resources(relevant_events);

        Ok(SummarizeResponse {
            markdown,
            summary,
            key_points,
            resources,
            duration_seconds: None,
            generated_locally: true,
            method: Some(format!("local_ai (Ollama: {})", settings.ollama_model)),
        })
    }

    fn summarize_basic(
        &self,
        task_title: &str,
        task_description: &str,
        events: &[ScoredEvent],
    ) -> SummarizeResponse {
        let mut unique_apps = BTreeSet::new();
        let mut app_timeline: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut resources = Vec::new();

        for event in events {
            let app = event.app_name.as_deref().unwrap_or("App");
            unique_apps.insert(app.to_string());

            let title = event.window_title.as_deref().unwrap_or("Activity");
            let list = app_timeline.entry(app.to_string()).or_default();
            if list.len() < 5 && !list.iter().any(|item| item == title) {
                list.push(title.to_string());
            }

            if let Some(ref url) = event.url {
                if !resources.contains(url) {
                    resources.push(url.clone());
                }
            }
        }

        let app_list: Vec<String> = unique_apps.into_iter().collect();
        let summary = if app_list.is_empty() {
            format!("Worked on '{task_title}'.")
        } else {
            format!("Worked on '{task_title}' using {}.", app_list.join(", "))
        };

        let mut md = format!("# {task_title}\n\n");
        if !task_description.is_empty() {
            md.push_str(&format!("**Description:** {task_description}\n\n"));
        }
        md.push_str(&format!("## Summary\n{summary}\n\n"));

        md.push_str("## Activity Timeline\n");
        for (app, titles) in &app_timeline {
            md.push_str(&format!("### {app}\n"));
            for t in titles {
                md.push_str(&format!("- {t}\n"));
            }
        }
        md.push('\n');

        md.push_str("## Key Points\n");
        let mut key_points = Vec::new();
        for (app, titles) in app_timeline.iter().take(4) {
            if let Some(first) = titles.first() {
                let kp = format!("Engaged with {app} on {first}");
                md.push_str(&format!("- {kp}\n"));
                key_points.push(kp);
            }
        }
        md.push('\n');

        md.push_str("## Resources Referenced\n");
        if resources.is_empty() {
            md.push_str("- No external resources captured.\n\n");
        } else {
            for r in resources.iter().take(10) {
                md.push_str(&format!("- {r}\n"));
            }
            md.push('\n');
        }

        SummarizeResponse {
            markdown: md,
            summary,
            key_points,
            resources: resources.into_iter().take(10).collect(),
            duration_seconds: None,
            generated_locally: true,
            method: Some("basic (pure Rust extractive)".to_string()),
        }
    }

    /// Pure-Rust embedding: returns empty vector so search relies seamlessly on
    /// SQLite FTS5 / BM25 keyword matching with zero PyTorch memory overhead.
    pub async fn embed_text(&self, _text: &str) -> Result<Vec<f32>, String> {
        Ok(Vec::new())
    }

    pub async fn ask_memory(&self, req: &AskMemoryRequest) -> Result<AskMemoryResponse, String> {
        let context = req.context.as_deref().unwrap_or("No context available.");
        let prompt = format!(
            "You are the TaskFlow memory engine. Answer this user's question concisely in 2-3 sentences \
            using ONLY the recorded desktop memory context below. Mention dates, projects, tools, and actions.\n\n\
            Question: {}\n\n\
            Memory Context:\n{}\n\n\
            Answer:",
            req.query, context
        );

        let answer = if req.mode.as_deref() == Some("cloud_ai")
            && req.cloud_base_url.is_some()
            && req.cloud_api_key.is_some()
        {
            let url = format!("{}/chat/completions", req.cloud_base_url.as_deref().unwrap().trim_end_matches('/'));
            let body = serde_json::json!({
                "model": req.cloud_model.as_deref().unwrap_or("default"),
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": 250,
                "temperature": 0.2
            });

            match self.client
                .post(&url)
                .header("Authorization", format!("Bearer {}", req.cloud_api_key.as_deref().unwrap()))
                .json(&body)
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    resp.json::<serde_json::Value>().await.ok().and_then(|v| {
                        v["choices"][0]["message"]["content"].as_str().map(str::trim).map(ToString::to_string)
                    })
                }
                _ => None,
            }
        } else if req.mode.as_deref() == Some("local_ai") && req.ollama_url.is_some() {
            let url = format!("{}/api/generate", req.ollama_url.as_deref().unwrap().trim_end_matches('/'));
            let body = serde_json::json!({
                "model": req.ollama_model.as_deref().unwrap_or("llama3.1:8b"),
                "prompt": prompt,
                "stream": false
            });
            match self.client.post(&url).json(&body).send().await {
                Ok(resp) if resp.status().is_success() => {
                    resp.json::<serde_json::Value>().await.ok().and_then(|v| {
                        v["response"].as_str().map(str::trim).map(ToString::to_string)
                    })
                }
                _ => None,
            }
        } else {
            None
        };

        let answer_text = answer.unwrap_or_else(|| {
            format!("Based on your recorded memory: {}", context.chars().take(200).collect::<String>())
        });

        Ok(AskMemoryResponse {
            query: req.query.clone(),
            answer: answer_text,
            scoped_count: 1,
            results: Vec::new(),
        })
    }
}

fn extract_keywords(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|w| w.to_lowercase())
        .filter(|w| w.len() > 3 && !FILTER_STOPWORDS.contains(&w.as_str()))
        .collect()
}

fn extract_tldr(markdown: &str) -> String {
    let mut in_tldr = false;
    for line in markdown.lines() {
        if line.contains("## TLDR") || line.contains("## Summary") {
            in_tldr = true;
            continue;
        }
        if in_tldr && line.starts_with("##") {
            break;
        }
        if in_tldr && !line.trim().is_empty() {
            return line.trim().to_string();
        }
    }
    String::new()
}

fn extract_key_points(markdown: &str) -> Vec<String> {
    let mut points = Vec::new();
    let mut in_kp = false;
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.contains("## Key Points") || trimmed.contains("## Key Highlights") {
            in_kp = true;
            continue;
        }
        if in_kp && trimmed.starts_with("##") {
            break;
        }
        if in_kp && (trimmed.starts_with('-') || trimmed.starts_with('*') || trimmed.starts_with('•')) {
            let cleaned = trimmed.trim_start_matches(|c| c == '-' || c == '*' || c == '•' || c == ' ').trim();
            if cleaned.len() >= 8 {
                points.push(cleaned.to_string());
            }
        }
    }
    points
}

fn extract_resources(events: &[ScoredEvent]) -> Vec<String> {
    let mut res = Vec::new();
    for e in events {
        if let Some(ref url) = e.url {
            if !res.contains(url) {
                res.push(url.clone());
            }
        }
    }
    res
}

fn build_activity_context(events: &[ScoredEvent]) -> String {
    let mut by_source: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for e in events {
        let src = e.app_name.as_deref().unwrap_or("Unknown");
        let mut text = String::new();
        if let Some(ref t) = e.window_title {
            text.push_str(t);
        }
        if let Some(ref c) = e.content {
            if !text.is_empty() {
                text.push_str(" | ");
            }
            text.push_str(&c.chars().take(300).collect::<String>());
        }
        if !text.is_empty() {
            by_source.entry(src.to_string()).or_default().push(text);
        }
    }

    if by_source.is_empty() {
        return "No content captured.".to_string();
    }

    let mut sections = Vec::new();
    let mut used = 0;
    let char_budget = 12000;

    for (source, texts) in by_source {
        if used >= char_budget {
            break;
        }
        let combined = texts.join("\n");
        let chunk: String = combined.chars().take(2000).collect();
        let section = format!("### {source}\n{chunk}\n");
        let len = section.len();
        sections.push(section);
        used += len;
    }

    sections.join("\n")
}

fn build_summary_prompt(
    task_title: &str,
    task_description: &str,
    events: &[ScoredEvent],
    prior_context: Option<&str>,
) -> String {
    let mut unique_apps = BTreeSet::new();
    for e in events {
        if let Some(ref app) = e.app_name {
            unique_apps.insert(app.replace(".exe", ""));
        }
    }
    let apps_str = unique_apps.into_iter().take(5).collect::<Vec<_>>().join(", ");
    let activity_ctx = build_activity_context(events);

    let prior_block = if let Some(prior) = prior_context {
        if !prior.trim().is_empty() {
            format!("\nPrior Memory Tree context (accumulated from previous daily notes and hub pages):\n{}\n", prior.trim())
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let duration_str = format!("{} events", events.len());
    let title_str = if !task_description.trim().is_empty() {
        format!("{task_title}\nTask Description: {}", task_description.trim())
    } else {
        task_title.to_string()
    };

    SUMMARY_PROMPT_TEMPLATE
        .replace("{task_title}", &title_str)
        .replace("{duration}", &duration_str)
        .replace("{apps}", &apps_str)
        .replace("{prior_block}", &prior_block)
        .replace("{activity_context}", &activity_ctx)
}

impl From<Event> for EventPayload {
    fn from(event: Event) -> Self {
        EventPayload {
            id: event.id,
            app_name: event.app_name,
            window_title: event.window_title,
            content: event.content,
            url: event.url,
            content_type: event.content_type,
            capture_method: event.capture_method,
            event_type: event.event_type,
            timestamp: event.timestamp,
        }
    }
}
