//! `BundleVerifyPolicy` — the verification contract for a peer's full
//! OSL identity bundle: the Ed25519 identity-signing key, the X25519
//! DH key, the ML-KEM-768 identity encapsulation key, and the signed
//! capability bitmap (the same "capability bundle" carried today on
//! [`crate::client::PubkeysResponse::rn_capabilities`]).
//!
//! ## Threat this closes
//!
//! The prototype key server is a carrier for the bundle, never its
//! integrity authority (see `client.rs`'s module docs). Two attacks
//! are relevant here:
//!
//! 1. **Field tampering** — the keyserver (or a MITM in front of it)
//!    edits one field of an otherwise-real bundle. `client.rs`'s
//!    [`crate::client::verify_peer_bundle`] already closes this: it
//!    reconstructs the signed message from the served fields and
//!    checks it against the served signature and the served key.
//! 2. **Whole-bundle substitution** — the keyserver swaps in a
//!    *different*, internally self-consistent bundle: every field
//!    signed correctly by *some* Ed25519 key, just not the one the
//!    caller actually trusts for this peer. A same-record check
//!    cannot catch this — the record is valid, just not *theirs*.
//!    `client.rs`'s own docs call this out as a residual trust
//!    boundary punted to "the identity/TOFU layer".
//!
//! `BundleVerifyPolicy` is that layer's contract. It never trusts a
//! signer key found inside the fetched bundle — there is no such
//! field on [`IdentityBundle`] to substitute. The caller must supply
//! the Ed25519 public key it already has pinned for this peer (from
//! local TOFU storage, or the identity's own key on first contact),
//! and the bundle is authenticated against THAT key and no other. A
//! keyserver-supplied substitute bundle — however well-formed and
//! however validly self-signed — fails verification, because it was
//! never signed by the pinned key.
//!
//! It additionally enforces **revision monotonicity**: a bundle is
//! accepted only if its `revision` is strictly greater than the
//! highest revision this caller has already accepted for the peer.
//! This closes a keyserver replay/rollback of a stale bundle (e.g.
//! one from before a legitimate key rotation, or one the owner has
//! since superseded).
//!
//! ## Scope
//!
//! This module defines the policy/contract only:
//! - [`IdentityBundle`] — the authenticated unit (all four fields
//!   travel together under one signature; there is no way to
//!   construct one that authenticates only a subset).
//! - [`BundleField`] — names which fields the policy requires
//!   authenticated, matching the A2 requirement exactly.
//! - [`BundleVerifyPolicy`] — the verifier.
//!
//! It performs no network I/O and is not wired into `client.rs`, the
//! TOFU store, or any caller. It also does not reuse
//! `client::PubkeysResponse` directly (a wire DTO with server-shaped
//! optionality) — a later wiring unit builds an [`IdentityBundle`]
//! from either a locally-held [`crate::identity::Identity`] or a
//! fetched bundle response.

use crypto::ed25519;

/// Fields of a full identity bundle that MUST be covered by the one
/// authenticating signature before any of them is used. This is the
/// A2 requirement spelled out as data: Ed25519 + X25519 + ML-KEM-768
/// identity keys, plus the capability bundle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BundleField {
    /// `IK_Ed25519` — the identity-signing key itself.
    Ed25519IdentityKey,
    /// `IK_X25519` — the PQXDH/DH identity key.
    X25519IdentityKey,
    /// `IK_MLKEM768` — the ML-KEM-768 identity encapsulation key.
    MlKem768IdentityKey,
    /// The signed protocol-capability bitmap (`rn_capabilities` on
    /// `client::PubkeysResponse` today).
    CapabilityBundle,
}

impl BundleField {
    /// Every field this policy requires authenticated. Fixed, not
    /// configurable: a partially-authenticated bundle is not a
    /// smaller version of this contract, it's a different one.
    pub const ALL: [BundleField; 4] = [
        BundleField::Ed25519IdentityKey,
        BundleField::X25519IdentityKey,
        BundleField::MlKem768IdentityKey,
        BundleField::CapabilityBundle,
    ];
}

/// The full identity bundle [`BundleVerifyPolicy`] authenticates as
/// one unit.
///
/// Key fields are raw fixed-size bytes (matching how
/// `identity::Identity` and `client::PubkeysResponse` already carry
/// them) rather than the typed `crypto::*` wrappers, so a later
/// wiring unit can build one of these from either side without
/// re-deriving anything here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityBundle {
    pub ed25519_identity_pub: [u8; ed25519::PUBLIC_KEY_SIZE],
    pub x25519_identity_pub: [u8; 32],
    pub mlkem768_identity_pub: [u8; crypto::ml_kem_768::ENCAPSULATION_KEY_SIZE],
    /// The capability bundle (protocol-capability bitmap). Whatever
    /// range/meaning validation a caller applies to the bitmap itself
    /// (e.g. `client::RN_CAP_MAX`) is out of scope here — this policy
    /// only authenticates that the value is the one the pinned key
    /// actually signed.
    pub capability_bundle: u32,
    /// Monotonic revision counter. Must strictly increase across
    /// bundles this policy accepts for the same peer.
    pub revision: u64,
    /// Ed25519 signature over [`IdentityBundle::signed_bytes`],
    /// produced by the bundle owner's OWN identity-signing secret —
    /// never the keyserver's.
    pub signature: [u8; ed25519::SIGNATURE_SIZE],
}

