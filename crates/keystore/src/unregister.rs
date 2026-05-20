//! Account-burn keyserver unregister.
//!
//! Mirrors `keyserver-cf/src/lib/canonical.ts`
//! `canonicalUnregisterBytes` + `endpoints/unregister.ts`. The flow:
//!
//!   1. Build canonical bytes:
//!      LP(domain) || LP(user_id) || LP(timestamp_ms as ASCII int)
//!   2. Sign with the OLD identity's Ed25519 secret.
//!   3. POST `{ signature_b64, timestamp_ms }` to
//!      `DELETE /v1/pubkeys/:user_id` BEFORE wiping the local
//!      identity. Server verifies the signature against its stored
//!      pubkey + 5-minute freshness window, then cascades the delete
//!      across users / wrapped_keys / prekey_bundles / opk_pool.
//!
//! This is the recovery path that lets a burned user_id re-register
//! cleanly. Without it, `/v1/register` hits Case C (different
//! ed25519_pub, no rotation proof) and rejects forever.

use crate::identity::Identity;
use crypto::ed25519;

pub const UNREGISTER_DOMAIN: &[u8] = b"discord-privacy-client/unregister/v1";

/// Canonical bytes the unregister signature covers. MUST agree
/// byte-for-byte with `canonicalUnregisterBytes` in
/// `keyserver-cf/src/lib/canonical.ts`.
pub fn canonical_unregister_bytes(user_id: &str, timestamp_ms: i64) -> Vec<u8> {
    let mut buf = Vec::new();
    write_lp(&mut buf, UNREGISTER_DOMAIN);
    write_lp(&mut buf, user_id.as_bytes());
    write_lp(&mut buf, timestamp_ms.to_string().as_bytes());
    buf
}

/// Sign `canonical_unregister_bytes(user_id, timestamp_ms)` with the
/// identity's Ed25519 key.
pub fn sign_unregister(identity: &Identity, timestamp_ms: i64) -> ed25519::Signature {
    let bytes = canonical_unregister_bytes(&identity.user_id, timestamp_ms);
    ed25519::sign(&identity.ed25519_secret, &bytes)
}

fn write_lp(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    buf.extend_from_slice(bytes);
}
