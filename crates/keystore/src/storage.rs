//! Plain-file identity storage.
//!
//! ⚠️  See the crate-level docs: this is **insecure prototype storage**.
//! No passphrase wrapping, no Argon2id KDF, no TPM seal. The on-disk
//! file is a JSON document with base64-encoded key bytes plus an
//! INSECURE banner field that v1 stable storage will reject.

use crate::identity::{Identity, IDENTITY_BLOB_VERSION};
use crate::{Error, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::{ml_kem_768, x25519};
use serde::{Deserialize, Serialize};
use std::path::Path;

const INSECURE_BANNER: &str =
    "INSECURE prototype storage — plain JSON, no passphrase, no TPM. \
     v1 stable replaces with TPM-sealed blob; do NOT use with real users.";

#[derive(Serialize, Deserialize)]
struct Blob {
    version: u32,
    user_id: String,
    x25519_secret_b64: String,
    x25519_public_b64: String,
    mlkem_secret_b64: String,
    mlkem_public_b64: String,
    insecure_banner: String,
}

/// Save `identity` to `path` as plain-JSON. Overwrites any existing
/// file at that path.
pub fn save_identity(path: &Path, identity: &Identity) -> Result<()> {
    let blob = Blob {
        version: IDENTITY_BLOB_VERSION,
        user_id: identity.user_id.clone(),
        x25519_secret_b64: STANDARD.encode(identity.x25519_secret.as_bytes()),
        x25519_public_b64: STANDARD.encode(identity.x25519_public.as_bytes()),
        mlkem_secret_b64: STANDARD.encode(identity.mlkem_secret_bytes()),
        mlkem_public_b64: STANDARD.encode(identity.mlkem_public_bytes),
        insecure_banner: INSECURE_BANNER.to_string(),
    };
    let json = serde_json::to_vec_pretty(&blob)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, &json)?;
    Ok(())
}

/// Load an identity from `path`.
pub fn load_identity(path: &Path) -> Result<Identity> {
    let bytes = std::fs::read(path)?;
    let blob: Blob = serde_json::from_slice(&bytes)?;
    if blob.version != IDENTITY_BLOB_VERSION {
        return Err(Error::BlobVersionMismatch {
            got: blob.version,
            expected: IDENTITY_BLOB_VERSION,
        });
    }
    let x25519_secret = decode_array::<{ x25519::SECRET_KEY_SIZE }>(
        "x25519_secret",
        &blob.x25519_secret_b64,
    )?;
    let x25519_public = decode_array::<{ x25519::PUBLIC_KEY_SIZE }>(
        "x25519_public",
        &blob.x25519_public_b64,
    )?;
    let mlkem_secret = decode_array::<{ ml_kem_768::DECAPSULATION_KEY_SIZE }>(
        "mlkem_secret",
        &blob.mlkem_secret_b64,
    )?;
    let mlkem_public = decode_array::<{ ml_kem_768::ENCAPSULATION_KEY_SIZE }>(
        "mlkem_public",
        &blob.mlkem_public_b64,
    )?;
    Ok(Identity::from_bytes(
        blob.user_id,
        x25519_secret,
        x25519_public,
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
