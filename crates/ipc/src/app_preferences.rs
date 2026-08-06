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
use std::path::Path;
use std::str::FromStr;

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
    pub ask_before_irreversible_actions: AskBeforeIrreversibleActionsChoice,
    #[serde(default)]
    pub rn_wire_policy_requested: bool,
    #[serde(default)]
    pub start_with_windows: StartWithWindowsChoice,
    #[serde(default)]
    pub idle_lock_time_choice: IdleLockTimeChoice,
    #[serde(default)]
    pub follow_active_app_choice: FollowActiveAppChoice,
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
