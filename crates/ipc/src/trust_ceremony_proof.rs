//! Assertion contract for a human trust ceremony.
//!
//! A key-change accept command must not treat a button click, UI state, or the
//! displayed value handed back to itself as proof. The caller has to present the
//! safety number the operator typed after comparing it out of band, and the
//! mutating command verifies it against the exact pending bundle before trust
//! state can move.

use core::fmt;

use serde::{Deserialize, Serialize};

/// The safety-number assertion supplied by a trusted local ceremony surface.
///
/// This value is still untrusted input until [`Self::verify_safety_number`]
/// succeeds against the pending bundle's expected number. It deliberately
/// carries the typed number, not a boolean such as `verified`, because a boolean
/// would let any caller manufacture consent without proving which key bundle
/// was compared.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustCeremonyProof {
    safety_number: String,
}

impl TrustCeremonyProof {
    pub fn new(safety_number: impl Into<String>) -> Self {
        Self {
            safety_number: safety_number.into(),
        }
    }

    pub fn safety_number(&self) -> &str {
        &self.safety_number
    }

    /// Verify the caller-supplied assertion against the expected safety number.
    ///
    /// Grouping characters are ignored so users can type the number with
    /// spaces, dashes, or line breaks. Empty, truncated, overlong, malformed, or
    /// mismatched inputs all refuse.
    pub fn verify_safety_number(&self, expected: &str) -> Result<(), TrustCeremonyProofError> {
        let expected = normalise_safety_number(expected);
        let supplied = normalise_safety_number(&self.safety_number);
        if supplied.is_empty() {
            return Err(TrustCeremonyProofError::NoSafetyNumberPresented);
        }
        if expected.len() != SAFETY_NUMBER_DIGITS || supplied.len() != SAFETY_NUMBER_DIGITS {
            return Err(TrustCeremonyProofError::SafetyNumberMismatch);
        }
        if constant_time_digits_match(&expected, &supplied) {
            Ok(())
        } else {
            Err(TrustCeremonyProofError::SafetyNumberMismatch)
        }
    }
}

/// Why a trust ceremony proof was refused.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TrustCeremonyProofError {
    NoSafetyNumberPresented,
    SafetyNumberMismatch,
}

impl fmt::Display for TrustCeremonyProofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSafetyNumberPresented => {
                write!(f, "OSL safety-number ceremony was not completed")
            }
            Self::SafetyNumberMismatch => write!(f, "OSL safety number does not match"),
        }
    }
}

impl fmt::Debug for TrustCeremonyProofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSafetyNumberPresented => {
                f.write_str("TrustCeremonyProofError::NoSafetyNumberPresented")
            }
            Self::SafetyNumberMismatch => {
                f.write_str("TrustCeremonyProofError::SafetyNumberMismatch")
            }
        }
    }
}

impl std::error::Error for TrustCeremonyProofError {}

/// Hand-written so the operator-entered number never appears in debug logs.
impl fmt::Debug for TrustCeremonyProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrustCeremonyProof")
            .field("safety_number", &"[REDACTED]")
            .finish()
    }
}

/// Hand-written for the same reason as `Debug`.
impl fmt::Display for TrustCeremonyProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TrustCeremonyProof([REDACTED])")
    }
}

const SAFETY_NUMBER_DIGITS: usize = 30;

fn normalise_safety_number(value: &str) -> String {
    value.chars().filter(char::is_ascii_digit).collect()
}

fn constant_time_digits_match(expected: &str, supplied: &str) -> bool {
    let comparison_value = |digits: &str| {
        let mut value = [0u8; 32];
        for (slot, digit) in value[..SAFETY_NUMBER_DIGITS].iter_mut().zip(digits.bytes()) {
            *slot = digit;
        }
        let length = u16::try_from(digits.len()).unwrap_or(u16::MAX);
        value[30..].copy_from_slice(&length.to_be_bytes());
        value
    };
    crate::revocation::ct_eq(&comparison_value(expected), &comparison_value(supplied))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_ceremony_proof() {
        let expected = "12345 67890 12345 67890 12345 67890";
        let grouped = TrustCeremonyProof::new("123-456\n789 012 345-678-901-234-567-890");
        grouped
            .verify_safety_number(expected)
            .expect("same digits with different grouping must verify");

        assert_eq!(
            TrustCeremonyProof::new("").verify_safety_number(expected),
            Err(TrustCeremonyProofError::NoSafetyNumberPresented)
        );
        assert_eq!(
            TrustCeremonyProof::new("12345 67890 12345 67890 12345 67891")
                .verify_safety_number(expected),
            Err(TrustCeremonyProofError::SafetyNumberMismatch)
        );
        assert_eq!(
            TrustCeremonyProof::new("12345 67890 12345 67890 12345").verify_safety_number(expected),
            Err(TrustCeremonyProofError::SafetyNumberMismatch)
        );
        assert_eq!(
            TrustCeremonyProof::new(expected).verify_safety_number(""),
            Err(TrustCeremonyProofError::SafetyNumberMismatch)
        );

        let proof = TrustCeremonyProof::new("99999 88888 77777 66666 55555 44444");
        let debug = format!("{proof:?}");
        let display = format!("{proof}");
        assert!(!debug.contains("99999"));
        assert!(!display.contains("99999"));
        assert!(debug.contains("TrustCeremonyProof"));
    }
}