/// Domain-separation + version tag for the bytes [`IdentityBundle`]
/// signs over. Distinct from `client::REG_DOMAIN` / `ROT_DOMAIN`:
/// this is a different signed contract (full-bundle + revision, not
/// a registration/rotation event).
pub const IDENTITY_BUNDLE_DOMAIN: &[u8] = b"OSL-IDENTITY-BUNDLE-v1";

impl IdentityBundle {
    /// Canonical bytes the signature is computed over. Covers every
    /// field in [`BundleField::ALL`] plus the revision counter — a
    /// signature can't be produced (or verified) over a subset.
    pub fn signed_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(
            IDENTITY_BUNDLE_DOMAIN.len()
                + ed25519::PUBLIC_KEY_SIZE
                + 32
                + crypto::ml_kem_768::ENCAPSULATION_KEY_SIZE
                + 4
                + 8,
        );
        buf.extend_from_slice(IDENTITY_BUNDLE_DOMAIN);
        buf.extend_from_slice(&self.ed25519_identity_pub);
        buf.extend_from_slice(&self.x25519_identity_pub);
        buf.extend_from_slice(&self.mlkem768_identity_pub);
        buf.extend_from_slice(&self.capability_bundle.to_be_bytes());
        buf.extend_from_slice(&self.revision.to_be_bytes());
        buf
    }
}

/// Why [`BundleVerifyPolicy::verify`] refused a bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BundleVerifyError {
    /// The signature does not verify against the pinned identity key.
    /// This is the outcome for BOTH plain tampering AND whole-bundle
    /// substitution by the keyserver: a substitute bundle is, by
    /// definition, not signed by the key the caller actually pinned
    /// for this peer.
    #[error(
        "identity bundle signature does not verify against the pinned identity key — refusing \
         (tampered field, or a keyserver-supplied substitute bundle)"
    )]
    SignatureInvalid,
    /// The bundle's revision did not strictly increase over the last
    /// one this policy accepted for the peer.
    #[error(
        "identity bundle revision {got} is not greater than the last accepted revision \
         {last_known} — refusing stale/regressed bundle"
    )]
    RevisionNotMonotonic { got: u64, last_known: u64 },
}

/// The verification contract for a full OSL identity bundle.
///
/// See the module docs for the threat model. In short:
/// [`BundleVerifyPolicy::verify`] authenticates `bundle` against a
/// caller-supplied `pinned_signer` key — NEVER a key read from
/// `bundle` itself, which is exactly what makes keyserver
/// substitution unrepresentable here — and enforces that
/// `bundle.revision` strictly increases over `last_known_revision`.
#[derive(Clone, Copy, Debug, Default)]
pub struct BundleVerifyPolicy;

impl BundleVerifyPolicy {
    pub fn new() -> Self {
        BundleVerifyPolicy
    }

    /// Fields this policy requires authenticated under one signature.
    /// Fixed at [`BundleField::ALL`]; exposed so callers/tests can
    /// assert the contract's shape without hardcoding it twice.
    pub fn required_fields(&self) -> [BundleField; 4] {
        BundleField::ALL
    }

