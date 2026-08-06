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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationWarningChoice {
    EveryTime,
    Once,
    BeforeSending,
    #[default]
    Never,
}

impl VerificationWarningChoice {
    pub const ALL: [Self; 4] = [
        Self::EveryTime,
        Self::Once,
        Self::BeforeSending,
        Self::Never,
    ];

    pub fn words(self) -> &'static str {
        match self {
            Self::EveryTime => "every time",
            Self::Once => "once",
            Self::BeforeSending => "before sending",
            Self::Never => "never",
        }
    }
}

pub fn parse_verification_warning_choice(input: &str) -> Result<VerificationWarningChoice, String> {
    let normalized = input.trim().to_ascii_lowercase().replace(['-', '_'], " ");
    VerificationWarningChoice::ALL
        .into_iter()
        .find(|choice| normalized == choice.words())
        .ok_or_else(|| format!("OSL: unknown verification warning choice '{input}'"))
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
    pub privacy_level: PrivacyLevel,
    #[serde(default)]
    pub privacy_level_rule_sets: HashMap<String, PrivacyLevelRuleSet>,
    #[serde(default)]
    pub verification_warning_choice: VerificationWarningChoice,
    #[serde(default)]
    pub alert_mode_choice: AlertModeChoice,
}

pub const APP_PREFERENCES_VERSION: u32 = 3;

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
