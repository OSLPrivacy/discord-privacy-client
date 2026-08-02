//! Local build-hash self-checking.
//!
//! This is deliberately a local comparison only.  A process that an attacker can
//! modify can also have this code modified, so `Verified` detects corruption and
//! failed updates but is not an attestation of the running machine.

use std::{collections::HashSet, fs, path::Path};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use minisign_verify::{PublicKey, Signature};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const UPDATER_CONFIG: &str = include_str!("../tauri.conf.json");
const BUNDLED_MANIFEST: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/build-hashes.json"));
const BUNDLED_SIGNATURE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/build-hashes.json.sig"));

/// The outcome displayed to the person using this copy of OSL.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildIntegrity {
    /// The executable's digest is in a valid, signed published-hash list.
    Verified,
    /// The list is valid but does not name this executable's digest.
    Mismatch,
    /// The local list could not be loaded, parsed, or authenticated.
    Unknown,
}

#[derive(Deserialize)]
struct Manifest {
    format: u8,
    builds: Vec<Build>,
}

#[derive(Deserialize)]
struct Build {
    exe_sha256: String,
}

/// Check the current executable against the manifest embedded by the build.
///
/// An absent build-time manifest intentionally produces `Unknown`; it must never
/// become a positive result merely because a developer or packager omitted it.
pub fn check_current() -> BuildIntegrity {
    let Ok(executable) = std::env::current_exe() else {
        return BuildIntegrity::Unknown;
    };
    check_executable(&executable, BUNDLED_MANIFEST, BUNDLED_SIGNATURE, updater_public_key())
}

fn updater_public_key() -> Option<String> {
    let encoded = serde_json::from_str::<serde_json::Value>(UPDATER_CONFIG)
        .ok()?
        .get("plugins")?
        .get("updater")?
        .get("pubkey")?
        .as_str()?;
    String::from_utf8(STANDARD.decode(encoded).ok()?).ok()
}

fn check_executable(
    executable: &Path,
    manifest_bytes: &[u8],
    signature_bytes: &[u8],
    public_key: Option<String>,
) -> BuildIntegrity {
    let Some(public_key) = public_key else {
        return BuildIntegrity::Unknown;
    };
    let Ok(signature) = std::str::from_utf8(signature_bytes)
        .ok()
        .and_then(|text| Signature::decode(text).ok())
        .ok_or(())
    else {
        return BuildIntegrity::Unknown;
    };
    let Ok(public_key) = PublicKey::decode(&public_key) else {
        return BuildIntegrity::Unknown;
    };
    if public_key.verify(manifest_bytes, &signature, false).is_err() {
        return BuildIntegrity::Unknown;
    }
    let Ok(manifest) = serde_json::from_slice::<Manifest>(manifest_bytes) else {
        return BuildIntegrity::Unknown;
    };
    if manifest.format != 1 || manifest.builds.is_empty() {
        return BuildIntegrity::Unknown;
    }
    let mut hashes = HashSet::new();
    for build in manifest.builds {
        if !valid_sha256(&build.exe_sha256) || !hashes.insert(build.exe_sha256) {
            return BuildIntegrity::Unknown;
        }
    }
    let Ok(executable) = fs::read(executable) else {
        return BuildIntegrity::Unknown;
    };
    let digest = format!("{:x}", Sha256::digest(executable));
    if hashes.contains(&digest) {
        BuildIntegrity::Verified
    } else {
        BuildIntegrity::Mismatch
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // A fixed independent test key and real pre-hashed minisign signature. This
    // keeps the test on the cryptographic verification path rather than a mock.
    const TEST_PUBLIC_KEY: &str = "untrusted comment: test\nRWQxMjM0NTY3OCslv3Koov09Jl3NBvNsBHzCFgomynJIU0sAdlI6QIJh";
    const TEST_SIGNATURE: &str = "untrusted comment: test\nRUQxMjM0NTY3OOxIzrHTGVRKDNp9td93JLkhGeRapaKP1Q8RlVdIHb2TmkSEBH6DMcmrfem8idFWB5MpLtJw0gjhFAkF8PrqpgE=\ntrusted comment: timestamp:1555779966\\tfile:build-hashes.json\nnPQfnEwYG7SvVDHpUZgcjd1tG2k7SKT4FWOHcLFNvq7yhxO0ada/qcYf8FaYtzUKSPf9bzzBL7BoiuDxa/H8Dg==";
    const TEST_MANIFEST: &[u8] = br#"{"format":1,"builds":[{"exe_sha256":"9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"}]}"#;

    #[test]
    fn tampered_executable_is_a_mismatch_and_missing_bundle_is_unknown() {
        let mut executable = NamedTempFile::new().unwrap();
        executable.write_all(b"test").unwrap();
        assert_eq!(
            check_executable(executable.path(), TEST_MANIFEST, TEST_SIGNATURE.as_bytes(), Some(TEST_PUBLIC_KEY.into())),
            BuildIntegrity::Verified,
        );

        executable.write_all(b" appended byte").unwrap();
        executable.flush().unwrap();
        assert_eq!(
            check_executable(executable.path(), TEST_MANIFEST, TEST_SIGNATURE.as_bytes(), Some(TEST_PUBLIC_KEY.into())),
            BuildIntegrity::Mismatch,
        );
        assert_eq!(
            check_executable(executable.path(), &[], &[], Some(TEST_PUBLIC_KEY.into())),
            BuildIntegrity::Unknown,
        );
    }
}
