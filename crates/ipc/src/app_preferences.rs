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
//!
//! 0713 added `rn_wire_policy_requested`, the saved next-generation
//! protected-message policy choice. Missing legacy files load as false.
//!
//! 3148 added `follow_active_app_choice`, the explicit on/off choice for
//! whether the OSL window follows whichever app is in front. Missing legacy
//! files load as off.

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

impl Default for PrivacyLevelRuleSet {
    fn default() -> Self {
        Self::for_level(PrivacyLevel::Balanced)
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

/// Whether the app should ask again before executing an irreversible action.
///
/// Default = `On`; fresh installs should make the user explicitly confirm
/// anything that cannot be undone unless they later choose otherwise.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AskBeforeIrreversibleActionsChoice {
    #[default]
    On,
    Off,
}

impl AskBeforeIrreversibleActionsChoice {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "on" => Ok(Self::On),
            "off" => Ok(Self::Off),
            other => Err(format!(
                "OSL: invalid ask-before-irreversible-actions choice '{other}'"
            )),
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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlertModeChoice {
    Silent,
    Quiet,
    #[default]
    Normal,
}

impl AlertModeChoice {
    pub const ALL: [Self; 3] = [Self::Silent, Self::Quiet, Self::Normal];

    pub fn words(self) -> &'static str {
        match self {
            Self::Silent => "silent",
            Self::Quiet => "quiet",
            Self::Normal => "normal",
        }
    }
}

pub fn parse_alert_mode_choice(input: &str) -> Result<AlertModeChoice, String> {
    let normalized = input.trim().to_ascii_lowercase().replace(['-', '_'], " ");
    AlertModeChoice::ALL
        .into_iter()
        .find(|choice| normalized == choice.words())
        .ok_or_else(|| format!("OSL: unknown alert mode choice '{input}'"))
}

/// How long OSL waits after the last owner activity before it locks itself.
/// Missing legacy preferences keep the historical 15-minute behavior; `Never`
/// is an explicit opt-out, not the default.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "choice", content = "seconds", rename_all = "snake_case")]
pub enum IdleLockTimeChoice {
    Never,
    AfterSeconds(u64),
}

impl Default for IdleLockTimeChoice {
    fn default() -> Self {
        Self::AfterSeconds(keystore::DEFAULT_INACTIVITY_SECONDS)
    }
}

impl IdleLockTimeChoice {
    pub fn seconds(self) -> Option<u64> {
        match self {
            Self::Never => None,
            Self::AfterSeconds(seconds) => Some(seconds),
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Never => "never".to_string(),
            Self::AfterSeconds(60) => "one minute".to_string(),
            Self::AfterSeconds(1) => "1 second".to_string(),
            Self::AfterSeconds(seconds) => format!("{seconds} seconds"),
        }
    }

    pub fn choice(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::AfterSeconds(60) => "one_minute",
            Self::AfterSeconds(_) => "seconds",
        }
    }
}

pub fn parse_idle_lock_time_choice(input: &str) -> Result<IdleLockTimeChoice, String> {
    let normalized = input.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "never" => return Ok(IdleLockTimeChoice::Never),
        "one minute" | "1 minute" | "1m" | "60s" => {
            return Ok(IdleLockTimeChoice::AfterSeconds(60));
        }
        _ => {}
    }

    let seconds = normalized.parse::<i64>().map_err(|_| {
        format!("OSL: unknown idle lock time '{input}'; use positive seconds or never")
    })?;
    if seconds <= 0 {
        return Err("OSL: idle lock time must be positive seconds or never".to_string());
    }
    Ok(IdleLockTimeChoice::AfterSeconds(seconds as u64))
}

/// Whether the OSL window should follow whichever app currently has focus.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FollowActiveAppChoice {
    #[default]
    Off,
    On,
}

impl FollowActiveAppChoice {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "on" => Ok(Self::On),
            "off" => Ok(Self::Off),
            _ => Err("follow_active_app_choice must be \"on\" or \"off\"".to_owned()),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
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
    pub ask_before_irreversible_actions: AskBeforeIrreversibleActionsChoice,
    #[serde(default)]
    pub rn_wire_policy_requested: bool,
    #[serde(default)]
    pub start_with_windows: StartWithWindowsChoice,
    #[serde(default)]
    pub idle_lock_time_choice: IdleLockTimeChoice,
    #[serde(default)]
    pub alert_mode_choice: AlertModeChoice,
    #[serde(default)]
    pub follow_active_app_choice: FollowActiveAppChoice,
    #[serde(default = "default_language_choice")]
    pub language: String,
    #[serde(default)]
    pub auto_whitelist_rules: HashMap<String, crate::auto_whitelist_rules::AutoWhitelistChoice>,
    #[serde(default)]
    pub privacy_level: PrivacyLevel,
    #[serde(default)]
    pub privacy_level_rule_sets: HashMap<String, PrivacyLevelRuleSet>,
    #[serde(default)]
    pub verification_warning: VerificationWarningChoice,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            version: 0,
            stego_mode: StegoMode::default(),
            tour: TourState::default(),
            update_channel: UpdateChannel::default(),
            ask_before_irreversible_actions: AskBeforeIrreversibleActionsChoice::default(),
            rn_wire_policy_requested: false,
            start_with_windows: StartWithWindowsChoice::default(),
            idle_lock_time_choice: IdleLockTimeChoice::default(),
            alert_mode_choice: AlertModeChoice::default(),
            follow_active_app_choice: FollowActiveAppChoice::default(),
            language: default_language_choice(),
            auto_whitelist_rules: HashMap::new(),
            privacy_level: PrivacyLevel::default(),
            privacy_level_rule_sets: HashMap::new(),
            verification_warning: VerificationWarningChoice::default(),
        }
    }
}

pub const APP_PREFERENCES_VERSION: u32 = 5;

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
