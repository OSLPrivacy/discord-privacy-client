//! `AccountOwnershipProof` -- a signed proof that one OSL identity controls
//! one platform account.
//!
//! The top-level wire shape is intentionally small and stable:
//! `platform_id`, `proof_type`, and `e`. `e` is the proof-type-specific
//! evidence envelope. For v1 that envelope carries a server-issued nonce,
//! owner id, expiry, and Ed25519 signature over canonical bytes that bind all
//! of them. `Debug` and `Display` are hand-written so account identifiers,
//! nonces, and signatures never spill into logs.

use crate::account_ownership_error::AccountOwnershipError;
use core::fmt;
use crypto::ed25519;
use serde::{Deserialize, Serialize};

pub const ACCOUNT_OWNERSHIP_PROOF_DOMAIN: &str = "OSL-ACCOUNT-OWNERSHIP-PROOF-v1\0";
pub const ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1: &str =
    "ed25519_identity_challenge_v1";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountOwnershipEvidence {
    pub owner_user_id: String,
    pub nonce_b64: String,
    pub issued_at_unix_seconds: u64,
    pub expires_at_unix_seconds: u64,
    pub signature_b64: String,
}

impl fmt::Debug for AccountOwnershipEvidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccountOwnershipEvidence")
            .field("owner_user_id", &"[REDACTED]")
            .field("nonce_b64", &"[REDACTED]")
            .field("issued_at_unix_seconds", &self.issued_at_unix_seconds)
            .field("expires_at_unix_seconds", &self.expires_at_unix_seconds)
            .field("signature_b64", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountOwnershipProof {
    pub platform_id: String,
    pub proof_type: String,
    pub e: AccountOwnershipEvidence,
}

impl AccountOwnershipProof {
    pub fn new(
        platform_id: impl Into<String>,
        proof_type: impl Into<String>,
        e: AccountOwnershipEvidence,
    ) -> Result<Self, AccountOwnershipError> {
        let proof = Self {
            platform_id: platform_id.into(),
            proof_type: proof_type.into(),
            e,
        };
        proof.validate_shape()?;
        Ok(proof)
    }

    pub fn validate_shape(&self) -> Result<(), AccountOwnershipError> {
        if self.platform_id.is_empty()
            || self.e.owner_user_id.is_empty()
            || self.e.issued_at_unix_seconds >= self.e.expires_at_unix_seconds
            || self.proof_type != ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1
        {
            return Err(AccountOwnershipError::ProofMalformed);
        }
        let nonce = decode_b64(&self.e.nonce_b64, crate::PROOF_CHALLENGE_NONCE_BYTES)?;
        let signature = decode_b64(&self.e.signature_b64, ed25519::SIGNATURE_SIZE)?;
        if nonce.len() != crate::PROOF_CHALLENGE_NONCE_BYTES
            || signature.len() != ed25519::SIGNATURE_SIZE
        {
            return Err(AccountOwnershipError::ProofMalformed);
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, AccountOwnershipError> {
        canonical_account_ownership_proof_bytes(
            &self.proof_type,
            &self.platform_id,
            &self.e.owner_user_id,
            &self.e.nonce_b64,
            self.e.issued_at_unix_seconds,
            self.e.expires_at_unix_seconds,
        )
    }
}

impl fmt::Debug for AccountOwnershipProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccountOwnershipProof")
            .field("platform_id", &"[REDACTED]")
            .field("proof_type", &self.proof_type)
            .field("e", &self.e)
            .finish()
    }
}

impl fmt::Display for AccountOwnershipProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AccountOwnershipProof(type={})", self.proof_type)
    }
}

pub fn canonical_account_ownership_proof_bytes(
    proof_type: &str,
    platform_id: &str,
    owner_user_id: &str,
    nonce_b64: &str,
    issued_at_unix_seconds: u64,
    expires_at_unix_seconds: u64,
) -> Result<Vec<u8>, AccountOwnershipError> {
    if proof_type != ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1
        || platform_id.is_empty()
        || owner_user_id.is_empty()
        || issued_at_unix_seconds >= expires_at_unix_seconds
    {
        return Err(AccountOwnershipError::ProofMalformed);
    }
    let nonce = decode_b64(nonce_b64, crate::PROOF_CHALLENGE_NONCE_BYTES)?;
    let mut out = Vec::new();
    lp_text(&mut out, ACCOUNT_OWNERSHIP_PROOF_DOMAIN);
    lp_text(&mut out, proof_type);
    lp_text(&mut out, platform_id);
    lp_text(&mut out, owner_user_id);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&issued_at_unix_seconds.to_be_bytes());
    out.extend_from_slice(&expires_at_unix_seconds.to_be_bytes());
    Ok(out)
}

fn lp_text(out: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
}

fn decode_b64(value: &str, expected_len: usize) -> Result<Vec<u8>, AccountOwnershipError> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;

    let bytes = STANDARD
        .decode(value)
        .map_err(|_| AccountOwnershipError::ProofMalformed)?;
    if bytes.len() == expected_len {
        Ok(bytes)
    } else {
        Err(AccountOwnershipError::ProofMalformed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence() -> AccountOwnershipEvidence {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine as _;

        AccountOwnershipEvidence {
            owner_user_id: "owner-osl-id".to_owned(),
            nonce_b64: STANDARD.encode([9u8; crate::PROOF_CHALLENGE_NONCE_BYTES]),
            issued_at_unix_seconds: 10,
            expires_at_unix_seconds: 70,
            signature_b64: STANDARD.encode([8u8; ed25519::SIGNATURE_SIZE]),
        }
    }

    #[test]
    fn account_ownership_proof() {
        let proof = AccountOwnershipProof::new(
            "platform-account-123",
            ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1,
            evidence(),
        )
        .expect("valid proof shape");
        assert_eq!(proof.platform_id, "platform-account-123");
        assert_eq!(
            proof.proof_type,
            ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1
        );
        assert_eq!(proof.e.owner_user_id, "owner-osl-id");

        let canonical = proof.canonical_bytes().expect("canonical bytes");
        assert!(canonical.starts_with(
            &(ACCOUNT_OWNERSHIP_PROOF_DOMAIN.len() as u32).to_be_bytes()
        ));
        assert!(canonical.windows("platform-account-123".len()).any(
            |window| window == b"platform-account-123"
        ));

        let debug = format!("{proof:?}");
        let display = format!("{proof}");
        for secret in [
            "platform-account-123",
            "owner-osl-id",
            &proof.e.nonce_b64,
            &proof.e.signature_b64,
        ] {
            assert!(!debug.contains(secret), "Debug leaked {secret}");
            assert!(!display.contains(secret), "Display leaked {secret}");
        }
    }

    #[test]
    fn unsupported_or_malformed_proof_refuses() {
        let err = AccountOwnershipProof::new("platform-account-123", "password", evidence())
            .expect_err("unsupported proof type must fail closed");
        assert_eq!(err, AccountOwnershipError::ProofMalformed);

        let mut bad = evidence();
        bad.nonce_b64 = "not base64".to_owned();
        let err = AccountOwnershipProof::new(
            "platform-account-123",
            ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1,
            bad,
        )
        .expect_err("malformed nonce must fail closed");
        assert_eq!(err, AccountOwnershipError::ProofMalformed);
    }
}
