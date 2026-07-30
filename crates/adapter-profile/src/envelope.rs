//! Version-1 signed adapter-profile envelope.
//!
//! The envelope binds metadata, signer identity, and the profile body digest to
//! a single domain-separated canonical byte string. It deliberately does not
//! decide whether the signer is trusted or whether the profile may be used.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use thiserror::Error;

pub const ADAPTER_PROFILE_ENVELOPE_VERSION: u32 = 1;
pub const ADAPTER_PROFILE_ENVELOPE_DOMAIN: &[u8] = b"osl/adapter-profile/envelope/v1";
pub const MAX_PROFILE_BYTES: usize = 256 * 1024;
pub const MAX_PROFILE_ID_BYTES: usize = 128;
pub const MAX_SIGNER_KEY_ID_BYTES: usize = 128;
pub const ED25519_PUBLIC_KEY_LEN: usize = 32;
pub const ED25519_SIGNATURE_LEN: usize = 64;
pub const SHA256_DIGEST_LEN: usize = 32;

/// A signed adapter-profile envelope.
///
/// Construction validates shape, freshness ordering, fixed-size cryptographic
/// fields, and the body digest. Signature verification is implemented by a
/// later unit and must be performed before callers parse or act on the profile.
#[derive(Clone, PartialEq, Eq)]
pub struct SignedProfile {
    envelope_version: u32,
    profile_schema_version: u32,
    profile_id: String,
    revision: u64,
    issued_at_unix_seconds: u64,
    expires_at_unix_seconds: u64,
    profile_bytes: Vec<u8>,
    profile_sha256: [u8; SHA256_DIGEST_LEN],
    signer_key_id: String,
    signer_public_key: [u8; ED25519_PUBLIC_KEY_LEN],
    signature: [u8; ED25519_SIGNATURE_LEN],
}

