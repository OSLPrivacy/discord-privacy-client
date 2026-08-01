//! Per-message expiry metadata for an encrypted payload.
//!
//! This is deliberately independent of a conversation scope. The caller
//! chooses a value while composing one message, encodes this envelope, and
//! encrypts the resulting bytes. The relay consequently receives neither the
//! exact lifetime nor a scope-level expiry preference.

/// A requested expiry starts at the recipient's first render, in seconds.
///
/// `None` means the sender chose no expiry. The exact value is carried inside
/// [`SealedMessageExpiryPayload`] and is not a server retention TTL.
pub type ViewLifetime = Option<u32>;

/// The smallest lifetime the product offers: one second.
pub const MIN_VIEW_LIFETIME_SECONDS: u32 = 1;
/// The longest lifetime the product offers: thirty days.
pub const MAX_VIEW_LIFETIME_SECONDS: u32 = 30 * 24 * 60 * 60;

const FORMAT_VERSION: u8 = 1;
const HEADER_BYTES: usize = 6;

/// Plaintext that must be encrypted as one unit by the caller.
///
/// The binary representation begins with a version, a presence flag, and the
/// exact `view_lifetime` in big-endian seconds. The remaining bytes are the
/// application payload. Keeping the metadata in this plaintext envelope makes
/// the message lifetime part of the authenticated sealed payload rather than
/// relay-visible upload metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SealedMessageExpiryPayload {
    /// Exact per-message expiry chosen at send time, or no expiry.
    pub view_lifetime: ViewLifetime,
    /// The message plaintext to seal alongside the lifetime.
    pub payload: Vec<u8>,
}

impl SealedMessageExpiryPayload {
    /// Select an expiry for one message. No scope or saved preference is read.
    pub fn new(view_lifetime: ViewLifetime, payload: Vec<u8>) -> Result<Self, String> {
        validate_view_lifetime(view_lifetime)?;
        Ok(Self {
            view_lifetime,
            payload,
        })
    }

    /// Encode the envelope bytes that the caller must encrypt.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_BYTES + self.payload.len());
        out.push(FORMAT_VERSION);
        match self.view_lifetime {
            Some(seconds) => {
                out.push(1);
                out.extend_from_slice(&seconds.to_be_bytes());
            }
            None => {
                out.push(0);
                out.extend_from_slice(&0u32.to_be_bytes());
            }
        }
        out.extend_from_slice(&self.payload);
        out
    }

    /// Recover per-message expiry metadata after decrypting the envelope.
    pub fn decode(sealed_plaintext: &[u8]) -> Result<Self, String> {
        if sealed_plaintext.len() < HEADER_BYTES || sealed_plaintext[0] != FORMAT_VERSION {
            return Err("OSL message expiry payload is malformed".to_owned());
        }
        let seconds = u32::from_be_bytes(
            sealed_plaintext[2..HEADER_BYTES]
                .try_into()
                .map_err(|_| "OSL message expiry payload is malformed")?,
        );
        let view_lifetime = match sealed_plaintext[1] {
            0 if seconds == 0 => None,
            1 => Some(seconds),
            _ => return Err("OSL message expiry payload is malformed".to_owned()),
        };
        Self::new(view_lifetime, sealed_plaintext[HEADER_BYTES..].to_vec())
    }
}

/// Reject an unhonourable choice; never silently rewrite it to another value.
pub fn validate_view_lifetime(view_lifetime: ViewLifetime) -> Result<(), String> {
    match view_lifetime {
        None | Some(MIN_VIEW_LIFETIME_SECONDS..=MAX_VIEW_LIFETIME_SECONDS) => Ok(()),
        Some(_) => Err("OSL message expiry must be between 1 second and 30 days".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_messages_in_one_scope_can_carry_different_exact_expiries() {
        // The same caller/scope may encode two independent messages. Their
        // envelope bytes, and the values the receiver recovers after decrypt,
        // must retain each send-time choice instead of consulting a shared dial.
        let short = SealedMessageExpiryPayload::new(Some(5), b"same scope".to_vec()).unwrap();
        let long = SealedMessageExpiryPayload::new(Some(86_400), b"same scope".to_vec()).unwrap();

        assert_ne!(short.encode(), long.encode());
        assert_eq!(
            SealedMessageExpiryPayload::decode(&short.encode())
                .unwrap()
                .view_lifetime,
            Some(5)
        );
        assert_eq!(
            SealedMessageExpiryPayload::decode(&long.encode())
                .unwrap()
                .view_lifetime,
            Some(86_400)
        );
    }

    #[test]
    fn no_expiry_and_the_full_owner_range_round_trip_exactly() {
        for lifetime in [
            None,
            Some(MIN_VIEW_LIFETIME_SECONDS),
            Some(MAX_VIEW_LIFETIME_SECONDS),
        ] {
            let message = SealedMessageExpiryPayload::new(lifetime, b"payload".to_vec()).unwrap();
            assert_eq!(
                SealedMessageExpiryPayload::decode(&message.encode()).unwrap(),
                message
            );
        }
    }

    #[test]
    fn unsupported_values_are_rejected_not_clamped() {
        for lifetime in [Some(0), Some(MAX_VIEW_LIFETIME_SECONDS + 1)] {
            assert!(SealedMessageExpiryPayload::new(lifetime, Vec::new()).is_err());
        }
    }
}
