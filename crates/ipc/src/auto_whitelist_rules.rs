//! Auto-whitelist rule names and per-app-kind validation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutoWhitelistChoice {
    Never,
    AskMe,
    Always,
    OnlyIfAFriend,
}

impl AutoWhitelistChoice {
    pub const ALL: [Self; 4] = [Self::Never, Self::AskMe, Self::Always, Self::OnlyIfAFriend];

    pub fn id(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::AskMe => "ask_me",
            Self::Always => "always",
            Self::OnlyIfAFriend => "only_if_a_friend",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::AskMe => "ask me",
            Self::Always => "always",
            Self::OnlyIfAFriend => "only if a friend",
        }
    }
}

impl Default for AutoWhitelistChoice {
    fn default() -> Self {
        Self::Never
    }
}

pub const AUTO_WHITELIST_APP_KINDS: [&str; 5] =
    ["discord", "telegram", "signal", "whatsapp", "outlook"];

pub fn parse_auto_whitelist_choice(input: &str) -> Result<AutoWhitelistChoice, String> {
    let normalized = input.trim().to_ascii_lowercase().replace('-', "_");
    AutoWhitelistChoice::ALL
        .into_iter()
        .find(|choice| normalized == choice.id() || normalized == choice.label())
        .ok_or_else(|| format!("OSL: unknown auto-whitelist rule choice '{input}'"))
}

pub fn normalize_auto_whitelist_app_kind(input: &str) -> Result<String, String> {
    let normalized = input.trim().to_ascii_lowercase().replace('-', "_");
    if AUTO_WHITELIST_APP_KINDS.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(format!("OSL: unknown auto-whitelist app kind '{input}'"))
    }
}
