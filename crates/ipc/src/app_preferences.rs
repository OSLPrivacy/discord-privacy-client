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
use std::collections::BTreeMap;
use std::path::Path;

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

/// When to warn before interacting with a conversation whose verification has
/// not been confirmed by the user.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum VerificationWarningChoice {
    #[default]
    #[serde(rename = "every time")]
    EveryTime,
    #[serde(rename = "once")]
    Once,
    #[serde(rename = "before sending")]
    BeforeSending,
    #[serde(rename = "never")]
    Never,
}

impl VerificationWarningChoice {
    pub const ALL: [Self; 4] = [
        Self::EveryTime,
        Self::Once,
        Self::BeforeSending,
        Self::Never,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::EveryTime => "every time",
            Self::Once => "once",
            Self::BeforeSending => "before sending",
            Self::Never => "never",
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyLevel {
    Basic,
    #[default]
    Balanced,
    Maximum,
}

impl PrivacyLevel {
    pub const ALL: [Self; 3] = [Self::Basic, Self::Balanced, Self::Maximum];

    pub fn id(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::Balanced => "balanced",
            Self::Maximum => "maximum",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Basic => "Basic",
            Self::Balanced => "Balanced",
            Self::Maximum => "Maximum",
        }
    }
}

pub fn parse_verification_warning_choice(
    choice: &str,
) -> Result<VerificationWarningChoice, String> {
    VerificationWarningChoice::ALL
        .into_iter()
        .find(|candidate| candidate.label() == choice)
        .ok_or_else(|| format!("OSL: unknown verification warning choice '{choice}'"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehaviourChoiceName {
    Position,
    RememberPlace,
    Movement,
    TrayPicture,
    Sound,
    Mute,
    QuietHours,
}

impl BehaviourChoiceName {
    pub const ALL: [Self; 7] = [
        Self::Position,
        Self::RememberPlace,
        Self::Movement,
        Self::TrayPicture,
        Self::Sound,
        Self::Mute,
        Self::QuietHours,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Position => "position",
            Self::RememberPlace => "remember place",
            Self::Movement => "movement",
            Self::TrayPicture => "tray picture",
            Self::Sound => "sound",
            Self::Mute => "mute",
            Self::QuietHours => "quiet hours",
pub fn parse_privacy_level(input: &str) -> Result<PrivacyLevel, String> {
    let normalized = input.trim().to_ascii_lowercase().replace('-', "_");
    PrivacyLevel::ALL
        .into_iter()
        .find(|level| normalized == level.id())
        .ok_or_else(|| format!("OSL: unknown privacy level '{input}'"))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrivacyLevelRuleSet {
    pub before_send_warnings: bool,
    pub attachment_cleaning: bool,
    pub cleanup_review_days: u16,
    pub public_post_checks: bool,
    pub vpn_required_actions: bool,
    pub protected_contacts_required: bool,
}

impl PrivacyLevelRuleSet {
    pub fn for_level(level: PrivacyLevel) -> Self {
        match level {
            PrivacyLevel::Basic => Self {
                before_send_warnings: false,
                attachment_cleaning: false,
                cleanup_review_days: 0,
                public_post_checks: false,
                vpn_required_actions: false,
                protected_contacts_required: false,
            },
            PrivacyLevel::Balanced => Self {
                before_send_warnings: true,
                attachment_cleaning: true,
                cleanup_review_days: 30,
                public_post_checks: false,
                vpn_required_actions: false,
                protected_contacts_required: false,
            },
            PrivacyLevel::Maximum => Self {
                before_send_warnings: true,
                attachment_cleaning: true,
                cleanup_review_days: 7,
                public_post_checks: true,
                vpn_required_actions: true,
                protected_contacts_required: true,
            },
        }
    }
}

pub fn parse_behaviour_choice_name(name: &str) -> Result<BehaviourChoiceName, String> {
    let name = name.trim();
    BehaviourChoiceName::ALL
        .into_iter()
        .find(|candidate| candidate.label() == name)
        .ok_or_else(|| format!("OSL: unknown behaviour choice '{name}'"))
}

pub fn normalize_behaviour_choice_value(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 {
        return Err("OSL: behaviour choice value is invalid".to_string());
    }
    Ok(value.to_string())
impl Default for PrivacyLevelRuleSet {
    fn default() -> Self {
        Self::for_level(PrivacyLevel::Balanced)
    }
}

/// Saved defaults applied when the user starts a new message.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageScopeDefault {
    #[default]
    Message,
    Conversation,
    App,
}

/// Saved preference for how outgoing text is written.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageWriterDefault {
    #[default]
    Plaintext,
    AiCovertext,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageDefaults {
    #[serde(default)]
    pub scope: MessageScopeDefault,
    #[serde(default = "default_message_timer_seconds")]
    pub timer_seconds: u32,
    #[serde(default = "default_display_length_seconds")]
    pub display_length_seconds: u32,
    #[serde(default)]
    pub writer: MessageWriterDefault,
}

impl Default for MessageDefaults {
    fn default() -> Self {
        Self {
            scope: MessageScopeDefault::default(),
            timer_seconds: default_message_timer_seconds(),
            display_length_seconds: default_display_length_seconds(),
            writer: MessageWriterDefault::default(),
        }
    }
}

fn default_message_timer_seconds() -> u32 {
    300
}

fn default_display_length_seconds() -> u32 {
    10
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
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
    pub auto_whitelist_rules: HashMap<String, crate::auto_whitelist_rules::AutoWhitelistChoice>,
    #[serde(default)]
    pub verification_warning: VerificationWarningChoice,
    #[serde(default)]
    pub behaviour_choices: HashMap<String, String>,
    pub privacy_level: PrivacyLevel,
    #[serde(default)]
    pub privacy_level_rule_sets: BTreeMap<String, PrivacyLevelRuleSet>,
    #[serde(default)]
    pub message_defaults: MessageDefaults,
}

pub const APP_PREFERENCES_VERSION: u32 = 2;

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
