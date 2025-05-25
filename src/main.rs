use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
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
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
enum MessageSource {
    Telegram,
    Discord,
    Github,
    Jira,
}

#[derive(Debug, Clone)]
struct Message {
    id: u64,
    source: MessageSource,
    content: String,
    timestamp: DateTime<Utc>,
    author: String,
}

struct App {
    messages: Vec<Message>,
    selected_message: Option<usize>,
}

impl App {
    fn new() -> App {
        let mut messages = Vec::new();
        
        messages.push(Message {
            id: 1,
            source: MessageSource::Discord,
            content: "Hey everyone! How's the project going?".to_string(),
            timestamp: Utc::now(),
            author: "Alice".to_string(),
        });
        
        messages.push(Message {
            id: 2,
            source: MessageSource::Github,
            content: "New pull request: Fix authentication bug".to_string(),
            timestamp: Utc::now(),
            author: "Bob".to_string(),
        });
        
        messages.push(Message {
            id: 3,
            source: MessageSource::Jira,
            content: "Task updated: Database migration".to_string(),
            timestamp: Utc::now(),
            author: "System".to_string(),
        });

        App {
            messages,
            selected_message: Some(0),
        }
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
}

fn main() -> Result<(), io::Error> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                .split(f.size());

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
                        Style::default().bg(Color::Blue).fg(Color::White)
                    } else {
                        Style::default()
                    };
                    
                    ListItem::new(content).style(style)
                })
                .collect();

            let messages_list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Messages"))
                .style(Style::default().fg(Color::White));

            f.render_widget(messages_list, chunks[0]);

            let content = if let Some(msg) = app.get_selected_message() {
                format!(
                    "Source: {:?}\nAuthor: {}\nTime: {}\n\n{}",
                    msg.source,
                    msg.author,
                    msg.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
                    msg.content
                )
            } else {
                "No message selected".to_string()
            };

            let content_area = Paragraph::new(content)
                .block(Block::default().borders(Borders::ALL).title("Content"))
                .style(Style::default().fg(Color::White));

            f.render_widget(content_area, chunks[1]);
        })?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
                _ => {}
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