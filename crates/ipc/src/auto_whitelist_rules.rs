//! Saved auto-whitelist rules, keyed by app kind.
//!
//! This is deliberately only a naming/rule surface. The send path still
//! resolves concrete recipients through the existing whitelist and friend
//! checks; these rules describe what the app may do when it sees a new
//! whitelist opportunity for a kind of app.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

/// The complete set of user-visible rule choices.
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

pub fn parse_auto_whitelist_choice(input: &str) -> Result<AutoWhitelistChoice, String> {
    let normalized = input.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    AutoWhitelistChoice::ALL
        .into_iter()
        .find(|choice| normalized == choice.id())
        .ok_or_else(|| format!("OSL: unknown auto-whitelist rule choice '{input}'"))
}

/// The complete set of Discord place kinds that can be whitelisted.
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

    pub fn label(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct message",
            Self::GroupChat => "group chat",
            Self::Server => "server",
            Self::ServerChannel => "server channel",
            Self::Thread => "thread",
        }
    }
}

pub fn discord_whitelist_kind_labels() -> Vec<&'static str> {
    DiscordWhitelistKind::ALL
        .iter()
        .map(|kind| kind.label())
        .collect()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SignalWhitelistKind {
    DirectMessage,
    GroupChat,
}

impl SignalWhitelistKind {
    pub const ALL: [Self; 2] = [Self::DirectMessage, Self::GroupChat];

    pub fn id(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct_message",
            Self::GroupChat => "group_chat",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct message",
            Self::GroupChat => "group chat",
        }
    }

    pub fn auto_rule_app_kind(self) -> AutoWhitelistAppKind {
        match self {
            Self::DirectMessage => AutoWhitelistAppKind::SignalDirectMessage,
            Self::GroupChat => AutoWhitelistAppKind::SignalGroupChat,
        }
    }

    pub fn auto_rule_app_kind_id(self) -> &'static str {
        match self {
            Self::DirectMessage => "signal_direct_message",
            Self::GroupChat => "signal_group_chat",
        }
    }

    pub fn allowed_place_kind(self) -> &'static str {
        self.id()
    }
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
