//! Bundled first-use model pack for local AI cover drafts.
//!
//! The pack bytes are compiled into the app and materialized only when the
//! install/profile path has no model file yet. Existing bytes are never trusted
//! by presence: the SHA-256 fingerprint is checked before metadata is verified
//! and before the writer parses the model file.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use cover_draft::{
    GenerationControl, LocalCoverModel, ModelError, ModelInput, ModelPackMetadata,
    ModelPackSignatureVerifier, ObservedArtifact, TrustedModelPack,
};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const MODEL_PACK_DOMAIN: &[u8] = b"osl-cover-draft-model-pack-v1";
const VERSION_DOMAIN: &[u8] = b"osl-bundled-covertext-version-v1";
const SIGNATURE_DOMAIN: &[u8] = b"osl-bundled-covertext-metadata-signature-v1";

pub const MODEL_FILE_NAME: &str = "osl-covertext-tiny-v1.oslmodel";
pub const MODEL_ID: &str = MODEL_FILE_NAME;
pub const VERSION_NUMBER: &str = "1.0.0";
pub const MAX_WORKING_SET_BYTES: u64 = 1024 * 1024;

const MODEL_BYTES: &[u8] = include_bytes!("../model-packs/osl-covertext-tiny-v1.oslmodel");

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BundledModelPackStatus {
    pub present: bool,
    pub version: &'static str,
    pub fingerprint: String,
    pub artifact_path: PathBuf,
}

#[derive(Debug, Eq, PartialEq)]
pub enum BundledModelPackError {
    Storage(String),
    RefusedByName {
        model_id: &'static str,
        expected_fingerprint: String,
        observed_fingerprint: String,
    },
    MetadataRejected,
    InvalidModelFile,
}

impl BundledModelPackError {
    pub fn refused_model_name(&self) -> Option<&'static str> {
        match self {
            Self::RefusedByName { model_id, .. } => Some(model_id),
            _ => None,
        }
    }
}

pub fn bundled_model_pack_path(install_root: &Path) -> PathBuf {
    install_root.join(MODEL_FILE_NAME)
}

/// Materialize the shipped pack on first use, then report the verified status.
///
/// If a file is already present it must match the pinned fingerprint. A changed
/// pack is refused by name instead of being overwritten into a passing state.
pub fn ensure_bundled_model_pack(
    install_root: &Path,
) -> Result<BundledModelPackStatus, BundledModelPackError> {
    fs::create_dir_all(install_root).map_err(storage)?;
    let path = bundled_model_pack_path(install_root);
    if !path.exists() {
        let partial = path.with_extension("oslmodel.part");
        fs::write(&partial, MODEL_BYTES).map_err(storage)?;
        fs::rename(&partial, &path).map_err(storage)?;
    }
    verified_status(&path)
}

pub fn verified_status(path: &Path) -> Result<BundledModelPackStatus, BundledModelPackError> {
    let (trusted_pack, bytes) = verify_existing_pack(path)?;
    parse_templates(&bytes)?;
    Ok(BundledModelPackStatus {
        present: true,
        version: VERSION_NUMBER,
        fingerprint: fingerprint_string(&trusted_pack.artifact_digest()),
        artifact_path: path.to_path_buf(),
    })
}

pub struct BundledCoverWriter {
    trusted_pack: TrustedModelPack,
    templates: Vec<String>,
    next: usize,
}

impl BundledCoverWriter {
    /// Load only after the model file fingerprint and metadata have been
    /// accepted. Tampered bytes never reach template parsing or generation.
    pub fn load(path: &Path) -> Result<Self, BundledModelPackError> {
        let (trusted_pack, bytes) = verify_existing_pack(path)?;
        let templates = parse_templates(&bytes)?;
        Ok(Self {
            trusted_pack,
            templates,
            next: 0,
        })
    }
}

impl LocalCoverModel for BundledCoverWriter {
    fn trusted_model_pack(&self) -> Option<&TrustedModelPack> {
        Some(&self.trusted_pack)
    }

    fn generate(
        &mut self,
        input: ModelInput<'_>,
        control: GenerationControl<'_>,
    ) -> Result<Zeroizing<String>, ModelError> {
        if control.should_stop() {
            return Err(ModelError::Cancelled);
        }
        if input.context().len() == 0 || self.templates.is_empty() {
            return Err(ModelError::Failed);
        }
        let template = &self.templates[self.next % self.templates.len()];
        self.next += 1;
        if template.len() > input.max_output_bytes {
            return Err(ModelError::Failed);
        }
        Ok(Zeroizing::new(template.clone()))
    }
}

