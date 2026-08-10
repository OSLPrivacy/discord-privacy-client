use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use std::{fmt, fs, path::Path};

use serde::{Deserialize, Serialize};

const SIGNING_DOMAIN: &[u8] = b"osl/build-proof/v1\0";
const SIGNED_PROOF_SCHEMA_VERSION: u8 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildProofInput {
    pub build_fingerprint: String,
    pub device_id: String,
    pub person_id: String,
    pub made_at_unix_seconds: u64,
    pub stops_counting_at_unix_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildProof {
    pub build_fingerprint: String,
    pub device_id: String,
    pub person_id: String,
    pub made_at_unix_seconds: u64,
    pub stops_counting_at_unix_seconds: u64,
}

/// One five-value build proof authenticated by OSL's offline Ed25519 key.
///
/// The public key is deliberately absent. Verifiers must be given an OSL trust
/// root independently instead of trusting a key supplied by the proof itself.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignedBuildProof {
    pub schema_version: u8,
    pub proof: BuildProof,
    pub signature_base64: String,
}

/// The complete answer produced by checking a signed build proof.
///
/// `CannotTell` is deliberately distinct from `Modified`: absence, damaged
/// evidence, an untrusted signature, or a proof outside its time window does
/// not establish that the build changed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildProofCheck {
    Unmodified,
    Modified,
    CannotTell,
}

impl fmt::Display for BuildProofCheck {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unmodified => "unmodified",
            Self::Modified => "modified",
            Self::CannotTell => "cannot tell",
        })
    }
}

pub fn make_build_proof(input: BuildProofInput) -> Result<BuildProof, String> {
    validate_fingerprint(&input.build_fingerprint)?;
    validate_identifier("device ID", &input.device_id)?;
    validate_identifier("person ID", &input.person_id)?;

    if input.stops_counting_at_unix_seconds <= input.made_at_unix_seconds {
        return Err("stops-counting-at time must be later than made-at time".to_owned());
    }

    Ok(BuildProof {
        build_fingerprint: input.build_fingerprint,
        device_id: input.device_id,
        person_id: input.person_id,
        made_at_unix_seconds: input.made_at_unix_seconds,
        stops_counting_at_unix_seconds: input.stops_counting_at_unix_seconds,
    })
}

pub fn sign_build_proof(
    proof: BuildProof,
    signing_seed: [u8; 32],
) -> Result<SignedBuildProof, String> {
    validate_proof(&proof)?;
    let signing_key = SigningKey::from_bytes(&signing_seed);
    let signature = signing_key.sign(&canonical_signing_bytes(&proof));
    Ok(SignedBuildProof {
        schema_version: SIGNED_PROOF_SCHEMA_VERSION,
        proof,
        signature_base64: STANDARD.encode(signature.to_bytes()),
    })
}

pub fn verify_signed_build_proof(
    signed: &SignedBuildProof,
    trusted_public_key: [u8; 32],
) -> Result<(), String> {
    if signed.schema_version != SIGNED_PROOF_SCHEMA_VERSION {
        return Err(format!(
            "unsupported signed proof schema version: {}",
            signed.schema_version
        ));
    }

    validate_proof(&signed.proof)?;

    let signature_bytes = STANDARD
        .decode(&signed.signature_base64)
        .map_err(|_| "bad-signature".to_owned())?;
    let signature_bytes: [u8; 64] = signature_bytes
        .try_into()
        .map_err(|_| "bad-signature".to_owned())?;
    let signature = Signature::from_bytes(&signature_bytes);
    let verifying_key = VerifyingKey::from_bytes(&trusted_public_key)
        .map_err(|error| format!("trusted public key is invalid: {error}"))?;

    verifying_key
        .verify_strict(&canonical_signing_bytes(&signed.proof), &signature)
        .map_err(|_| "bad-signature".to_owned())
}

