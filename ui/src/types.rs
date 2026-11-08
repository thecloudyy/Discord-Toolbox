use serde::{Deserialize, Serialize};

pub type Config = std::collections::BTreeMap<String, String>;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Profile {
    pub id: Option<String>,
    pub username: Option<String>,
    pub global_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Account {
    #[serde(flatten)]
    pub profile: Profile,
    pub client: String,
    pub token: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ScanReport {
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct BadgeStatus {
    #[serde(default)]
    pub running: bool,
    #[serde(default)]
    pub repeat: bool,
    #[serde(default)]
    pub finished: bool,
    #[serde(default)]
    pub stopped: bool,
    #[serde(default)]
    pub processed: usize,
    #[serde(default)]
    pub pass: usize,
    pub games: Option<usize>,
    pub hours: Option<f64>,
    pub username: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct UpdateInfo {
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub current: String,
    pub version: Option<String>,
    pub url: Option<String>,
    pub asset_name: Option<String>,
    pub asset_size: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpdateProgress {
    pub phase: String,
    pub received: u64,
    pub total: Option<u64>,
    pub percent: u8,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ProfileBadge {
    pub id: String,
    #[serde(default)]
    pub description: String,
    pub icon_url: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct HypeSquadStatus {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub house: Option<String>,
    #[serde(default)]
    pub flags: u64,
    #[serde(default)]
    pub badges: Vec<ProfileBadge>,
}

pub trait ConfigExt {
    fn value(&self, key: &str) -> String;
}

impl ConfigExt for Config {
    fn value(&self, key: &str) -> String {
        self.get(key).cloned().unwrap_or_default()
    }
}

impl Profile {
    pub fn display_name(&self) -> String {
        self.global_name
            .clone()
            .or_else(|| self.username.clone())
            .unwrap_or_else(|| "Discord account".into())
    }

    pub fn handle(&self) -> String {
        match &self.username {
            Some(name) => format!("@{name}"),
            None => String::new(),
        }
    }

    pub fn initials(&self) -> String {
        self.display_name()
            .chars()
            .next()
            .map(|value| value.to_uppercase().to_string())
            .unwrap_or_else(|| "?".into())
    }
}
