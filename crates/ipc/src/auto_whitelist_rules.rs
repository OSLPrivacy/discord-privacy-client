//! Auto-whitelist rule names and per-app-kind validation.

use serde::{
    Deserialize,
    Serialize,
};
use std::collections::BTreeMap;
use std::path::Path;
//! User rules for automatically allowing newly discovered places.

use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutoWhitelistChoice {
    #[default]
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
    if AUTO_WHITELIST_APP_KINDS.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(format!("OSL: unknown auto-whitelist app kind '{input}'"))
    }
}

impl Default for AutoWhitelistChoice {
    fn default() -> Self {
        Self::Never
    }
}
    ["discord", "telegram", "signal", "whatsapp", "outlook"];

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

pub fn parse_discord_whitelist_kind(input: &str) -> Result<DiscordWhitelistKind, String> {
    let normalized = input.trim().to_ascii_lowercase().replace('-', "_");
    DiscordWhitelistKind::ALL
        .into_iter()
        .find(|kind| normalized == kind.id() || normalized == kind.name())
        .ok_or_else(|| format!("OSL: unknown Discord whitelist kind '{input}'"))
}

pub fn discord_auto_whitelist_rule_key(kind: DiscordWhitelistKind) -> String {
    format!("discord:{}", kind.id())
}

pub fn discord_allowed_place_kind_for_rule_key(rule_key: &str) -> Option<&'static str> {
    let rest = rule_key.strip_prefix("discord:")?;
    DiscordWhitelistKind::ALL
        .into_iter()
        .find(|kind| rest == kind.id())
        .map(DiscordWhitelistKind::id)
}

/// The kind of app a saved auto-whitelist rule applies to.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AutoWhitelistAppKind {
    Chat,
    Email,
    Browser,
    Native,
    SignalDirectMessage,
    SignalGroupChat,
}

pub fn discord_whitelist_kind_labels() -> Vec<&'static str> {
    DiscordWhitelistKind::ALL
        .iter()
        .map(|kind| kind.label())
        .collect()
}

pub fn parse_signal_whitelist_kind(input: &str) -> Result<SignalWhitelistKind, String> {
    let normalized = input.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    SignalWhitelistKind::ALL
        .into_iter()
        .find(|kind| normalized == kind.id())
        .ok_or_else(|| format!("OSL: unknown Signal whitelist kind '{input}'"))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutoWhitelistRule {
    pub app_kind: AutoWhitelistAppKind,
    pub choice: AutoWhitelistChoice,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutoWhitelistRules {
    #[serde(default)]
    rules: BTreeMap<AutoWhitelistAppKind, AutoWhitelistChoice>,
}

impl AutoWhitelistRules {
    pub fn save(
        &mut self,
        app_kind: AutoWhitelistAppKind,
        choice: AutoWhitelistChoice,
    ) -> AutoWhitelistRule {
        self.rules.insert(app_kind, choice);
        AutoWhitelistRule { app_kind, choice }
    }

    pub fn query(&self, app_kind: AutoWhitelistAppKind) -> AutoWhitelistRuleQuery {
        AutoWhitelistRuleQuery {
            app_kind,
            saved_choice: self.rules.get(&app_kind).copied(),
            valid_choices: AutoWhitelistChoice::ALL.to_vec(),
        }
    }
}

pub fn load_auto_whitelist_rules(path: &Path) -> AutoWhitelistRules {
    let Ok(blob) = std::fs::read(path) else {
        return AutoWhitelistRules::default();
    };
    let plain = match crate::main_password::maybe_decrypt_file(path, &blob) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "OSL: load auto_whitelist_rules.json decrypt failed");
            return AutoWhitelistRules::default();
        }
    };
    serde_json::from_slice(&plain).unwrap_or_default()
}

pub fn write_auto_whitelist_rules(path: &Path, rules: &AutoWhitelistRules) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(rules)
        .map_err(|e| format!("OSL: serialize auto_whitelist_rules: {e}"))?;
    let out = crate::main_password::maybe_encrypt(&body)
        .map_err(|e| format!("OSL: encrypt auto_whitelist_rules: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &out).map_err(|e| format!("OSL: write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("OSL: rename {}: {e}", path.display()))?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutoWhitelistRuleQuery {
    pub app_kind: AutoWhitelistAppKind,
    pub saved_choice: Option<AutoWhitelistChoice>,
    pub valid_choices: Vec<AutoWhitelistChoice>,
}

impl AutoWhitelistRuleQuery {
    pub fn valid_choice_labels(&self) -> Vec<&'static str> {
        self.valid_choices
            .iter()
            .map(|choice| choice.label())
            .collect()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WhatsAppWhitelistKind {
    DirectMessage,
    GroupChat,
    Channel,
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

    pub fn auto_rule_app_kind(self) -> &'static str {
        match self {
            Self::DirectMessage => "whatsapp:direct_message",
            Self::GroupChat => "whatsapp:group_chat",
            Self::Channel => "whatsapp:channel",
        }
    }

    pub fn allowed_place_kind(self) -> &'static str {
        self.id()
    }
}

