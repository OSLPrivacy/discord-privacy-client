//! Durable sender-filter capability floor for the shipping keyserver client.
//!
//! The caller never supplies a capability bit, a state path, or a prior-state
//! boolean. The client probes the Worker itself and this module resolves one
//! fixed file inside the active account directory. Once capability version 1
//! has been observed, the write-once identity-signed receipt makes a later
//! capability disappearance a fail-closed downgrade across process restarts.

use crate::identity::Identity;
use crate::recipients::osl_config_dir;
use crate::{Error, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const FLOOR_FORMAT: &str = "osl.keyserver.sender-filter-capability-floor.v1";
const FLOOR_DOMAIN: &[u8] = b"OSL-KEYSERVER-SENDER-FILTER-FLOOR-v1\0";
const FLOOR_FILENAME: &str = "keyserver-sender-filter-capability-floor.json";
pub(crate) const SENDER_FILTER_CAPABILITY_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum SenderFilterCapabilityFloor {
    NeverObserved,
    Version1,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityFloorReceipt {
    format: String,
    recipient_user_id: String,
    capability_version: u32,
    first_observed_at_ms: i64,
    signature_b64: String,
}

fn floor_path() -> Result<PathBuf> {
    let directory = osl_config_dir().map_err(|error| {
        Error::Transport(format!(
            "sender-filter capability floor directory is unavailable: {error}"
        ))
    })?;
    Ok(directory.join(FLOOR_FILENAME))
}

fn lp(value: &[u8], output: &mut Vec<u8>) {
    output.extend_from_slice(&(value.len() as u32).to_be_bytes());
    output.extend_from_slice(value);
}

fn canonical_floor_bytes(
    recipient_user_id: &str,
    capability_version: u32,
    first_observed_at_ms: i64,
) -> Vec<u8> {
    let mut output = Vec::with_capacity(FLOOR_DOMAIN.len() + recipient_user_id.len() + 32);
    lp(FLOOR_DOMAIN, &mut output);
    lp(recipient_user_id.as_bytes(), &mut output);
    lp(capability_version.to_string().as_bytes(), &mut output);
    lp(first_observed_at_ms.to_string().as_bytes(), &mut output);
    output
}

fn validate_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Transport("sender-filter capability floor has no parent".into()))?;
    let metadata = fs::symlink_metadata(parent).map_err(Error::Io)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(Error::Transport(
            "sender-filter capability floor parent is not a real directory".into(),
        ));
    }
    Ok(())
}

fn load_from_path(path: &Path, identity: &Identity) -> Result<SenderFilterCapabilityFloor> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SenderFilterCapabilityFloor::NeverObserved)
        }
        Err(error) => return Err(Error::Io(error)),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(Error::Transport(
            "sender-filter capability floor is not a regular file".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(Error::Transport(
                "sender-filter capability floor permissions are not private".into(),
            ));
        }
    }
    let bytes = fs::read(path)?;
    if bytes.is_empty() || bytes.len() > 16 * 1024 {
        return Err(Error::Transport(
            "sender-filter capability floor is empty or oversized".into(),
        ));
    }
    let receipt: CapabilityFloorReceipt = serde_json::from_slice(&bytes)?;
    if receipt.format != FLOOR_FORMAT
        || receipt.recipient_user_id != identity.user_id
        || receipt.capability_version != SENDER_FILTER_CAPABILITY_VERSION
        || receipt.first_observed_at_ms <= 0
    {
        return Err(Error::Transport(
            "sender-filter capability floor identity or version mismatch".into(),
        ));
    }
    let signature_bytes = STANDARD.decode(&receipt.signature_b64).map_err(|_| {
        Error::Transport("sender-filter capability floor signature is not canonical base64".into())
    })?;
    let signature_array: [u8; 64] = signature_bytes.try_into().map_err(|_| {
        Error::Transport("sender-filter capability floor signature is not 64 bytes".into())
    })?;
    let signature = crypto::ed25519::Signature::from_bytes(signature_array);
    let message = canonical_floor_bytes(
        &receipt.recipient_user_id,
        receipt.capability_version,
        receipt.first_observed_at_ms,
    );
    if !crypto::ed25519::verify(&identity.ed25519_public, &message, &signature).unwrap_or(false) {
        return Err(Error::Transport(
            "sender-filter capability floor signature is invalid".into(),
        ));
    }
    Ok(SenderFilterCapabilityFloor::Version1)
}

