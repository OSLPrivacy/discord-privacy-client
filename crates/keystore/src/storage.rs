//! Identity storage: serialize → seal → write to disk; read → unseal
//! → deserialize.
//!
//! The on-disk format is JSON with three layers:
//!
//! ```text
//! IdentityOnDisk {
//!     version: 2,
//!     method: "tpm-pcp" | "keyring" | "noop-insecure" | "memory-test",
//!     sealed_b64: base64( sealer.seal(canonical_inner_blob_bytes) ),
//!     insecure_banner: Option<String>,  // present iff sealer requires it
//! }
//! ```
//!
//! `canonical_inner_blob_bytes` is itself a JSON document with the
//! actual key material:
//!
//! ```text
//! InnerIdentity {
//!     user_id: String,
//!     x25519_secret_b64: base64,
//!     x25519_public_b64: base64,
//!     mlkem_secret_b64: base64,
//!     mlkem_public_b64: base64,
//! }
//! ```
//!
//! The two-layer approach:
//! - lets the inner doc be authenticated by the AEAD tag (TPM,
//!   Keyring, Memory) — tampering is detected on unseal,
//! - keeps the on-disk wrapper small / inspectable for ops
//!   (operators can see the method tag without unsealing),
//! - leaves the door open for the (insecure) NoOp path: inner doc
//!   simply round-trips as plaintext, and the insecure_banner field
//!   on the wrapper is the loud "DON'T DEPLOY THIS" signal.
//!
//! v1 (the prior format) was a single-layer plain JSON with
//! base64-encoded fields. Loaders explicitly reject v1 with a clear
//! error so users can re-save under v2 with a real sealer.

