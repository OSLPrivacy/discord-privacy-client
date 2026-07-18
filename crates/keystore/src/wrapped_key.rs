//! Identity authorization for wrapped-key uploads.
//!
//! The canonical encoding mirrors
//! `keyserver-cf/src/lib/canonical.ts::canonicalWrappedKeyPostBytes`. Every
//! persisted field plus a short-lived timestamp is signed by the registered
//! sender identity, replacing the former client-wide bearer.

use crate::identity::Identity;
use crypto::ed25519;
use serde::Serialize;

pub const WRAPPED_KEY_POST_DOMAIN: &[u8] = b"discord-privacy-client/wrapped-key-post/v1";

#[derive(Clone, Debug, Serialize)]
pub struct WrappedKeyUpload {
    pub content_id: String,
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_message_kind: Option<String>,
    pub recipient_id: String,
    pub session_version: u32,
    pub share_index: u32,
    pub wrapped_share_blob: String,
    pub blob_version: u32,
    pub single_use: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_duration_seconds: Option<u32>,
    pub expires_at: String,
}

pub fn canonical_wrapped_key_post_bytes(
    sender_id: &str,
    upload: &WrappedKeyUpload,
    timestamp_ms: i64,
) -> Vec<u8> {
    let mut out = Vec::new();
    write_lp(&mut out, WRAPPED_KEY_POST_DOMAIN);
    write_lp(&mut out, upload.content_id.as_bytes());
    write_lp(&mut out, upload.content_type.as_bytes());
    write_lp(
        &mut out,
        upload
            .system_message_kind
            .as_deref()
            .unwrap_or("")
            .as_bytes(),
    );
    write_lp(&mut out, sender_id.as_bytes());
    write_lp(&mut out, upload.recipient_id.as_bytes());
    out.extend_from_slice(&upload.session_version.to_be_bytes());
    out.extend_from_slice(&upload.share_index.to_be_bytes());
    write_lp(&mut out, upload.wrapped_share_blob.as_bytes());
    out.extend_from_slice(&upload.blob_version.to_be_bytes());
    out.push(u8::from(upload.single_use));
    match upload.display_duration_seconds {
        Some(seconds) => {
            out.push(1);
            out.extend_from_slice(&seconds.to_be_bytes());
        }
        None => out.push(0),
    }
    write_lp(&mut out, upload.expires_at.as_bytes());
    write_lp(&mut out, timestamp_ms.to_string().as_bytes());
    out
}

pub fn sign_wrapped_key_post(
    identity: &Identity,
    upload: &WrappedKeyUpload,
    timestamp_ms: i64,
) -> ed25519::Signature {
    ed25519::sign(
        &identity.ed25519_secret,
        &canonical_wrapped_key_post_bytes(&identity.user_id, upload, timestamp_ms),
    )
}

fn write_lp(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u32).to_be_bytes());
    out.extend_from_slice(value);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn fixture() -> WrappedKeyUpload {
        WrappedKeyUpload {
            content_id: "message-1".into(),
            content_type: "text".into(),
            system_message_kind: None,
            recipient_id: "bob".into(),
            session_version: 1,
            share_index: 0,
            wrapped_share_blob: "AQIDBA==".into(),
            blob_version: 1,
            single_use: false,
            display_duration_seconds: None,
            expires_at: "2026-07-18T00:00:00.000Z".into(),
        }
    }

    #[test]
    fn canonical_encoding_binds_mutable_fields() {
        let base = fixture();
        let encoded = canonical_wrapped_key_post_bytes("alice", &base, 1_700_000_000_123);
        assert_eq!(
            hex(&encoded),
            concat!(
                "0000002a646973636f72642d707269766163792d636c69656e742f777261707065642d6b65792d706f73742f7631",
                "000000096d6573736167652d3100000004746578740000000000000005616c69636500000003626f62",
                "0000000100000000000000084151494442413d3d00000001000000000018323032362d30372d31385430303a30303a30302e3030305a",
                "0000000d31373030303030303030313233"
            )
        );
        let mut changed = base.clone();
        changed.recipient_id = "carol".into();
        assert_ne!(
            encoded,
            canonical_wrapped_key_post_bytes("alice", &changed, 1_700_000_000_123)
        );
        assert_ne!(
            encoded,
            canonical_wrapped_key_post_bytes("alice", &base, 1_700_000_000_124)
        );
    }

    #[test]
    fn signature_verifies_for_exact_upload_only() {
        let identity = crate::generate_identity("alice".into());
        let upload = fixture();
        let timestamp = 1_700_000_000_123;
        let signature = sign_wrapped_key_post(&identity, &upload, timestamp);
        let canonical = canonical_wrapped_key_post_bytes("alice", &upload, timestamp);
        assert!(ed25519::verify(&identity.ed25519_public, &canonical, &signature).unwrap());
    }
}
