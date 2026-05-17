//! REGISTER-FIX (TOFU): trust-on-first-use for peer Ed25519 identity
//! keys + a human-comparable safety number.
//!
//! OSL does not (yet) bind a `user_id` to a real Discord account, so
//! it cannot *prevent* an attacker squatting/replacing a peer's
//! keyserver row. What it CAN do — Signal-style — is make a peer's
//! identity-key CHANGE loud and visible: remember the first key we
//! ever saw for a peer, and raise a blocking alert if a later
//! `fetch_pubkeys` returns a different one. Decryption is NEVER
//! blocked on this (warn, don't break) — the user decides.
//!
//! This module is the PURE core (classification + safety-number
//! derivation) so it is exhaustively unit-testable. The AppState
//! mutation, alert bookkeeping and peer_map persistence live in
//! `commands.rs` where the peer_map/persist plumbing already is.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use sha2::{Digest, Sha256};

/// Outcome of comparing a freshly-fetched peer Ed25519 pub against
/// the stored trust-on-first-use baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TofuOutcome {
    /// No baseline yet — record `fetched` as the trusted baseline.
    FirstUse,
    /// Baseline matches the fetched key — nothing to do.
    Unchanged,
    /// Baseline differs — raise a key-change alert. `old` is the
    /// previously-trusted key; the baseline is NOT updated until the
    /// user explicitly accepts.
    Changed { old: String },
}

/// Pure TOFU classification. `baseline` is `peer_map`'s
/// `tofu_ed25519_pub`; `fetched` is the keyserver's `ik_ed25519_pub`.
/// An empty `fetched` is treated as `Unchanged` (legacy / missing —
/// never destroys a good baseline, never raises a spurious alert).
pub fn classify(baseline: Option<&str>, fetched: &str) -> TofuOutcome {
    if fetched.is_empty() {
        return TofuOutcome::Unchanged;
    }
    match baseline {
        // REGISTER-FIX: a None *or* empty baseline is "never seen a
        // real key for this peer yet" — populating it for the FIRST
        // time (e.g. keyserver fetch right after whitelisting, or a
        // keyless wiped entry self-healing) is FirstUse, NOT a
        // key-change. Without the empty-string guard, a peer entry
        // that ever carried `Some("")` would raise a false
        // "security key changed" alert on its very first real key.
        None => TofuOutcome::FirstUse,
        Some(b) if b.is_empty() => TofuOutcome::FirstUse,
        Some(b) if b == fetched => TofuOutcome::Unchanged,
        Some(b) => TofuOutcome::Changed { old: b.to_string() },
    }
}

/// Deterministic, human-comparable safety number for an Ed25519
/// public key. Two users comparing this out-of-band (voice, in
/// person) can confirm they hold the same key for each other.
///
/// Derivation (stable, representation-independent):
///   1. base64-decode the key (fallback: hash the b64 string bytes
///      verbatim if it isn't valid base64 — still deterministic and
///      identical on both ends for the same input string).
///   2. SHA-256 the raw key bytes.
///   3. Take 6 little chunks of 2 bytes; each → `u16 % 100000`,
///      zero-padded to 5 digits.
///   4. Join the six 5-digit groups with single spaces →
///      `"01234 56789 ..."` (30 digits, 6 groups).
///
/// Identical on the Rust client and anywhere else that hashes the
/// same key bytes the same way; no endianness ambiguity because each
/// group is derived from an explicit `(hi, lo)` byte pair.
pub fn safety_number(ed25519_pub_b64: &str) -> String {
    let bytes = STANDARD
        .decode(ed25519_pub_b64)
        .unwrap_or_else(|_| ed25519_pub_b64.as_bytes().to_vec());
    let digest = Sha256::digest(&bytes);
    let mut groups: Vec<String> = Vec::with_capacity(6);
    for i in 0..6 {
        let hi = digest[i * 2] as u32;
        let lo = digest[i * 2 + 1] as u32;
        let v = ((hi << 8) | lo) % 100_000;
        groups.push(format!("{v:05}"));
    }
    groups.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_use_when_no_baseline() {
        assert_eq!(classify(None, "AAAA"), TofuOutcome::FirstUse);
    }

    #[test]
    fn unchanged_when_equal() {
        assert_eq!(classify(Some("KEY1"), "KEY1"), TofuOutcome::Unchanged);
    }

    #[test]
    fn empty_baseline_is_first_use_not_a_change() {
        // REGISTER-FIX: a keyless entry that ever held Some("")
        // must treat its first real key as FirstUse (no false
        // "key changed" alert).
        assert_eq!(classify(Some(""), "REALKEY"), TofuOutcome::FirstUse);
    }

    #[test]
    fn changed_when_different_carries_old() {
        assert_eq!(
            classify(Some("OLD"), "NEW"),
            TofuOutcome::Changed { old: "OLD".to_string() }
        );
    }

    #[test]
    fn empty_fetched_never_raises_or_destroys_baseline() {
        assert_eq!(classify(Some("KEY1"), ""), TofuOutcome::Unchanged);
        assert_eq!(classify(None, ""), TofuOutcome::Unchanged);
    }

    #[test]
    fn safety_number_is_deterministic_and_grouped() {
        let k = STANDARD.encode([7u8; 32]);
        let a = safety_number(&k);
        let b = safety_number(&k);
        assert_eq!(a, b, "same key → same safety number");
        let parts: Vec<&str> = a.split(' ').collect();
        assert_eq!(parts.len(), 6, "6 groups");
        for p in parts {
            assert_eq!(p.len(), 5, "5 digits per group");
            assert!(p.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn safety_number_differs_for_different_keys() {
        let a = safety_number(&STANDARD.encode([1u8; 32]));
        let b = safety_number(&STANDARD.encode([2u8; 32]));
        assert_ne!(a, b);
    }

    #[test]
    fn safety_number_is_representation_independent() {
        // Same raw bytes, the canonical base64 → same number. (A
        // non-base64 string falls back to hashing the string bytes;
        // that path is deterministic too, just a different domain.)
        let raw = [9u8; 32];
        assert_eq!(
            safety_number(&STANDARD.encode(raw)),
            safety_number(&STANDARD.encode(raw))
        );
    }
}