    /// Verify `bundle` against `pinned_signer` — the Ed25519 identity
    /// key the caller already trusts for this peer, sourced from
    /// local TOFU state or a prior successful call, and `last_known_revision`
    /// — the highest revision this caller has already accepted for
    /// the peer (`None` on first contact).
    ///
    /// On success, returns `bundle.revision` so the caller can
    /// persist it as the new watermark for the next call.
    pub fn verify(
        &self,
        bundle: &IdentityBundle,
        pinned_signer: &ed25519::PublicKey,
        last_known_revision: Option<u64>,
    ) -> Result<u64, BundleVerifyError> {
        if let Some(last) = last_known_revision {
            if bundle.revision <= last {
                return Err(BundleVerifyError::RevisionNotMonotonic {
                    got: bundle.revision,
                    last_known: last,
                });
            }
        }

        let signature = ed25519::Signature::from_bytes(bundle.signature);
        let message = bundle.signed_bytes();
        match ed25519::verify(pinned_signer, &message, &signature) {
            Ok(true) => Ok(bundle.revision),
            _ => Err(BundleVerifyError::SignatureInvalid),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed_bundle(
        signer_secret: &ed25519::SecretKey,
        field_source_pub: &ed25519::PublicKey,
        revision: u64,
    ) -> IdentityBundle {
        // `field_source_pub` supplies the Ed25519 field of the bundle
        // itself (distinct from `signer_secret`, which produces the
        // authenticating signature) so tests can build a bundle whose
        // OWN fields belong to one identity while it's (mis)signed by
        // another's secret — modeling a keyserver-substituted record.
        let mut bundle = IdentityBundle {
            ed25519_identity_pub: *field_source_pub.as_bytes(),
            x25519_identity_pub: [0x11; 32],
            mlkem768_identity_pub: [0x22; crypto::ml_kem_768::ENCAPSULATION_KEY_SIZE],
            capability_bundle: 0b0000_0001,
            revision,
            signature: [0u8; ed25519::SIGNATURE_SIZE],
        };
        let sig = ed25519::sign(signer_secret, &bundle.signed_bytes());
        bundle.signature = *sig.as_bytes();
        bundle
    }

    #[test]
    fn fully_authenticated_bundle_passes() {
        let (owner_secret, owner_pub) = ed25519::generate_keypair();
        let bundle = signed_bundle(&owner_secret, &owner_pub, 1);

        let policy = BundleVerifyPolicy::new();
        let result = policy.verify(&bundle, &owner_pub, None);

        assert_eq!(result, Ok(1));
    }

    #[test]
    fn keyserver_substituted_bundle_is_refused() {
        // The caller has PINNED the real owner's Ed25519 key from
        // prior TOFU. The keyserver instead serves a bundle for a
        // different identity ("attacker") — internally self-consistent
        // (attacker signed their own real fields with their own real
        // secret) but never touched by the owner's secret at all.
        let (_owner_secret, owner_pub) = ed25519::generate_keypair();
        let (attacker_secret, attacker_pub) = ed25519::generate_keypair();
        let substitute_bundle = signed_bundle(&attacker_secret, &attacker_pub, 1);

        let policy = BundleVerifyPolicy::new();
        let result = policy.verify(&substitute_bundle, &owner_pub, None);

        assert_eq!(result, Err(BundleVerifyError::SignatureInvalid));
    }

    #[test]
    fn stale_or_regressed_revision_is_refused() {
        let (owner_secret, owner_pub) = ed25519::generate_keypair();
        let policy = BundleVerifyPolicy::new();

        // Accept revision 5 first.
        let rev5 = signed_bundle(&owner_secret, &owner_pub, 5);
        assert_eq!(policy.verify(&rev5, &owner_pub, None), Ok(5));

        // A keyserver replaying the SAME revision again is refused...
        let replay = signed_bundle(&owner_secret, &owner_pub, 5);
        assert_eq!(
            policy.verify(&replay, &owner_pub, Some(5)),
            Err(BundleVerifyError::RevisionNotMonotonic {
                got: 5,
                last_known: 5
            })
        );

        // ...and a rollback to an older revision is refused too, even
        // though it carries a perfectly valid owner signature.
        let rollback = signed_bundle(&owner_secret, &owner_pub, 3);
        assert_eq!(
            policy.verify(&rollback, &owner_pub, Some(5)),
            Err(BundleVerifyError::RevisionNotMonotonic {
                got: 3,
                last_known: 5
            })
        );

        // A genuinely newer revision from the real owner still passes.
        let rev6 = signed_bundle(&owner_secret, &owner_pub, 6);
        assert_eq!(policy.verify(&rev6, &owner_pub, Some(5)), Ok(6));
    }

    #[test]
    fn tampered_field_after_signing_is_refused() {
        // A field-level edit (the OTHER attack this policy also
        // catches, alongside whole-bundle substitution): the bitmap
        // is flipped after signing, so signed_bytes() no longer
        // matches what was signed.
        let (owner_secret, owner_pub) = ed25519::generate_keypair();
        let mut bundle = signed_bundle(&owner_secret, &owner_pub, 1);
        bundle.capability_bundle ^= 0b0000_0010;

        let policy = BundleVerifyPolicy::new();
        assert_eq!(
            policy.verify(&bundle, &owner_pub, None),
            Err(BundleVerifyError::SignatureInvalid)
        );
    }

    #[test]
    fn required_fields_cover_the_a2_contract() {
        let policy = BundleVerifyPolicy::new();
        let fields = policy.required_fields();
        assert!(fields.contains(&BundleField::Ed25519IdentityKey));
        assert!(fields.contains(&BundleField::X25519IdentityKey));
        assert!(fields.contains(&BundleField::MlKem768IdentityKey));
        assert!(fields.contains(&BundleField::CapabilityBundle));
    }
}