pub fn parse_whatsapp_whitelist_kind(input: &str) -> Result<WhatsAppWhitelistKind, String> {
    let normalized = input
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
        .replace(' ', "_");
    WhatsAppWhitelistKind::ALL
        .into_iter()
        .find(|kind| normalized == kind.id() || normalized == kind.name().replace(' ', "_"))
        .ok_or_else(|| format!("OSL: unknown WhatsApp whitelist kind '{input}'"))
}

pub fn whatsapp_allowed_place_kind_for_rule_key(rule_key: &str) -> Option<&'static str> {
    let rest = rule_key.strip_prefix("whatsapp:")?;
    WhatsAppWhitelistKind::ALL
        .into_iter()
        .find(|kind| rest == kind.id())
        .map(WhatsAppWhitelistKind::allowed_place_kind)
}

pub fn whatsapp_auto_whitelist_rule_key(kind: WhatsAppWhitelistKind) -> String {
    format!("whatsapp:{}", kind.id())
}

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
                "OSL: unknown auto-rule choice {raw:?}; valid choices: {}",
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

pub const X_DIRECT_MESSAGE_PLACE_KIND: &str = "direct_message";
pub const X_PUBLIC_POST_PLACE_KIND: &str = "public_post";
pub const X_PLACE_KINDS: [&str; 2] = [X_DIRECT_MESSAGE_PLACE_KIND, X_PUBLIC_POST_PLACE_KIND];

pub fn normalize_place_kind_for_app(app_kind: &str, raw: &str) -> Result<String, String> {
    let place_kind = normalize_place_kind(raw)?;
    if app_kind == "x" && !X_PLACE_KINDS.contains(&place_kind.as_str()) {
        return Err(format!(
            "OSL: unknown X whitelist place_kind {raw:?}; valid choices: {}",
            X_PLACE_KINDS.join(", ")
        ));
    }
    Ok(place_kind)
}

pub fn scoped_rule_key(app_kind: &str, place_kind: &str) -> String {
    format!("{app_kind}/{place_kind}")
}

pub fn lookup_rule(
    rules: &std::collections::HashMap<String, AutoWhitelistRule>,
    app_kind: &str,
    place_kind: Option<&str>,
) -> AutoWhitelistRule {
    place_kind
        .and_then(|kind| rules.get(&scoped_rule_key(app_kind, kind)))
        .or_else(|| rules.get(app_kind))
        .copied()
        .unwrap_or_default()
}

fn normalize_place_kind(raw: &str) -> Result<String, String> {
    let place_kind = raw.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    if place_kind.is_empty() {
        return Err("OSL: place_kind is empty".to_string());
    }
    if !place_kind
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
    {
        return Err("OSL: place_kind contains unsupported characters".to_string());
    }
    Ok(place_kind)
}

pub fn auto_whitelist_app_kind_for_place(
    place: &crate::allowed_places::AllowedPlaceRecord,
) -> Result<String, String> {
    let app = normalize_rule_part(&place.app);
    let kind = normalize_rule_part(&place.kind);
    match app.as_str() {
        "email" => match kind.as_str() {
            "address" | "email_address" => Ok("email_address".to_string()),
            "domain" | "email_domain" => Ok("email_domain".to_string()),
            _ => Err(format!(
                "OSL: unknown email auto-whitelist place kind '{}'",
                place.kind
            )),
        },
        "email_address" | "email_domain" => Ok(app),
        _ => normalize_auto_whitelist_app_kind(&place.app),
    }
}

fn normalize_rule_part(input: &str) -> String {
    input.trim().to_ascii_lowercase().replace('-', "_")
}

pub fn app_kind_rule_lookup(app_kind: &str) -> Result<String, String> {
    normalize_auto_whitelist_app_kind(app_kind)
}

pub fn telegram_kind_rule_lookup(kind: &str) -> Result<String, String> {
    let kind = crate::allowed_places::normalize_telegram_whitelist_kind(kind)?;
    Ok(format!("{}:{kind}", crate::allowed_places::APP_TELEGRAM))
}
