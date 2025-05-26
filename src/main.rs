use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use std::io;
use std::time::{Duration, Instant};
use chrono::{DateTime, Utc};

mod integrations;
mod config;

use config::Config;
use integrations::{IntegrationManager, telegram::TelegramProvider, discord::DiscordProvider, github::GitHubProvider, jira::JiraProvider};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MessageSource {
    Telegram,
    Discord,
    Github,
    Jira,
}

#[derive(Debug, Clone)]
pub struct Attachment {
    pub filename: String,
    pub url: String,
    pub file_type: AttachmentType,
    pub size: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum AttachmentType {
    Image,
    Video,
    Audio,
    Document,
    Other,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub id: u64,
    pub source: MessageSource,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub author: String,
    pub attachments: Vec<Attachment>,
    pub channel_id: Option<String>,
}

struct App {
    messages: Vec<Message>,
    selected_message: Option<usize>,
    integration_manager: IntegrationManager,
    input_mode: bool,
    input_text: String,
    last_refresh: Instant,
    message_limit: usize,
    colors: config::ColorConfig,
}

fn parse_color(color_name: &str) -> Color {
    match color_name.to_lowercase().as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "darkgray" | "darkgrey" => Color::DarkGray,
        "lightred" => Color::LightRed,
        "lightgreen" => Color::LightGreen,
        "lightyellow" => Color::LightYellow,
        "lightblue" => Color::LightBlue,
        "lightmagenta" => Color::LightMagenta,
        "lightcyan" => Color::LightCyan,
        "white" => Color::White,
        _ => Color::Reset, // Use terminal default
    }
}

impl App {
    async fn new(config: Config, telegram_provider: Option<TelegramProvider>) -> Result<App, Box<dyn std::error::Error + Send + Sync>> {
        let mut integration_manager = IntegrationManager::new();
        
        if let Some(provider) = telegram_provider {
            integration_manager.add_provider(Box::new(provider));
        }
        
        if let Some(discord_config) = config.discord {
            for channel_id in discord_config.channel_ids {
                let provider = DiscordProvider::new(
                    discord_config.user_token.clone(),
                    channel_id,
                );
                integration_manager.add_provider(Box::new(provider));
            }
        }
        
        if let Some(github_config) = config.github {
            let provider = GitHubProvider::new(
                github_config.token,
                github_config.username,
            );
            integration_manager.add_provider(Box::new(provider));
        }
        
        if let Some(jira_config) = config.jira {
            let provider = JiraProvider::new(
                jira_config.base_url,
                jira_config.email,
                jira_config.api_token,
                jira_config.project_keys,
            );
            integration_manager.add_provider(Box::new(provider));
        }

        // Fetch initial messages from all providers
        let messages = integration_manager.fetch_all_messages(None, Some(config.message_limit)).await;
        let selected_message = if messages.is_empty() { None } else { Some(0) };

        Ok(App {
            messages,
            selected_message,
            integration_manager,
            input_mode: false,
            input_text: String::new(),
            last_refresh: Instant::now(),
            message_limit: config.message_limit,
            colors: config.colors,
        })
    }
    
    async fn refresh_messages(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.messages = self.integration_manager.fetch_all_messages(None, Some(self.message_limit)).await;
        if self.messages.is_empty() {
            self.selected_message = None;
        } else if self.selected_message.is_none() {
            self.selected_message = Some(0);
        } else if let Some(selected) = self.selected_message {
            if selected >= self.messages.len() {
                self.selected_message = Some(self.messages.len() - 1);
            }
        }
        self.last_refresh = Instant::now();
        Ok(())
    }
    
    fn should_refresh(&self) -> bool {
        self.last_refresh.elapsed() >= Duration::from_secs(30) // Refresh every 30 seconds
    }

    fn select_next(&mut self) {
        if let Some(selected) = self.selected_message {
            if selected < self.messages.len() - 1 {
                self.selected_message = Some(selected + 1);
            }
        }
    }

    fn select_previous(&mut self) {
        if let Some(selected) = self.selected_message {
            if selected > 0 {
                self.selected_message = Some(selected - 1);
            }
        }
    }

    fn get_selected_message(&self) -> Option<&Message> {
        self.selected_message.and_then(|i| self.messages.get(i))
    }
    
