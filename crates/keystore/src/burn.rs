//! Burn flow types + canonical signing bytes.
//!
//! Mirrors `keyserver/src/canonical.js` `canonicalBurnBytes` and
//! `keyserver/src/db.js` `burnWrappedKeys` — the user-facing Rust API
//! for "delete my wrapped-key blobs from the server, signed by my
//! identity key so nobody else can do it for me".
//!
//! ## Wire format (must match the server byte-for-byte)
//!
//!   domain (LP, "discord-privacy-client/burn/v1")
//!   user_id (LP)
//!   timestamp_ms decimal string (LP)
//!   256-bit base64url request_id (LP)
//!   scope_str (LP, one of "single" | "to_user" | "all")
//!   target_kind (u8: 0 = none, 1 = content_id, 2 = user_id)
//!   if target_kind != 0: target_value (LP)
//!
//! `LP` = u32-BE length prefix followed by raw UTF-8 bytes.
//!
//! ## Server-side filter is sender-only
//!
//! The server *always* filters `sender_id = burning_user_id`, so even
//! if Alice signs a burn naming Bob's content, only her own rows get
//! deleted (deleted_count = 0 for her). The signature gates *who*
//! requests the burn; the SQL filter gates *whose content* it can
//! affect.

use crate::identity::Identity;
use crypto::ed25519;

pub const BURN_DOMAIN: &[u8] = b"discord-privacy-client/burn/v1";

/// Burn scope. Matches the server's three-way `scope` field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BurnScope {
    /// Burn one specific message by `content_id`.
    Single { content_id: String },
    /// Burn every message the caller sent to a particular peer.
    ToUser { user_id: String },
    /// Burn every message the caller has ever sent.
    All,
}

impl BurnScope {
    pub fn label(&self) -> &'static str {
        match self {
            BurnScope::Single { .. } => "single",
            BurnScope::ToUser { .. } => "to_user",
            BurnScope::All => "all",
        }
    }
}

/// Canonical bytes the burn signature covers. MUST agree with
/// `canonicalBurnBytes` in `keyserver/src/canonical.js`.
pub fn canonical_burn_bytes(
    user_id: &str,
    timestamp_ms: i64,
    request_id: &str,
    scope: &BurnScope,
) -> Vec<u8> {
    let mut buf = Vec::new();
    write_lp(&mut buf, BURN_DOMAIN);
    write_lp(&mut buf, user_id.as_bytes());
    write_lp(&mut buf, timestamp_ms.to_string().as_bytes());
    write_lp(&mut buf, request_id.as_bytes());
    write_lp(&mut buf, scope.label().as_bytes());
    match scope {
        BurnScope::Single { content_id } => {
            buf.push(1);
            write_lp(&mut buf, content_id.as_bytes());
        }
        BurnScope::ToUser { user_id } => {
            buf.push(2);
            write_lp(&mut buf, user_id.as_bytes());
        }
        BurnScope::All => {
            buf.push(0);
        }
    }
    buf
}

/// Sign `canonical_burn_bytes(user_id, scope)` with the identity's
/// Ed25519 key.
pub fn sign_burn(
    identity: &Identity,
    timestamp_ms: i64,
    request_id: &str,
    scope: &BurnScope,
) -> ed25519::Signature {
    let bytes = canonical_burn_bytes(&identity.user_id, timestamp_ms, request_id, scope);
    ed25519::sign(&identity.ed25519_secret, &bytes)
}

fn write_lp(buf: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("canonical field exceeds u32 length");
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_bytes_single_layout() {
        let scope = BurnScope::Single {
            content_id: "abc".to_string(),
        };
        let bytes = canonical_burn_bytes("alice", 1_700_000_000_123, "request", &scope);

        // Reconstruct expected layout manually to lock the format.
        let mut want = Vec::new();
        want.extend_from_slice(&(BURN_DOMAIN.len() as u32).to_be_bytes());
        want.extend_from_slice(BURN_DOMAIN);
        want.extend_from_slice(&5u32.to_be_bytes());
        want.extend_from_slice(b"alice");
        want.extend_from_slice(&13u32.to_be_bytes());
        want.extend_from_slice(b"1700000000123");
        want.extend_from_slice(&7u32.to_be_bytes());
        want.extend_from_slice(b"request");
        want.extend_from_slice(&6u32.to_be_bytes());
        want.extend_from_slice(b"single");
        want.push(1);
        want.extend_from_slice(&3u32.to_be_bytes());
        want.extend_from_slice(b"abc");
        assert_eq!(bytes, want);
    }

    #[test]
    fn canonical_bytes_to_user_layout() {
        let bytes = canonical_burn_bytes(
            "alice",
            1_700_000_000_123,
            "request",
            &BurnScope::ToUser {
                user_id: "bob".to_string(),
            },
        );
        let prefix_user_id = &bytes[bytes.len() - 7..];
        // u32(3) "bob"
        assert_eq!(prefix_user_id, &[0, 0, 0, 3, b'b', b'o', b'b']);
        // target_kind byte sits just before the LP target.
        assert_eq!(bytes[bytes.len() - 8], 2);
    }

    #[test]
    fn canonical_bytes_all_has_zero_target_kind() {
        let bytes = canonical_burn_bytes("alice", 1_700_000_000_123, "request", &BurnScope::All);
        assert_eq!(*bytes.last().unwrap(), 0u8);
    }

    #[test]
    fn round_trip_signature_verifies() {
        use crate::generate_identity;
        let id = generate_identity("alice".to_string());
        let scope = BurnScope::All;
        let sig = sign_burn(&id, 1_700_000_000_123, "request", &scope);
        let bytes = canonical_burn_bytes(&id.user_id, 1_700_000_000_123, "request", &scope);
        assert!(ed25519::verify(&id.ed25519_public, &bytes, &sig).unwrap());
    }
}
