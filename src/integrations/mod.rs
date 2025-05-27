use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::future;
use crate::{Message, MessageSource, Attachment};

pub mod telegram;
pub mod discord;
pub mod github;
pub mod jira;

#[async_trait]
pub trait MessageProvider {
    async fn fetch_messages(&self, since: Option<DateTime<Utc>>) -> Result<Vec<Message>, Box<dyn std::error::Error + Send + Sync>>;
    async fn fetch_messages_since_id(&self, last_message_id: Option<u64>) -> Result<Vec<Message>, Box<dyn std::error::Error + Send + Sync>>;
    async fn send_message(&self, content: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    #[allow(dead_code)]
    async fn send_message_with_attachment(&self, content: &str, attachment_path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    #[allow(dead_code)]
    async fn download_attachment(&self, attachment: &Attachment, save_path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_message(&self, message_id: u64) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    fn source(&self) -> MessageSource;
    fn channel_id(&self) -> Option<String>;
    fn provider_key(&self) -> String;
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
        
        // Fetch from all providers concurrently for better performance
        let futures: Vec<_> = self.providers.iter()
            .map(|provider| provider.fetch_messages(since))
            .collect();
            
        let results = future::join_all(futures).await;
        
        for result in results {
            if let Ok(messages) = result {
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
    
    pub async fn fetch_incremental_messages(&self, cache: &crate::database::MessageCache, limit: Option<usize>) -> Vec<Message> {
        let mut all_messages = Vec::new();
        
        // Fetch incrementally from all providers concurrently
        let futures: Vec<_> = self.providers.iter()
            .map(|provider| async {
                let provider_key = provider.provider_key();
                let last_message_id = cache.get_last_message_id(&provider_key).await.unwrap_or(None);
                provider.fetch_messages_since_id(last_message_id).await
            })
            .collect();
            
        let results = future::join_all(futures).await;
        
        for result in results {
            if let Ok(messages) = result {
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