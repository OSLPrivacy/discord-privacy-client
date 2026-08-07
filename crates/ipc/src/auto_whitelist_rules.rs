//! Auto-whitelist rule names and per-app-kind validation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutoWhitelistChoice {
    #[default]
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

pub const AUTO_WHITELIST_APP_KINDS: [&str; 5] =
    ["discord", "telegram", "signal", "whatsapp", "outlook"];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstagramWhitelistKind {
    DirectMessage,
    GroupChat,
    PublicPost,
    Story,
    Reel,
}

impl InstagramWhitelistKind {
    pub const ALL: [Self; 5] = [
        Self::DirectMessage,
        Self::GroupChat,
        Self::PublicPost,
        Self::Story,
        Self::Reel,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct_message",
            Self::GroupChat => "group_chat",
            Self::PublicPost => "public_post",
            Self::Story => "story",
            Self::Reel => "reel",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct message",
            Self::GroupChat => "group chat",
            Self::PublicPost => "public post",
            Self::Story => "story",
            Self::Reel => "reel",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TelegramWhitelistKind {
    DirectMessage,
    GroupChat,
    Channel,
    PublicPost,
    Supergroup,
    SavedMessages,
}

impl TelegramWhitelistKind {
    pub const ALL: [Self; 6] = [
        Self::DirectMessage,
        Self::GroupChat,
        Self::Channel,
        Self::PublicPost,
        Self::Supergroup,
        Self::SavedMessages,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct_message",
            Self::GroupChat => "group_chat",
            Self::Channel => "channel",
            Self::PublicPost => "public_post",
            Self::Supergroup => "supergroup",
            Self::SavedMessages => "saved_messages",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct message",
            Self::GroupChat => "group chat",
            Self::Channel => "channel",
            Self::PublicPost => "public post",
            Self::Supergroup => "supergroup",
            Self::SavedMessages => "saved messages",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
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
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessengerWhitelistKind {
    DirectMessage,
    GroupChat,
}

impl MessengerWhitelistKind {
    pub const ALL: [Self; 2] = [Self::DirectMessage, Self::GroupChat];

    pub fn id(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct_message",
            Self::GroupChat => "group_chat",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct message",
            Self::GroupChat => "group chat",
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

pub fn parse_instagram_whitelist_kind(input: &str) -> Result<InstagramWhitelistKind, String> {
    let normalized = input.trim().to_ascii_lowercase().replace('-', "_");
    InstagramWhitelistKind::ALL
        .into_iter()
        .find(|kind| normalized == kind.id() || normalized == kind.name())
        .ok_or_else(|| format!("OSL: unknown Instagram whitelist kind '{input}'"))
}

pub fn parse_telegram_whitelist_kind(input: &str) -> Result<TelegramWhitelistKind, String> {
    let normalized = input.trim().to_ascii_lowercase().replace('-', "_");
    TelegramWhitelistKind::ALL
        .into_iter()
        .find(|kind| normalized == kind.id() || normalized == kind.name())
        .ok_or_else(|| format!("OSL: unknown Telegram whitelist kind '{input}'"))
}

pub fn parse_messenger_whitelist_kind(input: &str) -> Result<MessengerWhitelistKind, String> {
    let normalized = input.trim().to_ascii_lowercase().replace('-', "_");
    MessengerWhitelistKind::ALL
        .into_iter()
        .find(|kind| normalized == kind.id() || normalized == kind.name())
        .ok_or_else(|| format!("OSL: unknown Messenger whitelist kind '{input}'"))
}

pub fn instagram_auto_whitelist_rule_key(kind: InstagramWhitelistKind) -> String {
    format!("instagram:{}", kind.id())
}

pub fn telegram_auto_whitelist_rule_key(kind: TelegramWhitelistKind) -> String {
    format!("telegram:{}", kind.id())
}

pub fn messenger_auto_whitelist_rule_key(kind: MessengerWhitelistKind) -> String {
    format!("messenger:{}", kind.id())
}

pub fn instagram_allowed_place_kind_for_rule_key(rule_key: &str) -> Option<&'static str> {
    let rest = rule_key.strip_prefix("instagram:")?;
    InstagramWhitelistKind::ALL
        .into_iter()
        .find(|kind| rest == kind.id())
        .map(InstagramWhitelistKind::id)
}

pub fn telegram_allowed_place_kind_for_rule_key(rule_key: &str) -> Option<&'static str> {
    let rest = rule_key.strip_prefix("telegram:")?;
    TelegramWhitelistKind::ALL
        .into_iter()
        .find(|kind| rest == kind.id())
        .map(TelegramWhitelistKind::id)
}

pub fn messenger_allowed_place_kind_for_rule_key(rule_key: &str) -> Option<&'static str> {
    let rest = rule_key.strip_prefix("messenger:")?;
    MessengerWhitelistKind::ALL
        .into_iter()
        .find(|kind| rest == kind.id())
        .map(MessengerWhitelistKind::id)
}

pub fn auto_whitelist_rule_key_for_place(app: &str, kind: &str) -> Result<String, String> {
    let normalized_app = app.trim().to_ascii_lowercase().replace('-', "_");
    if normalized_app == "instagram" {
        return Ok(instagram_auto_whitelist_rule_key(
            parse_instagram_whitelist_kind(kind)?,
        ));
    }
    if normalized_app == "telegram" {
        return Ok(telegram_auto_whitelist_rule_key(
            parse_telegram_whitelist_kind(kind)?,
        ));
    }
    if normalized_app == "messenger" {
        return Ok(messenger_auto_whitelist_rule_key(
            parse_messenger_whitelist_kind(kind)?,
        ));
    }
    normalize_auto_whitelist_app_kind(app)
}

pub fn normalize_auto_whitelist_app_kind(input: &str) -> Result<String, String> {
    let normalized = input.trim().to_ascii_lowercase().replace('-', "_");
    if let Some((app, kind)) = normalized.split_once(':') {
        if app == "instagram" {
            return Ok(instagram_auto_whitelist_rule_key(
                parse_instagram_whitelist_kind(kind)?,
            ));
        }
        if app == "telegram" {
            return Ok(telegram_auto_whitelist_rule_key(
                parse_telegram_whitelist_kind(kind)?,
            ));
        }
        if app == "messenger" {
            return Ok(messenger_auto_whitelist_rule_key(
                parse_messenger_whitelist_kind(kind)?,
            ));
        }
    }
pub fn normalize_auto_whitelist_app_kind(input: &str) -> Result<String, String> {
    let normalized = input.trim().to_ascii_lowercase().replace('-', "_");
    if AUTO_WHITELIST_APP_KINDS.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(format!("OSL: unknown auto-whitelist app kind '{input}'"))
    }
}
