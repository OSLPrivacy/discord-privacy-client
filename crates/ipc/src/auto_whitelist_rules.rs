//! User rules for automatically allowing newly discovered places.

use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutoWhitelistRule {
    #[default]
    Never,
    AskMe,
    Always,
    OnlyIfFriend,
}

impl AutoWhitelistRule {
    pub const VALID_CHOICES: [Self; 4] =
        [Self::Never, Self::AskMe, Self::Always, Self::OnlyIfFriend];

    pub fn as_label(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::AskMe => "ask me",
            Self::Always => "always",
            Self::OnlyIfFriend => "only if a friend",
        }
    }
}

impl FromStr for AutoWhitelistRule {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let normalized = raw.trim().to_ascii_lowercase().replace(['-', '_'], " ");
        match normalized.as_str() {
            "never" => Ok(Self::Never),
            "ask me" => Ok(Self::AskMe),
            "always" => Ok(Self::Always),
            "only if a friend" => Ok(Self::OnlyIfFriend),
            _ => Err(format!(
                "OSL: unknown auto-whitelist rule {raw:?}; valid choices: {}",
                Self::VALID_CHOICES
                    .iter()
                    .map(|rule| rule.as_label())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
}

pub fn normalize_app_kind(raw: &str) -> Result<String, String> {
    let app_kind = raw.trim().to_ascii_lowercase();
    if app_kind.is_empty() {
        return Err("OSL: app_kind is empty".to_string());
    }
    if !app_kind
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
    {
        return Err("OSL: app_kind contains unsupported characters".to_string());
    }
    Ok(app_kind)
}
