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

pub const CONTROL_INBOX_POST_DOMAIN: &[u8] = b"discord-privacy-client/control-inbox-post/v1";
pub const CONTROL_INBOX_GET_DOMAIN: &[u8] = b"discord-privacy-client/control-inbox-get/v1";
pub const CONTROL_INBOX_DELETE_DOMAIN: &[u8] = b"discord-privacy-client/control-inbox-delete/v1";
pub const SENDER_FILTER_FLOOR_GET_DOMAIN: &[u8] =
    b"discord-privacy-client/sender-filter-floor-get/v1";

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
    canonical_control_inbox_post_bytes_lane(
        sender_id,
        recipient_id,
        scope_id,
        timestamp_ms,
        bundle,
        None,
        None,
    )
}

/// The bilateral-burn revocation lane, `""` being the ordinary lane every
/// existing caller uses.
pub const CONTROL_INBOX_KIND_REVOCATION: &str = "revocation";

/// Canonical bytes for a POST that names a delivery lane.
///
/// Wire:
///   LP(domain) || LP(sender_id) || LP(recipient_id) || LP(scope_id)
///   || LP(timestamp_ms_str) || LP(kind) || sha256(bundle)
///   [ || LP(collapse_key) ]   -- only for a revocation
///
/// `kind` occupies the reserved empty slot documented above; `None` or `Some("")`
/// reproduces the pre-lane bytes byte-for-byte, so every deployed caller and the
/// deployed Worker are unaffected. `collapse_key` is an optional trailing
/// component, the same shape as the GET's `sender_id` filter.
///
/// Both are **signed**, and that is the security property:
///
/// - An attacker cannot strip `revocation` in transit to demote a queued burn
///   into the evictable ordinary lane, where the sender's own next 32 messages
///   would silently delete it.
/// - An attacker cannot add it to jump the non-evictable lane.
/// - A Worker too old to know about lanes reconstructs the pre-lane bytes and
///   refuses the request (401), so an un-upgraded server **fails closed** rather
///   than accepting a burn into a lane it will happily evict from.
pub fn canonical_control_inbox_post_bytes_lane(
    sender_id: &str,
    recipient_id: &str,
    scope_id: &str,
    timestamp_ms: i64,
    bundle: &[u8],
    kind: Option<&str>,
    collapse_key: Option<&str>,
) -> Vec<u8> {
    let mut buf = Vec::new();
    write_lp(&mut buf, CONTROL_INBOX_POST_DOMAIN);
    write_lp(&mut buf, sender_id.as_bytes());
    write_lp(&mut buf, recipient_id.as_bytes());
    write_lp(&mut buf, scope_id.as_bytes());
    write_lp(&mut buf, timestamp_ms.to_string().as_bytes());
    write_lp(&mut buf, kind.unwrap_or("").as_bytes());
    let mut hasher = Sha256::new();
    hasher.update(bundle);
    let digest = hasher.finalize();
    buf.extend_from_slice(&digest);
    if let Some(collapse_key) = collapse_key {
        write_lp(&mut buf, collapse_key.as_bytes());
    }
    buf
}

/// Canonical bytes the GET signature covers.
///
/// Wire: LP(domain) || LP(user_id) || LP(timestamp_ms_str)
///
/// Unchanged, and still the bytes an unfiltered drain signs. The
/// filtered form lives in
/// [`canonical_control_inbox_get_bytes_filtered`].
pub fn canonical_control_inbox_get_bytes(user_id: &str, timestamp_ms: i64) -> Vec<u8> {
    canonical_control_inbox_get_bytes_filtered(user_id, timestamp_ms, None)
}

