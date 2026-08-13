//! Source-side export for the offline Device Transfer protocol.
//!
//! The source device must first open its device-sealed identity.  The exported
//! bundle then has a distinct seal whose key is derived from the one-time code
//! and authenticated with the destination's transfer identifier.  A copied
//! `identity.json` therefore remains useless on another device.

use base64::{engine::general_purpose::STANDARD, Engine};
use crypto::{aead, hkdf, random};
use keystore::{Identity, Sealer};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const TRANSFER_BUNDLE_VERSION: u32 = 1;
const TRANSFER_CODE_KDF_DOMAIN: &[u8] = b"OSL/device-transfer/code-key/v1";
const TRANSFER_SEALER_METHOD: &str = "device-transfer-code-v1";

/// The portable, code-protected half of a Device Transfer export.
///
/// The destination public-key envelope and source authorization are added by
/// the subsequent protocol stages.  This type deliberately contains no code
/// or source-device sealer material.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransferBundle {
    pub version: u32,
    pub transfer_identifier: String,
    pub sealed_identity_b64: String,
}

/// Re-seal the source's device-bound `identity.json` for a one-time transfer.
///
/// `transfer_identifier` is AEAD associated data, so swapping it, the bundle,
/// or its ciphertext makes opening fail before an identity is returned.
pub fn export_identity_bundle(
    identity_path: &Path,
    source_sealer: &dyn Sealer,
    one_time_code: &str,
    transfer_identifier: &str,
) -> Result<TransferBundle, String> {
    validate_transfer_inputs(one_time_code, transfer_identifier)?;
    let identity = keystore::load_identity(identity_path, source_sealer)
        .map_err(|_| "OSL could not open the source identity for transfer".to_owned())?;
    seal_identity_for_transfer(&identity, one_time_code, transfer_identifier)
}

/// Open the code-protected identity in a transfer bundle.
///
/// This is intentionally narrow: it does not install anything.  The import
/// half must perform destination-key proof, authorization verification, anchor
/// adoption, and atomic promotion before it can use this result.
pub fn open_exported_identity(
    bundle: &TransferBundle,
    one_time_code: &str,
    transfer_identifier: &str,
) -> Result<Identity, String> {
    validate_transfer_inputs(one_time_code, transfer_identifier)?;
    if bundle.version != TRANSFER_BUNDLE_VERSION
        || bundle.transfer_identifier != transfer_identifier
    {
        return Err("OSL transfer bundle is not for this destination".to_owned());
    }

    let sealed_identity = STANDARD
        .decode(&bundle.sealed_identity_b64)
        .map_err(|_| "OSL transfer bundle is malformed".to_owned())?;
    let (scratch_dir, scratch) = scratch_path()?;
    let result = (|| {
        let staged_bytes = sealed_identity.len();
        fs::write(&scratch, sealed_identity)
            .map_err(|_| "OSL transfer bundle could not be staged".to_owned())?;
        // TASK 5402: the staging file is a real temp-directory at-rest surface,
        // even though it lives for one call. What lands there is the bundle's
        // own code-derived ciphertext, never the identity.
        keystore::secret_trace::record(
            keystore::secret_trace::SecretOp::Write,
            keystore::secret_trace::SecretClass::RecoveryPackage,
            keystore::secret_trace::Protection::UserDerivedAead,
            "osl_privacy_hub::device_transfer::open_exported_identity",
            &scratch,
            staged_bytes,
        );
        let transfer_sealer = TransferCodeSealer::derive(one_time_code, transfer_identifier)?;
        keystore::load_identity(&scratch, &transfer_sealer)
            .map_err(|_| "OSL transfer bundle could not be opened".to_owned())
    })();
    let _ = fs::remove_file(&scratch);
    let _ = fs::remove_dir(&scratch_dir);
    result
}

/// Verify and install a transfer bundle on the destination device.
///
/// All package identity and ciphertext integrity checks happen before the
/// destination profile path is created or overwritten.
pub fn restore_exported_identity(
    bundle: &TransferBundle,
    one_time_code: &str,
    transfer_identifier: &str,
    destination_identity_path: &Path,
    destination_sealer: &dyn Sealer,
) -> Result<Identity, String> {
    let identity = open_exported_identity(bundle, one_time_code, transfer_identifier)?;
    if let Some(parent) = destination_identity_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|_| "OSL transfer destination profile is unavailable".to_owned())?;
    }
    keystore::save_identity(destination_identity_path, &identity, destination_sealer)
        .map_err(|_| "OSL transfer destination profile could not be created".to_owned())?;
    Ok(identity)
}

