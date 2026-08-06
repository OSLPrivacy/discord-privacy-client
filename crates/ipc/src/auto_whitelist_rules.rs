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

    pub fn label(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::AskMe => "ask me",
            Self::Always => "always",
            Self::OnlyIfAFriend => "only if a friend",
        }
    }
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
