//! Admission of signed privacy receipts received through the control inbox.
//!
//! A receipt is classified from the outer v3 bundle at the inbox boundary,
//! then decrypted by the broker. This module authenticates its independent
//! identity signature and reduces it without permitting a later `Delivered`
//! frame to undo an earlier `Opened` frame.

use crypto::ed25519::PublicKey;
use ipc::receipt_wire::{
    PrivacyReceipt, PrivacyReceiptKind, ReceiptWireError, SignedPrivacyReceipt,
    MSG_TYPE_PRIVACY_RECEIPT,
};
use message_lifecycle::monotonic::{ReceiptKind, ReceiptMutation, ReceiptState};
use sha2::{Digest, Sha256};

const WIRE_VERSION_V3: u8 = 0x03;
const CONVERSATION_DOMAIN: &[u8] = b"OSL/privacy-receipt/conversation/v1";

/// A receipt envelope belongs to the inbox drain, not the legacy command
/// decrypt dispatcher. Authentication and decoding still happen before any
/// state changes.
pub fn is_receipt_bundle(bundle: &[u8]) -> bool {
    bundle.len() >= 2 && bundle[0] == WIRE_VERSION_V3 && bundle[1] == MSG_TYPE_PRIVACY_RECEIPT
}

/// Derive the opaque commitment signed into every receipt for this drain's
/// exact conversation. The provider conversation id never appears on-wire.
pub fn conversation_commitment(conversation_id: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(CONVERSATION_DOMAIN);
    hasher.update(conversation_id.as_bytes());
    hasher.finalize().into()
}

/// Decode, authenticate, bind, and monotonically reduce one receipt.
///
/// Callers must pass plaintext only after the v3 peer envelope was verified
/// and decrypted for the active manual-peer binding.
pub fn admit(
    plaintext: &[u8],
    signer: &PublicKey,
    expected_conversation: [u8; 32],
    state: &mut ReceiptState,
) -> Result<(PrivacyReceipt, ReceiptMutation), InboundReceiptError> {
    let signed = SignedPrivacyReceipt::decode_and_verify(plaintext, signer)
        .map_err(InboundReceiptError::InvalidFrame)?;
    if signed.receipt.conversation_commitment != expected_conversation {
        return Err(InboundReceiptError::WrongConversation);
    }
    let kind = match signed.receipt.kind {
        PrivacyReceiptKind::Delivered => ReceiptKind::Delivered,
        PrivacyReceiptKind::Opened => ReceiptKind::Opened,
    };
    Ok((signed.receipt, state.apply(kind)))
}

#[derive(Debug, Eq, PartialEq)]
pub enum InboundReceiptError {
    InvalidFrame(ReceiptWireError),
    WrongConversation,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto::ed25519;
    use ipc::receipt_wire::{PrivacyReceipt, PrivacyReceiptKind, SignedPrivacyReceipt};

    fn receipt(kind: PrivacyReceiptKind, commitment: [u8; 32]) -> PrivacyReceipt {
        PrivacyReceipt {
            kind,
            message_id: [7; 32],
            conversation_commitment: commitment,
            observed_at_unix_seconds: 1_780_000_000,
        }
    }

    #[test]
    fn tf_13_receipt_classified_at_the_inbox_drain_reaches_monotonic_reduction() {
        let (secret, public) = ed25519::generate_keypair();
        let commitment = conversation_commitment("active-conversation");
        let signed =
            SignedPrivacyReceipt::sign(receipt(PrivacyReceiptKind::Opened, commitment), &secret);
        let mut bundle = vec![WIRE_VERSION_V3, MSG_TYPE_PRIVACY_RECEIPT];
        bundle.extend_from_slice(&signed.encode());

        let mut state = ReceiptState::NotConfirmed;
        let plaintext = if is_receipt_bundle(&bundle) {
            &bundle[2..]
        } else {
            panic!("the inbox drain did not classify the privacy receipt")
        };
        let (_, mutation) = admit(plaintext, &public, commitment, &mut state).unwrap();

        assert_eq!(mutation, ReceiptMutation::Advanced);
        assert_eq!(state, ReceiptState::Opened);
    }

    #[test]
    fn receipt_for_another_conversation_cannot_change_this_rows_state() {
        let (secret, public) = ed25519::generate_keypair();
        let signed = SignedPrivacyReceipt::sign(
            receipt(
                PrivacyReceiptKind::Delivered,
                conversation_commitment("other"),
            ),
            &secret,
        );
        let mut state = ReceiptState::NotConfirmed;

        assert_eq!(
            admit(
                &signed.encode(),
                &public,
                conversation_commitment("active-conversation"),
                &mut state,
            ),
            Err(InboundReceiptError::WrongConversation)
        );
        assert_eq!(state, ReceiptState::NotConfirmed);
    }
}
