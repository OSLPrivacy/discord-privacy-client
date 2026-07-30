//! Trust-on-first-use for a peer's complete long-lived public-key
//! bundle plus a human-comparable safety number.
//!
//! The bundle is one trust object. Treating Ed25519 as the identity
//! while accepting unrelated X25519 / ML-KEM / ratchet keys would let
//! a keyserver redirect encryption without changing the number users
//! compared.
//!
//! This module is the PURE core (classification + safety-number
//! derivation) so it is exhaustively unit-testable. The AppState
//! mutation, alert bookkeeping and peer_map persistence live in
//! `commands.rs` where the peer_map/persist plumbing already is.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SAFETY_NUMBER_DOMAIN: &[u8] = b"OSL-SAFETY-NUMBER-v2";

/// Every long-lived public key whose replacement changes who can
/// decrypt or authenticate OSL traffic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyBundle {
    pub ed25519_pub: String,
    pub x25519_pub: String,
    pub mlkem768_pub: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ratchet_initial_pub: Option<String>,
}

/// Outcome of comparing a freshly fetched bundle against the stored
/// trust-on-first-use baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TofuOutcome {
    /// No baseline yet — record the fetched bundle as trusted.
    FirstUse,
    /// Baseline matches every fetched key — nothing to do.
    Unchanged,
    /// At least one component differs. The baseline is not updated
    /// until the user compares the new bundle number and accepts.
    Changed { old: KeyBundle },
}

/// Pure full-bundle TOFU classification.
pub fn classify(baseline: Option<&KeyBundle>, fetched: &KeyBundle) -> TofuOutcome {
    match baseline {
        None => TofuOutcome::FirstUse,
        Some(b) if b == fetched => TofuOutcome::Unchanged,
        Some(b) => TofuOutcome::Changed { old: b.clone() },
    }
}

/// Input contract for [`safety_number`].
///
/// `String`/`str` implementations are a temporary compile-compatibility
/// boundary for live owners that still pass only an Ed25519 key. They
/// deliberately return an empty, unusable value, so the legacy
/// ceremony fails closed until those callers provide a [`KeyBundle`].
#[doc(hidden)]
pub trait SafetyNumberInput {
    type Output;

    fn derive_safety_number(&self) -> Self::Output;
}

impl SafetyNumberInput for KeyBundle {
    type Output = Result<String, &'static str>;

    fn derive_safety_number(&self) -> Self::Output {
        derive_bundle_safety_number(self)
    }
}

impl SafetyNumberInput for String {
    type Output = String;

    fn derive_safety_number(&self) -> Self::Output {
        String::new()
    }
}

impl SafetyNumberInput for str {
    type Output = String;

    fn derive_safety_number(&self) -> Self::Output {
        String::new()
    }
}

/// Deterministic, human-comparable safety number for a complete
/// public-key bundle. An Ed25519-only legacy caller receives an empty
/// value that existing ceremony comparison code rejects.
pub fn safety_number<T: SafetyNumberInput + ?Sized>(material: &T) -> T::Output {
    material.derive_safety_number()
}

