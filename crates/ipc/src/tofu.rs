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
const SAFETY_NUMBER_DOMAIN_V3: &[u8] = b"OSL-SAFETY-NUMBER-v3";

/// Six groups of five decimal digits, the shape both devices display and the
/// shape `safety_number_matches` compares.
const SAFETY_NUMBER_GROUPS: usize = 6;
/// Bytes consumed per displayed group. Five bytes (40 bits) reduced mod 100000
/// is Signal's `DisplayableFingerprint` chunk encoding: 40 bits into a 5-digit
/// decimal group leaves a bias below 2^-23, so all 100000 values are reachable
/// and near-uniform. Two bytes — what v2 used — can only ever produce 0..=65535,
/// so a third of the group's range and every leading digit above 6 were
/// unreachable, costing ~3.6 bits per group for no benefit.
const SAFETY_NUMBER_BYTES_PER_GROUP: usize = 5;

const INVALID_BUNDLE: &str = "OSL: invalid key bundle for safety number";
const SELF_PAIR_REFUSAL: &str = "OSL: a safety number compares two identities, not one";

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
    absorb_canonical_bundle(&mut hasher, bundle)?;
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

/// Canonical byte encoding of one bundle, absorbed into `hasher`.
///
/// Each component is base64-decoded, then written as a big-endian `u32` length
/// followed by its raw bytes, in the fixed order
/// `ed25519 ‖ x25519 ‖ mlkem768 ‖ ratchet`. An absent ratchet key is a
/// zero-length component, so it is distinguishable from a present empty one and
/// the encoding cannot be made ambiguous by moving bytes between fields.
///
/// Decoding first is what makes the number representation-independent: two
/// devices that base64 the same key with different padding still agree.
fn absorb_canonical_bundle(hasher: &mut Sha256, bundle: &KeyBundle) -> Result<(), &'static str> {
    for encoded in [
        Some(bundle.ed25519_pub.as_str()),
        Some(bundle.x25519_pub.as_str()),
        Some(bundle.mlkem768_pub.as_str()),
        bundle.ratchet_initial_pub.as_deref(),
    ] {
        let decoded = match encoded {
            Some(value) => STANDARD.decode(value).map_err(|_| INVALID_BUNDLE)?,
            None => Vec::new(),
        };
        let len = u32::try_from(decoded.len()).map_err(|_| INVALID_BUNDLE)?;
        hasher.update(len.to_be_bytes());
        hasher.update(decoded);
    }
    Ok(())
}

/// Confirm a bundle can be canonicalised at all, without producing a number.
///
/// Callers that only need "are these keys well-formed?" must use this rather
/// than deriving a number and discarding it — a one-sided number is exactly the
/// value this module exists to stop producing.
pub fn validate_key_bundle(bundle: &KeyBundle) -> Result<(), &'static str> {
    let mut hasher = Sha256::new();
    absorb_canonical_bundle(&mut hasher, bundle)
}

