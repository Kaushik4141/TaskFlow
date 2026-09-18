pub mod github;
pub mod jira;
pub mod linear;
pub mod store;

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Integration {
    pub id: String,
    pub provider: String,
    pub name: String,
    #[serde(skip_serializing)]
    pub token: String,
    pub workspace: Option<String>,
    #[serde(skip_serializing)]
    pub extra: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Ticket {
    pub id: String,
    pub integration_id: Option<String>,
    pub provider: String,
    pub ticket_id: String,
    pub title: String,
    pub description: Option<String>,
    pub labels: Option<String>,
    pub priority: Option<String>,
    pub assignee: Option<String>,
    pub project: Option<String>,
    pub url: Option<String>,
    pub branch: Option<String>,
    #[serde(skip_serializing)]
    pub raw: Option<String>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub success: bool,
    pub message: String,
}

pub fn truncate_opt(value: Option<String>, max_chars: usize) -> Option<String> {
    value.map(|text| text.chars().take(max_chars).collect())
}

pub fn slugify_branch(prefix: Option<&str>, value: &str, max_chars: usize) -> String {
    let mut slug = String::new();
    if let Some(prefix) = prefix {
        slug.push_str(prefix.trim_matches('/'));
        slug.push('/');
    }

    let mut previous_hyphen = false;
    for ch in value.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            previous_hyphen = false;
        } else if !previous_hyphen && !slug.ends_with('/') {
            slug.push('-');
            previous_hyphen = true;
        }
        if slug.len() >= max_chars {
            break;
        }
    }

    slug.trim_matches('-').to_string()
}
