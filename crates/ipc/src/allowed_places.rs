use serde::{Deserialize, Serialize};

/// One local place the user has explicitly allowed.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowedPlaceRecord {
    pub app: String,
    pub account: String,
    pub kind: String,
    pub stable_id: String,
}

impl AllowedPlaceRecord {
    pub fn discord_direct_message(account: &str, conversation_id: &str) -> Self {
        Self {
            app: "discord".to_owned(),
            account: account.to_owned(),
            kind: "direct_message".to_owned(),
            stable_id: format!("discord:{account}:direct_message:{conversation_id}"),
        }
    }
}
