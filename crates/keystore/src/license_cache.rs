//! F2.2: license cache and license-state surface.
//!
//! Two responsibilities co-located in one module because they're
//! tightly coupled:
//!   1. Encrypted-at-rest cache of the user's license validation
//!      result, sealed via the same [`Sealer`] trait that protects
//!      `identity.json`. The plaintext license key lives ONLY
//!      inside the AEAD-sealed inner blob — never on the outer
//!      wrapper, never on disk in clear.
//!   2. The classifier that maps a keyserver `status` string
//!      (`"ACTIVE"`, `"GRACE"`, …) to the [`LicenseState`] enum
//!      F3's ad gate will consume.
//!
//! The on-disk envelope mirrors [`crate::storage`]'s
//! [`IdentityOnDisk`] one-for-one: two-layer JSON where the outer
//! wrapper carries a method tag + version, and the inner blob is
//! the serialised secrets behind the AEAD. See
//! `docs/design/unlock-and-duress.md` "Storage" for the design
//! rationale.
//!
//! Cache file path: `<config_dir>/license.json`. Bootstrap's
//! `create_dir_all` already provisions the directory; we don't
//! re-create it here.
//!
//! Versioning: blob version starts at 1. Future migrations bump
//! this; loaders raise [`Error::BlobVersionMismatch`] on mismatch.

use crate::sealer::Sealer;
use crate::{Error, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::Path;

const LICENSE_BLOB_VERSION: u32 = 1;

const INSECURE_BANNER: &str = "INSECURE prototype license cache — plain JSON, no TPM/keyring \
     sealing. The license plaintext is at rest unencrypted; treat \
     this host as compromised if the file is exfiltrated.";

/// Outer on-disk wrapper. The structure mirrors
/// [`crate::storage::IdentityOnDisk`] so operators / forensic
/// tools can introspect the method tag without unsealing.
#[derive(Debug, Serialize, Deserialize)]
pub struct LicenseCacheOnDisk {
    pub version: u32,
    pub method: String,
    pub sealed_b64: String,
    /// Present iff `method == "noop-insecure"`. Loaders SHOULD
    /// surface this to the user as a "your license cache is not
    /// hardware-sealed" warning. (The matching banner on
    /// identity.json already alerts on the same condition; this
    /// one is duplicate-ish but the file can be inspected
    /// independently of identity.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insecure_banner: Option<String>,
}

/// Inner blob, sealed before write. Never appears on disk in
/// plain form.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LicenseCacheInner {
    /// The full `OSL-XXXX-XXXX-XXXX-XXXX` plaintext as entered by
    /// the user. The keyserver stores SHA-256(plaintext) only; the
    /// plaintext is delivered once via email at issuance, then
    /// lives here in the sealed cache for re-validation.
    pub license_plaintext: String,
    /// Last `status` value the keyserver returned on a successful
    /// (HTTP 200) round-trip. One of `"ACTIVE" | "GRACE" |
    /// "CANCELLED" | "EXPIRED" | "REVOKED" | "UNKNOWN" | "PENDING"`.
    pub last_validated_status: String,
    /// Cached `current_period_end` (unix seconds) from the same
    /// round-trip. `None` for PENDING / UNKNOWN / pre-F2.0 records.
    pub current_period_end: Option<i64>,
    /// Unix seconds of the most recent SUCCESSFUL keyserver
    /// round-trip. F2.4's offline-grace logic compares this against
    /// `now()` against the 7-day window; a failed validation must
    /// NOT bump this, or the window would slide forever.
    pub last_validated_at: i64,
    /// Last `checksum_ok` value. `false` means the user mistyped
    /// the key — the cache still persists it so the UI can
    /// recover from the same value on the next attempt.
    pub checksum_ok: bool,
}

/// Save `cache` to `path` sealed under `sealer`. Atomically
/// overwrites any existing file. Parent directory must already
/// exist (bootstrap's `create_dir_all` covers this).
pub fn save_license_cache(
    path: &Path,
    cache: &LicenseCacheInner,
    sealer: &dyn Sealer,
) -> Result<()> {
    let inner_bytes = serde_json::to_vec(cache)?;
    let sealed = sealer.seal(&inner_bytes)?;
    let on_disk = LicenseCacheOnDisk {
        version: LICENSE_BLOB_VERSION,
        method: sealer.method_label().to_string(),
        sealed_b64: STANDARD.encode(&sealed),
        insecure_banner: if sealer.requires_insecure_banner() {
            Some(INSECURE_BANNER.to_string())
        } else {
            None
        },
    };
    let json = serde_json::to_vec_pretty(&on_disk)?;
    std::fs::write(path, &json)?;
    Ok(())
}

