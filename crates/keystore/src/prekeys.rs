//! Client-side prekey state management.
//!
//! Spec: `docs/design/prekey-infrastructure.md` + the design doc's
//! "Signed prekey" / "One-time prekey pool" subsections.
//!
//! Holds:
//! - The current SPK keypair (X25519) + its Ed25519 signature + the
//!   ISO-8601 rotated-at timestamp.
//! - The previous SPK keypair, retained for one rotation period so
//!   in-flight messages from clients with stale bundles still
//!   decrypt.
//! - The OPK pool — single-use X25519 prekeys, each with a
//!   monotonically-increasing `id` that the server uses to
//!   atomically pop one per fetch.
//!
//! Persistence: the secret halves are sealed under the active
//! [`crate::sealer::Sealer`] and stored in
//! `<dir>/prekeys.json`, alongside `identity.json`.
//!
//! ## Pool sizing
//!
//! Per design doc, **fixed targets** in v1: pool target 100,
//! replenish trigger 25. [`PrekeyConfig::default`] uses these.
//! Adaptive sizing is deferred to v2.
//!
//! ## SPK rotation
//!
//! Per design, weekly cadence — caller calls
//! [`PrekeyState::should_rotate_spk`] with the current time, and on
//! `true` calls [`PrekeyState::rotate_spk`]. The previous SPK is
//! kept on `previous_spk` for one rotation period.
//!
//! ## Canonical replenish encoding
//!
//! [`canonical_replenish_bytes`] produces the exact byte string the
//! server-side `canonicalReplenishBytes` (in `keyserver/src/canonical.js`)
//! expects. This is what the Ed25519 batch signature is computed
//! over.

