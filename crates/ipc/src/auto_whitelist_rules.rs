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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalWhitelistKind {
    DirectMessage,
    GroupChat,
    Story,
}

impl SignalWhitelistKind {
    pub const ALL: [Self; 3] = [Self::DirectMessage, Self::GroupChat, Self::Story];

    pub fn id(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct_message",
            Self::GroupChat => "group_chat",
            Self::Story => "story",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct message",
            Self::GroupChat => "group chat",
            Self::Story => "story",
        }
    }

    pub fn auto_rule_app_kind(self) -> &'static str {
        match self {
            Self::DirectMessage => "signal_direct_message",
            Self::GroupChat => "signal_group_chat",
            Self::Story => "signal_story",
        }
    }

    pub fn allowed_place_kind(self) -> &'static str {
        self.id()
    }
}

pub const AUTO_WHITELIST_APP_KINDS: [&str; 8] = [
    "discord",
    "telegram",
    "signal",
    "signal_direct_message",
    "signal_group_chat",
    "signal_story",
    "whatsapp",
    "outlook",
];

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

pub fn parse_signal_whitelist_kind(input: &str) -> Result<SignalWhitelistKind, String> {
    let normalized = input
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
        .replace(' ', "_");
    SignalWhitelistKind::ALL
        .into_iter()
        .find(|kind| normalized == kind.id() || normalized == kind.name().replace(' ', "_"))
        .ok_or_else(|| format!("OSL: unknown Signal whitelist kind '{input}'"))
}