fn seal_identity_for_transfer(
    identity: &Identity,
    one_time_code: &str,
    transfer_identifier: &str,
) -> Result<TransferBundle, String> {
    let (scratch_dir, scratch) = scratch_path()?;
    let result = (|| {
        let transfer_sealer = TransferCodeSealer::derive(one_time_code, transfer_identifier)?;
        keystore::save_identity(&scratch, identity, &transfer_sealer)
            .map_err(|_| "OSL could not seal the transfer bundle".to_owned())?;
        let sealed_identity =
            fs::read(&scratch).map_err(|_| "OSL could not read the transfer bundle".to_owned())?;
        keystore::secret_trace::record(
            keystore::secret_trace::SecretOp::Write,
            keystore::secret_trace::SecretClass::RecoveryPackage,
            keystore::secret_trace::Protection::UserDerivedAead,
            "osl_privacy_hub::device_transfer::export_identity_bundle",
            &scratch,
            sealed_identity.len(),
        );
        Ok(TransferBundle {
            version: TRANSFER_BUNDLE_VERSION,
            transfer_identifier: transfer_identifier.to_owned(),
            sealed_identity_b64: STANDARD.encode(sealed_identity),
        })
    })();
    let _ = fs::remove_file(&scratch);
    let _ = fs::remove_dir(&scratch_dir);
    result
}

fn validate_transfer_inputs(one_time_code: &str, transfer_identifier: &str) -> Result<(), String> {
    if one_time_code.is_empty() || transfer_identifier.is_empty() {
        return Err("OSL transfer code and destination identifier are required".to_owned());
    }
    Ok(())
}

fn scratch_path() -> Result<(PathBuf, PathBuf), String> {
    let mut suffix = String::with_capacity(32);
    for byte in random::random_bytes(16) {
        use std::fmt::Write;
        write!(&mut suffix, "{byte:02x}").expect("writing to a String cannot fail");
    }
    let directory = std::env::temp_dir().join(format!("osl-transfer-{suffix}"));
    fs::create_dir(&directory)
        .map_err(|_| "OSL transfer staging path is unavailable".to_owned())?;
    Ok((directory.clone(), directory.join("identity.json")))
}

struct TransferCodeSealer {
    key: aead::Key,
    transfer_identifier: Vec<u8>,
}

impl TransferCodeSealer {
    fn derive(one_time_code: &str, transfer_identifier: &str) -> Result<Self, String> {
        let key = hkdf::derive_32(
            transfer_identifier.as_bytes(),
            one_time_code.as_bytes(),
            TRANSFER_CODE_KDF_DOMAIN,
        )
        .map_err(|_| "OSL could not derive a transfer key".to_owned())?;
        Ok(Self {
            key: aead::Key::from_bytes(key),
            transfer_identifier: transfer_identifier.as_bytes().to_vec(),
        })
    }
}

