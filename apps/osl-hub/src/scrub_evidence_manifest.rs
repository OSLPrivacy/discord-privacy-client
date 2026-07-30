//! Content-bound evidence manifest for scrub runs.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

pub const SCRUB_EVIDENCE_MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScrubEvidenceTarget {
    pub service_id: String,
    pub account_id: String,
    pub scope_key: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScrubEvidenceManifest {
    pub schema_version: u32,
    pub commit: String,
    pub repository_tree: String,
    pub executable_sha256: String,
    pub target: ScrubEvidenceTarget,
    pub manifest_sha256: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrubEvidenceManifestError {
    InvalidSchema,
    InvalidCommit,
    InvalidRepositoryTree,
    InvalidExecutableHash,
    InvalidTarget,
    ManifestDigestMismatch,
}

impl fmt::Display for ScrubEvidenceManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidSchema => "scrub evidence manifest schema is invalid",
            Self::InvalidCommit => "scrub evidence manifest commit is invalid",
            Self::InvalidRepositoryTree => "scrub evidence manifest repository tree is invalid",
            Self::InvalidExecutableHash => "scrub evidence manifest executable hash is invalid",
            Self::InvalidTarget => "scrub evidence manifest target is invalid",
            Self::ManifestDigestMismatch => "scrub evidence manifest digest mismatch",
        })
    }
}

impl std::error::Error for ScrubEvidenceManifestError {}

impl ScrubEvidenceManifest {
    pub fn new(
        commit: impl Into<String>,
        repository_tree: impl Into<String>,
        executable_sha256: impl Into<String>,
        target: ScrubEvidenceTarget,
    ) -> Result<Self, ScrubEvidenceManifestError> {
        let mut manifest = Self {
            schema_version: SCRUB_EVIDENCE_MANIFEST_SCHEMA_VERSION,
            commit: commit.into(),
            repository_tree: repository_tree.into(),
            executable_sha256: executable_sha256.into(),
            target,
            manifest_sha256: String::new(),
        };
        validate_fields(&manifest)?;
        manifest.manifest_sha256 = manifest_digest(&manifest);
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), ScrubEvidenceManifestError> {
        validate_fields(self)?;
        if self.manifest_sha256 != manifest_digest(self) {
            return Err(ScrubEvidenceManifestError::ManifestDigestMismatch);
        }
        Ok(())
    }
}

fn validate_fields(manifest: &ScrubEvidenceManifest) -> Result<(), ScrubEvidenceManifestError> {
    if manifest.schema_version != SCRUB_EVIDENCE_MANIFEST_SCHEMA_VERSION {
        return Err(ScrubEvidenceManifestError::InvalidSchema);
    }
    if !git_object(&manifest.commit) {
        return Err(ScrubEvidenceManifestError::InvalidCommit);
    }
    if !git_object(&manifest.repository_tree) {
        return Err(ScrubEvidenceManifestError::InvalidRepositoryTree);
    }
    if !sha256_hex(&manifest.executable_sha256) {
        return Err(ScrubEvidenceManifestError::InvalidExecutableHash);
    }
    if !bounded_id(&manifest.target.service_id)
        || !bounded_id(&manifest.target.account_id)
        || !bounded_scope(&manifest.target.scope_key)
    {
        return Err(ScrubEvidenceManifestError::InvalidTarget);
    }
    Ok(())
}

fn manifest_digest(manifest: &ScrubEvidenceManifest) -> String {
    let mut digest = Sha256::new();
    absorb(&mut digest, "osl-scrub-evidence-manifest-v1");
    absorb(&mut digest, &manifest.schema_version.to_string());
    absorb(&mut digest, &manifest.commit);
    absorb(&mut digest, &manifest.repository_tree);
    absorb(&mut digest, &manifest.executable_sha256);
    absorb(&mut digest, &manifest.target.service_id);
    absorb(&mut digest, &manifest.target.account_id);
    absorb(&mut digest, &manifest.target.scope_key);
    format!("{:x}", digest.finalize())
}

fn absorb(digest: &mut Sha256, value: &str) {
    digest.update((value.len() as u64).to_le_bytes());
    digest.update(value.as_bytes());
}

fn git_object(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        && value.bytes().any(|b| b != b'0')
}

fn sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        && value.bytes().any(|b| b != b'0')
}

fn bounded_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b':' | b'_' | b'-'))
}

fn bounded_scope(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b':' | b'_' | b'-' | b'/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> ScrubEvidenceTarget {
        ScrubEvidenceTarget {
            service_id: "discord".to_owned(),
            account_id: "account-f80".to_owned(),
            scope_key: "server_channel:server-f80/channel-f80".to_owned(),
        }
    }

    fn manifest() -> ScrubEvidenceManifest {
        ScrubEvidenceManifest::new("a".repeat(40), "b".repeat(40), "c".repeat(64), target())
            .unwrap()
    }

    #[test]
    fn scrub_evidence_manifest_schema_binds_commit_tree_executable_and_target() {
        let manifest = manifest();
        manifest.validate().unwrap();
        assert_eq!(
            manifest.schema_version,
            SCRUB_EVIDENCE_MANIFEST_SCHEMA_VERSION
        );
        assert!(sha256_hex(&manifest.manifest_sha256));

        let mut swapped_commit = manifest.clone();
        swapped_commit.commit = "d".repeat(40);
        assert_eq!(
            swapped_commit.validate(),
            Err(ScrubEvidenceManifestError::ManifestDigestMismatch)
        );

        let mut swapped_tree = manifest.clone();
        swapped_tree.repository_tree = "e".repeat(40);
        assert_eq!(
            swapped_tree.validate(),
            Err(ScrubEvidenceManifestError::ManifestDigestMismatch)
        );

        let mut swapped_executable = manifest.clone();
        swapped_executable.executable_sha256 = "f".repeat(64);
        assert_eq!(
            swapped_executable.validate(),
            Err(ScrubEvidenceManifestError::ManifestDigestMismatch)
        );

        let mut swapped_target = manifest.clone();
        swapped_target.target.account_id = "account-other-f80".to_owned();
        assert_eq!(
            swapped_target.validate(),
            Err(ScrubEvidenceManifestError::ManifestDigestMismatch)
        );

        let invalid =
            ScrubEvidenceManifest::new("not-a-commit", "b".repeat(40), "c".repeat(64), target());
        assert_eq!(invalid, Err(ScrubEvidenceManifestError::InvalidCommit));
    }
}
