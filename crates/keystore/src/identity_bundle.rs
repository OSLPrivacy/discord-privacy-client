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

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
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

/// SPK + OPK material carried on a keyserver
/// `client::PrekeyBundleResponse`, merged in by
/// [`IdentityBundle::merge_prekey_bundle_response`] only after that
/// response's identity-key fields are confirmed to byte-match the
/// pinned bundle and the SPK signature verifies under the SAME pinned
/// Ed25519 key — never a key read off the response.
///
/// Distinct from the four [`BundleField::ALL`] fields: prekey material
/// is NOT part of [`IdentityBundle::signed_bytes`] and is never
/// covered by the whole-bundle owner signature. It carries its own,
/// independently-checked authentication (the SPK's own Ed25519
/// signature); the OPK and pool count are unsigned keyserver-served
/// state, same trust level they have on the wire.
///
/// Every field here is public wire material (public keys, a public
/// signature, a rotation timestamp, a pool count) — nothing secret,
/// so deriving `Debug` is safe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrekeyMaterial {
    pub spk_x25519_pub: [u8; 32],
    pub spk_signature: [u8; ed25519::SIGNATURE_SIZE],
    pub spk_rotated_at: String,
    /// `(id, x25519_pub)` of the one-time prekey the response carried,
    /// or `None` when the server's OPK pool was exhausted.
    pub opk: Option<(u32, [u8; 32])>,
    pub remaining_opk_count: u32,
}

/// Result of a successful [`IdentityBundle::merge_prekey_bundle_response`]:
/// the pinned identity bundle, byte-identical to the input — never
/// rebuilt from response data — paired with the prekey material the
/// response added.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergedIdentityBundle {
    pub identity: IdentityBundle,
    pub prekey: PrekeyMaterial,
}

/// Why [`IdentityBundle::merge_prekey_bundle_response`] refused a
/// `PrekeyBundleResponse`. In every case the bundle being merged into
/// is returned untouched by the caller — this type carries no partial
/// result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BundleMergeError {
    /// The bundle being merged into is already at or behind a
    /// revision this caller has previously accepted. Refused so a
    /// merge can never make a superseded bundle look current again.
    #[error(
        "cannot merge prekey material into identity bundle revision {bundle_revision} — caller \
         has already accepted revision {last_known} or newer"
    )]
    StaleBundleRevision { bundle_revision: u64, last_known: u64 },
    /// A response identity-key field is not valid base64, or does not
    /// decode to the expected fixed length.
    #[error("prekey-bundle response {field:?} is malformed (not valid fixed-length base64)")]
    MalformedField { field: BundleField },
    /// A response identity-key field decoded fine but does not
    /// byte-match the pinned bundle — a keyserver identity-field
    /// substitution attempt.
    #[error(
        "prekey-bundle response {field:?} does not match the pinned identity bundle — refusing \
         (keyserver identity-field substitution)"
    )]
    IdentityKeyMismatch { field: BundleField },
    /// `spk_pub` / `spk_signature` are not valid fixed-length base64.
    #[error("prekey-bundle response SPK fields are malformed (not valid fixed-length base64)")]
    MalformedSpk,
    /// The SPK signature does not verify under the pinned Ed25519
    /// identity key — the SPK was never signed by the identity this
    /// caller trusts, regardless of who signed the identity fields.
    #[error("prekey-bundle response SPK signature does not verify under the pinned identity key")]
    SpkSignatureInvalid,
    /// The response's `opk.pub_b64` is not valid 32-byte base64.
    #[error("prekey-bundle response OPK public key is malformed (not valid 32-byte base64)")]
    MalformedOpk,
}

