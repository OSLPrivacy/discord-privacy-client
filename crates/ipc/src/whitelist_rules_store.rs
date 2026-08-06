use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use thiserror::Error;

const RULES_FILE: &str = "whitelist_rules.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NewlyFoundConversationRule {
    Ask,
    Deny,
}

impl NewlyFoundConversationRule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Deny => "denied",
        }
    }
}

impl TryFrom<&str> for NewlyFoundConversationRule {
    type Error = WhitelistRulesStoreError;

    fn try_from(value: &str) -> Result<Self> {
        match value {
            "ask" => Ok(Self::Ask),
            "deny" | "denied" => Ok(Self::Deny),
            other => Err(WhitelistRulesStoreError::InvalidRule {
                rule: other.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WhitelistConversationDecision {
    Allowed,
    Denied,
    Ask,
}

impl WhitelistConversationDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Denied => "denied",
            Self::Ask => "ask",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhitelistRulesFile {
    pub version: u32,
    pub allowed_conversations: BTreeSet<String>,
    pub newly_found_conversation_rule: NewlyFoundConversationRule,
}

impl WhitelistRulesFile {
    pub fn new(
        allowed_conversations: impl IntoIterator<Item = String>,
        newly_found_conversation_rule: NewlyFoundConversationRule,
    ) -> Self {
        Self {
            version: 1,
            allowed_conversations: allowed_conversations.into_iter().collect(),
            newly_found_conversation_rule,
        }
    }

    pub fn lookup(&self, conversation_id: &str) -> WhitelistConversationDecision {
        if self.allowed_conversations.contains(conversation_id) {
            WhitelistConversationDecision::Allowed
        } else {
            match self.newly_found_conversation_rule {
                NewlyFoundConversationRule::Ask => WhitelistConversationDecision::Ask,
                NewlyFoundConversationRule::Deny => WhitelistConversationDecision::Denied,
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum WhitelistRulesStoreError {
    #[error("filesystem: {0}")]
    Fs(#[from] std::io::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid newly found conversation rule: {rule}")]
    InvalidRule { rule: String },
}

pub type Result<T> = std::result::Result<T, WhitelistRulesStoreError>;

pub fn whitelist_rules_path(app_data_dir: impl AsRef<Path>) -> PathBuf {
    app_data_dir.as_ref().join(RULES_FILE)
}

pub fn save_whitelist_rules(
    app_data_dir: impl AsRef<Path>,
    rules: &WhitelistRulesFile,
) -> Result<()> {
    std::fs::create_dir_all(app_data_dir.as_ref())?;
    let path = whitelist_rules_path(app_data_dir);
    let body = serde_json::to_string_pretty(rules)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn load_whitelist_rules(app_data_dir: impl AsRef<Path>) -> Result<WhitelistRulesFile> {
    let path = whitelist_rules_path(app_data_dir);
    let body = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&body)?)
}

pub fn lookup_whitelist_rule(
    app_data_dir: impl AsRef<Path>,
    conversation_id: impl AsRef<str>,
) -> Result<WhitelistConversationDecision> {
    let rules = load_whitelist_rules(app_data_dir)?;
    Ok(rules.lookup(conversation_id.as_ref()))
}
