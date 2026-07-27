//! Durable sender-filter capability floor for the shipping keyserver client.
//!
//! The caller never supplies a capability bit, a state path, or a prior-state
//! boolean. The client probes the Worker itself and this module resolves one
//! fixed file inside the active account directory and a second identity-bound
//! monotonic record in the shared OSL base. Both records are mandatory: their
//! absence is never interpreted as genesis. Until an independently anchored
//! floor-zero pair is provisioned, the shipping client therefore refuses
//! rather than reopening `NeverObserved` after local deletion.

use crate::identity::Identity;
use crate::recipients::{osl_base_dir, osl_config_dir};
use crate::{Error, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const FLOOR_FORMAT: &str = "osl.keyserver.sender-filter-capability-floor.v2";
const FLOOR_DOMAIN: &[u8] = b"OSL-KEYSERVER-SENDER-FILTER-FLOOR-v2\0";
const FLOOR_FILENAME: &str = "keyserver-sender-filter-capability-floor.json";
const IDENTITY_ANCHOR_DIRECTORY: &str = "identity-monotonic-records";
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
    identity_anchor_sha256: String,
    capability_version: u32,
    first_observed_at_ms: i64,
    signature_b64: String,
}

fn floor_paths(identity: &Identity) -> Result<(PathBuf, PathBuf)> {
    let directory = osl_config_dir().map_err(|error| {
        Error::Transport(format!(
            "sender-filter capability floor directory is unavailable: {error}"
        ))
    })?;
    let base = osl_base_dir().map_err(|error| {
        Error::Transport(format!(
            "sender-filter identity anchor directory is unavailable: {error}"
        ))
    })?;
    let anchor = identity_anchor_sha256(identity);
    Ok((
        directory.join(FLOOR_FILENAME),
        base.join(IDENTITY_ANCHOR_DIRECTORY)
            .join(format!("{anchor}.json")),
    ))
}

fn identity_anchor_sha256(identity: &Identity) -> String {
    let mut digest = Sha256::new();
    digest.update(b"OSL-IDENTITY-MONOTONIC-RECORD-v1\0");
    digest.update((identity.user_id.len() as u32).to_be_bytes());
    digest.update(identity.user_id.as_bytes());
    digest.update(identity.ed25519_public.as_bytes());
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn lp(value: &[u8], output: &mut Vec<u8>) {
    output.extend_from_slice(&(value.len() as u32).to_be_bytes());
    output.extend_from_slice(value);
}

fn canonical_floor_bytes(
    recipient_user_id: &str,
    identity_anchor_sha256: &str,
    capability_version: u32,
    first_observed_at_ms: i64,
) -> Vec<u8> {
    let mut output = Vec::with_capacity(FLOOR_DOMAIN.len() + recipient_user_id.len() + 96);
    lp(FLOOR_DOMAIN, &mut output);
    lp(recipient_user_id.as_bytes(), &mut output);
    lp(identity_anchor_sha256.as_bytes(), &mut output);
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

fn load_receipt(path: &Path, identity: &Identity) -> Result<Option<CapabilityFloorReceipt>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
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
        || receipt.identity_anchor_sha256 != identity_anchor_sha256(identity)
        || receipt.capability_version > SENDER_FILTER_CAPABILITY_VERSION
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
        &receipt.identity_anchor_sha256,
        receipt.capability_version,
        receipt.first_observed_at_ms,
    );
    if !crypto::ed25519::verify(&identity.ed25519_public, &message, &signature).unwrap_or(false) {
        return Err(Error::Transport(
            "sender-filter capability floor signature is invalid".into(),
        ));
    }
    Ok(Some(receipt))
}

fn load_from_paths(
    local_path: &Path,
    identity_anchor_path: &Path,
    identity: &Identity,
) -> Result<SenderFilterCapabilityFloor> {
    let local = load_receipt(local_path, identity)?;
    let anchor = load_receipt(identity_anchor_path, identity)?;
    match (local, anchor) {
        (None, None) => Err(Error::Transport(
            "sender-filter capability floor genesis is not provisioned".into(),
        )),
        (Some(local), Some(anchor))
            if local.identity_anchor_sha256 == anchor.identity_anchor_sha256
                && local.capability_version == anchor.capability_version
                && local.first_observed_at_ms == anchor.first_observed_at_ms
                && local.signature_b64 == anchor.signature_b64 =>
        {
            if local.capability_version == SENDER_FILTER_CAPABILITY_VERSION {
                Ok(SenderFilterCapabilityFloor::Version1)
            } else {
                Err(Error::Transport(
                    "sender-filter capability floor genesis is not independently provisioned"
                        .into(),
                ))
            }
        }
        (Some(_), Some(_)) => Err(Error::Transport(
            "sender-filter capability floor and identity anchor disagree".into(),
        )),
        _ => Err(Error::Transport(
            "sender-filter capability floor or identity anchor is absent".into(),
        )),
    }
}