/// The **two-party** safety number, v3 — the only number a human may be asked
/// to compare.
///
/// v2 (`derive_bundle_safety_number`) hashes ONE bundle, so the two devices in a
/// conversation necessarily display different digits and the only value either
/// operator can type is the one already on their own screen. That ceremony
/// authenticates nothing; it records that a button was pressed.
///
/// v3 hashes BOTH bundles in an order neither side chooses, so both devices
/// display identical digits and comparing them out of band is a real check:
///
/// ```text
/// low, high := the two bundles ordered by raw 32-byte ed25519_pub, lexicographic
/// digest    := SHA-256( "OSL-SAFETY-NUMBER-v3" ‖ canon(low) ‖ canon(high) )
/// groups    := for i in 0..6 { be_u40(digest[5i..5i+5]) % 100000, zero-padded to 5 }
/// number    := groups.join(" ")                       // 30 digits, 6 groups
/// ```
///
/// The construction follows Signal's numeric fingerprint: a domain-separated
/// hash over both parties' key material, ordered so the result is symmetric,
/// rendered as six 5-digit groups by Signal's `DisplayableFingerprint` chunk
/// encoding (five bytes reduced mod 100000). It differs from Signal in two
/// deliberate ways, both recorded in `03-CONTRACTS/identity.md` §3:
/// OSL combines the two parties into one 30-digit number rather than
/// concatenating two 30-digit halves into 60, and OSL does not apply Signal's
/// 5200 iterations of preimage hardening. Both are owner decisions (OQ-1).
///
/// Ordering is over the *decoded* Ed25519 bytes, not the base64 text, so the
/// two devices agree even if they encode the same key differently.
///
/// Deriving a number against yourself is an error, never a number: it would be
/// a value the app hands to itself and back.
pub fn safety_number_pair(a: &KeyBundle, b: &KeyBundle) -> Result<String, &'static str> {
    let a_ik = STANDARD
        .decode(&a.ed25519_pub)
        .map_err(|_| INVALID_BUNDLE)?;
    let b_ik = STANDARD
        .decode(&b.ed25519_pub)
        .map_err(|_| INVALID_BUNDLE)?;
    if a_ik.is_empty() || b_ik.is_empty() {
        return Err(INVALID_BUNDLE);
    }
    if a_ik == b_ik {
        return Err(SELF_PAIR_REFUSAL);
    }
    let (low, high) = if a_ik < b_ik { (a, b) } else { (b, a) };

    let mut hasher = Sha256::new();
    hasher.update(SAFETY_NUMBER_DOMAIN_V3);
    absorb_canonical_bundle(&mut hasher, low)?;
    absorb_canonical_bundle(&mut hasher, high)?;
    let digest = hasher.finalize();

    let mut groups: Vec<String> = Vec::with_capacity(SAFETY_NUMBER_GROUPS);
    for group in 0..SAFETY_NUMBER_GROUPS {
        let offset = group * SAFETY_NUMBER_BYTES_PER_GROUP;
        let mut wide = [0u8; 8];
        wide[8 - SAFETY_NUMBER_BYTES_PER_GROUP..]
            .copy_from_slice(&digest[offset..offset + SAFETY_NUMBER_BYTES_PER_GROUP]);
        let value = u64::from_be_bytes(wide) % 100_000;
        groups.push(format!("{value:05}"));
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

    /// The property the whole ceremony rests on: whichever device asks, the
    /// digits are the same, so comparing them out of band means something.
    #[test]
    fn pair_number_is_the_same_from_either_side() {
        let alice = bundle(1, 2, 3, Some(4));
        let bob = bundle(5, 6, 7, Some(8));
        let from_alice = safety_number_pair(&alice, &bob).unwrap();
        let from_bob = safety_number_pair(&bob, &alice).unwrap();
        assert_eq!(from_alice, from_bob);
        // And the ordering is not an accident of which key happens to sort low:
        // the reversed pair of identity keys must agree too.
        let low_first = bundle(0, 2, 3, Some(4));
        let high_first = bundle(255, 6, 7, Some(8));
        assert_eq!(
            safety_number_pair(&low_first, &high_first).unwrap(),
            safety_number_pair(&high_first, &low_first).unwrap()
        );
    }

    #[test]
    fn pair_number_is_thirty_digits_in_six_groups() {
        let number = safety_number_pair(&bundle(1, 2, 3, Some(4)), &bundle(5, 6, 7, None)).unwrap();
        let groups: Vec<&str> = number.split(' ').collect();
        assert_eq!(groups.len(), 6);
        assert!(groups.iter().all(|group| group.len() == 5));
        assert!(groups
            .iter()
            .all(|group| group.bytes().all(|byte| byte.is_ascii_digit())));
        assert_eq!(number.chars().filter(char::is_ascii_digit).count(), 30);
    }

    /// A one-sided number would collide for every pair that shares one member.
    /// Distinct counterparties must produce distinct numbers on the same device.
    #[test]
    fn different_counterparties_produce_different_numbers() {
        let me = bundle(1, 2, 3, Some(4));
        let with_bob = safety_number_pair(&me, &bundle(5, 6, 7, Some(8))).unwrap();
        let with_carol = safety_number_pair(&me, &bundle(9, 10, 11, Some(12))).unwrap();
        assert_ne!(with_bob, with_carol);
        // ...and the same counterparty seen by two different devices differs.
        let bob = bundle(5, 6, 7, Some(8));
        assert_ne!(
            safety_number_pair(&me, &bob).unwrap(),
            safety_number_pair(&bundle(20, 21, 22, None), &bob).unwrap()
        );
    }

    /// Whole-bundle, not just the identity key: swapping a transport key must
    /// move the number, or a keyserver could redirect encryption without
    /// changing what the two humans compared.
    #[test]
    fn changing_any_component_on_either_side_changes_the_pair_number() {
        let alice = bundle(1, 2, 3, Some(4));
        let bob = bundle(5, 6, 7, Some(8));
        let baseline = safety_number_pair(&alice, &bob).unwrap();
        for mutated in [
            bundle(5, 99, 7, Some(8)),
            bundle(5, 6, 99, Some(8)),
            bundle(5, 6, 7, Some(99)),
            bundle(5, 6, 7, None),
        ] {
            assert_ne!(
                safety_number_pair(&alice, &mutated).unwrap(),
                baseline,
                "a changed peer component must change the number"
            );
        }
        for mutated in [
            bundle(1, 99, 3, Some(4)),
            bundle(1, 2, 99, Some(4)),
            bundle(1, 2, 3, Some(99)),
            bundle(1, 2, 3, None),
        ] {
            assert_ne!(
                safety_number_pair(&mutated, &bob).unwrap(),
                baseline,
                "a changed local component must change the number"
            );
        }
    }

    #[test]
    fn verifying_against_yourself_is_an_error_not_a_number() {
        let me = bundle(1, 2, 3, Some(4));
        assert_eq!(safety_number_pair(&me, &me), Err(SELF_PAIR_REFUSAL));
        // Same identity key, different transport keys, is still one identity.
        let rotated = bundle(1, 77, 78, None);
        assert_eq!(safety_number_pair(&me, &rotated), Err(SELF_PAIR_REFUSAL));
    }

    #[test]
    fn malformed_bundles_refuse_rather_than_produce_digits() {
        let good = bundle(1, 2, 3, Some(4));
        let mut bad = bundle(5, 6, 7, Some(8));
        bad.ed25519_pub = "not base64!!".to_owned();
        assert!(safety_number_pair(&good, &bad).is_err());
        assert!(safety_number_pair(&bad, &good).is_err());
        let mut bad_transport = bundle(5, 6, 7, Some(8));
        bad_transport.mlkem768_pub = "not base64!!".to_owned();
        assert!(safety_number_pair(&good, &bad_transport).is_err());
        assert!(validate_key_bundle(&bad_transport).is_err());
        assert!(validate_key_bundle(&good).is_ok());
    }

    /// v2 and v3 must never collide by value: the domain tag change is what
    /// makes every cached v2 number invalid rather than silently accepted.
    #[test]
    fn the_v3_number_is_not_a_v2_number() {
        let alice = bundle(1, 2, 3, Some(4));
        let bob = bundle(5, 6, 7, Some(8));
        let pair = safety_number_pair(&alice, &bob).unwrap();
        assert_ne!(pair, derive_bundle_safety_number(&alice).unwrap());
        assert_ne!(pair, derive_bundle_safety_number(&bob).unwrap());
    }

    /// Five bytes per group, not two: with two bytes no group could exceed
    /// 65535, so a third of every group's range was unreachable. Sample enough
    /// pairs to observe a group above the old ceiling.
    #[test]
    fn groups_reach_the_whole_five_digit_range() {
        let me = bundle(1, 2, 3, Some(4));
        let mut seen_above_v2_ceiling = false;
        for peer in 10u8..120 {
            let number = safety_number_pair(&me, &bundle(peer, 2, 3, Some(4))).unwrap();
            if number
                .split(' ')
                .any(|group| group.parse::<u32>().unwrap() > 65_535)
            {
                seen_above_v2_ceiling = true;
                break;
            }
        }
        assert!(
            seen_above_v2_ceiling,
            "no group ever exceeded 65535 — the encoder is still consuming two bytes per group"
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