fn verify_existing_pack(path: &Path) -> Result<(TrustedModelPack, Vec<u8>), BundledModelPackError> {
    let bytes = fs::read(path).map_err(storage)?;
    let observed_digest: [u8; 32] = Sha256::digest(&bytes).into();
    let expected_digest = bundled_artifact_digest();
    if observed_digest != expected_digest {
        return Err(BundledModelPackError::RefusedByName {
            model_id: MODEL_ID,
            expected_fingerprint: fingerprint_string(&expected_digest),
            observed_fingerprint: fingerprint_string(&observed_digest),
        });
    }
    let observed = ObservedArtifact {
        digest: observed_digest,
        size: bytes.len() as u64,
    };
    let trusted_pack = TrustedModelPack::verify(metadata(), observed, &BundledMetadataVerifier)
        .map_err(|_| BundledModelPackError::MetadataRejected)?;
    Ok((trusted_pack, bytes))
}

fn parse_templates(bytes: &[u8]) -> Result<Vec<String>, BundledModelPackError> {
    let body = std::str::from_utf8(bytes).map_err(|_| BundledModelPackError::InvalidModelFile)?;
    let mut lines = body.lines();
    if lines.next() != Some("OSL_COVERTEXT_MODEL_PACK v1") {
        return Err(BundledModelPackError::InvalidModelFile);
    }
    let mut saw_id = false;
    let mut saw_version = false;
    let mut templates = Vec::new();
    for line in lines {
        if line == format!("id={MODEL_ID}") {
            saw_id = true;
        } else if line == format!("version={VERSION_NUMBER}") {
            saw_version = true;
        } else if let Some(cover) = line.strip_prefix("cover=") {
            if cover.trim().is_empty() || cover.contains("DPC0::") || cover.contains("DPC1::") {
                return Err(BundledModelPackError::InvalidModelFile);
            }
            templates.push(cover.to_owned());
        }
    }
    if saw_id && saw_version && !templates.is_empty() {
        Ok(templates)
    } else {
        Err(BundledModelPackError::InvalidModelFile)
    }
}

fn metadata() -> ModelPackMetadata {
    let version_digest = version_digest();
    let artifact_digest = bundled_artifact_digest();
    let artifact_size = MODEL_BYTES.len() as u64;
    let metadata_digest =
        canonical_metadata_digest(MODEL_ID, &version_digest, &artifact_digest, artifact_size);
    ModelPackMetadata {
        model_id: MODEL_ID.to_owned(),
        version_digest,
        artifact_digest,
        artifact_size,
        max_working_set_bytes: MAX_WORKING_SET_BYTES,
        signature: bundled_signature(&metadata_digest).to_vec(),
    }
}

fn version_digest() -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(VERSION_DOMAIN);
    hash_field(&mut hash, VERSION_NUMBER.as_bytes());
    hash.finalize().into()
}

fn bundled_artifact_digest() -> [u8; 32] {
    Sha256::digest(MODEL_BYTES).into()
}

fn canonical_metadata_digest(
    model_id: &str,
    version_digest: &[u8; 32],
    artifact_digest: &[u8; 32],
    artifact_size: u64,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(MODEL_PACK_DOMAIN);
    hash_field(&mut hash, model_id.as_bytes());
    hash_field(&mut hash, version_digest);
    hash_field(&mut hash, artifact_digest);
    hash.update(artifact_size.to_be_bytes());
    hash.update(MAX_WORKING_SET_BYTES.to_be_bytes());
    hash.finalize().into()
}

fn bundled_signature(metadata_digest: &[u8; 32]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(SIGNATURE_DOMAIN);
    hash_field(&mut hash, metadata_digest);
    hash.finalize().into()
}

struct BundledMetadataVerifier;

impl ModelPackSignatureVerifier for BundledMetadataVerifier {
    fn verify(&self, metadata_digest: &[u8; 32], signature: &[u8]) -> bool {
        signature == bundled_signature(metadata_digest)
    }
}

fn hash_field(hash: &mut Sha256, value: &[u8]) {
    hash.update((value.len() as u64).to_be_bytes());
    hash.update(value);
}

fn fingerprint_string(digest: &[u8; 32]) -> String {
    format!("sha256:{}", hex_digest(digest))
}

fn hex_digest(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn storage(error: io::Error) -> BundledModelPackError {
    BundledModelPackError::Storage(error.kind().to_string())
}
