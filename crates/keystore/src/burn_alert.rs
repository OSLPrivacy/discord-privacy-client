//! Signed burn-alert system messages.
//!
//! When a sender opts into an alerted burn (per
//! `docs/design/key-server-api.md` § "Alert messages"), the client
//! uploads a regular wrapped-keys row with `content_type = "system"`
//! and `system_message_kind = "burn-alert"`. The wrapped blob carries
//! a signed payload so the recipient knows the alert wasn't forged
//! by the server.
//!
//! Note this module owns *only the signature layer* — the payload
//! envelope (encryption to recipient, ratchet wrapping) is built at
//! the higher integration layer that already handles regular text
//! messages. Here we just produce / verify the bytes the sender's
//! `IK_Ed25519` signs.
//!
//! ## Wire format
//!
//!   domain (LP, "discord-privacy-client/burn-alert/v1")
//!   sender_id (LP)
//!   recipient_id (LP)
//!   scope (LP, "single" | "to_user" | "all")
//!   alert_text (LP, UTF-8 — what the recipient renders)
//!   issued_at_unix_seconds (u64 BE)

use crate::burn::BurnScope;
use crate::identity::Identity;
use crypto::ed25519;

pub const BURN_ALERT_DOMAIN: &[u8] = b"discord-privacy-client/burn-alert/v1";

/// Signed burn-alert payload that goes inside the wrapped blob the
/// recipient retrieves from `/v1/wrapped-keys/:content_id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BurnAlertPayload {
    pub sender_id: String,
    pub recipient_id: String,
    pub scope_label: String,
    pub alert_text: String,
    pub issued_at_unix_seconds: u64,
}

impl BurnAlertPayload {
    /// Build from a [`BurnScope`] without consuming it; takes
    /// `recipient_id` separately because a `BurnScope::All` doesn't
    /// know the recipient on its own.
    pub fn from_scope(
        sender: &Identity,
        recipient_id: impl Into<String>,
        scope: &BurnScope,
        alert_text: impl Into<String>,
        issued_at_unix_seconds: u64,
    ) -> Self {
        BurnAlertPayload {
            sender_id: sender.user_id.clone(),
            recipient_id: recipient_id.into(),
            scope_label: scope.label().to_string(),
            alert_text: alert_text.into(),
            issued_at_unix_seconds,
        }
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        write_lp(&mut buf, BURN_ALERT_DOMAIN);
        write_lp(&mut buf, self.sender_id.as_bytes());
        write_lp(&mut buf, self.recipient_id.as_bytes());
        write_lp(&mut buf, self.scope_label.as_bytes());
        write_lp(&mut buf, self.alert_text.as_bytes());
        buf.extend_from_slice(&self.issued_at_unix_seconds.to_be_bytes());
        buf
    }
}

/// Sign a `BurnAlertPayload` with the sender's identity Ed25519 key.
pub fn sign_burn_alert(
    sender: &Identity,
    payload: &BurnAlertPayload,
) -> ed25519::Signature {
    let bytes = payload.canonical_bytes();
    ed25519::sign(&sender.ed25519_secret, &bytes)
}

/// Verify a burn-alert signature against the sender's Ed25519 public
/// key. Recipients call this after decrypting the wrapped blob to
/// confirm the alert came from the named sender.
pub fn verify_burn_alert(
    sender_public: &ed25519::PublicKey,
    payload: &BurnAlertPayload,
    signature: &ed25519::Signature,
) -> bool {
    let bytes = payload.canonical_bytes();
    ed25519::verify(sender_public, &bytes, signature).unwrap_or(false)
}

fn write_lp(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    buf.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate_identity;

    fn payload(now: u64) -> BurnAlertPayload {
        BurnAlertPayload {
            sender_id: "alice".to_string(),
            recipient_id: "bob".to_string(),
            scope_label: "to_user".to_string(),
            alert_text: "@alice cleared message history with you".to_string(),
            issued_at_unix_seconds: now,
        }
    }

    #[test]
    fn signature_round_trips() {
        let alice = generate_identity("alice".to_string());
        let p = payload(1_700_000_000);
        let sig = sign_burn_alert(&alice, &p);
        assert!(verify_burn_alert(&alice.ed25519_public, &p, &sig));
    }

    #[test]
    fn signature_rejects_payload_tamper() {
        let alice = generate_identity("alice".to_string());
        let p = payload(1_700_000_000);
        let sig = sign_burn_alert(&alice, &p);
        let mut tampered = p.clone();
        tampered.alert_text.push('!');
        assert!(!verify_burn_alert(&alice.ed25519_public, &tampered, &sig));
    }

    #[test]
    fn signature_rejects_wrong_signer() {
        let alice = generate_identity("alice".to_string());
        let mallory = generate_identity("mallory".to_string());
        let p = payload(1_700_000_000);
        let sig = sign_burn_alert(&mallory, &p);
        // Even if Mallory signs Alice's exact canonical bytes,
        // Alice's public key won't verify.
        assert!(!verify_burn_alert(&alice.ed25519_public, &p, &sig));
    }

    #[test]
    fn from_scope_picks_correct_label() {
        let alice = generate_identity("alice".to_string());
        let p = BurnAlertPayload::from_scope(
            &alice,
            "bob",
            &BurnScope::All,
            "wiped",
            123,
        );
        assert_eq!(p.scope_label, "all");
        assert_eq!(p.sender_id, "alice");
        assert_eq!(p.recipient_id, "bob");
    }
}
