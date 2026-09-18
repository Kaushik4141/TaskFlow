use std::time::Duration;

use reqwest::Client;
use serde_json::{json, Value};
use tokio::time::timeout;

use super::{truncate_opt, Ticket};

pub struct LinearClient {
    token: String,
    client: Client,
}

impl LinearClient {
    pub fn new(token: String) -> Self {
        Self {
            token,
            client: Client::new(),
        }
    }

    pub async fn test_connection(&self) -> Result<String, String> {
        let body = self.graphql("{ viewer { name email } }", None).await?;
        Ok(body
            .get("data")
            .and_then(|data| data.get("viewer"))
            .and_then(|viewer| viewer.get("name").or_else(|| viewer.get("email")))
            .and_then(Value::as_str)
            .unwrap_or("Linear user")
            .to_string())
    }

    pub async fn get_assigned_issues(&self, max_results: u32) -> Result<Vec<Ticket>, String> {
        let query = r#"
        query AssignedIssues($first: Int!) {
          issues(
            filter: {
              assignee: { isMe: { eq: true } }
              state: { type: { nin: ["completed", "cancelled"] } }
            }
            first: $first
            orderBy: updatedAt
          ) {
            nodes {
              id
              identifier
              title
              description
              priority
              url
              branchName
              labels { nodes { name } }
              team { name }
              project { name }
            }
          }
        }
        "#;
        let body = self
            .graphql(query, Some(json!({ "first": max_results.min(50) })))
            .await?;
        let nodes = body
            .get("data")
            .and_then(|data| data.get("issues"))
            .and_then(|issues| issues.get("nodes"))
            .and_then(Value::as_array)
            .ok_or_else(|| "Linear response did not include issues".to_string())?;

        Ok(nodes.iter().map(parse_issue).collect())
    }

    pub async fn get_issue(&self, identifier: &str) -> Result<Ticket, String> {
        let query = r#"
        query Issue($identifier: String!) {
          issue(id: $identifier) {
            id
            identifier
            title
            description
            priority
            url
            branchName
            labels { nodes { name } }
            team { name }
            project { name }
          }
        }
        "#;
        let body = self
            .graphql(query, Some(json!({ "identifier": identifier })))
            .await?;
        let issue = body
            .get("data")
            .and_then(|data| data.get("issue"))
            .ok_or_else(|| "Linear issue was not found".to_string())?;
        Ok(parse_issue(issue))
    }

    async fn graphql(&self, query: &str, variables: Option<Value>) -> Result<Value, String> {
        let mut payload = json!({ "query": query });
        if let Some(variables) = variables {
            payload["variables"] = variables;
        }

        let response = timeout(
            Duration::from_secs(10),
            self.client
                .post("https://api.linear.app/graphql")
                .bearer_auth(&self.token)
                .json(&payload)
                .send(),
        )
        .await
        .map_err(|_| "Linear request timed out after 10 seconds".to_string())?
        .map_err(|err| format!("Linear request failed: {err}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!(
                "Linear returned {status}: {}",
                readable_error(&text)
            ));
        }

        let body = response
            .json::<Value>()
            .await
            .map_err(|err| err.to_string())?;
        if let Some(errors) = body.get("errors").and_then(Value::as_array) {
            let message = errors
                .iter()
                .filter_map(|error| error.get("message").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("; ");
            if !message.is_empty() {
                return Err(format!("Linear error: {message}"));
            }
        }
        Ok(body)
    }
}

fn parse_issue(issue: &Value) -> Ticket {
    let identifier = issue
        .get("identifier")
        .and_then(Value::as_str)
        .unwrap_or("LIN")
        .to_string();
    let title = issue
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Untitled Linear issue")
        .to_string();
    let labels = issue
        .get("labels")
        .and_then(|labels| labels.get("nodes"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|label| label.get("name").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(",")
        })
        .filter(|value| !value.is_empty());
    let team = issue
        .get("team")
        .and_then(|team| team.get("name"))
        .and_then(Value::as_str);
    let project = issue
        .get("project")
        .and_then(|project| project.get("name"))
        .and_then(Value::as_str);
    let project_name = match (team, project) {
        (Some(team), Some(project)) => Some(format!("{team} / {project}")),
        (Some(team), None) => Some(team.to_string()),
        (None, Some(project)) => Some(project.to_string()),
        _ => None,
    };

    Ticket {
        id: format!(
            "linear:{}",
            issue
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or(identifier.as_str())
        ),
        integration_id: None,
        provider: "linear".to_string(),
        ticket_id: identifier,
        title,
        description: truncate_opt(
            issue
                .get("description")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            2000,
        ),
        labels,
        priority: Some(priority_name(issue.get("priority").and_then(Value::as_i64)).to_string()),
        assignee: None,
        project: project_name,
        url: issue
            .get("url")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        branch: issue
            .get("branchName")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        raw: serde_json::to_string(issue).ok(),
        fetched_at: String::new(),
    }
}

fn priority_name(priority: Option<i64>) -> &'static str {
    match priority.unwrap_or(0) {
        1 => "Urgent",
        2 => "High",
        3 => "Medium",
        4 => "Low",
        _ => "No Priority",
    }
}

fn readable_error(text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|json| {
            json.get("errors")
                .and_then(Value::as_array)
                .and_then(|errors| errors.first())
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| text.chars().take(240).collect())
}
