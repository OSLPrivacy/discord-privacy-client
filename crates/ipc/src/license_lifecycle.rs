//! F2.4: license-validation lifecycle.
//!
//! Two entry points:
//!
//! - [`launch_classify`] — synchronous, cache-only. Called from
//!   `bootstrap::run_autostart` before the webview comes up.
//!   Loads `<config_dir>/license.json` (sealed), classifies via
//!   [`keystore::LicenseStateDto::from_cache`], stamps
//!   [`AppState::license_state`]. **No network.** A paid user
//!   reads `Paid` from the very first render; a never-licensed
//!   user reads `Free/"Unconfigured"`. This is the
//!   no-launch-flicker guarantee from the F2 spec.
//!
//! - [`refresh_license_state`] — sync, but does I/O + a keyserver
//!   round-trip. Called from `main.rs` setup (spawn_blocking-ed
//!   on an immediate fire and then on a 6-hour interval). Three
//!   load-bearing outcomes:
//!     - keyserver answered 2xx → update cache, bump
//!       `last_validated_at = now()`, classify online → `Paid`/`Free`
//!     - keyserver UNREACHABLE
//!       ([`keystore::Error::Transport`]) → do **not** touch the
//!       cache, do **not** bump `last_validated_at`. Honour the
//!       7-day offline-grace window: if the cached
//!       `last_validated_at + 7 days > now` AND the cached
//!       status is paid-equivalent, surface
//!       [`LicenseState::PaidOfflineGrace`]; otherwise
//!       [`LicenseState::Free`].
//!     - keyserver answered with a non-2xx
//!       ([`keystore::Error::HttpStatus`]) or returned a malformed
//!       body ([`keystore::Error::Json`]) → classify from the
//!       cache as-is, but do **not** extend grace. A reachable
//!       keyserver saying "no" is authoritative; only network-
//!       unreachable buys the grace window.
//!
//! Why typed `Error` matching instead of string-matching on
//! `cmd_osl_validate_license`'s error prefixes? The F2.1 ship
//! report flagged the prefix dispatch as a temporary measure
//! pending F2.4. This module is the consumer, and it pattern-
//! matches on the real enum.

use crate::state::AppState;
use keystore::{
    classify_state, load_license_cache, save_license_cache, select_best_sealer, Error,
    KeyServerClient, LicenseCacheInner, LicenseState, LicenseStateDto,
};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const SEVEN_DAYS_SECONDS: i64 = 7 * 86_400;

fn unix_seconds_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Synchronous, cache-only classify. Stamps
/// [`AppState::license_state`] without touching the network.
///
/// Three branches:
///   - cache loads cleanly → `LicenseStateDto::from_cache(&inner)`
///   - cache absent OR malformed OR sealer-rejected →
///     `LicenseStateDto::unconfigured()`
///
/// Bootstrap calls this once before the webview mounts; the
/// stamp is durable for the lifetime of the process unless
/// [`refresh_license_state`] overwrites it.
pub fn launch_classify(state: &AppState, dir: &Path) {
    let sealer = select_best_sealer();
    let cache_path = dir.join("license.json");
    let dto = match load_license_cache(&cache_path, sealer.as_ref()) {
        Ok(inner) => LicenseStateDto::from_cache(&inner),
        Err(_) => LicenseStateDto::unconfigured(),
    };
    *state
        .license_state
        .lock()
        .expect("license_state mutex poisoned") = dto;
}

/// Full sync refresh: load cache → call keyserver →
/// classify outcome → stamp AppState. See the module doc for
/// the policy on each outcome.
///
/// Production callers (`main.rs` setup) wrap this in
/// `tauri::async_runtime::spawn_blocking` so the underlying
/// `reqwest::blocking` call doesn't stall the async runtime.
///
/// `dir` is the OSL config directory (typically
/// `<config_dir>/osl/`). Tests pass a `tempdir()`; production
/// passes `keystore::osl_config_dir()`.
pub fn refresh_license_state(state: &AppState, dir: &Path) -> LicenseStateDto {
    // Production path: resolve the keyserver URL from
    // <dir>/keyserver.json. If unconfigured, surface the cached
    // classification as-is (no online refresh possible) — but we
    // still need to load the cache first to know what to return.
    let sealer = select_best_sealer();
    let cache_path = dir.join("license.json");
    // No cache yet (fresh install pre-activation) → Unconfigured;
    // there's no paid state to refresh until the user activates via
    // `cmd_osl_validate_license` (which writes license.json).
    if load_license_cache(&cache_path, sealer.as_ref()).is_err() {
        let dto = LicenseStateDto::unconfigured();
        stamp(state, &dto);
        return dto;
    }
    // Cache exists. Fresh installs have no keyserver.json; fall back
    // to the built-in production URL so periodic re-validation still
    // reaches prod. keyserver.json stays an override (dev/staging).
    // The F2.4 outcome handling in `refresh_license_state_with_url`
    // (Transport→grace, HttpStatus/Json→stale) is unchanged; only
    // the URL source changed (previously: no keyserver.json →
    // returned the cached classification without any network call).
    let base_url = crate::commands::resolve_keyserver_base_url(dir);
    refresh_license_state_with_url(state, dir, &base_url)
}

