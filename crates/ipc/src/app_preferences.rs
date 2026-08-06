//! Phase 9-B1 Task 1: `app_preferences.json`.
//!
//! User-tunable settings persisted at rest. v1 carries the Mode 0 vs
//! Mode 1 stego selector. Mirrors the [`crate::burned_scopes_file`]
//! pattern: small struct, atomic `.tmp + rename` write, OSL-ENC1
//! envelope when a file storage key is configured.
//!
//! On-disk default (missing file) is `Mode0` to preserve pre-B1
//! behavior — every existing install reads as if it had Mode 0
//! selected.
//!
//! 9-MODE1-FIX removed `always_preview_mode1` + `mode1_confirmed_scopes`:
//! Mode 1 sends fire chunks immediately, no preview modal. Legacy files
//! that still carry those fields load fine — serde silently drops
//! unknown JSON keys.
//!
//! 9-D added `tour` (onboarding tour resume/complete state). The
//! W4 removal dropped the old `vpn_warning_dismissed_forever` field;
//! legacy files carrying it still load (unknown keys are ignored).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

pub const DEFAULT_LANGUAGE_CHOICE: &str = "en";

pub fn default_language_choice() -> String {
    DEFAULT_LANGUAGE_CHOICE.to_string()
}

/// Active stego envelope. Mode 0 is the production `DPC0::<b64>`
/// path; Mode 1 is the multi-message `DPC1::<sentences>` cover
/// added in 9-B1.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StegoMode {
    #[default]
    Mode0,
    Mode1,
}

/// 9-D: onboarding tour resume state. Default = not started.
/// `completed` gates re-show across launches; `last_slide` is the
/// resume cursor (1..=9, or 0 when not yet started). `skipped` is
/// diagnostic — completed=true is the only suppression gate.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TourState {
    #[serde(default)]
    pub completed: bool,
    #[serde(default)]
    pub skipped: bool,
    #[serde(default)]
    pub last_slide: u8,
}

/// G3.3: auto-updater channel. `Stable` = everyone; `Beta` = a paid
/// perk (early access to newer builds). Default = `Stable`; legacy
/// `app_preferences.json` files without this key load as `Stable`.
///
/// NOTE: channel is a UX affordance, NOT a security boundary. The
/// worst case of a free user forcing `Beta` is a slightly-newer
/// *build* — never a paywalled capability (real paid features are
/// gated in the seal commands / server-side). The UI hides Beta for
/// non-paid users to set expectations; do not "harden" this into a
/// fake server-side eligibility check.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Beta,
}

impl UpdateChannel {
    /// Query-param value sent to the keyserver manifest endpoint.
    pub fn as_query_value(self) -> &'static str {
        match self {
            UpdateChannel::Stable => "stable",
            UpdateChannel::Beta => "beta",
        }
    }
}

/// User's choice for whether OSL should start when Windows starts.
/// This stores only the user's on/off preference; platform-specific
/// startup registration is handled outside `app_preferences.json`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StartWithWindowsChoice {
    On,
    #[default]
    Off,
}

impl StartWithWindowsChoice {
    pub fn as_value(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
        }
    }
}

impl FromStr for StartWithWindowsChoice {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "on" => Ok(Self::On),
            "off" => Ok(Self::Off),
            _ => Err(format!(
                "OSL: unknown start-with-Windows choice {raw:?}; valid choices: on, off"
            )),
        }
    }
}

/// Default reach granted to a newly added friend.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NewFriendAccountReach {
    #[default]
    ApprovedChatsOnly,
    AllSharedChats,
}

impl NewFriendAccountReach {
    pub fn as_value(self) -> &'static str {
        match self {
            Self::ApprovedChatsOnly => "approved_chats_only",
            Self::AllSharedChats => "all_shared_chats",
        }
    }
}

impl FromStr for NewFriendAccountReach {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().replace(['-', ' '], "_").as_str() {
            "approved_chats_only" => Ok(Self::ApprovedChatsOnly),
            "all_shared_chats" => Ok(Self::AllSharedChats),
            _ => Err(format!(
                "OSL: unknown new-friend account reach {raw:?}; valid choices: approved_chats_only, all_shared_chats"
            )),
        }
    }
}

/// Whether new-friend verification warnings are shown by default.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NewFriendVerificationWarnings {
    #[default]
    Enabled,
    Disabled,
}

impl NewFriendVerificationWarnings {
    pub fn as_value(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }
}

impl FromStr for NewFriendVerificationWarnings {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().replace(['-', ' '], "_").as_str() {
            "enabled" => Ok(Self::Enabled),
            "disabled" => Ok(Self::Disabled),
            _ => Err(format!(
                "OSL: unknown new-friend verification warnings {raw:?}; valid choices: enabled, disabled"
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppPreferences {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub stego_mode: StegoMode,
    #[serde(default)]
    pub tour: TourState,
    #[serde(default)]
    pub update_channel: UpdateChannel,
    #[serde(default)]
    pub auto_whitelist_rules: HashMap<String, crate::auto_whitelist_rules::AutoWhitelistRule>,
    #[serde(default)]
    pub new_friend_account_reach: NewFriendAccountReach,
    #[serde(default)]
    pub new_friend_auto_whitelist: crate::auto_whitelist_rules::AutoWhitelistRule,
    #[serde(default)]
    pub new_friend_verification_warnings: NewFriendVerificationWarnings,
    #[serde(default)]
    pub start_with_windows: StartWithWindowsChoice,
    #[serde(default = "default_language_choice")]
    pub language: String,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            version: 0,
            stego_mode: StegoMode::default(),
            tour: TourState::default(),
            update_channel: UpdateChannel::default(),
            auto_whitelist_rules: HashMap::new(),
            new_friend_account_reach: NewFriendAccountReach::default(),
            new_friend_auto_whitelist: crate::auto_whitelist_rules::AutoWhitelistRule::default(),
            new_friend_verification_warnings: NewFriendVerificationWarnings::default(),
            start_with_windows: StartWithWindowsChoice::default(),
            language: default_language_choice(),
        }
    }
}

pub const APP_PREFERENCES_VERSION: u32 = 4;

pub fn load_app_preferences(path: &Path) -> AppPreferences {
    let Ok(blob) = std::fs::read(path) else {
        return AppPreferences::default();
    };
    let plain = match crate::main_password::maybe_decrypt_file(path, &blob) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "OSL: load app_preferences.json decrypt failed");
            return AppPreferences::default();
        }
    };
    serde_json::from_slice(&plain).unwrap_or_default()
}

pub fn write_app_preferences(path: &Path, prefs: &AppPreferences) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(prefs)
        .map_err(|e| format!("OSL: serialize app_preferences: {e}"))?;
    let out = crate::main_password::maybe_encrypt(&body)
        .map_err(|e| format!("OSL: encrypt app_preferences: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &out).map_err(|e| format!("OSL: write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("OSL: rename {}: {e}", path.display()))?;
    Ok(())
}
