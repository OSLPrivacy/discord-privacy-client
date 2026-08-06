//! Auto-whitelist rule names and per-app-kind validation.

use serde::{Deserialize, Serialize};

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

pub const AUTO_WHITELIST_APP_KINDS: [&str; 7] = [
    "discord",
    "telegram",
    "signal",
    "whatsapp",
    "outlook",
    "email_address",
    "email_domain",
];

pub fn parse_auto_whitelist_choice(input: &str) -> Result<AutoWhitelistChoice, String> {
    let normalized = input.trim().to_ascii_lowercase().replace('-', "_");
    AutoWhitelistChoice::ALL
        .into_iter()
        .find(|choice| normalized == choice.id() || normalized == choice.label())
        .ok_or_else(|| format!("OSL: unknown auto-whitelist rule choice '{input}'"))
}

pub fn normalize_auto_whitelist_app_kind(input: &str) -> Result<String, String> {
    let normalized = normalize_rule_part(input);
    if AUTO_WHITELIST_APP_KINDS.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(format!("OSL: unknown auto-whitelist app kind '{input}'"))
    }
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
