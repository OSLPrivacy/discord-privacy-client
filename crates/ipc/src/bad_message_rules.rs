use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BadMessageRule {
    pub rule_name: String,
    pub private_word: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadMessageRuleName {
    PrivateWords,
}

impl BadMessageRuleName {
    pub const ALL: [Self; 1] = [Self::PrivateWords];

    pub fn name(self) -> &'static str {
        match self {
            Self::PrivateWords => "private words",
        }
    }
}

pub fn parse_bad_message_rule_name(input: &str) -> Result<BadMessageRuleName, String> {
    let normalized = input.trim().to_ascii_lowercase().replace(['-', '_'], " ");
    BadMessageRuleName::ALL
        .into_iter()
        .find(|rule| normalized == rule.name())
        .ok_or_else(|| format!("OSL: unknown rule name '{input}'"))
}

pub fn parse_private_word(input: &str) -> Result<String, String> {
    let word = input.trim();
    if word.is_empty() {
        Err("OSL: empty private word".to_string())
    } else {
        Ok(word.to_string())
    }
}
