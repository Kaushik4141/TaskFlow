use std::time::Duration;

use super::{slugify_branch, truncate_opt, Ticket};
use reqwest::Client;
use serde_json::Value;
use tokio::time::timeout;

pub struct JiraClient {
    base_url: String,
    email: String,
    token: String,
    client: Client,
}

impl JiraClient {
    pub fn new(base_url: String, email: String, token: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            email,
            token,
            client: Client::new(),
        }
    }

    pub async fn test_connection(&self) -> Result<String, String> {
        let response = self
            .send(
                self.client
                    .get(format!("{}/rest/api/3/myself", self.base_url))
                    .basic_auth(&self.email, Some(&self.token)),
            )
            .await?;

        let body = response
            .json::<Value>()
            .await
            .map_err(|err| err.to_string())?;
        Ok(body
            .get("displayName")
            .and_then(Value::as_str)
            .unwrap_or("Jira user")
            .to_string())
    }

    pub async fn get_assigned_issues(&self, max_results: u32) -> Result<Vec<Ticket>, String> {
        let jql = "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC";
        let max_results = max_results.min(50).to_string();
        let response = self
            .send(
                self.client
                    .get(format!("{}/rest/api/3/search", self.base_url))
                    .basic_auth(&self.email, Some(&self.token))
                    .query(&[
                        ("jql", jql),
                        ("maxResults", max_results.as_str()),
                        (
                            "fields",
                            "summary,description,labels,priority,status,project,assignee",
                        ),
                    ]),
            )
            .await?;

        let body = response
            .json::<Value>()
            .await
            .map_err(|err| err.to_string())?;
        let issues = body
            .get("issues")
            .and_then(Value::as_array)
            .ok_or_else(|| "Jira response did not include issues".to_string())?;

        Ok(issues.iter().map(|issue| self.parse_issue(issue)).collect())
    }

    pub async fn get_issue(&self, issue_key: &str) -> Result<Ticket, String> {
        let response = self
            .send(
                self.client
                    .get(format!("{}/rest/api/3/issue/{}", self.base_url, issue_key))
                    .basic_auth(&self.email, Some(&self.token))
                    .query(&[(
                        "fields",
                        "summary,description,labels,priority,status,project,assignee",
                    )]),
            )
            .await?;
        let body = response
            .json::<Value>()
            .await
            .map_err(|err| err.to_string())?;
        Ok(self.parse_issue(&body))
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response, String> {
        let response = timeout(Duration::from_secs(10), request.send())
            .await
            .map_err(|_| "Jira request timed out after 10 seconds".to_string())?
            .map_err(|err| format!("Jira request failed: {err}"))?;

        if response.status().is_success() {
            Ok(response)
        } else {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            Err(format!("Jira returned {status}: {}", readable_error(&text)))
        }
    }

    fn parse_issue(&self, issue: &Value) -> Ticket {
        let fields = issue.get("fields").unwrap_or(&Value::Null);
        let key = issue.get("key").and_then(Value::as_str).unwrap_or("JIRA");
        let title = fields
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or("Untitled Jira issue")
            .to_string();
        let labels = fields
            .get("labels")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .filter(|value| !value.is_empty());
        let description = fields
            .get("description")
            .map(extract_adf_text)
            .filter(|value| !value.trim().is_empty());
        let first_words = title
            .split_whitespace()
            .take(5)
            .collect::<Vec<_>>()
            .join(" ");
        let branch = slugify_branch(None, &format!("{key}-{first_words}"), 50);

        Ticket {
            id: format!("jira:{}:{}", self.base_url, key),
            integration_id: None,
            provider: "jira".to_string(),
            ticket_id: key.to_string(),
            title,
            description: truncate_opt(description, 2000),
            labels,
            priority: fields
                .get("priority")
                .and_then(|value| value.get("name"))
                .and_then(Value::as_str)
                .map(ToString::to_string),
            assignee: fields
                .get("assignee")
                .and_then(|value| value.get("displayName"))
                .and_then(Value::as_str)
                .map(ToString::to_string),
            project: fields
                .get("project")
                .and_then(|value| value.get("name"))
                .and_then(Value::as_str)
                .map(ToString::to_string),
            url: Some(format!("{}/browse/{}", self.base_url, key)),
            branch: Some(branch),
            raw: serde_json::to_string(issue).ok(),
            fetched_at: String::new(),
        }
    }
}

pub fn extract_adf_text(adf: &Value) -> String {
    fn walk(node: &Value, lines: &mut Vec<String>, current: &mut String) {
        match node.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = node.get("text").and_then(Value::as_str) {
                    current.push_str(text);
                }
            }
            Some("hardBreak") => {
                flush(lines, current);
            }
            Some("paragraph") | Some("heading") | Some("blockquote") | Some("listItem")
            | Some("bulletList") | Some("orderedList") | Some("codeBlock") => {
                if let Some(content) = node.get("content").and_then(Value::as_array) {
                    for child in content {
                        walk(child, lines, current);
                    }
                }
                flush(lines, current);
                return;
            }
            _ => {}
        }

        if let Some(content) = node.get("content").and_then(Value::as_array) {
            for child in content {
                walk(child, lines, current);
            }
        }
    }

    fn flush(lines: &mut Vec<String>, current: &mut String) {
        let trimmed = current.trim();
        if !trimmed.is_empty() {
            lines.push(trimmed.to_string());
            current.clear();
        }
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    walk(adf, &mut lines, &mut current);
    flush(&mut lines, &mut current);
    lines.join("\n")
}

fn readable_error(text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|json| {
            json.get("errorMessages")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .or_else(|| {
                    json.get("message")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| text.chars().take(240).collect())
}