use crate::identity::Identity;
use crate::sealer::Sealer;
use crate::{Error, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::{ed25519, x25519};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const REPLENISH_DOMAIN: &[u8] = b"discord-privacy-client/prekey-replenish/v1";

/// Per the design doc.
pub const SPK_ROTATION_INTERVAL_SECONDS: u64 = 7 * 24 * 60 * 60;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrekeyConfig {
    /// Target pool size after a replenish.
    pub opk_pool_target: u32,
    /// Threshold below which the client decides to replenish.
    pub opk_replenish_threshold: u32,
    /// SPK rotation interval in seconds. Default 7 days.
    pub spk_rotation_seconds: u64,
}

impl Default for PrekeyConfig {
    fn default() -> Self {
        PrekeyConfig {
            opk_pool_target: 100,
            opk_replenish_threshold: 25,
            spk_rotation_seconds: SPK_ROTATION_INTERVAL_SECONDS,
        }
    }
}

/// One OPK keypair the client retains. The server only ever sees
/// `public`; `secret` is consumed by the receive-side PQXDH handshake
/// when the OPK is popped.
#[derive(Clone, Serialize, Deserialize)]
pub struct OpkEntry {
    pub id: u32,
    #[serde(with = "byte_array_b64::array_32")]
    pub secret: [u8; 32],
    #[serde(with = "byte_array_b64::array_32")]
    pub public: [u8; 32],
}

/// Signed prekey + its rotation timestamp. The signature is
/// Ed25519 over `public` by the user's `IK_Ed25519` identity-signing
/// key.
#[derive(Clone, Serialize, Deserialize)]
pub struct SpkEntry {
    #[serde(with = "byte_array_b64::array_32")]
    pub secret: [u8; 32],
    #[serde(with = "byte_array_b64::array_32")]
    pub public: [u8; 32],
    #[serde(with = "byte_array_b64::array_64")]
    pub signature: [u8; 64],
    pub rotated_at_unix_seconds: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PrekeyState {
    pub config: PrekeyConfig,
    pub current_spk: SpkEntry,
    pub previous_spk: Option<SpkEntry>,
    pub opk_pool: Vec<OpkEntry>,
    pub next_opk_id: u32,
}

impl PrekeyState {
    /// Generate a fresh prekey state — initial SPK + a target-sized
    /// OPK pool.
    pub fn new(identity: &Identity, config: PrekeyConfig, now_unix_seconds: u64) -> Self {
        let spk = make_spk(identity, now_unix_seconds);
        let mut state = PrekeyState {
            config,
            current_spk: spk,
            previous_spk: None,
            opk_pool: Vec::new(),
            next_opk_id: 0,
        };
        let target = state.config.opk_pool_target;
        state.add_opk_batch(target);
        state
    }

    /// Returns true iff `(now - current_spk.rotated_at) >= rotation_interval`.
    pub fn should_rotate_spk(&self, now_unix_seconds: u64) -> bool {
        now_unix_seconds.saturating_sub(self.current_spk.rotated_at_unix_seconds)
            >= self.config.spk_rotation_seconds
    }

    /// Generate a fresh SPK and move the existing one into
    /// `previous_spk`. Returns the new `SpkEntry`.
    pub fn rotate_spk(&mut self, identity: &Identity, now_unix_seconds: u64) -> &SpkEntry {
        let new_spk = make_spk(identity, now_unix_seconds);
        let old = std::mem::replace(&mut self.current_spk, new_spk);
        self.previous_spk = Some(old);
        &self.current_spk
    }

    /// Returns true iff the server-reported pool count is at or
    /// below the configured replenish threshold.
    pub fn should_replenish(&self, server_remaining_opk_count: u32) -> bool {
        server_remaining_opk_count <= self.config.opk_replenish_threshold
    }

    /// Generate `count` fresh OPKs and append to the pool. Returns
    /// the slice of the pool that was newly added (the caller ships
    /// the public halves to the server).
    pub fn add_opk_batch(&mut self, count: u32) -> &[OpkEntry] {
        let start = self.opk_pool.len();
        for _ in 0..count {
            let (sec, pub_key) = x25519::generate_keypair();
            self.opk_pool.push(OpkEntry {
                id: self.next_opk_id,
                secret: *sec.as_bytes(),
                public: *pub_key.as_bytes(),
            });
            self.next_opk_id = self.next_opk_id.saturating_add(1);
        }
        &self.opk_pool[start..]
    }

    /// Compute the count short of the configured target.
    pub fn replenish_count_to_target(&self, server_remaining: u32) -> u32 {
        self.config.opk_pool_target.saturating_sub(server_remaining)
    }

    /// Remove the OPK with `id` from the local pool (called after a
    /// PQXDH initiation has consumed it on the receive side).
    /// Idempotent: removing a nonexistent id is a no-op.
    pub fn consume_opk(&mut self, id: u32) -> bool {
        let idx = self.opk_pool.iter().position(|o| o.id == id);
        if let Some(i) = idx {
            self.opk_pool.swap_remove(i);
            true
        } else {
            false
        }
    }
}

fn make_spk(identity: &Identity, now_unix_seconds: u64) -> SpkEntry {
    let (sec, pub_key) = x25519::generate_keypair();
    let sig = ed25519::sign(&identity.ed25519_secret, pub_key.as_bytes());
    SpkEntry {
        secret: *sec.as_bytes(),
        public: *pub_key.as_bytes(),
        signature: *sig.as_bytes(),
        rotated_at_unix_seconds: now_unix_seconds,
    }
}

// ---- canonical replenish encoding ----

/// Format a Unix-seconds timestamp into the ISO-8601-Z form the
/// server stores. Matches Node's `Date(t).toISOString()` for any t
/// representable in milliseconds.
pub fn iso_8601_from_unix_seconds(t: u64) -> String {
    // Avoid pulling chrono in. Compute Y/M/D/H/M/S manually.
    let secs = t as i64;
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    let mut days = secs / 86400;
    let mut year = 1970i64;
    loop {
        let leap = is_leap_year(year);
        let yd = if leap { 366 } else { 365 };
        if days >= yd {
            days -= yd;
            year += 1;
        } else {
            break;
        }
    }
    let month_lengths = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 0usize;
    for (i, ml) in month_lengths.iter().enumerate() {
        if days >= *ml {
            days -= ml;
        } else {
            month = i;
            break;
        }
    }
    let day = days + 1;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.000Z",
        year,
        month + 1,
        day,
        h,
        m,
        s
    )
}

fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// One OPK in the canonical replenish payload (id + base64 of the
/// public key).
#[derive(Clone, Debug)]
pub struct ReplenishOpk {
    pub id: u32,
    pub pub_b64: String,
}

/// SPK section of the canonical replenish payload. `pub_b64` and
/// `signature_b64` are the base64-string forms (NOT decoded) so the
/// server reconstructs the exact same byte string.
#[derive(Clone, Debug)]
pub struct ReplenishSpk {
    pub pub_b64: String,
    pub signature_b64: String,
    pub rotated_at: String,
}

/// Compute the canonical bytes the Ed25519 batch signature covers.
/// Mirrors `canonicalReplenishBytes` in `keyserver/src/canonical.js`
/// exactly. Both sides MUST produce identical bytes for the
/// signature to verify.
pub fn canonical_replenish_bytes(
    user_id: &str,
    timestamp_ms: i64,
    request_id: &str,
    spk: Option<&ReplenishSpk>,
    opks: &[ReplenishOpk],
) -> Vec<u8> {
    let mut buf = Vec::new();
    write_lp_str(&mut buf, std::str::from_utf8(REPLENISH_DOMAIN).unwrap());
    write_lp_str(&mut buf, user_id);
    write_lp_str(&mut buf, &timestamp_ms.to_string());
    write_lp_str(&mut buf, request_id);
    buf.push(if spk.is_some() { 1 } else { 0 });
    if let Some(s) = spk {
        write_lp_str(&mut buf, &s.pub_b64);
        write_lp_str(&mut buf, &s.signature_b64);
        write_lp_str(&mut buf, &s.rotated_at);
    }
    let count = u32::try_from(opks.len()).expect("OPK batch exceeds u32 count");
    buf.extend_from_slice(&count.to_be_bytes());
    for o in opks {
        buf.extend_from_slice(&o.id.to_be_bytes());
        write_lp_str(&mut buf, &o.pub_b64);
    }
    buf
}

fn write_lp_str(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let len = u32::try_from(bytes.len()).expect("canonical field exceeds u32 length");
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(bytes);
}

/// Sign the canonical replenish bytes with the given identity's
/// Ed25519 key. The base64-encoded signature is what the server's
/// `batch_signature_b64` field expects.
pub fn sign_replenish_batch(
    identity: &Identity,
    user_id: &str,
    timestamp_ms: i64,
    request_id: &str,
    spk: Option<&ReplenishSpk>,
    opks: &[ReplenishOpk],
) -> ed25519::Signature {
    let bytes = canonical_replenish_bytes(user_id, timestamp_ms, request_id, spk, opks);
    ed25519::sign(&identity.ed25519_secret, &bytes)
}

// ---- persistence ----

#[derive(Serialize, Deserialize)]
struct PrekeyStateOnDisk {
    version: u32,
    method: String,
    sealed_b64: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    insecure_banner: Option<String>,
}

pub fn save_prekey_state(path: &Path, state: &PrekeyState, sealer: &dyn Sealer) -> Result<()> {
    let inner = serde_json::to_vec(state)?;
    let sealed = sealer.seal(&inner)?;
    let on_disk = PrekeyStateOnDisk {
        version: 1,
        method: sealer.method_label().to_string(),
        sealed_b64: STANDARD.encode(&sealed),
        insecure_banner: if sealer.requires_insecure_banner() {
            Some(
                "INSECURE prototype storage — plain JSON, no TPM. \
                 v1 stable replaces with TPM-sealed blob; do NOT use \
                 with real users."
                    .to_string(),
            )
        } else {
            None
        },
    };
    let json = serde_json::to_vec_pretty(&on_disk)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, &json)?;
    Ok(())
}