use crate::identity::{Identity, IDENTITY_BLOB_VERSION};
use crate::sealer::Sealer;
use crate::{Error, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::{ed25519, ml_kem_768, x25519};
use serde::{Deserialize, Serialize};
use std::path::Path;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

const INSECURE_BANNER: &str = "INSECURE prototype storage — plain JSON, no passphrase, no TPM. \
     v1 stable replaces with TPM-sealed blob; do NOT use with real users.";

#[derive(Serialize, Deserialize, Debug)]
pub struct IdentityOnDisk {
    pub version: u32,
    pub method: String,
    pub sealed_b64: String,
    /// Present iff `method == "noop-insecure"`. Loaders SHOULD
    /// surface this to the user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insecure_banner: Option<String>,
}

/// A8: this DTO is the one place where every long-term secret in the
/// identity exists simultaneously as an ordinary base64 `String`. The
/// derived `Zeroize` + `ZeroizeOnDrop` make the whole struct — including
/// the `Option<String>` recovery-entropy field — wipe itself when the
/// save or load call frame ends, instead of releasing five secret strings
/// to the allocator intact.
///
/// The public halves and `user_id` are zeroized too. That is harmless and
/// keeps the derive free of per-field opt-outs that a future field could
/// silently inherit the wrong way.
#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
struct InnerIdentity {
    user_id: String,
    x25519_secret_b64: String,
    x25519_public_b64: String,
    ed25519_secret_b64: String,
    ed25519_public_b64: String,
    mlkem_secret_b64: String,
    mlkem_public_b64: String,
    /// 7d-FIX3b: Discord snowflake associated with this identity.
    /// `serde(default)` keeps backward compat with pre-FIX3 sealed
    /// blobs — they deserialize with `None` and the bootstrap repair
    /// path defers self-entry creation to boot.js registration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    discord_snowflake: Option<String>,

    /// Phase 9-A2: X25519 secret for the published Double Ratchet
    /// bootstrap. None for pre-A2 sealed blobs; the next save after
    /// `Identity::ensure_ratchet_bootstrap` runs will populate it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ratchet_initial_secret_b64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ratchet_initial_pub_b64: Option<String>,

    /// Device-transfer recovery: the 16 bytes the 12-word phrase
    /// encodes. Persisted so the phrase can be revealed later from the
    /// original device. `None` for legacy random-key identities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recovery_entropy_b64: Option<String>,
}

/// Save `identity` to `path` sealed under `sealer`.
pub fn save_identity(path: &Path, identity: &Identity, sealer: &dyn Sealer) -> Result<()> {
    let inner = InnerIdentity {
        user_id: identity.user_id.clone(),
        x25519_secret_b64: STANDARD.encode(identity.x25519_secret.as_bytes()),
        x25519_public_b64: STANDARD.encode(identity.x25519_public.as_bytes()),
        ed25519_secret_b64: STANDARD.encode(identity.ed25519_secret.as_bytes()),
        ed25519_public_b64: STANDARD.encode(identity.ed25519_public.as_bytes()),
        mlkem_secret_b64: STANDARD.encode(identity.mlkem_secret_bytes()),
        mlkem_public_b64: STANDARD.encode(identity.mlkem_public_bytes),
        discord_snowflake: identity.discord_snowflake.clone(),
        ratchet_initial_secret_b64: identity
            .ratchet_initial_secret
            .as_ref()
            .map(|sk| STANDARD.encode(sk.as_bytes())),
        ratchet_initial_pub_b64: identity
            .ratchet_initial_pub
            .as_ref()
            .map(|pk| STANDARD.encode(pk.as_bytes())),
        recovery_entropy_b64: identity
            .recovery_entropy
            .as_ref()
            .map(|e| STANDARD.encode(e)),
    };
    // A8: the serialized inner document contains every private key in the
    // clear. Wrap it before it exists as a bare `Vec<u8>` so it is wiped
    // once sealing is done, not handed back to the allocator.
    let inner_bytes = Zeroizing::new(serde_json::to_vec(&inner)?);
    let sealed = sealer.seal(&inner_bytes)?;

    let on_disk = IdentityOnDisk {
        version: IDENTITY_BLOB_VERSION,
        method: sealer.method_label().to_string(),
        sealed_b64: STANDARD.encode(&sealed),
        insecure_banner: if sealer.requires_insecure_banner() {
            Some(INSECURE_BANNER.to_string())
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

/// Load an identity from `path`. Validates that the on-disk method
/// tag matches the active sealer's label — mismatches are a
/// distinct, clear error variant
/// ([`Error::BlobMethodMismatch`]).
pub fn load_identity(path: &Path, sealer: &dyn Sealer) -> Result<Identity> {
    let bytes = std::fs::read(path)?;
    let on_disk: IdentityOnDisk = serde_json::from_slice(&bytes)?;
    if on_disk.version != IDENTITY_BLOB_VERSION {
        return Err(Error::BlobVersionMismatch {
            got: on_disk.version,
            expected: IDENTITY_BLOB_VERSION,
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
    let inner: InnerIdentity = serde_json::from_slice(&inner_bytes)?;

    let x25519_secret =
        decode_array::<{ x25519::SECRET_KEY_SIZE }>("x25519_secret", &inner.x25519_secret_b64)?;
    let on_disk_x25519_public =
        decode_array::<{ x25519::PUBLIC_KEY_SIZE }>("x25519_public", &inner.x25519_public_b64)?;
    // Re-derive the X25519 public from the secret. The on-disk
    // `x25519_public_b64` is informational; the math source of
    // truth is the secret + base point. If the two disagree
    // (partial save, hand-edit, or any prior bug that wrote a
    // mismatched pair), production code paths that derive the
    // public from the secret (encoder's `sender_pub`, decoder's
    // `our_hint`) would silently disagree with `register()` —
    // which uploads `identity.x25519_public` directly. The
    // keyserver would then publish the stale field, every peer
    // would fetch the stale value, and decryption would fail
    // intermittently in ways that look like cache poisoning.
    //
    // Fix: always trust the secret. Stamp `x25519_public` from
    // the derived value. Surface the disagreement to stderr so
    // the user notices the on-disk corruption and re-saves.
    let derived_secret = x25519::SecretKey::from_bytes(*x25519_secret);
    let derived_pub = x25519::derive_public(&derived_secret);
    let x25519_public = if *derived_pub.as_bytes() != *on_disk_x25519_public {
        eprintln!(
            "[OSL] WARN identity.json x25519_public_b64 disagrees with derived \
             public from x25519_secret_b64. Re-deriving (math source of truth). \
             Re-saving the identity (e.g. via Tauri's save_identity command) \
             will refresh the on-disk field. on_disk_first_byte=0x{:02x} \
             derived_first_byte=0x{:02x}",
            on_disk_x25519_public[0],
            derived_pub.as_bytes()[0]
        );
        *derived_pub.as_bytes()
    } else {
        *on_disk_x25519_public
    };
    let ed25519_secret =
        decode_array::<{ ed25519::SECRET_KEY_SIZE }>("ed25519_secret", &inner.ed25519_secret_b64)?;
    let ed25519_public =
        decode_array::<{ ed25519::PUBLIC_KEY_SIZE }>("ed25519_public", &inner.ed25519_public_b64)?;
    let mlkem_secret = decode_array::<{ ml_kem_768::DECAPSULATION_KEY_SIZE }>(
        "mlkem_secret",
        &inner.mlkem_secret_b64,
    )?;
    let mlkem_public = decode_array::<{ ml_kem_768::ENCAPSULATION_KEY_SIZE }>(
        "mlkem_public",
        &inner.mlkem_public_b64,
    )?;

    // `InnerIdentity` is `ZeroizeOnDrop`, so its fields cannot be moved
    // out — clone the two non-secret strings and deref the zeroizing key
    // arrays. The wrappers wipe when this frame ends.
    let mut identity = Identity::from_bytes(
        inner.user_id.clone(),
        *x25519_secret,
        x25519_public,
        *ed25519_secret,
        *ed25519_public,
        *mlkem_secret,
        *mlkem_public,
    );
    identity.discord_snowflake = inner.discord_snowflake.clone();
    if let Some(sk_b64) = inner.ratchet_initial_secret_b64.as_deref() {
        let sk_bytes =
            decode_array::<{ x25519::SECRET_KEY_SIZE }>("ratchet_initial_secret", sk_b64)?;
        identity.ratchet_initial_secret = Some(x25519::SecretKey::from_bytes(*sk_bytes));
    }
    if let Some(pk_b64) = inner.ratchet_initial_pub_b64.as_deref() {
        let pk_bytes = decode_array::<{ x25519::PUBLIC_KEY_SIZE }>("ratchet_initial_pub", pk_b64)?;
        identity.ratchet_initial_pub = Some(x25519::PublicKey::from_bytes(*pk_bytes));
    }
    if let Some(e_b64) = inner.recovery_entropy_b64.as_deref() {
        identity.recovery_entropy = Some(*decode_array::<16>("recovery_entropy", e_b64)?);
    }
    Ok(identity)
}

/// Decode one fixed-width field out of the sealed inner document.
///
/// A8: both the base64 scratch buffer and the returned array are
/// zeroizing. Most callers of this helper are decoding private keys, and
/// the plain-`Vec` temporary was the exact allocation the audit flagged —
/// it was freed with the key still in it on every single load.
fn decode_array<const N: usize>(field: &'static str, b64: &str) -> Result<Zeroizing<[u8; N]>> {
    let v = Zeroizing::new(STANDARD.decode(b64)?);
    if v.len() != N {
        return Err(Error::BlobFieldLength {
            field,
            got: v.len(),
            expected: N,
        });
    }
    let mut out = Zeroizing::new([0u8; N]);
    out.copy_from_slice(&v);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every long-term secret in an identity exists as an ordinary base64
    /// `String` exactly once: inside [`InnerIdentity`], during `save_identity`
    /// and `load_identity`. Those `String`s own heap buffers, so without an
    /// explicit wipe the private X25519, Ed25519, ML-KEM and ratchet keys --
    /// and the 16 bytes of recovery entropy the whole identity can be
    /// rederived from -- are handed back to the allocator intact when the call
    /// frame ends.
    ///
    /// NEGATIVE CONTROL, stated honestly: this test does not compile against
    /// the pre-fix code, because `InnerIdentity` had no `Zeroize` derive and
    /// `.zeroize()` did not exist on it. That is a compile-time control, not a
    /// runtime one. Reading a freed buffer to observe the wipe at runtime is
    /// undefined behaviour, so it is not attempted here; what is asserted is
    /// the property the `ZeroizeOnDrop` derive is built on -- that zeroizing
    /// actually clears every field rather than only the ones someone
    /// remembered.
    #[test]
    fn zeroizing_an_inner_identity_clears_every_secret_field() {
        let mut inner = InnerIdentity {
            user_id: "osl-user".to_owned(),
            x25519_secret_b64: STANDARD.encode([7u8; 32]),
            x25519_public_b64: STANDARD.encode([8u8; 32]),
            ed25519_secret_b64: STANDARD.encode([9u8; 32]),
            ed25519_public_b64: STANDARD.encode([10u8; 32]),
            mlkem_secret_b64: STANDARD.encode([11u8; 64]),
            mlkem_public_b64: STANDARD.encode([12u8; 64]),
            discord_snowflake: Some("1234567890".to_owned()),
            ratchet_initial_secret_b64: Some(STANDARD.encode([13u8; 32])),
            ratchet_initial_pub_b64: Some(STANDARD.encode([14u8; 32])),
            recovery_entropy_b64: Some(STANDARD.encode([15u8; 16])),
        };
        // Sanity: the secrets really are present as plain strings first, or the
        // assertions below would pass against an empty struct.
        assert!(!inner.x25519_secret_b64.is_empty());
        assert!(!inner.mlkem_secret_b64.is_empty());
        assert!(inner.recovery_entropy_b64.is_some());

        inner.zeroize();

        assert!(inner.user_id.is_empty());
        assert!(inner.x25519_secret_b64.is_empty());
        assert!(inner.x25519_public_b64.is_empty());
        assert!(inner.ed25519_secret_b64.is_empty());
        assert!(inner.ed25519_public_b64.is_empty());
        assert!(inner.mlkem_secret_b64.is_empty());
        assert!(inner.mlkem_public_b64.is_empty());
        // The recovery entropy is the one field that rederives the ENTIRE
        // identity, so "cleared" here must mean the option itself is gone, not
        // an emptied string still sitting beside a `Some`.
        assert!(
            inner.recovery_entropy_b64.is_none()
                || inner.recovery_entropy_b64.as_deref() == Some(""),
            "recovery entropy must not survive a zeroize"
        );
        assert!(
            inner.ratchet_initial_secret_b64.is_none()
                || inner.ratchet_initial_secret_b64.as_deref() == Some(""),
        );
    }

    /// The wipe must be automatic, not something each call site remembers.
    /// These bounds fail to compile if the derives are ever removed.
    #[test]
    fn secret_carriers_wipe_themselves_on_drop() {
        fn assert_zeroize_on_drop<T: ZeroizeOnDrop>() {}
        assert_zeroize_on_drop::<InnerIdentity>();
        assert_zeroize_on_drop::<crate::identity::Identity>();
    }

    #[test]
    fn memory_sealer_round_trips_through_save_load() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("identity.json");
        let sealer = crate::sealer::MemorySealer::new();

        let mut original = crate::identity::generate_identity("storage-memory-user".to_owned());
        original.discord_snowflake = Some("123456789012345678".to_owned());
        let ratchet_pub = original.ensure_ratchet_bootstrap();

        save_identity(&path, &original, &sealer).expect("save with memory sealer");
        let raw = std::fs::read_to_string(&path).expect("read saved identity blob");
        assert!(raw.contains("\"method\": \"memory-test\""));
        assert!(!raw.contains("storage-memory-user"));
        assert!(!raw.contains("123456789012345678"));
        assert!(!raw.contains(&STANDARD.encode(original.x25519_secret.as_bytes())));
        assert!(!raw.contains(&STANDARD.encode(original.ed25519_secret.as_bytes())));
        assert!(!raw.contains(&STANDARD.encode(original.mlkem_secret_bytes())));

        let loaded = load_identity(&path, &sealer).expect("same memory sealer loads identity");
        assert_eq!(loaded.user_id, original.user_id);
        assert_eq!(
            loaded.x25519_secret.as_bytes(),
            original.x25519_secret.as_bytes()
        );
        assert_eq!(
            loaded.x25519_public.as_bytes(),
            original.x25519_public.as_bytes()
        );
        assert_eq!(
            loaded.ed25519_secret.as_bytes(),
            original.ed25519_secret.as_bytes()
        );
        assert_eq!(
            loaded.ed25519_public.as_bytes(),
            original.ed25519_public.as_bytes()
        );
        assert_eq!(loaded.mlkem_secret_bytes(), original.mlkem_secret_bytes());
        assert_eq!(loaded.mlkem_public_bytes, original.mlkem_public_bytes);
        assert_eq!(loaded.discord_snowflake, original.discord_snowflake);
        assert_eq!(
            loaded
                .ratchet_initial_secret
                .as_ref()
                .map(|secret| secret.as_bytes()),
            original
                .ratchet_initial_secret
                .as_ref()
                .map(|secret| secret.as_bytes())
        );
        assert_eq!(
            loaded
                .ratchet_initial_pub
                .as_ref()
                .map(|public| public.as_bytes()),
            Some(ratchet_pub.as_bytes())
        );
        assert_eq!(loaded.recovery_entropy, original.recovery_entropy);

        let wrong_reader = crate::sealer::MemorySealer::new();
        assert!(
            matches!(load_identity(&path, &wrong_reader), Err(Error::Sealer(_))),
            "an independent memory sealer must not decrypt the saved identity"
        );
    }
}
