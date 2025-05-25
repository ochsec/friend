use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::Value;
use crate::{Message, MessageSource, Attachment};
use super::MessageProvider;

pub struct GitHubProvider {
    token: String,
    username: String,
    client: Client,
}

impl GitHubProvider {
    pub fn new(token: String, username: String) -> Self {
        Self {
            token,
            username,
            client: Client::new(),
        }
    }

    fn parse_notification(&self, notif: &Value) -> Option<Message> {
        let id = notif["id"].as_str()?.parse::<u64>().ok()?;
        let subject = notif["subject"]["title"].as_str().unwrap_or("No title");
        let reason = notif["reason"].as_str().unwrap_or("notification");
        let repo = notif["repository"]["full_name"].as_str().unwrap_or("unknown/repo");
        let timestamp_str = notif["updated_at"].as_str()?;
        
        let timestamp = DateTime::parse_from_rfc3339(timestamp_str)
            .ok()?
            .with_timezone(&Utc);
        
        let content = format!("{}: {} ({})", repo, subject, reason);
        
        Some(Message {
            id,
            source: MessageSource::Github,
            content,
            timestamp,
            author: "GitHub".to_string(),
            attachments: vec![],
        })
    }

    fn parse_event(&self, event: &Value) -> Option<Message> {
        let id = event["id"].as_str()?.parse::<u64>().ok()?;
        let event_type = event["type"].as_str().unwrap_or("Unknown");
        let repo = event["repo"]["name"].as_str().unwrap_or("unknown/repo");
        let actor = event["actor"]["login"].as_str().unwrap_or("Unknown");
        let timestamp_str = event["created_at"].as_str()?;
        
        let timestamp = DateTime::parse_from_rfc3339(timestamp_str)
            .ok()?
            .with_timezone(&Utc);
        
        let content = match event_type {
            "PushEvent" => {
                let commits = event["payload"]["commits"].as_array().map(|c| c.len()).unwrap_or(0);
                format!("{} pushed {} commits to {}", actor, commits, repo)
            },
            "IssuesEvent" => {
                let action = event["payload"]["action"].as_str().unwrap_or("unknown");
                let title = event["payload"]["issue"]["title"].as_str().unwrap_or("issue");
                format!("{} {} issue: {} in {}", actor, action, title, repo)
            },
            "PullRequestEvent" => {
                let action = event["payload"]["action"].as_str().unwrap_or("unknown");
                let title = event["payload"]["pull_request"]["title"].as_str().unwrap_or("PR");
                format!("{} {} PR: {} in {}", actor, action, title, repo)
            },
            _ => format!("{} {} in {}", actor, event_type, repo),
        };
        
        Some(Message {
            id,
            source: MessageSource::Github,
            content,
            timestamp,
            author: actor.to_string(),
            attachments: vec![],
        })
    }
}

#[async_trait]
impl MessageProvider for GitHubProvider {
    async fn fetch_messages(&self, _since: Option<DateTime<Utc>>) -> Result<Vec<Message>, Box<dyn std::error::Error + Send + Sync>> {
        let mut all_messages = Vec::new();
        
        let notifications_url = "https://api.github.com/notifications";
        let events_url = format!("https://api.github.com/users/{}/events", self.username);
        
        let auth_header = format!("token {}", self.token);
        
        let notifications_response = self.client
            .get(notifications_url)
            .header("Authorization", &auth_header)
            .header("User-Agent", "friend-tui")
            .send()
            .await?;
            
        if let Ok(notifications) = notifications_response.json::<Vec<Value>>().await {
            for notif in notifications {
                if let Some(msg) = self.parse_notification(&notif) {
                    all_messages.push(msg);
                }
            }
        }
        
        let events_response = self.client
            .get(&events_url)
            .header("Authorization", &auth_header)
            .header("User-Agent", "friend-tui")
            .send()
            .await?;
            
        if let Ok(events) = events_response.json::<Vec<Value>>().await {
            for event in events {
                if let Some(msg) = self.parse_event(&event) {
                    all_messages.push(msg);
                }
            }
        }
        
        all_messages.sort_by(|a, b| b.timestamp.cmp(&a.timestamp)); // Already newest first - keep it
        Ok(all_messages)
    }

    async fn send_message(&self, _content: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("GitHub does not support sending messages through this interface".into())
    }

    async fn send_message_with_attachment(&self, _content: &str, _attachment_path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("GitHub does not support sending messages through this interface".into())
    }

    async fn download_attachment(&self, _attachment: &crate::Attachment, _save_path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("GitHub attachments are not downloadable through this interface".into())
    }

    fn source(&self) -> MessageSource {
        MessageSource::Github
    }
}