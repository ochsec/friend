use sqlx::{SqlitePool, Row};
use chrono::{DateTime, Utc};
use std::str::FromStr;
use crate::{Message, MessageSource, Attachment, AttachmentType};

pub struct MessageCache {
    pool: SqlitePool,
}

impl MessageCache {
    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        // Connect to SQLite database (will create file if it doesn't exist)
        let options = sqlx::sqlite::SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await?;
        
        // Create tables if they don't exist
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY,
                source TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp DATETIME NOT NULL,
                author TEXT NOT NULL,
                channel_id TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS attachments (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                message_id INTEGER NOT NULL,
                filename TEXT NOT NULL,
                url TEXT NOT NULL,
                file_type TEXT NOT NULL,
                size INTEGER,
                FOREIGN KEY (message_id) REFERENCES messages (id)
            )
            "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sync_state (
                provider_key TEXT PRIMARY KEY,
                last_message_id INTEGER,
                last_sync DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&pool)
        .await?;

        // Create indexes for better query performance
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp)")
            .execute(&pool)
            .await?;
        
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_source ON messages(source)")
            .execute(&pool)
            .await?;

        Ok(Self { pool })
    }

    pub async fn get_cached_messages(&self, limit: Option<usize>) -> Result<Vec<Message>, sqlx::Error> {
        let limit_clause = limit.map(|l| format!("LIMIT {}", l)).unwrap_or_default();
        
        let query = format!(
            "SELECT id, source, content, timestamp, author, channel_id FROM messages ORDER BY timestamp DESC {}",
            limit_clause
        );
        
        let rows = sqlx::query(&query).fetch_all(&self.pool).await?;
        
        let mut messages = Vec::new();
        for row in rows {
            let message_id: i64 = row.get("id");
            let source_str: String = row.get("source");
            let content: String = row.get("content");
            let timestamp: DateTime<Utc> = row.get("timestamp");
            let author: String = row.get("author");
            let channel_id: Option<String> = row.get("channel_id");

            let source = match source_str.as_str() {
                "Telegram" => MessageSource::Telegram,
                "Discord" => MessageSource::Discord,
                "Github" => MessageSource::Github,
                "Jira" => MessageSource::Jira,
                _ => continue,
            };

            // Get attachments for this message
            let attachment_rows = sqlx::query(
                "SELECT filename, url, file_type, size FROM attachments WHERE message_id = ?"
            )
            .bind(message_id)
            .fetch_all(&self.pool)
            .await?;

            let attachments: Vec<Attachment> = attachment_rows
                .into_iter()
                .map(|row| {
                    let file_type_str: String = row.get("file_type");
                    let file_type = match file_type_str.as_str() {
                        "Image" => AttachmentType::Image,
                        "Video" => AttachmentType::Video,
                        "Audio" => AttachmentType::Audio,
                        "Document" => AttachmentType::Document,
                        _ => AttachmentType::Other,
                    };

                    Attachment {
                        filename: row.get("filename"),
                        url: row.get("url"),
                        file_type,
                        size: row.get("size"),
                    }
                })
                .collect();

            messages.push(Message {
                id: message_id as u64,
                source,
                content,
                timestamp,
                author,
                attachments,
                channel_id,
            });
        }

        Ok(messages)
    }

    pub async fn cache_messages(&self, messages: &[Message]) -> Result<(), sqlx::Error> {
        for message in messages {
            // Insert or replace message
            sqlx::query(
                r#"
                INSERT OR REPLACE INTO messages (id, source, content, timestamp, author, channel_id)
                VALUES (?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(message.id as i64)
            .bind(format!("{:?}", message.source))
            .bind(&message.content)
            .bind(&message.timestamp)
            .bind(&message.author)
            .bind(&message.channel_id)
            .execute(&self.pool)
            .await?;

            // Delete existing attachments for this message
            sqlx::query("DELETE FROM attachments WHERE message_id = ?")
                .bind(message.id as i64)
                .execute(&self.pool)
                .await?;

            // Insert new attachments
            for attachment in &message.attachments {
                sqlx::query(
                    r#"
                    INSERT INTO attachments (message_id, filename, url, file_type, size)
                    VALUES (?, ?, ?, ?, ?)
                    "#,
                )
                .bind(message.id as i64)
                .bind(&attachment.filename)
                .bind(&attachment.url)
                .bind(format!("{:?}", attachment.file_type))
                .bind(attachment.size.map(|s| s as i64))
                .execute(&self.pool)
                .await?;
            }
        }

        Ok(())
    }

    pub async fn get_last_message_id(&self, provider_key: &str) -> Result<Option<u64>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT last_message_id FROM sync_state WHERE provider_key = ?"
        )
        .bind(provider_key)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.get::<i64, _>("last_message_id") as u64))
    }

    pub async fn update_sync_state(&self, provider_key: &str, last_message_id: u64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO sync_state (provider_key, last_message_id, last_sync)
            VALUES (?, ?, CURRENT_TIMESTAMP)
            "#,
        )
        .bind(provider_key)
        .bind(last_message_id as i64)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_messages_since(&self, since: DateTime<Utc>, limit: Option<usize>) -> Result<Vec<Message>, sqlx::Error> {
        let limit_clause = limit.map(|l| format!("LIMIT {}", l)).unwrap_or_default();
        
        let query = format!(
            "SELECT id, source, content, timestamp, author, channel_id FROM messages WHERE timestamp > ? ORDER BY timestamp DESC {}",
            limit_clause
        );
        
        let rows = sqlx::query(&query)
            .bind(since)
            .fetch_all(&self.pool)
            .await?;
        
        let mut messages = Vec::new();
        for row in rows {
            let message_id: i64 = row.get("id");
            let source_str: String = row.get("source");
            let source = match source_str.as_str() {
                "Telegram" => MessageSource::Telegram,
                "Discord" => MessageSource::Discord,
                "Github" => MessageSource::Github,
                "Jira" => MessageSource::Jira,
                _ => continue,
            };

            messages.push(Message {
                id: message_id as u64,
                source,
                content: row.get("content"),
                timestamp: row.get("timestamp"),
                author: row.get("author"),
                attachments: vec![], // Skip attachments for incremental updates for now
                channel_id: row.get("channel_id"),
            });
        }

        Ok(messages)
    }
}