impl IdentityBundle {
    /// Merge a keyserver `PrekeyBundleResponse` into this
    /// already-[`BundleVerifyPolicy::verify`]'d bundle.
    ///
    /// This is the A2 full-bundle merge step: the prekey-bundle
    /// endpoint response carries the peer's identity-key fields again
    /// (unsigned, as served — see `client.rs`'s module docs), plus
    /// fresh SPK/OPK material. A keyserver or MITM in front of it can
    /// serve a `PrekeyBundleResponse` for a *different* identity just
    /// as easily as it can serve a substitute `PubkeysResponse`, so
    /// this merge applies the same pinned-key discipline as
    /// [`BundleVerifyPolicy::verify`]: every identity-key field on
    /// `response` must byte-match the corresponding field already on
    /// `self` — which the caller obtained via `verify()` against its
    /// own pinned key, never a key read off `response` — or the whole
    /// merge is refused. On any refusal `self` is returned untouched;
    /// there is no partial merge because the returned
    /// [`MergedIdentityBundle::identity`] is only ever constructed, at
    /// the very end, as a clone of `self` — the identity-key checks
    /// and the SPK signature check all happen first and short-circuit
    /// on the first failure.
    ///
    /// `last_known_revision` is the same watermark
    /// [`BundleVerifyPolicy::verify`] takes: the highest revision this
    /// caller has already accepted for the peer. It is enforced here
    /// too (`self.revision` must be strictly greater), not because
    /// this call re-verifies the whole-bundle signature, but because
    /// attaching fresh prekey material to an identity bundle the
    /// caller knows to be stale would make that stale bundle look
    /// current again.
    ///
    /// On success, [`MergedIdentityBundle::identity`] is exactly
    /// `self` — the whole-bundle owner signature and every identity
    /// field are carried through unmodified — paired with the
    /// [`PrekeyMaterial`] extracted from `response`.
    pub fn merge_prekey_bundle_response(
        &self,
        response: &crate::client::PrekeyBundleResponse,
        last_known_revision: Option<u64>,
    ) -> Result<MergedIdentityBundle, BundleMergeError> {
        if let Some(last) = last_known_revision {
            if self.revision <= last {
                return Err(BundleMergeError::StaleBundleRevision {
                    bundle_revision: self.revision,
                    last_known: last,
                });
            }
        }

        let Ok(resp_ed25519_bytes) = STANDARD.decode(&response.ik_ed25519_pub) else {
            return Err(BundleMergeError::MalformedField {
                field: BundleField::Ed25519IdentityKey,
            });
        };
        let Ok(resp_ed25519_arr) = <[u8; ed25519::PUBLIC_KEY_SIZE]>::try_from(
            resp_ed25519_bytes.as_slice(),
        ) else {
            return Err(BundleMergeError::MalformedField {
                field: BundleField::Ed25519IdentityKey,
            });
        };
        if resp_ed25519_arr != self.ed25519_identity_pub {
            return Err(BundleMergeError::IdentityKeyMismatch {
                field: BundleField::Ed25519IdentityKey,
            });
        }

        let Ok(resp_x25519_bytes) = STANDARD.decode(&response.ik_x25519_pub) else {
            return Err(BundleMergeError::MalformedField {
                field: BundleField::X25519IdentityKey,
            });
        };
        let Ok(resp_x25519_arr) = <[u8; 32]>::try_from(resp_x25519_bytes.as_slice()) else {
            return Err(BundleMergeError::MalformedField {
                field: BundleField::X25519IdentityKey,
            });
        };
        if resp_x25519_arr != self.x25519_identity_pub {
            return Err(BundleMergeError::IdentityKeyMismatch {
                field: BundleField::X25519IdentityKey,
            });
        }

        let Ok(resp_mlkem768_bytes) = STANDARD.decode(&response.ik_mlkem768_pub) else {
            return Err(BundleMergeError::MalformedField {
                field: BundleField::MlKem768IdentityKey,
            });
        };
        let Ok(resp_mlkem768_arr) = <[u8; crypto::ml_kem_768::ENCAPSULATION_KEY_SIZE]>::try_from(
            resp_mlkem768_bytes.as_slice(),
        ) else {
            return Err(BundleMergeError::MalformedField {
                field: BundleField::MlKem768IdentityKey,
            });
        };
        if resp_mlkem768_arr != self.mlkem768_identity_pub {
            return Err(BundleMergeError::IdentityKeyMismatch {
                field: BundleField::MlKem768IdentityKey,
            });
        }

        // Every identity field on `response` matches the pinned
        // bundle. Now authenticate the prekey material itself: the
        // SPK's own Ed25519 signature, checked under the SAME pinned
        // key just confirmed above (`self.ed25519_identity_pub`) —
        // matching how `prekeys::make_spk` signs (raw SPK pub bytes,
        // no domain tag) and how the keyserver itself verifies it.
        let Ok(spk_pub_bytes) = STANDARD.decode(&response.spk_pub) else {
            return Err(BundleMergeError::MalformedSpk);
        };
        let Ok(spk_pub_arr) = <[u8; 32]>::try_from(spk_pub_bytes.as_slice()) else {
            return Err(BundleMergeError::MalformedSpk);
        };
        let Ok(spk_sig_bytes) = STANDARD.decode(&response.spk_signature) else {
            return Err(BundleMergeError::MalformedSpk);
        };
        let Ok(spk_sig_arr) =
            <[u8; ed25519::SIGNATURE_SIZE]>::try_from(spk_sig_bytes.as_slice())
        else {
            return Err(BundleMergeError::MalformedSpk);
        };
        let pinned_signer = ed25519::PublicKey::from_bytes(self.ed25519_identity_pub);
        let spk_signature = ed25519::Signature::from_bytes(spk_sig_arr);
        match ed25519::verify(&pinned_signer, &spk_pub_arr, &spk_signature) {
            Ok(true) => {}
            _ => return Err(BundleMergeError::SpkSignatureInvalid),
        }

        let opk = match &response.opk {
            Some(o) => {
                let Ok(opk_pub_bytes) = STANDARD.decode(&o.pub_b64) else {
                    return Err(BundleMergeError::MalformedOpk);
                };
                let Ok(opk_pub_arr) = <[u8; 32]>::try_from(opk_pub_bytes.as_slice()) else {
                    return Err(BundleMergeError::MalformedOpk);
                };
                Some((o.id, opk_pub_arr))
            }
            None => None,
        };

        Ok(MergedIdentityBundle {
            identity: self.clone(),
            prekey: PrekeyMaterial {
                spk_x25519_pub: spk_pub_arr,
                spk_signature: spk_sig_arr,
                spk_rotated_at: response.spk_rotated_at.clone(),
                opk,
                remaining_opk_count: response.remaining_opk_count,
            },
        })
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

    // ---- merge_prekey_bundle_response ----

    /// Build a `PrekeyBundleResponse` whose identity-key fields match
    /// `bundle` exactly and whose SPK is signed by `spk_signer_secret`
    /// (the real owner's secret in the happy path; a different
    /// identity's secret when modeling a forged SPK).
    fn matching_prekey_response(
        bundle: &IdentityBundle,
        spk_signer_secret: &ed25519::SecretKey,
        spk_pub: [u8; 32],
        opk: Option<(u32, [u8; 32])>,
        remaining_opk_count: u32,
    ) -> crate::client::PrekeyBundleResponse {
        let spk_sig = ed25519::sign(spk_signer_secret, &spk_pub);
        crate::client::PrekeyBundleResponse {
            user_id: "peer".to_string(),
            ik_x25519_pub: STANDARD.encode(bundle.x25519_identity_pub),
            ik_ed25519_pub: STANDARD.encode(bundle.ed25519_identity_pub),
            ik_mlkem768_pub: STANDARD.encode(bundle.mlkem768_identity_pub),
            spk_pub: STANDARD.encode(spk_pub),
            spk_signature: STANDARD.encode(spk_sig.as_bytes()),
            spk_rotated_at: "2026-07-29T00:00:00.000Z".to_string(),
            opk: opk.map(|(id, pub_key)| crate::client::PrekeyBundleOpk {
                id,
                pub_b64: STANDARD.encode(pub_key),
            }),
            remaining_opk_count,
            ik_ratchet_initial_pub: None,
        }
    }

    #[test]
    fn valid_prekey_response_merges() {
        let (owner_secret, owner_pub) = ed25519::generate_keypair();
        let bundle = signed_bundle(&owner_secret, &owner_pub, 1);
        let spk_pub = [0x33; 32];
        let response =
            matching_prekey_response(&bundle, &owner_secret, spk_pub, Some((7, [0x44; 32])), 42);

        let merged = bundle
            .merge_prekey_bundle_response(&response, None)
            .expect("valid response must merge");

        // Identity fields carry through byte-identical — never rebuilt.
        assert_eq!(merged.identity, bundle);
        assert_eq!(merged.prekey.spk_x25519_pub, spk_pub);
        assert_eq!(merged.prekey.opk, Some((7, [0x44; 32])));
        assert_eq!(merged.prekey.remaining_opk_count, 42);
    }

    #[test]
    fn substituted_identity_key_in_response_is_refused() {
        // Caller has pinned the real owner's bundle. The keyserver's
        // prekey-bundle response instead carries an ATTACKER's Ed25519
        // identity key in the ik_ed25519_pub field (the response has
        // no signature of its own over these fields at all — see
        // `client.rs`'s `PrekeyBundleResponse` docs — so this is
        // exactly the substitution the merge must catch by comparing
        // against the already-pinned bundle).
        let (owner_secret, owner_pub) = ed25519::generate_keypair();
        let bundle = signed_bundle(&owner_secret, &owner_pub, 1);
        let (_attacker_secret, attacker_pub) = ed25519::generate_keypair();

        let spk_pub = [0x33; 32];
        let mut response = matching_prekey_response(&bundle, &owner_secret, spk_pub, None, 10);
        response.ik_ed25519_pub = STANDARD.encode(attacker_pub.as_bytes());

        let result = bundle.merge_prekey_bundle_response(&response, None);

        assert_eq!(
            result,
            Err(BundleMergeError::IdentityKeyMismatch {
                field: BundleField::Ed25519IdentityKey
            })
        );
    }

    #[test]
    fn regressed_revision_is_refused() {
        let (owner_secret, owner_pub) = ed25519::generate_keypair();
        // The caller last accepted revision 5 for this peer, but the
        // bundle in hand is only revision 3 — stale relative to what
        // this caller already knows. Merging fresh prekey material
        // onto it would make the stale bundle look current again.
        let stale_bundle = signed_bundle(&owner_secret, &owner_pub, 3);
        let spk_pub = [0x33; 32];
        let response =
            matching_prekey_response(&stale_bundle, &owner_secret, spk_pub, None, 10);

        let result = stale_bundle.merge_prekey_bundle_response(&response, Some(5));

        assert_eq!(
            result,
            Err(BundleMergeError::StaleBundleRevision {
                bundle_revision: 3,
                last_known: 5,
            })
        );
    }

    #[test]
    fn refused_merge_leaves_bundle_untouched() {
        let (owner_secret, owner_pub) = ed25519::generate_keypair();
        let bundle = signed_bundle(&owner_secret, &owner_pub, 1);
        let bundle_before = bundle.clone();

        let (_attacker_secret, attacker_pub) = ed25519::generate_keypair();
        let spk_pub = [0x33; 32];
        let mut response = matching_prekey_response(&bundle, &owner_secret, spk_pub, None, 10);
        response.ik_x25519_pub = STANDARD.encode(attacker_pub.as_bytes());

        let result = bundle.merge_prekey_bundle_response(&response, None);

        assert!(result.is_err());
        // `merge_prekey_bundle_response` takes `&self`; there is no
        // path through it that can mutate `bundle`. Assert the
        // observable proof anyway: every field is exactly what it was
        // before the refused call.
        assert_eq!(bundle, bundle_before);
    }

    #[test]
    fn forged_spk_signature_is_refused() {
        // Every identity-key field matches the pinned bundle, but the
        // SPK itself was signed by a different secret than the pinned
        // Ed25519 key — e.g. a keyserver that swaps in its own SPK
        // under a real identity's other fields. The merge must catch
        // this the same way `verify()` catches whole-bundle
        // substitution: authenticate only against the pinned key.
        let (owner_secret, owner_pub) = ed25519::generate_keypair();
        let bundle = signed_bundle(&owner_secret, &owner_pub, 1);
        let (forger_secret, _forger_pub) = ed25519::generate_keypair();

        let spk_pub = [0x33; 32];
        let response = matching_prekey_response(&bundle, &forger_secret, spk_pub, None, 10);

        let result = bundle.merge_prekey_bundle_response(&response, None);

        assert_eq!(result, Err(BundleMergeError::SpkSignatureInvalid));
    }
}