impl Sealer for TransferCodeSealer {
    fn method_label(&self) -> &'static str {
        TRANSFER_SEALER_METHOD
    }

    fn is_tpm_backed(&self) -> bool {
        false
    }

    fn requires_insecure_banner(&self) -> bool {
        false
    }

    fn seal(&self, plaintext: &[u8]) -> keystore::sealer::Result<Vec<u8>> {
        let nonce = random::random_nonce();
        let ciphertext = aead::seal(&self.key, &nonce, &self.transfer_identifier, plaintext)?;
        let mut sealed = Vec::with_capacity(aead::NONCE_SIZE + ciphertext.len());
        sealed.extend_from_slice(nonce.as_bytes());
        sealed.extend_from_slice(&ciphertext);
        Ok(sealed)
    }

    fn unseal(&self, ciphertext: &[u8]) -> keystore::sealer::Result<zeroize::Zeroizing<Vec<u8>>> {
        if ciphertext.len() < aead::NONCE_SIZE + aead::TAG_SIZE {
            return Err(keystore::SealerError::Malformed(
                "transfer ciphertext is too short".to_owned(),
            ));
        }
        let mut nonce_bytes = [0_u8; aead::NONCE_SIZE];
        nonce_bytes.copy_from_slice(&ciphertext[..aead::NONCE_SIZE]);
        let nonce = aead::Nonce::from_bytes(nonce_bytes);
        let plaintext = aead::open(
            &self.key,
            &nonce,
            &self.transfer_identifier,
            &ciphertext[aead::NONCE_SIZE..],
        )?;
        Ok(zeroize::Zeroizing::new(plaintext))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn test_path(name: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "osl-device-transfer-{name}-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn exported_identity_requires_its_one_time_code() {
        let source_path = test_path("source");
        let source_sealer = keystore::MemorySealer::new();
        let identity = keystore::identity_from_entropy([29; 16], "transfer-test".to_owned());
        keystore::save_identity(&source_path, &identity, &source_sealer).unwrap();

        let bundle = export_identity_bundle(
            &source_path,
            &source_sealer,
            "624891",
            "destination-transfer-id",
        )
        .expect("a transfer bundle is produced");
        let opened = open_exported_identity(&bundle, "624891", "destination-transfer-id")
            .expect("the matching code opens the bundle");
        assert_eq!(opened.user_id, identity.user_id);
        assert!(
            open_exported_identity(&bundle, "000000", "destination-transfer-id").is_err(),
            "a wrong code must fail closed"
        );

        let _ = fs::remove_file(source_path);
    }

    #[test]
    fn task_0456_package_identity_and_integrity_stop_before_profile_creation() {
        let source_path = test_path("source-task-0456");
        let source_sealer = keystore::MemorySealer::new();
        let destination_sealer = keystore::MemorySealer::new();
        let identity = keystore::identity_from_entropy([45; 16], "task-0456-source".to_owned());
        keystore::save_identity(&source_path, &identity, &source_sealer).unwrap();

        let bundle = export_identity_bundle(
            &source_path,
            &source_sealer,
            "624891",
            "destination-transfer-id",
        )
        .expect("a transfer bundle is produced");

        let intact_dir = tempfile::TempDir::new().unwrap();
        let intact_profile = intact_dir.path().join("identity.json");
        let restored = restore_exported_identity(
            &bundle,
            "624891",
            "destination-transfer-id",
            &intact_profile,
            &destination_sealer,
        )
        .expect("an intact package creates a destination profile");
        let intact_created_profiles = usize::from(intact_profile.exists());
        assert_eq!(restored.user_id, identity.user_id);
        assert_eq!(intact_created_profiles, 1);

        let wrong_identity_dir = tempfile::TempDir::new().unwrap();
        let wrong_identity_profile = wrong_identity_dir.path().join("identity.json");
        let wrong_identity_error = match restore_exported_identity(
            &bundle,
            "624891",
            "other-destination-transfer-id",
            &wrong_identity_profile,
            &destination_sealer,
        ) {
            Ok(_) => panic!("a package for another destination must not create a profile"),
            Err(error) => error,
        };
        let wrong_identity_created_profiles = usize::from(wrong_identity_profile.exists());
        assert!(
            wrong_identity_error.contains("not for this destination"),
            "{wrong_identity_error}"
        );
        assert_eq!(wrong_identity_created_profiles, 0);

        let mut changed_package = bundle.clone();
        let mut sealed_identity = STANDARD
            .decode(&changed_package.sealed_identity_b64)
            .unwrap();
        let changed_byte_index = sealed_identity
            .len()
            .checked_sub(1)
            .expect("sealed transfer package is non-empty");
        sealed_identity[changed_byte_index] ^= 0x01;
        changed_package.sealed_identity_b64 = STANDARD.encode(sealed_identity);

        let tampered_dir = tempfile::TempDir::new().unwrap();
        let tampered_profile = tampered_dir.path().join("identity.json");
        let tampered_error = match restore_exported_identity(
            &changed_package,
            "624891",
            "destination-transfer-id",
            &tampered_profile,
            &destination_sealer,
        ) {
            Ok(_) => panic!("a tampered package must not create a profile"),
            Err(error) => error,
        };
        let tampered_created_profiles = usize::from(tampered_profile.exists());
        assert!(
            tampered_error.contains("could not be opened"),
            "{tampered_error}"
        );
        assert_eq!(tampered_created_profiles, 0);

        println!("TASK0456 intact_restore_created_profiles={intact_created_profiles}");
        println!("TASK0456 package_identity_created_profiles={wrong_identity_created_profiles}");
        println!("TASK0456 changed_package_bytes=1");
        println!("TASK0456 changed_package_byte_index={changed_byte_index}");
        println!("TASK0456 tampered_restore_created_profiles={tampered_created_profiles}");
        println!("TASK0456 tampered_restore_result=refused_before_profile_create");
        println!("TASK0456 tampered_restore_error={tampered_error}");

        let _ = fs::remove_file(source_path);
    }
}