/// Check authenticated proof material against the build fingerprint observed
/// now. This function is total: uncertainty is an answer, not an error.
pub fn check_build_proof(
    signed: Option<&SignedBuildProof>,
    trusted_public_key: Option<[u8; 32]>,
    observed_build_fingerprint: &str,
    checked_at_unix_seconds: u64,
) -> BuildProofCheck {
    let (Some(signed), Some(trusted_public_key)) = (signed, trusted_public_key) else {
        return BuildProofCheck::CannotTell;
    };
    if validate_fingerprint(observed_build_fingerprint).is_err()
        || verify_signed_build_proof(signed, trusted_public_key).is_err()
        || checked_at_unix_seconds < signed.proof.made_at_unix_seconds
        || checked_at_unix_seconds >= signed.proof.stops_counting_at_unix_seconds
    {
        return BuildProofCheck::CannotTell;
    }

    if signed.proof.build_fingerprint == observed_build_fingerprint {
        BuildProofCheck::Unmodified
    } else {
        BuildProofCheck::Modified
    }
}

/// Read one signed proof file and classify it. A missing path and every read or
/// parse failure are evidence absence, so they return `CannotTell`.
pub fn check_build_proof_file(
    proof_path: Option<&Path>,
    trusted_public_key: Option<[u8; 32]>,
    observed_build_fingerprint: &str,
    checked_at_unix_seconds: u64,
) -> BuildProofCheck {
    let Some(proof_path) = proof_path else {
        return BuildProofCheck::CannotTell;
    };
    let Ok(bytes) = fs::read(proof_path) else {
        return BuildProofCheck::CannotTell;
    };
    let Ok(signed) = serde_json::from_slice::<SignedBuildProof>(&bytes) else {
        return BuildProofCheck::CannotTell;
    };
    check_build_proof(
        Some(&signed),
        trusted_public_key,
        observed_build_fingerprint,
        checked_at_unix_seconds,
    )
}

fn validate_proof(proof: &BuildProof) -> Result<(), String> {
    validate_fingerprint(&proof.build_fingerprint)?;
    validate_identifier("device ID", &proof.device_id)?;
    validate_identifier("person ID", &proof.person_id)?;
    if proof.stops_counting_at_unix_seconds <= proof.made_at_unix_seconds {
        return Err("stops-counting-at time must be later than made-at time".to_owned());
    }
    Ok(())
}

fn canonical_signing_bytes(proof: &BuildProof) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(
        SIGNING_DOMAIN.len()
            + proof.build_fingerprint.len()
            + proof.device_id.len()
            + proof.person_id.len()
            + 40,
    );
    bytes.extend_from_slice(SIGNING_DOMAIN);
    append_length_prefixed(&mut bytes, proof.build_fingerprint.as_bytes());
    append_length_prefixed(&mut bytes, proof.device_id.as_bytes());
    append_length_prefixed(&mut bytes, proof.person_id.as_bytes());
    bytes.extend_from_slice(&proof.made_at_unix_seconds.to_be_bytes());
    bytes.extend_from_slice(&proof.stops_counting_at_unix_seconds.to_be_bytes());
    bytes
}

fn append_length_prefixed(output: &mut Vec<u8>, value: &[u8]) {
    let length = u64::try_from(value.len()).expect("proof field length fits in u64");
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(value);
}

fn validate_fingerprint(value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(
            "build fingerprint must be exactly 64 lowercase hexadecimal characters".to_owned(),
        );
    }
    Ok(())
}

fn validate_identifier(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{label} must not be empty"));
    }
    if value != value.trim() {
        return Err(format!("{label} must not start or end with whitespace"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{label} must not contain control characters"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_input() -> BuildProofInput {
        BuildProofInput {
            build_fingerprint: "a".repeat(64),
            device_id: "device-3096".to_owned(),
            person_id: "person-3096".to_owned(),
            made_at_unix_seconds: 1_754_828_400,
            stops_counting_at_unix_seconds: 1_754_914_800,
        }
    }

    #[test]
    fn stop_time_must_be_later_than_made_time() {
        let input = BuildProofInput {
            stops_counting_at_unix_seconds: 1_754_828_400,
            ..valid_input()
        };
        assert_eq!(
            make_build_proof(input).unwrap_err(),
            "stops-counting-at time must be later than made-at time"
        );
    }
}
