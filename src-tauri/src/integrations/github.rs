use std::time::Duration;

use reqwest::Client;
use serde_json::Value;
use tokio::time::timeout;

use super::{slugify_branch, truncate_opt, Ticket};

pub struct GitHubClient {
    token: String,
    client: Client,
}

impl GitHubClient {
    pub fn new(token: String) -> Self {
        Self {
            token,
            client: Client::new(),
        }
    }

    pub async fn test_connection(&self) -> Result<String, String> {
        let response = self
            .send(self.client.get("https://api.github.com/user"))
            .await?;
        let body = response
            .json::<Value>()
            .await
            .map_err(|err| err.to_string())?;
        Ok(body
            .get("login")
            .and_then(Value::as_str)
            .unwrap_or("GitHub user")
            .to_string())
    }

    pub async fn get_assigned_issues(&self, max_results: u32) -> Result<Vec<Ticket>, String> {
        let per_page = max_results.min(50).to_string();
        let response = self
            .send(self.client.get("https://api.github.com/issues").query(&[
                ("filter", "assigned"),
                ("state", "open"),
                ("per_page", per_page.as_str()),
            ]))
            .await?;
        let body = response
            .json::<Value>()
            .await
            .map_err(|err| err.to_string())?;
        let issues = body
            .as_array()
            .ok_or_else(|| "GitHub response did not include issues".to_string())?;

        Ok(issues.iter().map(parse_issue).collect())
    }

    pub async fn get_issue(&self, owner: &str, repo: &str, number: u32) -> Result<Ticket, String> {
        let response = self
            .send(self.client.get(format!(
                "https://api.github.com/repos/{owner}/{repo}/issues/{number}"
            )))
            .await?;
        let body = response
            .json::<Value>()
            .await
            .map_err(|err| err.to_string())?;
        Ok(parse_issue(&body))
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response, String> {
        let response = timeout(
            Duration::from_secs(10),
            request
                .bearer_auth(&self.token)
                .header("Accept", "application/vnd.github.v3+json")
                .header("User-Agent", "TaskFlow/1.0")
                .send(),
        )
        .await
        .map_err(|_| "GitHub request timed out after 10 seconds".to_string())?
        .map_err(|err| format!("GitHub request failed: {err}"))?;

        if response.status().is_success() {
            Ok(response)
        } else {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            Err(format!(
                "GitHub returned {status}: {}",
                readable_error(&text)
            ))
        }
    }
}

fn parse_issue(issue: &Value) -> Ticket {
    let number = issue.get("number").and_then(Value::as_u64).unwrap_or(0);
    let title = issue
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Untitled GitHub issue")
        .to_string();
    let labels = issue
        .get("labels")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|label| label.get("name").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(",")
        })
        .filter(|value| !value.is_empty());
    let repository = issue
        .get("repository_url")
        .and_then(Value::as_str)
        .and_then(repository_from_url);
    let priority = labels.as_deref().map(priority_from_labels);
    let branch = slugify_branch(Some("feature"), &format!("{number}-{title}"), 50);

    Ticket {
        id: format!(
            "github:{}:{}",
            repository.clone().unwrap_or_else(|| "unknown".to_string()),
            number
        ),
        integration_id: None,
        provider: "github".to_string(),
        ticket_id: format!("#{number}"),
        title,
        description: truncate_opt(
            issue
                .get("body")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            2000,
        ),
        labels,
        priority: Some(priority.unwrap_or("Medium").to_string()),
        assignee: issue
            .get("assignee")
            .and_then(|value| value.get("login"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        project: repository,
        url: issue
            .get("html_url")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        branch: Some(branch),
        raw: serde_json::to_string(issue).ok(),
        fetched_at: String::new(),
    }
}

fn repository_from_url(url: &str) -> Option<String> {
    let parts = url.split('/').rev().take(2).collect::<Vec<_>>();
    if parts.len() == 2 {
        Some(format!("{}/{}", parts[1], parts[0]))
    } else {
        None
    }
}

fn priority_from_labels(labels: &str) -> &'static str {
    let lower = labels.to_lowercase();
    if lower.contains("critical") || labels.contains("P0") {
        "Critical"
    } else if lower.contains("high") || labels.contains("P1") {
        "High"
    } else if lower.contains("low") || labels.contains("P3") {
        "Low"
    } else {
        "Medium"
    }
}

fn readable_error(text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|json| {
            json.get("message")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| text.chars().take(240).collect())
}
