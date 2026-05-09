//! Bucket-based padding for text messages.
//!
//! Spec: `docs/design/pqxdh-double-ratchet.md` "Padding" subsection.
//!
//! Layout:
//!
//! ```text
//! [u32 BE plaintext length][plaintext bytes][zero padding to bucket boundary]
//! ```
//!
//! Padding is applied **before** AEAD seal, so it lives inside the AEAD
//! ciphertext and is authenticated by the tag — it cannot be stripped
//! without invalidating the AEAD.
//!
//! v1 buckets (text): 64 / 128 / 256 / 512 / 1024 bytes.
//!
//! Attachment streaming buckets (256 KB / 1 MB / 5 MB / 10 MB / 25 MB)
//! and 16 KB-chunked streaming AEAD pending future commits.

use crate::error::{Error, Result};

pub const TEXT_BUCKETS: &[usize] = &[64, 128, 256, 512, 1024];
const LENGTH_PREFIX_SIZE: usize = 4;

/// Largest plaintext size that fits in any v1 text bucket.
pub fn max_text_plaintext_size() -> usize {
    *TEXT_BUCKETS.last().expect("TEXT_BUCKETS is non-empty") - LENGTH_PREFIX_SIZE
}

/// Pad `plaintext` to the smallest text bucket that fits the length-prefixed
/// plaintext. Returns the padded buffer (always exactly bucket size).
pub fn pad_text(plaintext: &[u8]) -> Result<Vec<u8>> {
    let needed = LENGTH_PREFIX_SIZE
        .checked_add(plaintext.len())
        .ok_or(Error::PaddingOverflow {
            max: max_text_plaintext_size(),
            got: plaintext.len(),
        })?;

    let bucket = TEXT_BUCKETS
        .iter()
        .copied()
        .find(|&b| b >= needed)
        .ok_or(Error::PaddingOverflow {
            max: max_text_plaintext_size(),
            got: plaintext.len(),
        })?;

    let len = u32::try_from(plaintext.len()).map_err(|_| Error::PaddingOverflow {
        max: max_text_plaintext_size(),
        got: plaintext.len(),
    })?;

    let mut out = vec![0u8; bucket];
    out[..LENGTH_PREFIX_SIZE].copy_from_slice(&len.to_be_bytes());
    out[LENGTH_PREFIX_SIZE..LENGTH_PREFIX_SIZE + plaintext.len()].copy_from_slice(plaintext);
    Ok(out)
}

/// Strip padding, returning the original plaintext.
///
/// Validates that `padded.len()` matches a known bucket size and that the
/// claimed length fits within the bucket's payload capacity.
pub fn unpad_text(padded: &[u8]) -> Result<Vec<u8>> {
    if padded.len() < LENGTH_PREFIX_SIZE {
        return Err(Error::PaddingTruncated);
    }
    if !TEXT_BUCKETS.contains(&padded.len()) {
        return Err(Error::PaddingUnknownBucket { got: padded.len() });
    }
    let mut len_bytes = [0u8; LENGTH_PREFIX_SIZE];
    len_bytes.copy_from_slice(&padded[..LENGTH_PREFIX_SIZE]);
    let claimed = u32::from_be_bytes(len_bytes) as usize;
    let capacity = padded.len() - LENGTH_PREFIX_SIZE;
    if claimed > capacity {
        return Err(Error::PaddingMalformed { claimed, capacity });
    }
    Ok(padded[LENGTH_PREFIX_SIZE..LENGTH_PREFIX_SIZE + claimed].to_vec())
}