pub(crate) fn load_sender_filter_capability_floor(
    identity: &Identity,
) -> Result<SenderFilterCapabilityFloor> {
    let (local, anchor) = floor_paths(identity)?;
    load_from_paths(&local, &anchor, identity)
}

fn sync_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Transport("sender-filter record has no parent".into()))?;
    #[cfg(windows)]
    let directory = {
        use std::os::windows::fs::OpenOptionsExt;
        let mut options = OpenOptions::new();
        options.read(true).custom_flags(0x02000000);
        options.open(parent)?
    };
    #[cfg(not(windows))]
    let directory = File::open(parent)?;
    directory.sync_all()?;
    Ok(())
}

fn ensure_private_directory(path: &Path) -> Result<()> {
    match fs::create_dir(path) {
        Ok(()) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
            }
            sync_parent(path)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(Error::Io(error)),
    }
    validate_parent(&path.join("record"))?;
    Ok(())
}

fn write_receipt_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    validate_parent(path)?;
    let temporary = path.with_extension("json.pending");
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = match options.open(&temporary) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(Error::Transport(
                "sender-filter capability floor has an incomplete atomic write".into(),
            ))
        }
        Err(error) => return Err(Error::Io(error)),
    };
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, path)?;
    sync_parent(path)?;
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
    let (local_path, anchor_path) = floor_paths(identity)?;
    match load_from_paths(&local_path, &anchor_path, identity)? {
        SenderFilterCapabilityFloor::Version1 => return Ok(()),
        SenderFilterCapabilityFloor::NeverObserved => {}
    }
    if let Some(parent) = anchor_path.parent() {
        ensure_private_directory(parent)?;
    }
    let identity_anchor = identity_anchor_sha256(identity);
    let message = canonical_floor_bytes(
        &identity.user_id,
        &identity_anchor,
        SENDER_FILTER_CAPABILITY_VERSION,
        observed_at_ms,
    );
    let signature = crypto::ed25519::sign(&identity.ed25519_secret, &message);
    let receipt = CapabilityFloorReceipt {
        format: FLOOR_FORMAT.to_owned(),
        recipient_user_id: identity.user_id.clone(),
        identity_anchor_sha256: identity_anchor,
        capability_version: SENDER_FILTER_CAPABILITY_VERSION,
        first_observed_at_ms: observed_at_ms,
        signature_b64: STANDARD.encode(signature.as_bytes()),
    };
    let bytes = serde_json::to_vec(&receipt)?;
    // Anchor first. A crash after this point leaves one-sided state, which is a
    // refusal rather than a reset to NeverObserved.
    write_receipt_atomically(&anchor_path, &bytes)?;
    write_receipt_atomically(&local_path, &bytes)?;
    match load_from_paths(&local_path, &anchor_path, identity)? {
        SenderFilterCapabilityFloor::Version1 => Ok(()),
        SenderFilterCapabilityFloor::NeverObserved => Err(Error::Transport(
            "sender-filter capability floor was not durably recorded".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_paths(label: &str) -> (PathBuf, PathBuf) {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "osl-sender-filter-floor-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        let anchor_directory = directory.join(IDENTITY_ANCHOR_DIRECTORY);
        fs::create_dir(&anchor_directory).unwrap();
        (
            directory.join(FLOOR_FILENAME),
            anchor_directory.join("identity-anchor.json"),
        )
    }

    fn write_signed_pair(
        local_path: &Path,
        anchor_path: &Path,
        identity: &Identity,
        capability_version: u32,
        observed_at_ms: i64,
    ) -> Result<()> {
        let identity_anchor = identity_anchor_sha256(identity);
        let message = canonical_floor_bytes(
            &identity.user_id,
            &identity_anchor,
            capability_version,
            observed_at_ms,
        );
        let signature = crypto::ed25519::sign(&identity.ed25519_secret, &message);
        let receipt = CapabilityFloorReceipt {
            format: FLOOR_FORMAT.to_owned(),
            recipient_user_id: identity.user_id.clone(),
            identity_anchor_sha256: identity_anchor,
            capability_version,
            first_observed_at_ms: observed_at_ms,
            signature_b64: STANDARD.encode(signature.as_bytes()),
        };
        let bytes = serde_json::to_vec(&receipt)?;
        write_receipt_atomically(anchor_path, &bytes)?;
        write_receipt_atomically(local_path, &bytes)?;
        Ok(())
    }

    fn provision_test_genesis(
        local_path: &Path,
        anchor_path: &Path,
        identity: &Identity,
    ) -> Result<()> {
        write_signed_pair(local_path, anchor_path, identity, 0, 1)
    }

    fn record_at_paths(
        local_path: &Path,
        anchor_path: &Path,
        identity: &Identity,
        observed_at_ms: i64,
    ) -> Result<()> {
        if matches!(
            load_from_paths(local_path, anchor_path, identity),
            Ok(SenderFilterCapabilityFloor::Version1)
        ) {
            return Ok(());
        }
        write_signed_pair(
            local_path,
            anchor_path,
            identity,
            SENDER_FILTER_CAPABILITY_VERSION,
            observed_at_ms,
        )?;
        if load_from_paths(local_path, anchor_path, identity)?
            != SenderFilterCapabilityFloor::Version1
        {
            return Err(Error::Transport("test floor did not persist".into()));
        }
        Ok(())
    }

    #[test]
    fn signed_floor_is_write_once_and_survives_a_fresh_load() {
        let (local, anchor) = test_paths("positive");
        let identity = crate::generate_identity("recipient-positive".into());
        assert!(load_from_paths(&local, &anchor, &identity).is_err());
        record_at_paths(&local, &anchor, &identity, 1_700_000_000_000).unwrap();
        assert_eq!(
            load_from_paths(&local, &anchor, &identity).unwrap(),
            SenderFilterCapabilityFloor::Version1
        );
        let before = fs::read(&local).unwrap();
        record_at_paths(&local, &anchor, &identity, 1_800_000_000_000).unwrap();
        assert_eq!(fs::read(&local).unwrap(), before);
        let _ = fs::remove_dir_all(local.parent().unwrap());
    }

    #[test]
    fn deleted_local_floor_cannot_reset_the_external_identity_anchor() {
        let (local, anchor) = test_paths("deleted-local");
        let identity = crate::generate_identity("recipient-positive".into());
        provision_test_genesis(&local, &anchor, &identity).unwrap();
        record_at_paths(&local, &anchor, &identity, 1_700_000_000_000).unwrap();
        fs::remove_file(&local).unwrap();
        assert!(load_from_paths(&local, &anchor, &identity).is_err());
        let _ = fs::remove_dir_all(local.parent().unwrap());
    }

    #[test]
    fn malformed_wrong_identity_and_tampered_floors_fail_closed() {
        let (local, anchor) = test_paths("negative");
        let identity = crate::generate_identity("recipient-positive".into());
        let other = crate::generate_identity("recipient-other".into());
        provision_test_genesis(&local, &anchor, &identity).unwrap();
        record_at_paths(&local, &anchor, &identity, 1_700_000_000_000).unwrap();
        assert!(load_from_paths(&local, &anchor, &other).is_err());

        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&local).unwrap()).unwrap();
        value["capability_version"] = serde_json::json!(2);
        fs::write(&local, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(load_from_paths(&local, &anchor, &identity).is_err());
        let _ = fs::remove_dir_all(local.parent().unwrap());
    }

    #[test]
    fn deleting_both_records_cannot_reopen_never_observed_after_restart() {
        let (local, anchor) = test_paths("deleted-both");
        let identity = crate::generate_identity("recipient-restart".into());
        record_at_paths(&local, &anchor, &identity, 1_700_000_000_000).unwrap();

        fs::remove_file(&local).unwrap();
        fs::remove_file(&anchor).unwrap();

        // A fresh load models a restarted process: there is no in-memory bit
        // that can turn missing durable authority back into NeverObserved.
        assert!(load_from_paths(&local, &anchor, &identity).is_err());
        assert!(load_from_paths(&local, &anchor, &identity).is_err());
        let _ = fs::remove_dir_all(local.parent().unwrap());
    }

    #[test]
    fn replayed_signed_floor_zero_cannot_reopen_never_observed() {
        let (local, anchor) = test_paths("replayed-genesis");
        let identity = crate::generate_identity("recipient-replay".into());
        provision_test_genesis(&local, &anchor, &identity).unwrap();

        // Even an internally signed, mutually matching old floor-zero pair is
        // not production authority. It remains a refusal across fresh loads.
        assert!(load_from_paths(&local, &anchor, &identity).is_err());
        assert!(load_from_paths(&local, &anchor, &identity).is_err());
        let _ = fs::remove_dir_all(local.parent().unwrap());
    }
}
