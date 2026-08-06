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
                "OSL: unknown auto-rule choice {raw:?}; valid choices: {}",
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
