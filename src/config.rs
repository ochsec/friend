use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub telegram: Option<TelegramConfig>,
    pub discord: Option<DiscordConfig>,
    pub github: Option<GitHubConfig>,
    pub jira: Option<JiraConfig>,
    pub message_limit: usize,
    pub colors: ColorConfig,
}

#[derive(Debug, Clone)]
pub struct ColorConfig {
    pub selected_bg: Option<String>,
    pub selected_fg: Option<String>,
    pub input_active: Option<String>,
    pub input_inactive: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TelegramConfig {
    pub api_id: i32,
    pub api_hash: String,
    pub phone: String,
    pub session_file: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DiscordConfig {
    pub user_token: String,
    pub channel_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GitHubConfig {
    pub token: String,
    pub username: String,
}

#[derive(Debug, Clone)]
pub struct JiraConfig {
    pub base_url: String,
    pub email: String,
    pub api_token: String,
    pub project_keys: Vec<String>,
}

impl Config {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        dotenv::dotenv().ok();

        let telegram = if let (Ok(api_id_str), Ok(api_hash), Ok(phone)) = (
            env::var("TELEGRAM_API_ID"),
            env::var("TELEGRAM_API_HASH"),
            env::var("TELEGRAM_PHONE"),
        ) {
            if let Ok(api_id) = api_id_str.parse::<i32>() {
                let session_file = env::var("TELEGRAM_SESSION_FILE").ok();
                Some(TelegramConfig { api_id, api_hash, phone, session_file })
            } else {
                None
            }
        } else {
            None
        };

        let discord = if let (Ok(user_token), Ok(channel_ids_str)) = (
            env::var("DISCORD_USER_TOKEN"),
            env::var("DISCORD_CHANNEL_IDS"),
        ) {
            let channel_ids: Vec<String> = channel_ids_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            
            if !channel_ids.is_empty() {
                Some(DiscordConfig { user_token, channel_ids })
            } else {
                None
            }
        } else {
            None
        };

        let github = if let (Ok(token), Ok(username)) = (
            env::var("GITHUB_TOKEN"),
            env::var("GITHUB_USERNAME"),
        ) {
            Some(GitHubConfig { token, username })
        } else {
            None
        };

        let jira = if let (Ok(base_url), Ok(email), Ok(api_token), Ok(project_keys_str)) = (
            env::var("JIRA_BASE_URL"),
            env::var("JIRA_EMAIL"),
            env::var("JIRA_API_TOKEN"),
            env::var("JIRA_PROJECT_KEY"),
        ) {
            let project_keys: Vec<String> = project_keys_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            
            if !project_keys.is_empty() {
                Some(JiraConfig {
                    base_url,
                    email,
                    api_token,
                    project_keys,
                })
            } else {
                None
            }
        } else {
            None
        };

        let message_limit = env::var("MESSAGE_LIMIT")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(100); // Default to 100 messages

        let colors = ColorConfig {
            selected_bg: env::var("SELECTED_BG_COLOR").ok(),
            selected_fg: env::var("SELECTED_FG_COLOR").ok(),
            input_active: env::var("INPUT_ACTIVE_COLOR").ok(),
            input_inactive: env::var("INPUT_INACTIVE_COLOR").ok(),
        };

        Ok(Config {
            telegram,
            discord,
            github,
            jira,
            message_limit,
            colors,
        })
    }

    pub fn has_any_provider(&self) -> bool {
        self.telegram.is_some() || self.discord.is_some() || self.github.is_some() || self.jira.is_some()
    }
}