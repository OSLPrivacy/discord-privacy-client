//! Stored allowed-place record identity.
//!
//! The store actions live in later tasks. This module names the durable row
//! shape those actions will persist and query.

use serde::{Deserialize, Serialize};

pub const APP_DISCORD: &str = "discord";
pub const KIND_DIRECT_MESSAGE: &str = "direct_message";

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
}
