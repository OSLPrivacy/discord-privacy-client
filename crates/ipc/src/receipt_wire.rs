//! Signed, content-free privacy-receipt frames.
//!
//! `Delivered` means this device materialized the payload; `Opened` means it
//! began rendering it.  These are assertions by the recipient's device, so a
//! sender must authenticate them with the recipient identity key before
//! recording either fact.  The frame deliberately has no plaintext, pointer,
//! account name, or chat title.

use crypto::ed25519;
use thiserror::Error;

/// Outer message-type byte for a privacy receipt. `0x0A`/`0x0B` are already
/// assigned to bilateral-burn revocation frames, so receipts take the next
/// free slot.
pub const MSG_TYPE_PRIVACY_RECEIPT: u8 = 0x0C;

const FRAME_VERSION: u8 = 1;
const DELIVERED_KIND: u8 = 1;
const OPENED_KIND: u8 = 2;
const FRAME_PREFIX_LEN: usize = 1 + 1 + 1 + 32 + 32 + 8;
/// Exact byte length of an encoded [`SignedPrivacyReceipt`].
pub const PRIVACY_RECEIPT_FRAME_LEN: usize = FRAME_PREFIX_LEN + ed25519::SIGNATURE_SIZE;
const SIGNING_DOMAIN: &[u8] = b"OSL/privacy-receipt/v1";

/// A recipient-controlled privacy receipt state that may cross the wire.
///
/// This intentionally does not reuse the local `ReceiptStatus`: the local
/// state machine has states that must never be reported to a peer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivacyReceiptKind {
    Delivered,
    Opened,
}

impl PrivacyReceiptKind {
    fn to_wire(self) -> u8 {
        match self {
            Self::Delivered => DELIVERED_KIND,
            Self::Opened => OPENED_KIND,
        }
    }

    fn from_wire(value: u8) -> Result<Self, ReceiptWireError> {
        match value {
            DELIVERED_KIND => Ok(Self::Delivered),
            OPENED_KIND => Ok(Self::Opened),
            other => Err(ReceiptWireError::UnknownKind(other)),
        }
    }
}

/// The signed receipt fact. `conversation_commitment` is an opaque,
/// caller-derived commitment that binds a receipt to the intended
/// conversation without putting its name or identifier on the wire.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrivacyReceipt {
    pub kind: PrivacyReceiptKind,
    pub message_id: [u8; 32],
    pub conversation_commitment: [u8; 32],
    pub observed_at_unix_seconds: u64,
}

/// A [`PrivacyReceipt`] authenticated by the reporting device identity key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignedPrivacyReceipt {
    pub receipt: PrivacyReceipt,
    pub signature: [u8; ed25519::SIGNATURE_SIZE],
}

impl SignedPrivacyReceipt {
    /// Sign a receipt with the local recipient identity key.
    pub fn sign(receipt: PrivacyReceipt, signer: &ed25519::SecretKey) -> Self {
        let signature = ed25519::sign(signer, &signing_bytes(&receipt));
        Self {
            receipt,
            signature: *signature.as_bytes(),
        }
    }

    /// Encode a fixed-width frame beginning with its outer message type.
    pub fn encode(&self) -> [u8; PRIVACY_RECEIPT_FRAME_LEN] {
        let mut frame = unsigned_frame_bytes(&self.receipt);
        frame[FRAME_PREFIX_LEN..].copy_from_slice(&self.signature);
        frame
    }

