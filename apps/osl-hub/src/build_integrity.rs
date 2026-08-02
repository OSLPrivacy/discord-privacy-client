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

/// Offline publication status for a digest a peer reported.  This lookup is
/// intentionally only against the bundled signed list; it never contacts Rekor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublishedHash {
    Published,
    Unpublished,
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
    check_executable(
        &executable,
        BUNDLED_MANIFEST,
        BUNDLED_SIGNATURE,
        updater_public_key(),
    )
}

/// Look up a peer-reported raw SHA-256 digest without any runtime network call.
pub fn published_hash_status(digest: &[u8; 32]) -> PublishedHash {
    published_hash_status_from_assets(
        digest,
        BUNDLED_MANIFEST,
        BUNDLED_SIGNATURE,
        updater_public_key(),
    )
}

fn published_hash_status_from_assets(
    digest: &[u8; 32],
    manifest: &[u8],
    signature: &[u8],
    public_key: Option<String>,
) -> PublishedHash {
    let Some(hashes) = verified_hashes(manifest, signature, public_key) else {
        return PublishedHash::Unknown;
    };
    if hashes.contains(&hex_digest(digest)) {
        PublishedHash::Published
    } else {
        PublishedHash::Unpublished
    }
}

fn updater_public_key() -> Option<String> {
    // Bind the parsed Value first: chaining straight into as_str() borrowed a
    // temporary that was dropped at the end of the statement.
    let config = serde_json::from_str::<serde_json::Value>(UPDATER_CONFIG).ok()?;
    let encoded = config
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
    let Some(hashes) = verified_hashes(manifest_bytes, signature_bytes, public_key) else {
        return BuildIntegrity::Unknown;
    };
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

fn verified_hashes(
    manifest_bytes: &[u8],
    signature_bytes: &[u8],
    public_key: Option<String>,
) -> Option<HashSet<String>> {
    let public_key = PublicKey::decode(&public_key?).ok()?;
    let signature = Signature::decode(std::str::from_utf8(signature_bytes).ok()?).ok()?;
    public_key.verify(manifest_bytes, &signature, false).ok()?;
    let manifest = serde_json::from_slice::<Manifest>(manifest_bytes).ok()?;
    if manifest.format != 1 || manifest.builds.is_empty() {
        return None;
    }
    let mut hashes = HashSet::new();
    for build in manifest.builds {
        if !valid_sha256(&build.exe_sha256) || !hashes.insert(build.exe_sha256) {
            return None;
        }
    }
    Some(hashes)
}

fn hex_digest(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| {
            byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte.is_ascii_hexdigit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // A fixed independent test key and real pre-hashed minisign signature. This
    // keeps the test on the cryptographic verification path rather than a mock.
    const TEST_PUBLIC_KEY: &str =
        "untrusted comment: test\nRWQxMjM0NTY3OCslv3Koov09Jl3NBvNsBHzCFgomynJIU0sAdlI6QIJh";
    const TEST_SIGNATURE: &str = "untrusted comment: test\nRUQxMjM0NTY3OOxIzrHTGVRKDNp9td93JLkhGeRapaKP1Q8RlVdIHb2TmkSEBH6DMcmrfem8idFWB5MpLtJw0gjhFAkF8PrqpgE=\ntrusted comment: timestamp:1555779966\\tfile:build-hashes.json\nnPQfnEwYG7SvVDHpUZgcjd1tG2k7SKT4FWOHcLFNvq7yhxO0ada/qcYf8FaYtzUKSPf9bzzBL7BoiuDxa/H8Dg==";
    const TEST_MANIFEST: &[u8] = br#"{"format":1,"builds":[{"exe_sha256":"9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"}]}"#;

    #[test]
    fn tampered_executable_is_a_mismatch_and_missing_bundle_is_unknown() {
        let mut executable = NamedTempFile::new().unwrap();
        executable.write_all(b"test").unwrap();
        assert_eq!(
            check_executable(
                executable.path(),
                TEST_MANIFEST,
                TEST_SIGNATURE.as_bytes(),
                Some(TEST_PUBLIC_KEY.into())
            ),
            BuildIntegrity::Verified,
        );

        executable.write_all(b" appended byte").unwrap();
        executable.flush().unwrap();
        assert_eq!(
            check_executable(
                executable.path(),
                TEST_MANIFEST,
                TEST_SIGNATURE.as_bytes(),
                Some(TEST_PUBLIC_KEY.into())
            ),
            BuildIntegrity::Mismatch,
        );
        assert_eq!(
            check_executable(executable.path(), &[], &[], Some(TEST_PUBLIC_KEY.into())),
            BuildIntegrity::Unknown,
        );
    }

    #[test]
    fn fabricated_hash_is_unpublished_without_a_transparency_log_query() {
        let published = [
            0x9f, 0x86, 0xd0, 0x81, 0x88, 0x4c, 0x7d, 0x65, 0x9a, 0x2f, 0xea, 0xa0, 0xc5, 0x5a,
            0xd0, 0x15, 0xa3, 0xbf, 0x4f, 0x1b, 0x2b, 0x0b, 0x82, 0x2c, 0xd1, 0x5d, 0x6c, 0x15,
            0xb0, 0xf0, 0x0a, 0x08,
        ];
        assert_eq!(
            published_hash_status_from_assets(
                &published,
                TEST_MANIFEST,
                TEST_SIGNATURE.as_bytes(),
                Some(TEST_PUBLIC_KEY.into()),
            ),
            PublishedHash::Published,
        );
        assert_eq!(
            published_hash_status_from_assets(
                &[0x42; 32],
                TEST_MANIFEST,
                TEST_SIGNATURE.as_bytes(),
                Some(TEST_PUBLIC_KEY.into()),
            ),
            PublishedHash::Unpublished,
        );
    }
}
