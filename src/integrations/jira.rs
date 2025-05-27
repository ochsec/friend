use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::Value;
use crate::{Message, MessageSource};
use super::MessageProvider;

pub struct JiraProvider {
    base_url: String,
    email: String,
    api_token: String,
    project_keys: Vec<String>,
    client: Client,
}

impl JiraProvider {
    pub fn new(base_url: String, email: String, api_token: String, project_keys: Vec<String>) -> Self {
        Self {
            base_url,
            email,
            api_token,
            project_keys,
            client: Client::new(),
        }
    }

    fn parse_issue(&self, issue: &Value) -> Option<Message> {
        let key = issue["key"].as_str()?;
        let fields = &issue["fields"];
        let summary = fields["summary"].as_str().unwrap_or("No summary");
        let status = fields["status"]["name"].as_str().unwrap_or("Unknown");
        let assignee = fields["assignee"]["displayName"].as_str().unwrap_or("Unassigned");
        let updated_str = fields["updated"].as_str()?;
        
        let timestamp = DateTime::parse_from_rfc3339(updated_str)
            .ok()?
            .with_timezone(&Utc);
        
        let content = format!("{}: {} (Status: {})", key, summary, status);
        
        let id = key.chars().filter(|c| c.is_ascii_digit()).collect::<String>()
            .parse::<u64>().unwrap_or(0);
        
        Some(Message {
            id,
            source: MessageSource::Jira,
            content,
            timestamp,
            author: assignee.to_string(),
            attachments: vec![],
            channel_id: None,
        })
    }

    fn get_auth_header(&self) -> String {
        use base64::Engine;
        let credentials = format!("{}:{}", self.email, self.api_token);
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        format!("Basic {}", encoded)
    }
}

#[async_trait]
impl MessageProvider for JiraProvider {
    async fn fetch_messages(&self, since: Option<DateTime<Utc>>) -> Result<Vec<Message>, Box<dyn std::error::Error + Send + Sync>> {
        let project_filter = if self.project_keys.len() == 1 {
            format!("project = {}", self.project_keys[0])
        } else {
            format!("project IN ({})", self.project_keys.join(", "))
        };
        
        let mut jql = project_filter;
        
        if let Some(since_time) = since {
            let since_str = since_time.format("%Y-%m-%d %H:%M").to_string();
            jql.push_str(&format!(" AND updated >= '{}'", since_str));
        }
        
        jql.push_str(" ORDER BY updated DESC");
        
        let url = format!("{}/rest/api/3/search", self.base_url);
        
        let query_params = [
            ("jql", jql),
            ("maxResults", "100".to_string()),
            ("fields", "summary,status,assignee,updated".to_string()),
        ];
        
        let response = self.client
            .get(&url)
            .header("Authorization", self.get_auth_header())
            .header("Accept", "application/json")
            .query(&query_params)
            .send()
            .await?;
            
        let data: Value = response.json().await?;
        
        let mut messages = Vec::new();
        if let Some(issues) = data["issues"].as_array() {
            for issue in issues {
                if let Some(msg) = self.parse_issue(issue) {
                    messages.push(msg);
                }
            }
        }
        
        Ok(messages)
    }

    async fn send_message(&self, content: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/rest/api/3/issue", self.base_url);
        
        let project_key = self.project_keys.first()
            .ok_or("No project keys configured")?;
        
        let payload = serde_json::json!({
            "fields": {
                "project": {
                    "key": project_key
                },
                "summary": content,
                "description": {
                    "type": "doc",
                    "version": 1,
                    "content": [
                        {
                            "type": "paragraph",
                            "content": [
                                {
                                    "type": "text",
                                    "text": content
                                }
                            ]
                        }
                    ]
                },
                "issuetype": {
                    "name": "Task"
                }
            }
        });
        
        self.client
            .post(&url)
            .header("Authorization", self.get_auth_header())
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;
            
        Ok(())
    }

    async fn send_message_with_attachment(&self, _content: &str, _attachment_path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("Jira attachment sending not implemented in this interface".into())
    }

    async fn download_attachment(&self, _attachment: &crate::Attachment, _save_path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("Jira attachment downloads not implemented in this interface".into())
    }

    fn source(&self) -> MessageSource {
        MessageSource::Jira
    }

    fn channel_id(&self) -> Option<String> {
        None
    }
    
    fn provider_key(&self) -> String {
        format!("jira_{}", self.base_url.replace("https://", "").replace("http://", ""))
    }
    
    async fn fetch_messages_since_id(&self, _last_message_id: Option<u64>) -> Result<Vec<Message>, Box<dyn std::error::Error + Send + Sync>> {
        // For now, just use the regular fetch method
        self.fetch_messages(None).await
    }
}