/// Test-seam variant of [`refresh_license_state`]. Takes the
/// keyserver `base_url` explicitly so tests can point at an
/// in-process mock server. Production callers use the no-URL
/// form, which reads `keyserver.json` itself.
///
/// Same outcome policy as [`refresh_license_state`].
pub fn refresh_license_state_with_url(
    state: &AppState,
    dir: &Path,
    base_url: &str,
) -> LicenseStateDto {
    let sealer = select_best_sealer();
    let cache_path = dir.join("license.json");

    // Load cache (re-read here so the test seam works
    // independently of the production path's read).
    let cache = match load_license_cache(&cache_path, sealer.as_ref()) {
        Ok(inner) => inner,
        Err(_) => {
            let dto = LicenseStateDto::unconfigured();
            stamp(state, &dto);
            return dto;
        }
    };

    let client = match KeyServerClient::new(base_url) {
        Ok(c) => c,
        Err(_) => {
            let dto = LicenseStateDto::from_cache(&cache);
            stamp(state, &dto);
            return dto;
        }
    };

    // Dispatch on the typed Error variants. NOT string-matching
    // — see module doc.
    let now = unix_seconds_now();
    match client.validate_license(&cache.license_plaintext) {
        Ok(resp) => {
            // F2.4 tidy-up: only persist the cache when the
            // server gave us something durable. An UNKNOWN /
            // checksum-false reply means the user's local cache
            // is junk; do NOT overwrite the existing-good cache
            // with garbage status. (Symmetrical to the change in
            // cmd_osl_validate_license — see commands.rs.)
            let durable = resp.checksum_ok && resp.status != "UNKNOWN";
            if durable {
                let updated = LicenseCacheInner {
                    license_plaintext: cache.license_plaintext.clone(),
                    last_validated_status: resp.status.clone(),
                    current_period_end: resp.current_period_end,
                    last_validated_at: now,
                    checksum_ok: resp.checksum_ok,
                };
                if let Err(e) = save_license_cache(&cache_path, &updated, sealer.as_ref()) {
                    eprintln!("[OSL] WARN refresh_license_state: save_license_cache failed: {e}");
                }
            }
            let dto = LicenseStateDto {
                state: classify_state(&resp.status),
                raw_status: resp.status,
                current_period_end: resp.current_period_end,
                // Only bump on a durable success — see above.
                last_validated_at: if durable {
                    Some(now)
                } else {
                    Some(cache.last_validated_at)
                },
            };
            stamp(state, &dto);
            dto
        }
        Err(Error::Transport(_)) => {
            // Keyserver UNREACHABLE. Honour grace. The cache is
            // not touched; last_validated_at is NOT bumped (that's
            // what makes the window slide closed once we're past
            // 7 days without a successful round-trip).
            let dto = offline_grace_from_cache(&cache, now);
            stamp(state, &dto);
            dto
        }
        Err(Error::HttpStatus { status, body }) => {
            // Keyserver ANSWERED but rejected. Do NOT extend
            // grace — a reachable rejection is authoritative.
            // Surface the cached classification as-is (online
            // mapping only) so the UI doesn't suddenly flip on a
            // transient 429. F3's ad gate will see Paid for an
            // ACTIVE cache + a 429 — same as before the refresh
            // attempt.
            eprintln!("[OSL] refresh_license_state: keyserver rejected (status {status}): {body}");
            let dto = LicenseStateDto::from_cache(&cache);
            stamp(state, &dto);
            dto
        }
        Err(Error::Json(e)) => {
            // Keyserver answered but body was unparseable. Same
            // treatment as HttpStatus.
            eprintln!("[OSL] refresh_license_state: malformed keyserver response: {e}");
            let dto = LicenseStateDto::from_cache(&cache);
            stamp(state, &dto);
            dto
        }
        Err(e) => {
            // Any other Error variant (Io, Sealer, Base64,
            // BlobVersionMismatch, etc.) — none of these are
            // reachable from validate_license's HTTP-path code,
            // but be defensive. Treat like HttpStatus.
            eprintln!("[OSL] refresh_license_state: unexpected error: {e}");
            let dto = LicenseStateDto::from_cache(&cache);
            stamp(state, &dto);
            dto
        }
    }
}

/// Decide PaidOfflineGrace vs Free for the unreachable-keyserver
/// case, given the cached row + current wallclock. Extracted so
/// tests can exercise the policy without spinning up a fake
/// network failure.
pub fn offline_grace_from_cache(cache: &LicenseCacheInner, now: i64) -> LicenseStateDto {
    let within_grace = cache.last_validated_at + SEVEN_DAYS_SECONDS > now;
    let is_paid_status = matches!(
        cache.last_validated_status.as_str(),
        "ACTIVE" | "CANCELLED" | "GRACE"
    );
    let state = if within_grace && is_paid_status {
        LicenseState::PaidOfflineGrace
    } else {
        LicenseState::Free
    };
    LicenseStateDto {
        state,
        raw_status: cache.last_validated_status.clone(),
        current_period_end: cache.current_period_end,
        // Surface the original last_validated_at so the UI can
        // tell the user "last refreshed 3 hours ago" if needed.
        last_validated_at: Some(cache.last_validated_at),
    }
}

fn stamp(state: &AppState, dto: &LicenseStateDto) {
    *state
        .license_state
        .lock()
        .expect("license_state mutex poisoned") = dto.clone();
}