/// Load + unseal the cache at `path`. Errors:
///   - [`Error::Io`] when the file is missing or unreadable.
///   - [`Error::BlobVersionMismatch`] when the on-disk version
///     differs from [`LICENSE_BLOB_VERSION`].
///   - [`Error::BlobMethodMismatch`] when the on-disk method tag
///     differs from `sealer.method_label()` (e.g. the user
///     migrated from TPM → no-TPM and the wrong sealer is being
///     used to load).
///   - [`Error::Sealer`] when the AEAD rejects the ciphertext
///     (tampered file, corrupt key material, wrong key).
///   - [`Error::Json`] / [`Error::Base64`] on malformed wrappers.
///
/// Per F2.2 spec: downstream callers treat any of these as "no
/// cache" rather than surfacing the variant. The variants exist
/// for debugging / logging.
pub fn load_license_cache(path: &Path, sealer: &dyn Sealer) -> Result<LicenseCacheInner> {
    let bytes = std::fs::read(path)?;
    let on_disk: LicenseCacheOnDisk = serde_json::from_slice(&bytes)?;
    if on_disk.version != LICENSE_BLOB_VERSION {
        return Err(Error::BlobVersionMismatch {
            got: on_disk.version,
            expected: LICENSE_BLOB_VERSION,
        });
    }
    if on_disk.method != sealer.method_label() {
        return Err(Error::BlobMethodMismatch {
            got: on_disk.method,
            expected: sealer.method_label().to_string(),
        });
    }
    let sealed = STANDARD.decode(&on_disk.sealed_b64)?;
    let inner_bytes = sealer.unseal(&sealed)?;
    let inner: LicenseCacheInner = serde_json::from_slice(&inner_bytes)?;
    Ok(inner)
}

// =====================================================================
// LicenseState — the public surface F3 consumes.
// =====================================================================

/// Three-state UX flag computed from the cached keyserver status
/// and (in F2.4) the offline-grace window.
///
/// F3's ad-gate logic reads this directly; the underlying
/// 6-state subscription model (PENDING/ACTIVE/CANCELLED/GRACE/
/// REVOKED/EXPIRED) does NOT leak past the keystore boundary.
///
/// - [`LicenseState::Paid`] — features unlocked. Mapped from
///   keyserver `ACTIVE`, `CANCELLED` (still-paid-through-period-end),
///   `GRACE` (Stripe payment-retry window).
/// - [`LicenseState::Free`] — features locked. Mapped from
///   `EXPIRED`, `REVOKED`, `UNKNOWN`, `PENDING`, no cache, or any
///   unrecognised status.
/// - [`LicenseState::PaidOfflineGrace`] — F2.4 only. Set when the
///   cached state is paid-equivalent, the most recent successful
///   validation was within 7 days, and the current re-validate
///   attempt failed because the keyserver was unreachable. UI
///   should surface a "couldn't refresh license" banner; features
///   remain unlocked.
///
/// Serialisation: serde renders each variant as its name (`"Paid"`,
/// `"Free"`, `"PaidOfflineGrace"`). F3's JS reads these strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LicenseState {
    Paid,
    Free,
    PaidOfflineGrace,
}

/// Map a keyserver `status` string to [`LicenseState`].
///
/// `CANCELLED` (user cancelled but is paid through `current_period_end`)
/// maps to `Paid` — Stripe keeps them paid until the period closes,
/// and the keyserver's hourly cron sweep flips them to `EXPIRED`
/// afterwards.
///
/// `GRACE` (Stripe payment-retry window after a failed renewal)
/// maps to `Paid` too — UI may surface a "payment failed" banner
/// (the consumer decides) but features stay unlocked, matching
/// the F1 state-machine intent.
///
/// All other statuses, including unrecognised ones, map to `Free`.
/// This is the pure-online classifier; F2.4 adds the offline-grace
/// overlay.
pub fn classify_state(keyserver_status: &str) -> LicenseState {
    match keyserver_status {
        "ACTIVE" | "CANCELLED" | "GRACE" => LicenseState::Paid,
        _ => LicenseState::Free,
    }
}

/// IPC DTO surfaced to the webview by `cmd_osl_get_license_state`.
/// `state` is the bottom-line flag F3 consumes; `raw_status` and
/// `current_period_end` are surfaced for the UI's "Renews on …" /
/// "Subscription cancelled" copy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LicenseStateDto {
    pub state: LicenseState,
    /// Keyserver's raw `status` from the most recent successful
    /// round-trip, or `"Unconfigured"` when there's no cache.
    pub raw_status: String,
    /// Unix seconds. None for unconfigured / pre-F2.0 records.
    pub current_period_end: Option<i64>,
    /// Unix seconds of the most recent successful round-trip.
    /// None for unconfigured.
    pub last_validated_at: Option<i64>,
}

impl LicenseStateDto {
    /// Build a DTO from a loaded cache row. Pure-online
    /// classification — F2.4 wraps this to overlay
    /// `PaidOfflineGrace` when the latest re-validate attempt
    /// failed.
    pub fn from_cache(cache: &LicenseCacheInner) -> Self {
        Self {
            state: classify_state(&cache.last_validated_status),
            raw_status: cache.last_validated_status.clone(),
            current_period_end: cache.current_period_end,
            last_validated_at: Some(cache.last_validated_at),
        }
    }

    /// DTO for the "no cache exists" case. `raw_status` is
    /// `"Unconfigured"` so the UI can distinguish "user never
    /// entered a key" from "key entered but status is unknown".
    pub fn unconfigured() -> Self {
        Self {
            state: LicenseState::Free,
            raw_status: "Unconfigured".to_string(),
            current_period_end: None,
            last_validated_at: None,
        }
    }
}

/// `Default::default()` for [`LicenseStateDto`] is the
/// "Unconfigured" shape. Used by [`AppState::default`] (via the
/// derive-cascade through `Mutex<LicenseStateDto>`) so a fresh
/// AppState — before the launch classify hook runs — already
/// presents as Free/Unconfigured.
impl Default for LicenseStateDto {
    fn default() -> Self {
        Self::unconfigured()
    }
}