impl SignedProfile {
    pub fn new(
        profile_schema_version: u32,
        profile_id: impl Into<String>,
        revision: u64,
        issued_at_unix_seconds: u64,
        expires_at_unix_seconds: u64,
        profile_bytes: impl Into<Vec<u8>>,
        signer_key_id: impl Into<String>,
        signer_public_key: [u8; ED25519_PUBLIC_KEY_LEN],
        signature: [u8; ED25519_SIGNATURE_LEN],
    ) -> Result<Self, EnvelopeError> {
        let profile_bytes = profile_bytes.into();
        let profile_sha256 = canonical_profile_digest(&profile_bytes);
        Self::from_parts(
            ADAPTER_PROFILE_ENVELOPE_VERSION,
            profile_schema_version,
            profile_id.into(),
            revision,
            issued_at_unix_seconds,
            expires_at_unix_seconds,
            profile_bytes,
            profile_sha256,
            signer_key_id.into(),
            signer_public_key,
            signature,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn from_parts(
        envelope_version: u32,
        profile_schema_version: u32,
        profile_id: String,
        revision: u64,
        issued_at_unix_seconds: u64,
        expires_at_unix_seconds: u64,
        profile_bytes: Vec<u8>,
        profile_sha256: [u8; SHA256_DIGEST_LEN],
        signer_key_id: String,
        signer_public_key: [u8; ED25519_PUBLIC_KEY_LEN],
        signature: [u8; ED25519_SIGNATURE_LEN],
    ) -> Result<Self, EnvelopeError> {
        let value = Self {
            envelope_version,
            profile_schema_version,
            profile_id,
            revision,
            issued_at_unix_seconds,
            expires_at_unix_seconds,
            profile_bytes,
            profile_sha256,
            signer_key_id,
            signer_public_key,
            signature,
        };
        value.validate()?;
        Ok(value)
    }

    /// Parse the public JSON wire envelope and validate it before returning.
    pub fn from_wire_json(json: &[u8]) -> Result<Self, EnvelopeError> {
        let raw: RawSignedProfile =
            serde_json::from_slice(json).map_err(|source| EnvelopeError::Json { source })?;
        let profile_bytes = decode_b64("profile_b64", &raw.profile_b64)?;
        let digest =
            decode_fixed_b64::<SHA256_DIGEST_LEN>("profile_sha256_b64", &raw.profile_sha256_b64)?;
        let signer_public_key = decode_fixed_b64::<ED25519_PUBLIC_KEY_LEN>(
            "signer_public_key_b64",
            &raw.signer_public_key_b64,
        )?;
        let signature =
            decode_fixed_b64::<ED25519_SIGNATURE_LEN>("signature_b64", &raw.signature_b64)?;
        Self::from_parts(
            raw.envelope_version,
            raw.profile_schema_version,
            raw.profile_id,
            raw.revision,
            raw.issued_at_unix_seconds,
            raw.expires_at_unix_seconds,
            profile_bytes,
            digest,
            raw.signer_key_id,
            signer_public_key,
            signature,
        )
    }

    /// Serialize to the stable public JSON wire envelope.
    pub fn to_wire_json(&self) -> Result<Vec<u8>, EnvelopeError> {
        self.validate()?;
        let raw = RawSignedProfile {
            envelope_version: self.envelope_version,
            profile_schema_version: self.profile_schema_version,
            profile_id: self.profile_id.clone(),
            revision: self.revision,
            issued_at_unix_seconds: self.issued_at_unix_seconds,
            expires_at_unix_seconds: self.expires_at_unix_seconds,
            profile_b64: STANDARD.encode(&self.profile_bytes),
            profile_sha256_b64: STANDARD.encode(self.profile_sha256),
            signer_key_id: self.signer_key_id.clone(),
            signer_public_key_b64: STANDARD.encode(self.signer_public_key),
            signature_b64: STANDARD.encode(self.signature),
        };
        serde_json::to_vec(&raw).map_err(|source| EnvelopeError::Json { source })
    }

    /// Validate the envelope shape. This is a structural check, not a trust
    /// check.
    pub fn validate(&self) -> Result<(), EnvelopeError> {
        if self.envelope_version != ADAPTER_PROFILE_ENVELOPE_VERSION {
            return Err(EnvelopeError::EnvelopeVersion {
                got: self.envelope_version,
                expected: ADAPTER_PROFILE_ENVELOPE_VERSION,
            });
        }
        if self.profile_schema_version == 0 {
            return Err(EnvelopeError::ZeroSchemaVersion);
        }
        validate_identifier(
            "profile_id",
            &self.profile_id,
            MAX_PROFILE_ID_BYTES,
            IdentifierKind::Profile,
        )?;
        if self.revision == 0 {
            return Err(EnvelopeError::ZeroRevision);
        }
        if self.issued_at_unix_seconds >= self.expires_at_unix_seconds {
            return Err(EnvelopeError::ValidityWindow);
        }
        if self.profile_bytes.is_empty() {
            return Err(EnvelopeError::ProfileBodyEmpty);
        }
        if self.profile_bytes.len() > MAX_PROFILE_BYTES {
            return Err(EnvelopeError::ProfileBodyTooLarge {
                got: self.profile_bytes.len(),
                max: MAX_PROFILE_BYTES,
            });
        }
        validate_identifier(
            "signer_key_id",
            &self.signer_key_id,
            MAX_SIGNER_KEY_ID_BYTES,
            IdentifierKind::SignerKey,
        )?;
        if self.signer_public_key.iter().all(|byte| *byte == 0) {
            return Err(EnvelopeError::SignerPublicKeyEmpty);
        }
        if self.signature.iter().all(|byte| *byte == 0) {
            return Err(EnvelopeError::SignatureEmpty);
        }
        let digest = canonical_profile_digest(&self.profile_bytes);
        if self.profile_sha256 != digest {
            return Err(EnvelopeError::DigestMismatch);
        }
        Ok(())
    }

    /// Canonical bytes covered by the profile signature.
    pub fn signing_payload(&self) -> Result<Vec<u8>, EnvelopeError> {
        self.validate()?;
        let mut out = Vec::new();
        write_lp(&mut out, ADAPTER_PROFILE_ENVELOPE_DOMAIN)?;
        out.extend_from_slice(&self.envelope_version.to_be_bytes());
        out.extend_from_slice(&self.profile_schema_version.to_be_bytes());
        write_lp(&mut out, self.profile_id.as_bytes())?;
        out.extend_from_slice(&self.revision.to_be_bytes());
        out.extend_from_slice(&self.issued_at_unix_seconds.to_be_bytes());
        out.extend_from_slice(&self.expires_at_unix_seconds.to_be_bytes());
        write_lp(&mut out, self.signer_key_id.as_bytes())?;
        write_lp(&mut out, &self.signer_public_key)?;
        out.extend_from_slice(&(self.profile_bytes.len() as u64).to_be_bytes());
        write_lp(&mut out, &self.profile_sha256)?;
        Ok(out)
    }

    pub fn envelope_version(&self) -> u32 {
        self.envelope_version
    }

    pub fn profile_schema_version(&self) -> u32 {
        self.profile_schema_version
    }

    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn issued_at_unix_seconds(&self) -> u64 {
        self.issued_at_unix_seconds
    }

    pub fn expires_at_unix_seconds(&self) -> u64 {
        self.expires_at_unix_seconds
    }

    pub fn profile_bytes(&self) -> &[u8] {
        &self.profile_bytes
    }

    pub fn profile_sha256(&self) -> &[u8; SHA256_DIGEST_LEN] {
        &self.profile_sha256
    }

    pub fn signer_key_id(&self) -> &str {
        &self.signer_key_id
    }

    pub fn signer_public_key(&self) -> &[u8; ED25519_PUBLIC_KEY_LEN] {
        &self.signer_public_key
    }

    pub fn signature(&self) -> &[u8; ED25519_SIGNATURE_LEN] {
        &self.signature
    }
}

impl fmt::Debug for SignedProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignedProfile")
            .field("envelope_version", &self.envelope_version)
            .field("profile_schema_version", &self.profile_schema_version)
            .field("profile_id", &Redacted)
            .field("revision", &self.revision)
            .field("issued_at_unix_seconds", &self.issued_at_unix_seconds)
            .field("expires_at_unix_seconds", &self.expires_at_unix_seconds)
            .field("profile_bytes_len", &self.profile_bytes.len())
            .field("profile_sha256", &Redacted)
            .field("signer_key_id", &Redacted)
            .field("signer_public_key", &Redacted)
            .field("signature", &Redacted)
            .finish()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSignedProfile {
    envelope_version: u32,
    profile_schema_version: u32,
    profile_id: String,
    revision: u64,
    issued_at_unix_seconds: u64,
    expires_at_unix_seconds: u64,
    profile_b64: String,
    profile_sha256_b64: String,
    signer_key_id: String,
    signer_public_key_b64: String,
    signature_b64: String,
}

#[derive(Debug, Error)]
pub enum EnvelopeError {
    #[error("envelope version mismatch: got {got}, expected {expected}")]
    EnvelopeVersion { got: u32, expected: u32 },
    #[error("profile schema version must be nonzero")]
    ZeroSchemaVersion,
    #[error("{field} is empty")]
    IdentifierEmpty { field: &'static str },
    #[error("{field} is too long: got {got}, max {max}")]
    IdentifierTooLong {
        field: &'static str,
        got: usize,
        max: usize,
    },
    #[error("{field} contains a refused character")]
    IdentifierCharacter { field: &'static str },
    #[error("profile revision must be nonzero")]
    ZeroRevision,
    #[error("profile validity window is empty or inverted")]
    ValidityWindow,
    #[error("profile body is empty")]
    ProfileBodyEmpty,
    #[error("profile body is too large: got {got}, max {max}")]
    ProfileBodyTooLarge { got: usize, max: usize },
    #[error("base64 decode failed for {field}")]
    Base64 {
        field: &'static str,
        #[source]
        source: base64::DecodeError,
    },
    #[error("{field} length mismatch: got {got}, expected {expected}")]
    FixedLength {
        field: &'static str,
        got: usize,
        expected: usize,
    },
    #[error("signer public key is empty")]
    SignerPublicKeyEmpty,
    #[error("signature is empty")]
    SignatureEmpty,
    #[error("profile digest does not match profile body")]
    DigestMismatch,
    #[error("canonical field length exceeds u32")]
    FieldTooLarge,
    #[error("signed profile JSON refused")]
    Json {
        #[source]
        source: serde_json::Error,
    },
}

/// Compute the profile body digest carried by the signed envelope.
pub fn canonical_profile_digest(profile_bytes: &[u8]) -> [u8; SHA256_DIGEST_LEN] {
    let digest = Sha256::digest(profile_bytes);
    let mut out = [0u8; SHA256_DIGEST_LEN];
    out.copy_from_slice(&digest);
    out
}

fn decode_b64(field: &'static str, encoded: &str) -> Result<Vec<u8>, EnvelopeError> {
    STANDARD
        .decode(encoded)
        .map_err(|source| EnvelopeError::Base64 { field, source })
}

fn decode_fixed_b64<const N: usize>(
    field: &'static str,
    encoded: &str,
) -> Result<[u8; N], EnvelopeError> {
    let decoded = decode_b64(field, encoded)?;
    if decoded.len() != N {
        return Err(EnvelopeError::FixedLength {
            field,
            got: decoded.len(),
            expected: N,
        });
    }
    let mut out = [0u8; N];
    out.copy_from_slice(&decoded);
    Ok(out)
}

#[derive(Clone, Copy)]
enum IdentifierKind {
    Profile,
    SignerKey,
}

fn validate_identifier(
    field: &'static str,
    value: &str,
    max_bytes: usize,
    kind: IdentifierKind,
) -> Result<(), EnvelopeError> {
    if value.is_empty() {
        return Err(EnvelopeError::IdentifierEmpty { field });
    }
    if value.len() > max_bytes {
        return Err(EnvelopeError::IdentifierTooLong {
            field,
            got: value.len(),
            max: max_bytes,
        });
    }
    let ok = value.as_bytes().iter().copied().all(|b| match b {
        b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'_' | b'-' | b':' => true,
        b'/' if matches!(kind, IdentifierKind::Profile) => true,
        _ => false,
    });
    if !ok {
        return Err(EnvelopeError::IdentifierCharacter { field });
    }
    Ok(())
}

fn write_lp(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EnvelopeError> {
    let len = u32::try_from(bytes.len()).map_err(|_| EnvelopeError::FieldTooLarge)?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

struct Redacted;

impl fmt::Debug for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed_profile() -> SignedProfile {
        SignedProfile::new(
            1,
            "discord/reference",
            7,
            1_700_000_000,
            1_700_086_400,
            br#"{"service":"discord","selector":"alice@example.invalid"}"#.to_vec(),
            "release-key-2026-07",
            [8u8; ED25519_PUBLIC_KEY_LEN],
            [9u8; ED25519_SIGNATURE_LEN],
        )
        .unwrap()
    }

    #[test]
    fn signed_profile_envelope_builds_canonical_payload_and_digest() {
        let signed = signed_profile();

        assert_eq!(signed.envelope_version(), ADAPTER_PROFILE_ENVELOPE_VERSION);
        assert_eq!(
            signed.profile_sha256(),
            &canonical_profile_digest(signed.profile_bytes())
        );

        let first = signed.signing_payload().unwrap();
        let second = signed.signing_payload().unwrap();
        assert_eq!(first, second);
        assert!(first.starts_with(&(ADAPTER_PROFILE_ENVELOPE_DOMAIN.len() as u32).to_be_bytes()));
        assert!(first
            .windows(SHA256_DIGEST_LEN)
            .any(|window| window == signed.profile_sha256().as_slice()));
    }

    #[test]
    fn signed_profile_envelope_wire_round_trips_strictly() {
        let signed = signed_profile();
        let wire = signed.to_wire_json().unwrap();
        let parsed = SignedProfile::from_wire_json(&wire).unwrap();
        assert_eq!(parsed, signed);

        let mut value: serde_json::Value = serde_json::from_slice(&wire).unwrap();
        value["unexpected"] = serde_json::json!(true);
        let mutated = serde_json::to_vec(&value).unwrap();
        assert!(matches!(
            SignedProfile::from_wire_json(&mutated),
            Err(EnvelopeError::Json { .. })
        ));
    }

    #[test]
    fn signed_profile_envelope_rejects_digest_tampering_from_wire() {
        let signed = signed_profile();
        let wire = signed.to_wire_json().unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&wire).unwrap();
        value["profile_b64"] = serde_json::json!(STANDARD.encode(b"changed profile"));
        let mutated = serde_json::to_vec(&value).unwrap();

        assert!(matches!(
            SignedProfile::from_wire_json(&mutated),
            Err(EnvelopeError::DigestMismatch)
        ));
    }

    #[test]
    fn signed_profile_envelope_rejects_missing_authority_material() {
        assert!(matches!(
            SignedProfile::new(
                1,
                "discord/reference",
                1,
                1,
                2,
                b"{}".to_vec(),
                "",
                [0u8; ED25519_PUBLIC_KEY_LEN],
                [0u8; ED25519_SIGNATURE_LEN],
            ),
            Err(EnvelopeError::IdentifierEmpty {
                field: "signer_key_id"
            })
        ));

        assert!(matches!(
            SignedProfile::new(
                1,
                "discord/reference",
                1,
                1,
                2,
                b"{}".to_vec(),
                "release-key-2026-07",
                [0u8; ED25519_PUBLIC_KEY_LEN],
                [1u8; ED25519_SIGNATURE_LEN],
            ),
            Err(EnvelopeError::SignerPublicKeyEmpty)
        ));

        assert!(matches!(
            SignedProfile::new(
                1,
                "discord/reference",
                1,
                1,
                2,
                b"{}".to_vec(),
                "release-key-2026-07",
                [1u8; ED25519_PUBLIC_KEY_LEN],
                [0u8; ED25519_SIGNATURE_LEN],
            ),
            Err(EnvelopeError::SignatureEmpty)
        ));

        let signed = signed_profile();
        let wire = signed.to_wire_json().unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&wire).unwrap();
        value["signature_b64"] = serde_json::json!(STANDARD.encode([1u8; 63]));
        let mutated = serde_json::to_vec(&value).unwrap();
        assert!(matches!(
            SignedProfile::from_wire_json(&mutated),
            Err(EnvelopeError::FixedLength {
                field: "signature_b64",
                got: 63,
                expected: ED25519_SIGNATURE_LEN,
            })
        ));
    }

    #[test]
    fn signed_profile_debug_redacts_profile_and_key_material() {
        let signed = signed_profile();
        let text = format!("{signed:?}");

        assert!(text.contains("SignedProfile"));
        assert!(text.contains("profile_bytes_len"));
        assert!(!text.contains("alice@example.invalid"));
        assert!(!text.contains("release-key-2026-07"));
        assert!(!text.contains("discord/reference"));
        assert!(!text.contains("999999"));
    }
}
