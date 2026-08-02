//! Recipient-side construction and timing of privacy receipts.
//!
//! A `Delivered` fact is created only after the protected payload has
//! materialized.  The callback shape makes the ordering explicit at receive
//! sites: a failed materialization cannot reach the receipt transport, while a
//! successful one emits before the caller is given the plaintext to render.

use crypto::ed25519::SecretKey;
use ipc::receipt_wire::{PrivacyReceipt, PrivacyReceiptKind, SignedPrivacyReceipt};
use sha2::{Digest, Sha256};

const MESSAGE_DOMAIN: &[u8] = b"OSL/privacy-receipt/message/v1";

/// Make the fixed-width, opaque receipt identifier for one logical message.
///
/// The app's message identifiers are provider-facing strings, so they must not
/// be copied into the receipt frame.  Sender and recipient derive this same
/// commitment from the authenticated logical-message identifier.
pub fn message_commitment(message_id: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(MESSAGE_DOMAIN);
    hasher.update(message_id.as_bytes());
    hasher.finalize().into()
}

/// Construct the recipient-signed `Delivered` fact for a materialized message.
pub fn sign_delivered(
    message_id: &str,
    conversation_commitment: [u8; 32],
    observed_at_unix_seconds: u64,
    signer: &SecretKey,
) -> SignedPrivacyReceipt {
    SignedPrivacyReceipt::sign(
        PrivacyReceipt {
            kind: PrivacyReceiptKind::Delivered,
            message_id: message_commitment(message_id),
            conversation_commitment,
            observed_at_unix_seconds,
        },
        signer,
    )
}

/// Materialize a payload, then emit its receipt before returning it to a
/// renderer.  If materialization fails, `emit` is never called.
pub fn materialize_then_emit<T, E>(
    materialize: impl FnOnce() -> Result<T, E>,
    emit: impl FnOnce(&T) -> Result<(), E>,
) -> Result<T, E> {
    let payload = materialize()?;
    emit(&payload)?;
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto::ed25519;
    use std::cell::RefCell;

    #[test]
    fn tf_14_delivered_is_signed_after_materialization_and_before_render() {
        let (secret, public) = ed25519::generate_keypair();
        let events = RefCell::new(Vec::new());
        let signed = materialize_then_emit(
            || {
                events.borrow_mut().push("materialized");
                Ok::<_, ()>(b"plaintext".to_vec())
            },
            |_| {
                events.borrow_mut().push("delivered");
                Ok(())
            },
        )
        .expect("the authenticated payload materializes");
        events.borrow_mut().push("rendered");

        let receipt = sign_delivered("logical-message", [4; 32], 1_780_000_000, &secret);
        let verified = SignedPrivacyReceipt::decode_and_verify(&receipt.encode(), &public)
            .expect("Delivered is signed by the recipient identity");

        assert_eq!(signed, b"plaintext");
        assert_eq!(verified.receipt.kind, PrivacyReceiptKind::Delivered);
        assert_eq!(
            events.into_inner(),
            ["materialized", "delivered", "rendered"]
        );
    }

    #[test]
    fn tf_14_undecryptable_payload_never_emits_delivered() {
        let emitted = RefCell::new(false);
        let result = materialize_then_emit(
            || Err::<Vec<u8>, _>(()),
            |_| {
                *emitted.borrow_mut() = true;
                Ok(())
            },
        );

        assert_eq!(result, Err(()));
        assert!(!*emitted.borrow());
    }
}
