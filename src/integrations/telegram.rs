use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::Value;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use crate::{Message, MessageSource, Attachment, AttachmentType};
use super::MessageProvider;

pub struct TelegramProvider {
    bot_token: String,
    chat_id: Option<String>,
    chat_ids: Vec<String>,
    client: Client,
}

impl TelegramProvider {
    pub fn new(bot_token: String, chat_id: String) -> Self {
        Self {
            bot_token,
            chat_id: Some(chat_id.clone()),
            chat_ids: vec![chat_id],
            client: Client::new(),
        }
    }

    pub async fn new_auto_discover(bot_token: String) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = Client::new();
        let chat_ids = Self::discover_chat_ids(&bot_token, &client).await?;
        
        Ok(Self {
            bot_token,
            chat_id: None,
            chat_ids,
            client,
        })
    }

    async fn discover_chat_ids(bot_token: &str, client: &Client) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("https://api.telegram.org/bot{}/getUpdates", bot_token);
        
        let params = [
            ("limit", "100".to_string()),
            ("offset", "-100".to_string()), // Get recent updates
        ];
        
        let response = client
            .get(&url)
            .query(&params)
            .send()
            .await?;
            
        let data: Value = response.json().await?;
        
        let mut chat_ids = std::collections::HashSet::new();
        
        if let Some(result) = data["result"].as_array() {
            for update in result {
                // Check for messages
                if let Some(message) = update["message"].as_object() {
                    if let Some(chat) = message["chat"].as_object() {
                        if let Some(chat_id) = chat["id"].as_i64() {
                            chat_ids.insert(chat_id.to_string());
                        }
                    }
                }
                
                // Check for edited messages
                if let Some(edited_message) = update["edited_message"].as_object() {
                    if let Some(chat) = edited_message["chat"].as_object() {
                        if let Some(chat_id) = chat["id"].as_i64() {
                            chat_ids.insert(chat_id.to_string());
                        }
                    }
                }
                
                // Check for channel posts
                if let Some(channel_post) = update["channel_post"].as_object() {
                    if let Some(chat) = channel_post["chat"].as_object() {
                        if let Some(chat_id) = chat["id"].as_i64() {
                            chat_ids.insert(chat_id.to_string());
                        }
                    }
                }
            }
        }
        
        Ok(chat_ids.into_iter().collect())
    }

    fn parse_message(&self, msg: &Value) -> Option<Message> {
        let message_id = msg["message_id"].as_u64()?;
        let text = msg["text"].as_str().unwrap_or("").to_string();
        let from = msg["from"]["first_name"].as_str().unwrap_or("Unknown");
        let date = msg["date"].as_i64()?;
        
        let timestamp = DateTime::from_timestamp(date, 0)?;
        
        let mut attachments = Vec::new();
        
        if let Some(photo) = msg["photo"].as_array() {
            if let Some(largest_photo) = photo.last() {
                if let Some(file_id) = largest_photo["file_id"].as_str() {
                    attachments.push(Attachment {
                        filename: format!("photo_{}.jpg", message_id),
                        url: file_id.to_string(),
                        file_type: AttachmentType::Image,
                        size: largest_photo["file_size"].as_u64(),
                    });
                }
            }
        }
        
        if let Some(document) = msg["document"].as_object() {
            if let Some(file_id) = document["file_id"].as_str() {
                let filename = document["file_name"].as_str().unwrap_or("document").to_string();
                let file_type = match filename.split('.').last().unwrap_or("") {
                    "jpg" | "jpeg" | "png" | "gif" | "webp" => AttachmentType::Image,
                    "mp4" | "avi" | "mov" | "mkv" => AttachmentType::Video,
                    "mp3" | "wav" | "ogg" => AttachmentType::Audio,
                    "pdf" | "doc" | "docx" | "txt" => AttachmentType::Document,
                    _ => AttachmentType::Other,
                };
                
                attachments.push(Attachment {
                    filename,
                    url: file_id.to_string(),
                    file_type,
                    size: document["file_size"].as_u64(),
                });
            }
        }
        
        if let Some(video) = msg["video"].as_object() {
            if let Some(file_id) = video["file_id"].as_str() {
                attachments.push(Attachment {
                    filename: format!("video_{}.mp4", message_id),
                    url: file_id.to_string(),
                    file_type: AttachmentType::Video,
                    size: video["file_size"].as_u64(),
                });
            }
        }
        
        if let Some(audio) = msg["audio"].as_object() {
            if let Some(file_id) = audio["file_id"].as_str() {
                let filename = audio["file_name"].as_str()
                    .unwrap_or(&format!("audio_{}.mp3", message_id))
                    .to_string();
                
                attachments.push(Attachment {
                    filename,
                    url: file_id.to_string(),
                    file_type: AttachmentType::Audio,
                    size: audio["file_size"].as_u64(),
                });
            }
        }
        
        Some(Message {
            id: message_id,
            source: MessageSource::Telegram,
            content: text,
            timestamp,
            author: from.to_string(),
            attachments,
        })
    }

    async fn get_file_url(&self, file_id: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("https://api.telegram.org/bot{}/getFile", self.bot_token);
        
        let params = [("file_id", file_id)];
        
        let response = self.client
            .get(&url)
            .query(&params)
            .send()
            .await?;
            
        let data: Value = response.json().await?;
        
        if let Some(file_path) = data["result"]["file_path"].as_str() {
            Ok(format!("https://api.telegram.org/file/bot{}/{}", self.bot_token, file_path))
        } else {
            Err("Failed to get file path".into())
        }
    }
}

