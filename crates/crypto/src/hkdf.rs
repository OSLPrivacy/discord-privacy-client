//! HKDF-SHA256 wrapper.
//!
//! Spec: `docs/design/pqxdh-double-ratchet.md` (HKDF-SHA256 used
//! throughout the construction with domain-separation labels).
//! Library: RustCrypto `hkdf` + `sha2`.

use crate::error::{Error, Result};
use hkdf::Hkdf;
use sha2::Sha256;

/// Extract-and-expand HKDF-SHA256.
///
/// `salt` may be empty (RFC 5869 §2.2 allows zero-length salt; in that case
/// HKDF substitutes a string of HashLen zero bytes).
/// `ikm` is the input key material.
/// `info` provides domain-separation context.
/// `length` is the desired output key material length in bytes.
/// Maximum `length` per RFC 5869 §2.3 is 255 × HashLen = 8160 bytes for SHA-256.
pub fn derive(salt: &[u8], ikm: &[u8], info: &[u8], length: usize) -> Result<Vec<u8>> {
    let salt_opt = if salt.is_empty() { None } else { Some(salt) };
    let hk = Hkdf::<Sha256>::new(salt_opt, ikm);
    let mut out = vec![0u8; length];
    hk.expand(info, &mut out)
        .map_err(|e| Error::HkdfExpand(format!("{e:?}")))?;
    Ok(out)
}

/// Convenience: derive a fixed 32-byte key.
pub fn derive_32(salt: &[u8], ikm: &[u8], info: &[u8]) -> Result<[u8; 32]> {
    let v = derive(salt, ikm, info, 32)?;
    let mut out = [0u8; 32];
    out.copy_from_slice(&v);
    Ok(out)
}
