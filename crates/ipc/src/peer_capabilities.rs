//! Declared peer support for destructive message features.
//!
//! The declaration belongs to an authenticated peer handshake beside the build
//! hash.  A transport acknowledgement, a missing error, or any other observed
//! behaviour is intentionally not an input to this module: old peers can be
//! silent, and silence must never be treated as feature support.

use serde::{Deserialize, Serialize};

const VIEW_ONCE_BIT: u8 = 1 << 0;
const EXPIRY_BIT: u8 = 1 << 1;
const RECEIPTS_BIT: u8 = 1 << 2;
const KNOWN_FEATURE_BITS: u8 = VIEW_ONCE_BIT | EXPIRY_BIT | RECEIPTS_BIT;

/// A destructive feature whose client-side enforcement a peer explicitly
/// declares in its authenticated handshake.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerFeature {
    ViewOnce,
    Expiry,
    Receipts,
}

impl PeerFeature {
    const fn bit(self) -> u8 {
        match self {
            Self::ViewOnce => VIEW_ONCE_BIT,
            Self::Expiry => EXPIRY_BIT,
            Self::Receipts => RECEIPTS_BIT,
        }
    }
}

/// A compact, explicit declaration carried by an authenticated peer handshake.
///
/// `build_hash` is retained with the declaration so callers cannot detach a
/// capability claim from the build that made it.  The handshake's signature and
/// peer-identity verification are performed by its owner before this value is
/// accepted here.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PeerHandshakeDeclaration {
    build_hash: String,
    feature_bits: u8,
}

impl PeerHandshakeDeclaration {
    /// Construct a declaration only when its build hash and bitmap are
    /// canonical.  Unknown bits are rejected rather than guessed at.
    pub fn new(build_hash: impl Into<String>, feature_bits: u8) -> Result<Self, CapabilityError> {
        let build_hash = build_hash.into();
        let declaration = Self {
            build_hash,
            feature_bits,
        };
        declaration.validate()?;
        Ok(declaration)
    }

    fn validate(&self) -> Result<(), CapabilityError> {
        if !canonical_build_hash(&self.build_hash) {
            return Err(CapabilityError::InvalidBuildHash);
        }
        if self.feature_bits & !KNOWN_FEATURE_BITS != 0 {
            return Err(CapabilityError::UnknownFeatureBits);
        }
        Ok(())
    }
}

/// Why a proposed handshake declaration cannot be accepted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityError {
    InvalidBuildHash,
    UnknownFeatureBits,
}

/// The only capability state compose-time policy may use for one peer.
///
/// `None` from [`Self::from_authenticated_handshake`] is an old or silent peer
/// and resolves to this empty set.  This makes the safe answer the default.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PeerCapabilities(u8);

impl PeerCapabilities {
    /// Consume an already-authenticated handshake declaration.  Missing
    /// declarations grant no support; this API deliberately cannot observe a
    /// successful send or a lack of peer error.
    pub fn from_authenticated_handshake(declaration: Option<&PeerHandshakeDeclaration>) -> Self {
        Self(
            declaration
                .and_then(|value| value.validate().ok().map(|()| value.feature_bits))
                .unwrap_or(0),
        )
    }

    /// Whether this peer explicitly declared support for `feature`.
    pub const fn supports(self, feature: PeerFeature) -> bool {
        self.0 & feature.bit() != 0
    }
}

fn canonical_build_hash(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tf_04_silent_old_peer_declares_no_destructive_feature_support() {
        let capabilities = PeerCapabilities::from_authenticated_handshake(None);

        assert!(!capabilities.supports(PeerFeature::ViewOnce));
        assert!(!capabilities.supports(PeerFeature::Expiry));
        assert!(!capabilities.supports(PeerFeature::Receipts));
    }

    #[test]
    fn declared_features_are_available_only_from_the_handshake_declaration() {
        let declaration =
            PeerHandshakeDeclaration::new("a".repeat(40), VIEW_ONCE_BIT | RECEIPTS_BIT)
                .expect("canonical declaration");

        let capabilities = PeerCapabilities::from_authenticated_handshake(Some(&declaration));
        assert!(capabilities.supports(PeerFeature::ViewOnce));
        assert!(!capabilities.supports(PeerFeature::Expiry));
        assert!(capabilities.supports(PeerFeature::Receipts));
    }

    #[test]
    fn malformed_declarations_do_not_create_a_capability_claim() {
        assert_eq!(
            PeerHandshakeDeclaration::new("not-a-build", VIEW_ONCE_BIT),
            Err(CapabilityError::InvalidBuildHash)
        );
        assert_eq!(
            PeerHandshakeDeclaration::new("b".repeat(64), KNOWN_FEATURE_BITS | (1 << 7)),
            Err(CapabilityError::UnknownFeatureBits)
        );
    }
}