/// Canonical bytes for a drain that asks the server to return only rows
/// from one sender.
///
/// Wire: LP(domain) || LP(user_id) || LP(timestamp_ms_str)
///       || LP(sender_id)   -- only when a filter is requested
///
/// The filter is a **signed** component, mirroring
/// `canonicalControlInboxGetBytes` in
/// `keyserver-cf/src/lib/canonical.ts`. Two consequences, both
/// deliberate:
///
/// - `None` reproduces the pre-filter bytes exactly, so an unfiltered
///   drain against any worker (old or new) is unaffected.
/// - An attacker who strips `?sender=` from the request in flight makes
///   the server reconstruct the unfiltered bytes, which this signature
///   does not cover, so the request is refused (401). The starvation the
///   filter fixes cannot be silently reinstated in transit — and a
///   worker too old to know about the parameter refuses the request for
///   the same reason, so an un-upgraded server also fails closed rather
///   than serving an unfiltered page the caller would mistake for a
///   filtered one.
pub fn canonical_control_inbox_get_bytes_filtered(
    user_id: &str,
    timestamp_ms: i64,
    sender_id: Option<&str>,
) -> Vec<u8> {
    let mut buf = Vec::new();
    write_lp(&mut buf, CONTROL_INBOX_GET_DOMAIN);
    write_lp(&mut buf, user_id.as_bytes());
    write_lp(&mut buf, timestamp_ms.to_string().as_bytes());
    if let Some(sender) = sender_id {
        write_lp(&mut buf, sender.as_bytes());
    }
    buf
}

