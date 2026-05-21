//! Phase 6.4: control-message inbox.
//!
//! Out-of-band delivery for non-content control wires (SKDM
//! bundles, burn markers, SKDM_REQUESTs, recovery SKDMs). Pre-6.4,
//! these were posted to Discord channels (with prose-token cover
//! wrap) where they were visible as ciphertext noise to observers
//! and consumed cipher-store upload budget for the cover. Post-6.4,
//! they go through `keyserver.oslprivacy.com/v1/control-inbox` --
//! invisible to Discord entirely.
//!
//! Mirrors `keyserver-cf/src/lib/canonical.ts` + `endpoints/
//! control-inbox.ts`. The canonical-bytes implementations on both
//! sides MUST stay byte-identical or signatures fail to verify.
//!
//! Sigs cover ed25519 over canonical bytes formed from the
//! operation domain + the relevant fields. The freshness window
//! mirrors UNREGISTER's: 5 minutes.

use crate::identity::Identity;
use crypto::ed25519;
use sha2::{Digest, Sha256};

pub const CONTROL_INBOX_POST_DOMAIN: &[u8] =
    b"discord-privacy-client/control-inbox-post/v1";
pub const CONTROL_INBOX_GET_DOMAIN: &[u8] =
    b"discord-privacy-client/control-inbox-get/v1";
pub const CONTROL_INBOX_DELETE_DOMAIN: &[u8] =
    b"discord-privacy-client/control-inbox-delete/v1";

/// Canonical bytes the POST signature covers. Must agree byte-for-
/// byte with `canonicalControlInboxPostBytes` in the keyserver TS.
///
/// Wire:
///   LP(domain) || LP(sender_id) || LP(recipient_id) || LP(scope_id)
///   || LP(timestamp_ms_str) || LP("") || sha256(bundle)
///
/// The empty-LP slot is a reserved-for-future-fields placeholder so
/// new fields can be added without breaking existing signatures.
/// `sha256(bundle)` is appended raw (32 bytes); the LP wrapper is
/// not needed because the byte length is fixed by the hash.
pub fn canonical_control_inbox_post_bytes(
    sender_id: &str,
    recipient_id: &str,
    scope_id: &str,
    timestamp_ms: i64,
    bundle: &[u8],
) -> Vec<u8> {
    let mut buf = Vec::new();
    write_lp(&mut buf, CONTROL_INBOX_POST_DOMAIN);
    write_lp(&mut buf, sender_id.as_bytes());
    write_lp(&mut buf, recipient_id.as_bytes());
    write_lp(&mut buf, scope_id.as_bytes());
    write_lp(&mut buf, timestamp_ms.to_string().as_bytes());
    write_lp(&mut buf, b"");
    let mut hasher = Sha256::new();
    hasher.update(bundle);
    let digest = hasher.finalize();
    buf.extend_from_slice(&digest);
    buf
}

/// Canonical bytes the GET signature covers.
///
/// Wire: LP(domain) || LP(user_id) || LP(timestamp_ms_str)
pub fn canonical_control_inbox_get_bytes(user_id: &str, timestamp_ms: i64) -> Vec<u8> {
    let mut buf = Vec::new();
    write_lp(&mut buf, CONTROL_INBOX_GET_DOMAIN);
    write_lp(&mut buf, user_id.as_bytes());
    write_lp(&mut buf, timestamp_ms.to_string().as_bytes());
    buf
}

/// Canonical bytes the DELETE signature covers.
///
/// Wire: LP(domain) || LP(user_id) || LP(inbox_id_hex) || LP(timestamp_ms_str)
pub fn canonical_control_inbox_delete_bytes(
    user_id: &str,
    inbox_id_hex: &str,
    timestamp_ms: i64,
) -> Vec<u8> {
    let mut buf = Vec::new();
    write_lp(&mut buf, CONTROL_INBOX_DELETE_DOMAIN);
    write_lp(&mut buf, user_id.as_bytes());
    write_lp(&mut buf, inbox_id_hex.as_bytes());
    write_lp(&mut buf, timestamp_ms.to_string().as_bytes());
    buf
}

pub fn sign_control_inbox_post(
    identity: &Identity,
    recipient_id: &str,
    scope_id: &str,
    timestamp_ms: i64,
    bundle: &[u8],
) -> ed25519::Signature {
    let bytes = canonical_control_inbox_post_bytes(
        &identity.user_id,
        recipient_id,
        scope_id,
        timestamp_ms,
        bundle,
    );
    ed25519::sign(&identity.ed25519_secret, &bytes)
}

pub fn sign_control_inbox_get(
    identity: &Identity,
    timestamp_ms: i64,
) -> ed25519::Signature {
    let bytes = canonical_control_inbox_get_bytes(&identity.user_id, timestamp_ms);
    ed25519::sign(&identity.ed25519_secret, &bytes)
}

pub fn sign_control_inbox_delete(
    identity: &Identity,
    inbox_id_hex: &str,
    timestamp_ms: i64,
) -> ed25519::Signature {
    let bytes = canonical_control_inbox_delete_bytes(
        &identity.user_id,
        inbox_id_hex,
        timestamp_ms,
    );
    ed25519::sign(&identity.ed25519_secret, &bytes)
}

fn write_lp(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    buf.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_canonical_bytes_are_deterministic() {
        let a = canonical_control_inbox_post_bytes(
            "sender",
            "recipient",
            "gc:123",
            1700000000000,
            b"opaque-bundle-bytes",
        );
        let b = canonical_control_inbox_post_bytes(
            "sender",
            "recipient",
            "gc:123",
            1700000000000,
            b"opaque-bundle-bytes",
        );
        assert_eq!(a, b);
    }

    #[test]
    fn post_canonical_bytes_change_with_bundle() {
        let a = canonical_control_inbox_post_bytes(
            "sender",
            "recipient",
            "gc:123",
            1700000000000,
            b"bundle-a",
        );
        let b = canonical_control_inbox_post_bytes(
            "sender",
            "recipient",
            "gc:123",
            1700000000000,
            b"bundle-b",
        );
        assert_ne!(a, b);
    }

    #[test]
    fn get_and_delete_canonical_bytes_differ_for_same_user() {
        let g = canonical_control_inbox_get_bytes("u", 1);
        let d = canonical_control_inbox_delete_bytes("u", "00", 1);
        assert_ne!(g, d);
    }
}
