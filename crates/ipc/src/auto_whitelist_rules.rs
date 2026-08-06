//! Auto-whitelist rule names and per-place validation.

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WhatsAppWhitelistKind {
    DirectMessage,
    GroupChat,
    Channel,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiscordWhitelistKind {
    DirectMessage,
    GroupChat,
    Server,
    ServerChannel,
    Thread,
}

impl DiscordWhitelistKind {
    pub const ALL: [Self; 5] = [
        Self::DirectMessage,
        Self::GroupChat,
        Self::Server,
        Self::ServerChannel,
        Self::Thread,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct_message",
            Self::GroupChat => "group_chat",
            Self::Server => "server",
            Self::ServerChannel => "server_channel",
            Self::Thread => "thread",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct message",
            Self::GroupChat => "group chat",
            Self::Server => "server",
            Self::ServerChannel => "server channel",
            Self::Thread => "thread",
        }
    }
}

impl WhatsAppWhitelistKind {
    pub const ALL: [Self; 3] = [Self::DirectMessage, Self::GroupChat, Self::Channel];

    pub fn id(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct_message",
            Self::GroupChat => "group_chat",
            Self::Channel => "channel",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct message",
            Self::GroupChat => "group chat",
            Self::Channel => "channel",
        }
    }
}

pub fn parse_auto_whitelist_choice(input: &str) -> Result<AutoWhitelistChoice, String> {
    let normalized = input.trim().to_ascii_lowercase().replace('-', "_");
    AutoWhitelistChoice::ALL
        .into_iter()
        .find(|choice| normalized == choice.id() || normalized == choice.label())
        .ok_or_else(|| format!("OSL: unknown auto-whitelist rule choice '{input}'"))
}

pub fn parse_discord_whitelist_kind(input: &str) -> Result<DiscordWhitelistKind, String> {
    let normalized = input.trim().to_ascii_lowercase().replace('-', "_");
    DiscordWhitelistKind::ALL
        .into_iter()
        .find(|kind| normalized == kind.id() || normalized == kind.name())
        .ok_or_else(|| format!("OSL: unknown Discord whitelist kind '{input}'"))
}

pub fn parse_whatsapp_whitelist_kind(input: &str) -> Result<WhatsAppWhitelistKind, String> {
    let normalized = input.trim().to_ascii_lowercase().replace('-', "_");
    WhatsAppWhitelistKind::ALL
        .into_iter()
        .find(|kind| normalized == kind.id() || normalized == kind.name())
        .ok_or_else(|| format!("OSL: unknown WhatsApp whitelist kind '{input}'"))
}

pub fn discord_auto_whitelist_rule_key(kind: DiscordWhitelistKind) -> String {
    format!("discord:{}", kind.id())
}

pub fn whatsapp_auto_whitelist_rule_key(kind: WhatsAppWhitelistKind) -> String {
    format!("whatsapp:{}", kind.id())
}

pub fn discord_allowed_place_kind_for_rule_key(rule_key: &str) -> Option<&'static str> {
    let rest = rule_key.strip_prefix("discord:")?;
    DiscordWhitelistKind::ALL
        .into_iter()
        .find(|kind| rest == kind.id())
        .map(DiscordWhitelistKind::id)
}

pub fn whatsapp_allowed_place_kind_for_rule_key(rule_key: &str) -> Option<&'static str> {
    let rest = rule_key.strip_prefix("whatsapp:")?;
    WhatsAppWhitelistKind::ALL
        .into_iter()
        .find(|kind| rest == kind.id())
        .map(WhatsAppWhitelistKind::id)
}

pub fn normalize_auto_whitelist_app_kind(input: &str) -> Result<String, String> {
    let normalized = input.trim().to_ascii_lowercase().replace('-', "_");
    if let Some((app, kind)) = normalized.split_once(':') {
        if app == "discord" {
            return Ok(discord_auto_whitelist_rule_key(
                parse_discord_whitelist_kind(kind)?,
            ));
        }
        if app == "whatsapp" {
            return Ok(whatsapp_auto_whitelist_rule_key(
                parse_whatsapp_whitelist_kind(kind)?,
            ));
        }
    }
    if AUTO_WHITELIST_APP_KINDS.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(format!("OSL: unknown auto-whitelist app kind '{input}'"))
    }
}