/// Canonical bytes for a fresh observation of the independently administered
/// sender-filter capability floor.
///
/// Wire: LP(domain) || LP(user_id) || LP(timestamp_ms_str) || LP(request_id)
///
/// `request_id` is 256 bits of client-generated entropy. The D1-backed Worker
/// echoes it in the response, so a cached response cannot be substituted after
/// local state deletion or process restart.
pub fn canonical_sender_filter_floor_get_bytes(
    user_id: &str,
    timestamp_ms: i64,
    request_id: &str,
) -> Vec<u8> {
    let mut buf = Vec::new();
    write_lp(&mut buf, SENDER_FILTER_FLOOR_GET_DOMAIN);
    write_lp(&mut buf, user_id.as_bytes());
    write_lp(&mut buf, timestamp_ms.to_string().as_bytes());
    write_lp(&mut buf, request_id.as_bytes());
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

/// Sign a POST that names a delivery lane. `None, None` is byte-identical to
/// [`sign_control_inbox_post`].
pub fn sign_control_inbox_post_lane(
    identity: &Identity,
    recipient_id: &str,
    scope_id: &str,
    timestamp_ms: i64,
    bundle: &[u8],
    kind: Option<&str>,
    collapse_key: Option<&str>,
) -> ed25519::Signature {
    let bytes = canonical_control_inbox_post_bytes_lane(
        &identity.user_id,
        recipient_id,
        scope_id,
        timestamp_ms,
        bundle,
        kind,
        collapse_key,
    );
    ed25519::sign(&identity.ed25519_secret, &bytes)
}

pub fn sign_control_inbox_get(identity: &Identity, timestamp_ms: i64) -> ed25519::Signature {
    let bytes = canonical_control_inbox_get_bytes(&identity.user_id, timestamp_ms);
    ed25519::sign(&identity.ed25519_secret, &bytes)
}

/// Sign a drain that asks for only one sender's rows. `None` is
/// byte-identical to [`sign_control_inbox_get`].
pub fn sign_control_inbox_get_filtered(
    identity: &Identity,
    timestamp_ms: i64,
    sender_id: Option<&str>,
) -> ed25519::Signature {
    let bytes =
        canonical_control_inbox_get_bytes_filtered(&identity.user_id, timestamp_ms, sender_id);
    ed25519::sign(&identity.ed25519_secret, &bytes)
}

pub fn sign_sender_filter_floor_get(
    identity: &Identity,
    timestamp_ms: i64,
    request_id: &str,
) -> ed25519::Signature {
    let bytes =
        canonical_sender_filter_floor_get_bytes(&identity.user_id, timestamp_ms, request_id);
    ed25519::sign(&identity.ed25519_secret, &bytes)
}

pub fn sign_control_inbox_delete(
    identity: &Identity,
    inbox_id_hex: &str,
    timestamp_ms: i64,
) -> ed25519::Signature {
    let bytes = canonical_control_inbox_delete_bytes(&identity.user_id, inbox_id_hex, timestamp_ms);
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

    /// The ordinary lane must not change: a deployed worker verifies these exact
    /// bytes, so a lane-aware client posting ordinary traffic stays compatible.
    #[test]
    fn the_ordinary_lane_signs_the_legacy_post_bytes() {
        let legacy = canonical_control_inbox_post_bytes("s", "r", "gc:1", 7, b"b");
        assert_eq!(
            legacy,
            canonical_control_inbox_post_bytes_lane("s", "r", "gc:1", 7, b"b", None, None)
        );
        assert_eq!(
            legacy,
            canonical_control_inbox_post_bytes_lane("s", "r", "gc:1", 7, b"b", Some(""), None)
        );
    }

    /// The lane and the collapse key must both change the bytes, or an attacker
    /// could strip `revocation` in transit and demote a burn into the evictable
    /// lane where the sender's own next 32 messages delete it silently.
    #[test]
    fn the_lane_and_collapse_key_are_bound_into_the_signed_bytes() {
        let ordinary = canonical_control_inbox_post_bytes("s", "r", "gc:1", 7, b"b");
        let revocation = canonical_control_inbox_post_bytes_lane(
            "s",
            "r",
            "gc:1",
            7,
            b"b",
            Some(CONTROL_INBOX_KIND_REVOCATION),
            Some(&"a".repeat(64)),
        );
        assert_ne!(ordinary, revocation);
        // Changing only the collapse key changes the bytes.
        assert_ne!(
            revocation,
            canonical_control_inbox_post_bytes_lane(
                "s",
                "r",
                "gc:1",
                7,
                b"b",
                Some(CONTROL_INBOX_KIND_REVOCATION),
                Some(&"b".repeat(64)),
            )
        );
        // Changing only the lane changes the bytes.
        assert_ne!(
            revocation,
            canonical_control_inbox_post_bytes_lane(
                "s",
                "r",
                "gc:1",
                7,
                b"b",
                Some(""),
                Some(&"a".repeat(64)),
            )
        );
        // And no lane value can collide with a different scope_id split, because
        // every component is length-prefixed.
        assert_ne!(
            canonical_control_inbox_post_bytes_lane("s", "r", "gc:1", 7, b"b", Some("x"), None),
            canonical_control_inbox_post_bytes_lane("s", "r", "gc:1x", 7, b"b", None, None)
        );
    }

    /// The unfiltered form must not change: a deployed worker verifies
    /// these exact bytes.
    #[test]
    fn an_unfiltered_drain_signs_the_legacy_bytes() {
        assert_eq!(
            canonical_control_inbox_get_bytes("u", 7),
            canonical_control_inbox_get_bytes_filtered("u", 7, None)
        );
    }

    /// Adding, removing or changing the filter must change the bytes, or
    /// the signature would not bind it and an attacker could strip it.
    #[test]
    fn the_sender_filter_is_bound_into_the_signed_bytes() {
        let unfiltered = canonical_control_inbox_get_bytes_filtered("u", 7, None);
        let a = canonical_control_inbox_get_bytes_filtered("u", 7, Some("peer-a"));
        let b = canonical_control_inbox_get_bytes_filtered("u", 7, Some("peer-b"));
        assert_ne!(unfiltered, a);
        assert_ne!(a, b);
        // And a filtered form is never a prefix-extension collision with
        // a different user_id / timestamp split, because every component
        // is length-prefixed.
        assert_ne!(
            canonical_control_inbox_get_bytes_filtered("u", 7, Some("x")),
            canonical_control_inbox_get_bytes_filtered("ux", 7, None)
        );
    }
}