#[async_trait]
impl MessageProvider for TelegramProvider {
    async fn fetch_messages(&self, since: Option<DateTime<Utc>>) -> Result<Vec<Message>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("https://api.telegram.org/bot{}/getUpdates", self.bot_token);
        
        let mut params = vec![("limit", "100".to_string())];
        if let Some(since_time) = since {
            params.push(("offset", since_time.timestamp().to_string()));
        }
        
        let response = self.client
            .get(&url)
            .query(&params)
            .send()
            .await?;
            
        let data: Value = response.json().await?;
        
        let mut messages = Vec::new();
        if let Some(result) = data["result"].as_array() {
            for update in result {
                if let Some(message) = update["message"].as_object() {
                    // Filter by chat IDs if we have specific ones
                    if let Some(msg_chat_id) = message["chat"]["id"].as_i64() {
                        let msg_chat_id_str = msg_chat_id.to_string();
                        if self.chat_ids.contains(&msg_chat_id_str) {
                            if let Some(parsed_msg) = self.parse_message(&Value::Object(message.clone())) {
                                messages.push(parsed_msg);
                            }
                        }
                    }
                }
            }
        }
        
        messages.sort_by(|a, b| b.timestamp.cmp(&a.timestamp)); // Newest first
        Ok(messages)
    }

    async fn send_message(&self, content: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token);
        
        // Send to the first available chat, or the originally specified chat
        let target_chat_id = if let Some(ref chat_id) = self.chat_id {
            chat_id
        } else if let Some(first_chat) = self.chat_ids.first() {
            first_chat
        } else {
            return Err("No chat ID available for sending message".into());
        };
        
        let params = [
            ("chat_id", target_chat_id),
            ("text", &content.to_string()),
        ];
        
        self.client
            .post(&url)
            .form(&params)
            .send()
            .await?;
            
        Ok(())
    }

    async fn send_message_with_attachment(&self, content: &str, attachment_path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let file_path = Path::new(attachment_path);
        let file_name = file_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");
            
        let file_bytes = tokio::fs::read(attachment_path).await?;
        
        let file_part = reqwest::multipart::Part::bytes(file_bytes)
            .file_name(file_name.to_string());
            
        let extension = file_path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
            
        let (url, form_field) = match extension {
            "jpg" | "jpeg" | "png" | "gif" | "webp" => {
                (format!("https://api.telegram.org/bot{}/sendPhoto", self.bot_token), "photo")
            },
            "mp4" | "avi" | "mov" | "mkv" => {
                (format!("https://api.telegram.org/bot{}/sendVideo", self.bot_token), "video")
            },
            "mp3" | "wav" | "ogg" => {
                (format!("https://api.telegram.org/bot{}/sendAudio", self.bot_token), "audio")
            },
            _ => {
                (format!("https://api.telegram.org/bot{}/sendDocument", self.bot_token), "document")
            },
        };
        
        // Send to the first available chat, or the originally specified chat
        let target_chat_id = if let Some(ref chat_id) = self.chat_id {
            chat_id.clone()
        } else if let Some(first_chat) = self.chat_ids.first() {
            first_chat.clone()
        } else {
            return Err("No chat ID available for sending attachment".into());
        };
        
        let form = reqwest::multipart::Form::new()
            .text("chat_id", target_chat_id)
            .text("caption", content.to_string())
            .part(form_field, file_part);
        
        self.client
            .post(&url)
            .multipart(form)
            .send()
            .await?;
            
        Ok(())
    }

    async fn download_attachment(&self, attachment: &Attachment, save_path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let file_url = self.get_file_url(&attachment.url).await?;
        
        let response = self.client
            .get(&file_url)
            .send()
            .await?;
            
        let bytes = response.bytes().await?;
        
        let mut file = File::create(save_path).await?;
        file.write_all(&bytes).await?;
        
        Ok(())
    }

    fn source(&self) -> MessageSource {
        MessageSource::Telegram
    }
}