/// Complete-bundle derivation.
///
/// Derivation (stable, representation-independent):
///   1. Hash a domain tag and each base64-decoded component in fixed
///      order, length-prefixed. Absence of the ratchet key is a zero
///      length component.
///   2. SHA-256 that canonical byte sequence.
///   3. Take 6 little chunks of 2 bytes; each → `u16 % 100000`,
///      zero-padded to 5 digits.
///   4. Join the six 5-digit groups with single spaces →
///      `"01234 56789 ..."` (30 digits, 6 groups).
fn derive_bundle_safety_number(bundle: &KeyBundle) -> Result<String, &'static str> {
    let mut hasher = Sha256::new();
    hasher.update(SAFETY_NUMBER_DOMAIN);
    for encoded in [
        Some(bundle.ed25519_pub.as_str()),
        Some(bundle.x25519_pub.as_str()),
        Some(bundle.mlkem768_pub.as_str()),
        bundle.ratchet_initial_pub.as_deref(),
    ] {
        let decoded = match encoded {
            Some(value) => STANDARD
                .decode(value)
                .map_err(|_| "OSL: invalid key bundle for safety number")?,
            None => Vec::new(),
        };
        let len = u32::try_from(decoded.len())
            .map_err(|_| "OSL: invalid key bundle for safety number")?;
        hasher.update(len.to_be_bytes());
        hasher.update(decoded);
    }
    let digest = hasher.finalize();
    let mut groups: Vec<String> = Vec::with_capacity(6);
    for i in 0..6 {
        let hi = digest[i * 2] as u32;
        let lo = digest[i * 2 + 1] as u32;
        let v = ((hi << 8) | lo) % 100_000;
        groups.push(format!("{v:05}"));
    }
    Ok(groups.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_use_when_no_baseline() {
        assert_eq!(
            classify(None, &bundle(1, 2, 3, Some(4))),
            TofuOutcome::FirstUse
        );
    }

    #[test]
    fn unchanged_when_equal() {
        let b = bundle(1, 2, 3, Some(4));
        assert_eq!(classify(Some(&b), &b), TofuOutcome::Unchanged);
    }

    #[test]
    fn changing_any_component_changes_the_trust_object() {
        let old = bundle(1, 2, 3, Some(4));
        for new in [
            bundle(9, 2, 3, Some(4)),
            bundle(1, 9, 3, Some(4)),
            bundle(1, 2, 9, Some(4)),
            bundle(1, 2, 3, Some(9)),
            bundle(1, 2, 3, None),
        ] {
            assert_eq!(
                classify(Some(&old), &new),
                TofuOutcome::Changed { old: old.clone() }
            );
        }
    }

    fn bundle(ed: u8, x: u8, mlkem: u8, ratchet: Option<u8>) -> KeyBundle {
        KeyBundle {
            ed25519_pub: STANDARD.encode([ed; 32]),
            x25519_pub: STANDARD.encode([x; 32]),
            mlkem768_pub: STANDARD.encode([mlkem; 1184]),
            ratchet_initial_pub: ratchet.map(|b| STANDARD.encode([b; 32])),
        }
    }

    #[test]
    fn changed_carries_the_complete_old_bundle() {
        let old = bundle(1, 2, 3, Some(4));
        assert_eq!(
            classify(Some(&old), &bundle(1, 8, 3, Some(4))),
            TofuOutcome::Changed { old }
        );
    }

    #[test]
    fn safety_number_is_deterministic_and_grouped() {
        let k = bundle(7, 8, 9, Some(10));
        let a = safety_number(&k).unwrap();
        let b = safety_number(&k).unwrap();
        assert_eq!(a, b, "same key → same safety number");
        let parts: Vec<&str> = a.split(' ').collect();
        assert_eq!(parts.len(), 6, "6 groups");
        for p in parts {
            assert_eq!(p.len(), 5, "5 digits per group");
            assert!(p.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn safety_number_changes_with_every_bundle_component() {
        let base = bundle(1, 2, 3, Some(4));
        let number = safety_number(&base).unwrap();
        for changed in [
            bundle(9, 2, 3, Some(4)),
            bundle(1, 9, 3, Some(4)),
            bundle(1, 2, 9, Some(4)),
            bundle(1, 2, 3, Some(9)),
            bundle(1, 2, 3, None),
        ] {
            assert_ne!(number, safety_number(&changed).unwrap());
        }
    }

    #[test]
    fn invalid_base64_refuses_instead_of_hashing_an_ambiguous_fallback() {
        let mut invalid = bundle(1, 2, 3, Some(4));
        invalid.x25519_pub = "not-base64".to_owned();
        assert_eq!(
            safety_number(&invalid),
            Err("OSL: invalid key bundle for safety number")
        );
    }

    #[test]
    fn legacy_ed25519_only_callers_receive_no_usable_number() {
        assert!(safety_number(&"ed25519-only".to_string()).is_empty());
        assert!(safety_number("ed25519-only").is_empty());
    }
}