pub(crate) fn load_sender_filter_capability_floor(
    identity: &Identity,
) -> Result<SenderFilterCapabilityFloor> {
    load_from_path(&floor_path()?, identity)
}

fn write_receipt(path: &Path, bytes: &[u8]) -> Result<()> {
    validate_parent(path)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
        Err(error) => return Err(Error::Io(error)),
    };
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

pub(crate) fn record_sender_filter_capability_floor(
    identity: &Identity,
    observed_at_ms: i64,
) -> Result<()> {
    if observed_at_ms <= 0 {
        return Err(Error::Transport(
            "sender-filter capability observation time is invalid".into(),
        ));
    }
    let path = floor_path()?;
    match load_from_path(&path, identity)? {
        SenderFilterCapabilityFloor::Version1 => return Ok(()),
        SenderFilterCapabilityFloor::NeverObserved => {}
    }
    let message = canonical_floor_bytes(
        &identity.user_id,
        SENDER_FILTER_CAPABILITY_VERSION,
        observed_at_ms,
    );
    let signature = crypto::ed25519::sign(&identity.ed25519_secret, &message);
    let receipt = CapabilityFloorReceipt {
        format: FLOOR_FORMAT.to_owned(),
        recipient_user_id: identity.user_id.clone(),
        capability_version: SENDER_FILTER_CAPABILITY_VERSION,
        first_observed_at_ms: observed_at_ms,
        signature_b64: STANDARD.encode(signature.as_bytes()),
    };
    let bytes = serde_json::to_vec(&receipt)?;
    write_receipt(&path, &bytes)?;
    match load_from_path(&path, identity)? {
        SenderFilterCapabilityFloor::Version1 => Ok(()),
        SenderFilterCapabilityFloor::NeverObserved => Err(Error::Transport(
            "sender-filter capability floor was not durably recorded".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "osl-sender-filter-floor-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        directory.join(FLOOR_FILENAME)
    }

    fn record_at_path(path: &Path, identity: &Identity, observed_at_ms: i64) -> Result<()> {
        let message = canonical_floor_bytes(
            &identity.user_id,
            SENDER_FILTER_CAPABILITY_VERSION,
            observed_at_ms,
        );
        let signature = crypto::ed25519::sign(&identity.ed25519_secret, &message);
        let receipt = CapabilityFloorReceipt {
            format: FLOOR_FORMAT.to_owned(),
            recipient_user_id: identity.user_id.clone(),
            capability_version: SENDER_FILTER_CAPABILITY_VERSION,
            first_observed_at_ms: observed_at_ms,
            signature_b64: STANDARD.encode(signature.as_bytes()),
        };
        write_receipt(path, &serde_json::to_vec(&receipt)?)?;
        match load_from_path(path, identity)? {
            SenderFilterCapabilityFloor::Version1 => Ok(()),
            SenderFilterCapabilityFloor::NeverObserved => {
                Err(Error::Transport("test floor did not persist".into()))
            }
        }
    }

    #[test]
    fn signed_floor_is_write_once_and_survives_a_fresh_load() {
        let path = test_path("positive");
        let identity = crate::generate_identity("recipient-positive".into());
        assert_eq!(
            load_from_path(&path, &identity).unwrap(),
            SenderFilterCapabilityFloor::NeverObserved
        );
        record_at_path(&path, &identity, 1_700_000_000_000).unwrap();
        assert_eq!(
            load_from_path(&path, &identity).unwrap(),
            SenderFilterCapabilityFloor::Version1
        );
        let before = fs::read(&path).unwrap();
        record_at_path(&path, &identity, 1_800_000_000_000).unwrap();
        assert_eq!(fs::read(&path).unwrap(), before);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn malformed_wrong_identity_and_tampered_floors_fail_closed() {
        let path = test_path("negative");
        let identity = crate::generate_identity("recipient-positive".into());
        let other = crate::generate_identity("recipient-other".into());
        record_at_path(&path, &identity, 1_700_000_000_000).unwrap();
        assert!(load_from_path(&path, &other).is_err());

        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["capability_version"] = serde_json::json!(2);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(load_from_path(&path, &identity).is_err());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
