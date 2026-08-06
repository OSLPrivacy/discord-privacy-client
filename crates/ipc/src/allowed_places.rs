//! Stored allowed-place record identity.
//!
//! The store actions live in later tasks. This module names the durable row
//! shape those actions will persist and query.

use serde::{Deserialize, Serialize};

pub const APP_DISCORD: &str = "discord";
pub const APP_TELEGRAM: &str = "telegram";
pub const KIND_DIRECT_MESSAGE: &str = "direct_message";
pub const KIND_GROUP_CHAT: &str = "group_chat";
pub const KIND_CHANNEL: &str = "channel";
pub const KIND_PUBLIC_POST: &str = "public_post";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AllowedPlaceKind {
    pub app: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AllowedPlaceRecord {
    pub app: String,
    pub account: String,
    pub kind: String,
    pub stable_id: String,
}

impl AllowedPlaceRecord {
    pub fn discord_direct_message(
        account: impl Into<String>,
        direct_message_id: impl Into<String>,
    ) -> Self {
        let account = account.into();
        let direct_message_id = direct_message_id.into();
        Self {
            app: APP_DISCORD.to_string(),
            account: account.clone(),
            kind: KIND_DIRECT_MESSAGE.to_string(),
            stable_id: format!("{APP_DISCORD}:{account}:{KIND_DIRECT_MESSAGE}:{direct_message_id}"),
        }
    }

    pub fn telegram(
        account: impl Into<String>,
        kind: impl AsRef<str>,
        place_id: impl Into<String>,
    ) -> Result<Self, String> {
        let account = account.into();
        let kind = normalize_telegram_whitelist_kind(kind.as_ref())?;
        let place_id = place_id.into();
        Ok(Self {
            app: APP_TELEGRAM.to_string(),
            account: account.clone(),
            kind: kind.clone(),
            stable_id: format!("{APP_TELEGRAM}:{account}:{kind}:{place_id}"),
        })
    }
}

pub fn validate_allowed_place_record(record: &AllowedPlaceRecord) -> Result<(), String> {
    validate_allowed_place_field(&record.app, "app")?;
    validate_allowed_place_field(&record.account, "account")?;
    validate_allowed_place_field(&record.kind, "kind")?;
    validate_allowed_place_field(&record.stable_id, "stable ID")?;

    let expected_prefix = format!("{}:{}:{}:", record.app, record.account, record.kind);
    if !record.stable_id.starts_with(&expected_prefix) {
        return Err(
            "OSL: allowed-place stable ID does not match app, account, and kind".to_string(),
        );
    }
    Ok(())
}

fn validate_allowed_place_field(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 512 || value.contains('\0') {
        return Err(format!("OSL: allowed-place {label} is invalid"));
    }
    Ok(())
}

pub fn telegram_whitelist_kinds() -> Vec<AllowedPlaceKind> {
    [
        KIND_DIRECT_MESSAGE,
        KIND_GROUP_CHAT,
        KIND_CHANNEL,
        KIND_PUBLIC_POST,
    ]
    .into_iter()
    .map(|name| AllowedPlaceKind {
        app: APP_TELEGRAM.to_string(),
        name: name.to_string(),
    })
    .collect()
}

pub fn normalize_telegram_whitelist_kind(input: &str) -> Result<String, String> {
    let normalized = input.trim().to_ascii_lowercase().replace('-', "_");
    if [
        KIND_DIRECT_MESSAGE,
        KIND_GROUP_CHAT,
        KIND_CHANNEL,
        KIND_PUBLIC_POST,
    ]
    .contains(&normalized.as_str())
    {
        Ok(normalized)
    } else {
        Err(format!("OSL: unknown Telegram whitelist kind '{input}'"))
    }
}