pub fn load_prekey_state(path: &Path, sealer: &dyn Sealer) -> Result<PrekeyState> {
    let bytes = std::fs::read(path)?;
    let on_disk: PrekeyStateOnDisk = serde_json::from_slice(&bytes)?;
    if on_disk.version != 1 {
        return Err(Error::BlobVersionMismatch {
            got: on_disk.version,
            expected: 1,
        });
    }
    if on_disk.method != sealer.method_label() {
        return Err(Error::BlobMethodMismatch {
            got: on_disk.method,
            expected: sealer.method_label().to_string(),
        });
    }
    let sealed = STANDARD.decode(&on_disk.sealed_b64)?;
    let inner = sealer.unseal(&sealed)?;
    let state: PrekeyState = serde_json::from_slice(&inner)?;
    Ok(state)
}

// ---- byte-array base64 serde helpers ----

mod byte_array_b64 {
    pub mod array_32 {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        use serde::de::Error as DeError;
        use serde::{Deserialize, Deserializer, Serializer};

        pub fn serialize<S: Serializer>(v: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
            s.serialize_str(&STANDARD.encode(v))
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
            let s: String = String::deserialize(d)?;
            let v = STANDARD.decode(&s).map_err(D::Error::custom)?;
            if v.len() != 32 {
                return Err(D::Error::custom(format!(
                    "expected 32 bytes, got {}",
                    v.len()
                )));
            }
            let mut out = [0u8; 32];
            out.copy_from_slice(&v);
            Ok(out)
        }
    }

    pub mod array_64 {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        use serde::de::Error as DeError;
        use serde::{Deserialize, Deserializer, Serializer};

        pub fn serialize<S: Serializer>(v: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
            s.serialize_str(&STANDARD.encode(v))
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
            let s: String = String::deserialize(d)?;
            let v = STANDARD.decode(&s).map_err(D::Error::custom)?;
            if v.len() != 64 {
                return Err(D::Error::custom(format!(
                    "expected 64 bytes, got {}",
                    v.len()
                )));
            }
            let mut out = [0u8; 64];
            out.copy_from_slice(&v);
            Ok(out)
        }
    }
}
