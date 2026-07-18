//! Ed25519 authorization for keyserver GETs that consume server state.
//!
//! A client-wide bearer is not an identity credential. These canonical
//! messages bind the registered requester, intended recipient, concrete
//! destructive-read target and a short-lived timestamp.

use crate::identity::Identity;
use crypto::ed25519;

pub const PREKEY_BUNDLE_GET_DOMAIN: &[u8] = b"discord-privacy-client/prekey-bundle-get/v1";
pub const WRAPPED_KEY_GET_DOMAIN: &[u8] = b"discord-privacy-client/wrapped-key-get/v1";

fn write_lp(buf: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    buf.extend_from_slice(bytes);
}

pub fn canonical_prekey_bundle_get_bytes(
    requester_id: &str,
    recipient_id: &str,
    timestamp_ms: i64,
) -> Vec<u8> {
    let mut out = Vec::new();
    write_lp(
        &mut out,
        std::str::from_utf8(PREKEY_BUNDLE_GET_DOMAIN).expect("ASCII domain"),
    );
    write_lp(&mut out, requester_id);
    write_lp(&mut out, recipient_id);
    // Explicit target id. For a prekey pop the target is the recipient's
    // bundle, so this intentionally repeats recipient_id.
    write_lp(&mut out, recipient_id);
    write_lp(&mut out, &timestamp_ms.to_string());
    out
}

pub fn sign_prekey_bundle_get(
    requester: &Identity,
    recipient_id: &str,
    timestamp_ms: i64,
) -> ed25519::Signature {
    let message = canonical_prekey_bundle_get_bytes(&requester.user_id, recipient_id, timestamp_ms);
    ed25519::sign(&requester.ed25519_secret, &message)
}

pub fn canonical_wrapped_key_get_bytes(
    requester_id: &str,
    recipient_id: &str,
    content_id: &str,
    timestamp_ms: i64,
) -> Vec<u8> {
    let mut out = Vec::new();
    write_lp(
        &mut out,
        std::str::from_utf8(WRAPPED_KEY_GET_DOMAIN).expect("ASCII domain"),
    );
    write_lp(&mut out, requester_id);
    write_lp(&mut out, recipient_id);
    write_lp(&mut out, content_id);
    write_lp(&mut out, &timestamp_ms.to_string());
    out
}

pub fn sign_wrapped_key_get(
    recipient: &Identity,
    content_id: &str,
    timestamp_ms: i64,
) -> ed25519::Signature {
    let message = canonical_wrapped_key_get_bytes(
        &recipient.user_id,
        &recipient.user_id,
        content_id,
        timestamp_ms,
    );
    ed25519::sign(&recipient.ed25519_secret, &message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn canonical_vectors_match_worker() {
        assert_eq!(
            hex(&canonical_prekey_bundle_get_bytes(
                "alice",
                "bob",
                1_700_000_000_123,
            )),
            "0000002b646973636f72642d707269766163792d636c69656e742f7072656b65792d62756e646c652d6765742f7631\
             00000005616c69636500000003626f6200000003626f620000000d31373030303030303030313233"
                .replace(' ', ""),
        );
        assert_eq!(
            hex(&canonical_wrapped_key_get_bytes(
                "bob",
                "bob",
                "message-1",
                1_700_000_000_123,
            )),
            "00000029646973636f72642d707269766163792d636c69656e742f777261707065642d6b65792d6765742f7631\
             00000003626f6200000003626f62000000096d6573736167652d310000000d31373030303030303030313233"
                .replace(' ', ""),
        );
    }
}
