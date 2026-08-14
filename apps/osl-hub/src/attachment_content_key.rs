//! One attachment, one content-encryption key.
//!
//! Every attachment OSL seals draws its own 32-byte content-encryption key
//! (CEK) straight from the operating system's CSPRNG, through
//! [`crypto::random::random_bytes`]. The draw is the *only* input: a CEK is
//! never
//!
//! * derived from the file's bytes,
//! * derived from the attachment's public metadata (original filename, MIME
//!   type, plaintext size, content id, attachment index, object id),
//! * derived from, or shared with, any other attachment's CEK, and
//! * wrapped under any other attachment's CEK.
//!
//! The CEK for one attachment travels to the recipient inside that one
//! attachment's own control-inbox notice, sealed by the manual-peer envelope
//! in `broker::deliver_peer_attachment`. That wrap takes exactly one CEK as
//! input, so no notice can carry a second attachment's secret and no notice
//! is a function of any ciphertext.
//!
//! The consequence this module exists to guarantee: exposing one attachment's
//! complete key material and metadata buys an attacker exactly that one
//! attachment, and exactly zero bytes of every other attachment — same
//! account, same message, same tier or a different tier.
//!
//! Nonce freshness is a *separate* property, owned by
//! `crypto::attachment::StreamEncryptor` (a fresh 20-byte random base-nonce
//! prefix per attachment). Fresh nonces do not stand in for key separation:
//! a shared key with fresh nonces still lets one exposed key open every
//! attachment. Both properties are required, and
//! `tests/task_0044c_attachment_key_boundary.rs` proves the key one on its
//! own.
//!
//! This module deliberately exposes no fingerprint, digest or identifier
//! derived from a CEK. Any observer that wants to compare keys must compute
//! its own fingerprint with its own code, so a key-separation claim can never
//! be self-certified by the code that generated the keys.

/// Bytes in one attachment content-encryption key.
pub const CONTENT_KEY_BYTES: usize = 32;

/// Bytes in one attachment content id.
pub const CONTENT_ID_BYTES: usize = 16;

/// Draw a fresh content-encryption key for exactly one attachment.
///
/// Callers must call this once per attachment and must not reuse, cache or
/// share the result across attachments.
pub fn draw_content_key() -> [u8; CONTENT_KEY_BYTES] {
    let mut key = [0u8; CONTENT_KEY_BYTES];
    key.copy_from_slice(&crypto::random::random_bytes(CONTENT_KEY_BYTES));
    key
}

/// Draw a fresh content id for exactly one attachment. The content id is
/// public (it is bound into the stream header) and is never an input to
/// [`draw_content_key`].
pub fn draw_content_id() -> [u8; CONTENT_ID_BYTES] {
    let mut content_id = [0u8; CONTENT_ID_BYTES];
    content_id.copy_from_slice(&crypto::random::random_bytes(CONTENT_ID_BYTES));
    content_id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separate_draws_do_not_repeat() {
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..64 {
            assert!(seen.insert(draw_content_key()), "content key repeated");
        }
        assert_eq!(seen.len(), 64);
    }

    #[test]
    fn a_drawn_key_is_not_all_zero() {
        assert_ne!(draw_content_key(), [0u8; CONTENT_KEY_BYTES]);
        assert_ne!(draw_content_id(), [0u8; CONTENT_ID_BYTES]);
    }
}
