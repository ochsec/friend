use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crate::{Message, MessageSource, Attachment};

pub mod telegram;
pub mod discord;
pub mod github;
pub mod jira;

#[async_trait]
pub trait MessageProvider {
    async fn fetch_messages(&self, since: Option<DateTime<Utc>>) -> Result<Vec<Message>, Box<dyn std::error::Error + Send + Sync>>;
    async fn send_message(&self, content: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn send_message_with_attachment(&self, content: &str, attachment_path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn download_attachment(&self, attachment: &Attachment, save_path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    fn source(&self) -> MessageSource;
}

pub struct IntegrationManager {
    pub providers: Vec<Box<dyn MessageProvider + Send + Sync>>,
}

impl IntegrationManager {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    pub fn add_provider(&mut self, provider: Box<dyn MessageProvider + Send + Sync>) {
        self.providers.push(provider);
    }

    pub async fn fetch_all_messages(&self, since: Option<DateTime<Utc>>, limit: Option<usize>) -> Vec<Message> {
        let mut all_messages = Vec::new();
        
        for provider in &self.providers {
            if let Ok(messages) = provider.fetch_messages(since).await {
                all_messages.extend(messages);
            }
        }
        
        all_messages.sort_by(|a, b| b.timestamp.cmp(&a.timestamp)); // Newest first
        
        // Apply limit if specified
        if let Some(limit) = limit {
            all_messages.truncate(limit);
        }
        
        all_messages
    }
}