    /// Decode and authenticate a receipt against the recipient's pinned
    /// identity signing key.  No parsed field is returned before verification.
    pub fn decode_and_verify(
        frame: &[u8],
        signer: &ed25519::PublicKey,
    ) -> Result<Self, ReceiptWireError> {
        if frame.len() != PRIVACY_RECEIPT_FRAME_LEN {
            return Err(ReceiptWireError::InvalidLength {
                got: frame.len(),
                expected: PRIVACY_RECEIPT_FRAME_LEN,
            });
        }
        if frame[0] != MSG_TYPE_PRIVACY_RECEIPT {
            return Err(ReceiptWireError::WrongMessageType(frame[0]));
        }
        if frame[1] != FRAME_VERSION {
            return Err(ReceiptWireError::UnsupportedVersion(frame[1]));
        }

        let kind = PrivacyReceiptKind::from_wire(frame[2])?;
        let mut message_id = [0u8; 32];
        message_id.copy_from_slice(&frame[3..35]);
        let mut conversation_commitment = [0u8; 32];
        conversation_commitment.copy_from_slice(&frame[35..67]);
        let mut observed_at = [0u8; 8];
        observed_at.copy_from_slice(&frame[67..75]);
        let receipt = PrivacyReceipt {
            kind,
            message_id,
            conversation_commitment,
            observed_at_unix_seconds: u64::from_be_bytes(observed_at),
        };
        let mut signature = [0u8; ed25519::SIGNATURE_SIZE];
        signature.copy_from_slice(&frame[FRAME_PREFIX_LEN..]);
        if signature.iter().all(|byte| *byte == 0) {
            return Err(ReceiptWireError::Unsigned);
        }

        let signature = ed25519::Signature::from_bytes(signature);
        let valid = ed25519::verify(signer, &signing_bytes(&receipt), &signature)
            .map_err(|_| ReceiptWireError::InvalidSignature)?;
        if !valid {
            return Err(ReceiptWireError::InvalidSignature);
        }
        Ok(Self {
            receipt,
            signature: *signature.as_bytes(),
        })
    }
}

fn unsigned_frame_bytes(receipt: &PrivacyReceipt) -> [u8; PRIVACY_RECEIPT_FRAME_LEN] {
    let mut frame = [0u8; PRIVACY_RECEIPT_FRAME_LEN];
    frame[0] = MSG_TYPE_PRIVACY_RECEIPT;
    frame[1] = FRAME_VERSION;
    frame[2] = receipt.kind.to_wire();
    frame[3..35].copy_from_slice(&receipt.message_id);
    frame[35..67].copy_from_slice(&receipt.conversation_commitment);
    frame[67..75].copy_from_slice(&receipt.observed_at_unix_seconds.to_be_bytes());
    frame
}

fn signing_bytes(receipt: &PrivacyReceipt) -> Vec<u8> {
    let unsigned = unsigned_frame_bytes(receipt);
    let mut bytes = Vec::with_capacity(SIGNING_DOMAIN.len() + FRAME_PREFIX_LEN);
    bytes.extend_from_slice(SIGNING_DOMAIN);
    bytes.extend_from_slice(&unsigned[..FRAME_PREFIX_LEN]);
    bytes
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ReceiptWireError {
    #[error("receipt frame length is {got}, expected {expected}")]
    InvalidLength { got: usize, expected: usize },
    #[error("receipt frame has message type {0:#04x}, expected privacy receipt")]
    WrongMessageType(u8),
    #[error("receipt frame version {0} is unsupported")]
    UnsupportedVersion(u8),
    #[error("receipt frame kind {0} is unknown")]
    UnknownKind(u8),
    #[error("receipt frame is unsigned")]
    Unsigned,
    #[error("receipt frame signature is invalid")]
    InvalidSignature,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt(kind: PrivacyReceiptKind) -> PrivacyReceipt {
        PrivacyReceipt {
            kind,
            message_id: [0x11; 32],
            conversation_commitment: [0x22; 32],
            observed_at_unix_seconds: 1_780_000_000,
        }
    }

    #[test]
    fn tf_11_signed_delivered_and_opened_frames_round_trip() {
        let (secret, public) = ed25519::generate_keypair();
        for kind in [PrivacyReceiptKind::Delivered, PrivacyReceiptKind::Opened] {
            let signed = SignedPrivacyReceipt::sign(receipt(kind), &secret);
            assert_eq!(
                SignedPrivacyReceipt::decode_and_verify(&signed.encode(), &public).unwrap(),
                signed
            );
        }
    }

    #[test]
    fn tf_11_rejects_an_unsigned_or_tampered_receipt() {
        let (secret, public) = ed25519::generate_keypair();
        let signed = SignedPrivacyReceipt::sign(receipt(PrivacyReceiptKind::Delivered), &secret);

        let mut unsigned = signed.encode();
        unsigned[FRAME_PREFIX_LEN..].fill(0);
        assert_eq!(
            SignedPrivacyReceipt::decode_and_verify(&unsigned, &public),
            Err(ReceiptWireError::Unsigned)
        );

        let mut tampered = signed.encode();
        tampered[3] ^= 1;
        assert_eq!(
            SignedPrivacyReceipt::decode_and_verify(&tampered, &public),
            Err(ReceiptWireError::InvalidSignature)
        );
    }
}
