use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::Value;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use crate::{Message, MessageSource, Attachment, AttachmentType};
use super::MessageProvider;

pub struct DiscordProvider {
    user_token: String,
    channel_id: String,
    client: Client,
}

impl DiscordProvider {
    pub fn new(user_token: String, channel_id: String) -> Self {
        Self {
            user_token,
            channel_id,
            client: Client::new(),
        }
    }

    fn parse_message(&self, msg: &Value) -> Option<Message> {
        let id = msg["id"].as_str()?.parse::<u64>().ok()?;
        let content = msg["content"].as_str().unwrap_or("").to_string();
        let author = msg["author"]["username"].as_str().unwrap_or("Unknown");
        let timestamp_str = msg["timestamp"].as_str()?;
        
        let timestamp = DateTime::parse_from_rfc3339(timestamp_str)
            .ok()?
            .with_timezone(&Utc);
        
        let mut attachments = Vec::new();
        
        if let Some(attachments_array) = msg["attachments"].as_array() {
            for attachment in attachments_array {
                if let Some(url) = attachment["url"].as_str() {
                    let filename = attachment["filename"].as_str().unwrap_or("attachment").to_string();
                    let size = attachment["size"].as_u64();
                    
                    let file_type = if let Some(content_type) = attachment["content_type"].as_str() {
                        match content_type.split('/').next().unwrap_or("") {
                            "image" => AttachmentType::Image,
                            "video" => AttachmentType::Video,
                            "audio" => AttachmentType::Audio,
                            "text" | "application" => AttachmentType::Document,
                            _ => AttachmentType::Other,
                        }
                    } else {
                        match filename.split('.').last().unwrap_or("") {
                            "jpg" | "jpeg" | "png" | "gif" | "webp" => AttachmentType::Image,
                            "mp4" | "avi" | "mov" | "mkv" => AttachmentType::Video,
                            "mp3" | "wav" | "ogg" => AttachmentType::Audio,
                            "pdf" | "doc" | "docx" | "txt" => AttachmentType::Document,
                            _ => AttachmentType::Other,
                        }
                    };
                    
                    attachments.push(Attachment {
                        filename,
                        url: url.to_string(),
                        file_type,
                        size,
                    });
                }
            }
        }
        
        Some(Message {
            id,
            source: MessageSource::Discord,
            content,
            timestamp,
            author: author.to_string(),
            attachments,
            channel_id: Some(self.channel_id.clone()),
        })
    }
}

#[async_trait]
impl MessageProvider for DiscordProvider {
    async fn fetch_messages(&self, since: Option<DateTime<Utc>>) -> Result<Vec<Message>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("https://discord.com/api/v10/channels/{}/messages", self.channel_id);
        
        let mut query_params = vec![("limit", "100".to_string())];
        if let Some(since_time) = since {
            query_params.push(("after", since_time.timestamp().to_string()));
        }
        
        let response = self.client
            .get(&url)
            .header("Authorization", &self.user_token)
            .query(&query_params)
            .send()
            .await?;
            
        let messages_data: Vec<Value> = response.json().await?;
        
        let mut messages = Vec::new();
        for msg_data in messages_data {
            if let Some(parsed_msg) = self.parse_message(&msg_data) {
                messages.push(parsed_msg);
            }
        }
        
        messages.sort_by(|a, b| b.timestamp.cmp(&a.timestamp)); // Newest first
        Ok(messages)
    }

    async fn send_message(&self, content: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("https://discord.com/api/v10/channels/{}/messages", self.channel_id);
        
        let payload = serde_json::json!({
            "content": content
        });
        
        self.client
            .post(&url)
            .header("Authorization", &self.user_token)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;
            
        Ok(())
    }

    async fn send_message_with_attachment(&self, content: &str, attachment_path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("https://discord.com/api/v10/channels/{}/messages", self.channel_id);
        
        let file_path = Path::new(attachment_path);
        let file_name = file_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");
            
        let file_bytes = tokio::fs::read(attachment_path).await?;
        
        let file_part = reqwest::multipart::Part::bytes(file_bytes)
            .file_name(file_name.to_string());
        
        let payload_json = serde_json::json!({
            "content": content
        });
        
        let form = reqwest::multipart::Form::new()
            .text("payload_json", payload_json.to_string())
            .part("files[0]", file_part);
        
        self.client
            .post(&url)
            .header("Authorization", &self.user_token)
            .multipart(form)
            .send()
            .await?;
            
        Ok(())
    }

    async fn download_attachment(&self, attachment: &Attachment, save_path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let response = self.client
            .get(&attachment.url)
            .send()
            .await?;
            
        let bytes = response.bytes().await?;
        
        let mut file = File::create(save_path).await?;
        file.write_all(&bytes).await?;
        
        Ok(())
    }

    fn source(&self) -> MessageSource {
        MessageSource::Discord
    }

    fn channel_id(&self) -> Option<String> {
        Some(self.channel_id.clone())
    }
}