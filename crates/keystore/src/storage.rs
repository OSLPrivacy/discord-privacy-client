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

const INSECURE_BANNER: &str =
    "INSECURE prototype storage — plain JSON, no passphrase, no TPM. \
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

#[derive(Serialize, Deserialize)]
struct InnerIdentity {
    user_id: String,
    x25519_secret_b64: String,
    x25519_public_b64: String,
    ed25519_secret_b64: String,
    ed25519_public_b64: String,
    mlkem_secret_b64: String,
    mlkem_public_b64: String,
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
    };
    let inner_bytes = serde_json::to_vec(&inner)?;
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

    let x25519_secret = decode_array::<{ x25519::SECRET_KEY_SIZE }>(
        "x25519_secret",
        &inner.x25519_secret_b64,
    )?;
    let x25519_public = decode_array::<{ x25519::PUBLIC_KEY_SIZE }>(
        "x25519_public",
        &inner.x25519_public_b64,
    )?;
    let ed25519_secret = decode_array::<{ ed25519::SECRET_KEY_SIZE }>(
        "ed25519_secret",
        &inner.ed25519_secret_b64,
    )?;
    let ed25519_public = decode_array::<{ ed25519::PUBLIC_KEY_SIZE }>(
        "ed25519_public",
        &inner.ed25519_public_b64,
    )?;
    let mlkem_secret = decode_array::<{ ml_kem_768::DECAPSULATION_KEY_SIZE }>(
        "mlkem_secret",
        &inner.mlkem_secret_b64,
    )?;
    let mlkem_public = decode_array::<{ ml_kem_768::ENCAPSULATION_KEY_SIZE }>(
        "mlkem_public",
        &inner.mlkem_public_b64,
    )?;

    Ok(Identity::from_bytes(
        inner.user_id,
        x25519_secret,
        x25519_public,
        ed25519_secret,
        ed25519_public,
        mlkem_secret,
        mlkem_public,
    ))
}

fn decode_array<const N: usize>(field: &'static str, b64: &str) -> Result<[u8; N]> {
    let v = STANDARD.decode(b64)?;
    if v.len() != N {
        return Err(Error::BlobFieldLength {
            field,
            got: v.len(),
            expected: N,
        });
    }
    let mut out = [0u8; N];
    out.copy_from_slice(&v);
    Ok(out)
}
