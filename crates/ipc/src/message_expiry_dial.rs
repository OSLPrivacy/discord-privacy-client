//! Per-message expiry metadata for an encrypted payload.
//!
//! This is deliberately independent of a conversation scope. The caller
//! chooses a value while composing one message, encodes this envelope, and
//! encrypts the resulting bytes. The relay consequently receives neither the
//! exact lifetime nor a scope-level expiry preference.

use serde::{Deserialize, Serialize};

/// A requested expiry starts at the recipient's first render, in seconds.
///
/// `None` means the sender chose no expiry. The exact value is carried inside
/// [`SealedMessageExpiryPayload`] and is not a server retention TTL.
pub type ViewLifetime = Option<u32>;

/// The smallest lifetime the product offers: one second.
pub const MIN_VIEW_LIFETIME_SECONDS: u32 = 1;
/// The longest lifetime the product offers: thirty days.
pub const MAX_VIEW_LIFETIME_SECONDS: u32 = 30 * 24 * 60 * 60;

const DAY_SECONDS: u64 = 24 * 60 * 60;
const HOUR_SECONDS: u64 = 60 * 60;
const MINUTE_SECONDS: u64 = 60;

const FORMAT_VERSION: u8 = 1;
const HEADER_BYTES: usize = 6;

/// Direct timer fields supplied by the composer.
///
/// `Default` is intentionally all zeroes, including `days = 0`: a blank timer
/// means no per-message expiry. Non-zero timers are validated and converted to
/// the exact seconds value carried inside the sealed payload.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MessageTimer {
    #[serde(default)]
    pub days: u32,
    #[serde(default)]
    pub hours: u32,
    #[serde(default)]
    pub minutes: u32,
    #[serde(default)]
    pub seconds: u32,
}

impl MessageTimer {
    /// Convert the direct timer fields into the sealed view lifetime.
    ///
    /// Hours, minutes and seconds use their normal clock ranges so that each
    /// direct field has one meaning. The total lifetime remains the final
    /// policy boundary: zero means no expiry, and non-zero values may not exceed
    /// thirty days.
    pub fn view_lifetime(self) -> Result<ViewLifetime, String> {
        if self.hours >= 24 || self.minutes >= 60 || self.seconds >= 60 {
            return Err("OSL message expiry must be between 1 second and 30 days".to_owned());
        }
        let total = u64::from(self.days)
            .saturating_mul(DAY_SECONDS)
            .saturating_add(u64::from(self.hours).saturating_mul(HOUR_SECONDS))
            .saturating_add(u64::from(self.minutes).saturating_mul(MINUTE_SECONDS))
            .saturating_add(u64::from(self.seconds));
        if total == 0 {
            return Ok(None);
        }
        let seconds = u32::try_from(total)
            .map_err(|_| "OSL message expiry must be between 1 second and 30 days".to_owned())?;
        validate_view_lifetime(Some(seconds))?;
        Ok(Some(seconds))
    }
}

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

    /// Select an expiry from direct day/hour/minute/second timer fields.
    pub fn new_with_timer(timer: MessageTimer, payload: Vec<u8>) -> Result<Self, String> {
        Self::new(timer.view_lifetime()?, payload)
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

    #[test]
    fn direct_timer_validation_accepts_30_days_and_rejects_30_days_plus_one_second() {
        let default_timer = MessageTimer::default();
        let missing_days_timer: MessageTimer =
            serde_json::from_str(r#"{"hours":1,"minutes":2,"seconds":3}"#).unwrap();
        let accepted_timer = MessageTimer {
            days: 30,
            ..MessageTimer::default()
        };
        let rejected_timer = MessageTimer {
            days: 30,
            seconds: 1,
            ..MessageTimer::default()
        };

        let accepted = accepted_timer.view_lifetime().unwrap();
        let rejected = rejected_timer.view_lifetime();
        let rejected_seconds = u64::from(MAX_VIEW_LIFETIME_SECONDS) + 1;

        println!(
            "TASK1341 direct_timer_validation default_days={} omitted_days_default={} accepted_days={} accepted_seconds={} rejected_seconds={} rejected={}",
            default_timer.days,
            missing_days_timer.days,
            accepted_timer.days,
            accepted.unwrap(),
            rejected_seconds,
            rejected.is_err()
        );

        assert_eq!(default_timer.days, 0);
        assert_eq!(missing_days_timer.days, 0);
        assert_eq!(missing_days_timer.view_lifetime(), Ok(Some(3_723)));
        assert_eq!(accepted, Some(MAX_VIEW_LIFETIME_SECONDS));
        assert_eq!(rejected_seconds, 2_592_001);
        assert!(rejected.is_err());
    }
}