    async fn send_message(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.input_text.is_empty() {
            return Ok(());
        }
        
        let message_content = self.input_text.clone();
        self.input_text.clear();
        self.input_mode = false;
        
        // Determine which provider to use based on the selected message
        let (target_source, target_channel) = if let Some(selected_msg) = self.get_selected_message() {
            (Some(selected_msg.source), selected_msg.channel_id.clone())
        } else {
            (None, None)
        };
        
        // Find a provider that matches both the target source and channel
        let providers = &self.integration_manager.providers;
        let target_provider = if let Some(source) = target_source {
            providers.iter().find(|p| {
                p.source() == source && 
                (target_channel.is_none() || p.channel_id() == target_channel || 
                 (source == MessageSource::Telegram && p.channel_id().is_none())) // Telegram client handles all chats
            })
        } else {
            providers.first()
        };
        
        if let Some(provider) = target_provider {
            let send_result = if target_source == Some(MessageSource::Telegram) && target_channel.is_some() {
                // Special handling for Telegram - send to specific chat
                if let Some(chat_id) = &target_channel {
                    // We need to downcast to TelegramProvider to access send_message_to_chat
                    // For now, let's use a simpler approach and add the chat context to the message
                    provider.send_message(&format!("Reply to chat {}: {}", chat_id, message_content)).await
                } else {
                    provider.send_message(&message_content).await
                }
            } else {
                provider.send_message(&message_content).await
            };

            match send_result {
                Ok(()) => {
                    // Refresh messages to show the sent message
                    if let Err(e) = self.refresh_messages().await {
                        eprintln!("Error refreshing after sending: {}", e);
                    }
                }
                Err(e) => {
                    // Add a local error message if sending failed
                    let error_source = target_source.unwrap_or(MessageSource::Discord);
                    let error_message = Message {
                        id: (self.messages.len() + 1) as u64,
                        source: error_source,
                        content: format!("❌ Failed to send: {} (Error: {})", message_content, e),
                        timestamp: Utc::now(),
                        author: "System".to_string(),
                        attachments: vec![],
                        channel_id: None,
                    };
                    self.messages.push(error_message);
                    self.selected_message = Some(self.messages.len() - 1);
                }
            }
        } else {
            // No matching provider available
            let error_source = target_source.unwrap_or(MessageSource::Discord);
            let error_message = Message {
                id: (self.messages.len() + 1) as u64,
                source: error_source,
                content: format!("❌ No provider configured for {:?}: {}", error_source, message_content),
                timestamp: Utc::now(),
                author: "System".to_string(),
                attachments: vec![],
                channel_id: None,
            };
            self.messages.push(error_message);
            self.selected_message = Some(self.messages.len() - 1);
        }
        
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = Config::from_env()?;
    
    if !config.has_any_provider() {
        eprintln!("No providers configured. Please check your .env file.");
        eprintln!("Copy .env.example to .env and fill in your tokens.");
        return Ok(());
    }

    // Handle Telegram authentication before starting TUI
    let mut telegram_provider = None;
    if let Some(ref telegram_config) = config.telegram {
        println!("Initializing Telegram client...");
        println!("API ID: {}", telegram_config.api_id);
        println!("Phone: {}", telegram_config.phone);
        println!("Session file: {:?}", telegram_config.session_file);
        
        match TelegramProvider::new(
            telegram_config.api_id,
            telegram_config.api_hash.clone(),
            telegram_config.phone.clone(),
            telegram_config.session_file.clone(),
        ).await {
            Ok(provider) => {
                println!("Telegram authentication successful!");
                telegram_provider = Some(provider);
            }
            Err(e) => {
                eprintln!("Failed to authenticate with Telegram: {}", e);
                eprintln!("Error details: {:?}", e);
                eprintln!("Please check your credentials and try again.");
                return Err(e);
            }
        }
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(config, telegram_provider).await?;

    loop {
        // Auto-refresh messages periodically
        if app.should_refresh() && !app.input_mode {
            if let Err(e) = app.refresh_messages().await {
                eprintln!("Error refreshing messages: {}", e);
            }
        }
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                .split(f.area());
                
            let content_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(3)].as_ref())
                .split(chunks[1]);

            let items: Vec<ListItem> = app
                .messages
                .iter()
                .enumerate()
                .map(|(i, msg)| {
                    let source_prefix = match msg.source {
                        MessageSource::Discord => "🎮",
                        MessageSource::Telegram => "✈️",
                        MessageSource::Github => "🐙",
                        MessageSource::Jira => "📋",
                    };
                    
                    let content = format!(
                        "{} {} - {} ({})",
                        source_prefix,
                        msg.author,
                        msg.content,
                        msg.timestamp.format("%H:%M")
                    );
                    
                    let style = if Some(i) == app.selected_message {
                        let mut style = Style::default();
                        if let Some(ref bg_color) = app.colors.selected_bg {
                            style = style.bg(parse_color(bg_color));
                        } else {
                            style = style.bg(Color::Blue); // Default
                        }
                        if let Some(ref fg_color) = app.colors.selected_fg {
                            style = style.fg(parse_color(fg_color));
                        }
                        style
                    } else {
                        Style::default()
                    };
                    
                    ListItem::new(content).style(style)
                })
                .collect();

            let messages_list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Messages"))
                .style(Style::default());

            let mut list_state = ratatui::widgets::ListState::default();
            if let Some(selected) = app.selected_message {
                list_state.select(Some(selected));
            }

            f.render_stateful_widget(messages_list, chunks[0], &mut list_state);

            let content = if let Some(msg) = app.get_selected_message() {
                let mut text = format!(
                    "Source: {:?}\nAuthor: {}\nTime: {}\n\n{}",
                    msg.source,
                    msg.author,
                    msg.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
                    msg.content
                );
                
                if !msg.attachments.is_empty() {
                    text.push_str("\n\nAttachments:");
                    for attachment in &msg.attachments {
                        let type_icon = match attachment.file_type {
                            AttachmentType::Image => "🖼️",
                            AttachmentType::Video => "🎥",
                            AttachmentType::Audio => "🎵",
                            AttachmentType::Document => "📄",
                            AttachmentType::Other => "📎",
                        };
                        
                        let size_str = if let Some(size) = attachment.size {
                            format!(" ({}B)", size)
                        } else {
                            String::new()
                        };
                        
                        text.push_str(&format!("\n  {} {}{}", type_icon, attachment.filename, size_str));
                    }
                }
                
                text
            } else {
                "No message selected".to_string()
            };

            let content_area = Paragraph::new(content)
                .block(Block::default().borders(Borders::ALL).title("Content"))
                .style(Style::default());

            f.render_widget(content_area, content_chunks[0]);
            
            let input_style = if app.input_mode {
                let color = if let Some(ref active_color) = app.colors.input_active {
                    parse_color(active_color)
                } else {
                    Color::Yellow // Default
                };
                Style::default().fg(color)
            } else {
                let color = if let Some(ref inactive_color) = app.colors.input_inactive {
                    parse_color(inactive_color)
                } else {
                    Color::DarkGray // Default
                };
                Style::default().fg(color)
            };
            
            let input_title = if app.input_mode {
                "Input (Tab to send, Esc to cancel)"
            } else {
                "Input (Enter to type, Tab to send)"
            };
            
            let input_area = Paragraph::new(app.input_text.as_str())
                .block(Block::default().borders(Borders::ALL).title(input_title))
                .style(input_style);

            f.render_widget(input_area, content_chunks[1]);
            
            if app.input_mode {
                f.set_cursor_position((
                    content_chunks[1].x + app.input_text.len() as u16 + 1,
                    content_chunks[1].y + 1,
                ));
            }
        })?;

        if let Event::Key(key) = event::read()? {
            if app.input_mode {
                match key.code {
                    KeyCode::Enter => {
                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                            // Shift+Enter to send message
                            if let Err(e) = app.send_message().await {
                                eprintln!("Error sending message: {}", e);
                            }
                        }
                        // Regular Enter does nothing in input mode
                    }
                    KeyCode::Esc => {
                        app.input_mode = false;
                        app.input_text.clear();
                    }
                    KeyCode::Backspace => {
                        app.input_text.pop();
                    }
                    KeyCode::Char(c) => {
                        app.input_text.push(c);
                    }
                    KeyCode::Tab => {
                        // Alternative: Use Tab to send message in input mode
                        if let Err(e) = app.send_message().await {
                            eprintln!("Error sending message: {}", e);
                        }
                    }
                    _ => {}
                }
            } else {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                    KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
                    KeyCode::Char('r') => {
                        if let Err(e) = app.refresh_messages().await {
                            eprintln!("Error refreshing messages: {}", e);
                        }
                    }
                    KeyCode::Enter => {
                        // Enter to start typing
                        app.input_mode = true;
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;

    Ok(())
}