use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub telegram: Option<TelegramConfig>,
    pub discord: Option<DiscordConfig>,
    pub github: Option<GitHubConfig>,
    pub jira: Option<JiraConfig>,
}

#[derive(Debug, Clone)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_ids: Option<Vec<String>>,
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
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        dotenv::dotenv().ok();

        let telegram = if let Ok(bot_token) = env::var("TELEGRAM_BOT_TOKEN") {
            let chat_ids = if let Ok(chat_ids_str) = env::var("TELEGRAM_CHAT_IDS") {
                let ids: Vec<String> = chat_ids_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                
                if ids.is_empty() { None } else { Some(ids) }
            } else {
                None
            };
            
            Some(TelegramConfig { bot_token, chat_ids })
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

        Ok(Config {
            telegram,
            discord,
            github,
            jira,
        })
    }

    pub fn has_any_provider(&self) -> bool {
        self.telegram.is_some() || self.discord.is_some() || self.github.is_some() || self.jira.is_some()
    }
}