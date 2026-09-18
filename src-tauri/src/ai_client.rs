use serde::{Deserialize, Serialize};

use crate::database::events::Event;
use crate::database::tasks::Task;

#[derive(Clone)]
pub struct AiClient {
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

#[derive(Debug, Deserialize)]
struct HealthResponse {
    status: String,
}

impl AiClient {
    pub fn new() -> Self {
        AiClient {
            base_url: "http://127.0.0.1:7878".to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn is_ready(&self) -> bool {
        match self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => response
                .json::<HealthResponse>()
                .await
                .map(|health| health.status == "ready")
                .unwrap_or(false),
            _ => false,
        }
    }

    pub async fn filter_events(
        &self,
        task: &Task,
        task_description: &str,
        events: Vec<EventPayload>,
    ) -> Result<FilterResponse, String> {
        let request = FilterRequest {
            task_title: task.title.clone(),
            task_description: task_description.to_string(),
            source_labels: task.source_labels.clone(),
            source_priority: task.source_priority.clone(),
            source_project: task.source_project.clone(),
            source_body: task
                .source_body
                .clone()
                .map(|body| body.chars().take(500).collect()),
            events,
        };

        self.client
            .post(format!("{}/filter", self.base_url))
            .json(&request)
            .send()
            .await
            .map_err(|err| err.to_string())?
            .error_for_status()
            .map_err(|err| err.to_string())?
            .json::<FilterResponse>()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn summarize(
        &self,
        task_title: &str,
        task_description: &str,
        relevant_events: Vec<ScoredEvent>,
        settings: &crate::commands::SummarySettings,
        prior_context: Option<String>,
    ) -> Result<SummarizeResponse, String> {
        let request = SummarizeRequest {
            task_title: task_title.to_string(),
            task_description: task_description.to_string(),
            relevant_events,
            mode: settings.mode.clone(),
            cloud_base_url: Some(settings.cloud_base_url.clone()),
            cloud_api_key: Some(settings.cloud_api_key.clone()),
            cloud_model: Some(settings.cloud_model.clone()),
            ollama_url: Some(settings.ollama_url.clone()),
            ollama_model: Some(settings.ollama_model.clone()),
            prior_context,
        };

        self.client
            .post(format!("{}/summarize", self.base_url))
            .json(&request)
            .send()
            .await
            .map_err(|err| err.to_string())?
            .error_for_status()
            .map_err(|err| err.to_string())?
            .json::<SummarizeResponse>()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn embed_text(&self, text: &str) -> Result<Vec<f32>, String> {
        #[derive(Deserialize)]
        struct EmbedResponse {
            vector: Vec<f64>,
            #[allow(dead_code)]
            dimensions: usize,
        }

        let response = self
            .client
            .post(format!("{}/embed", self.base_url))
            .json(&serde_json::json!({ "text": text }))
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|err| format!("Failed to reach sidecar for embedding: {err}"))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Embedding error: {error_text}"));
        }

        let result: EmbedResponse = response
            .json()
            .await
            .map_err(|err| format!("Failed to parse embedding response: {err}"))?;

        Ok(result.vector.into_iter().map(|v| v as f32).collect())
    }
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
