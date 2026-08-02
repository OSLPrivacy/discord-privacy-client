//! Resumable, fail-closed installation of optional local model artifacts.
//!
//! A model is never exposed to the runtime until its bytes match the digest
//! pinned by the shipping binary *and* `TrustedModelPack` accepts its signed
//! metadata. Any failure has a truthful non-fatal result so callers keep using
//! the built-in word-bank carrier.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use cover_draft::{
    ModelPackMetadata, ModelPackSignatureVerifier, ObservedArtifact, TrustedModelPack,
};
use sha2::{Digest, Sha256};

const COPY_BUFFER_BYTES: usize = 64 * 1024;

/// The artifact identity compiled into the app for a selected model release.
///
/// The caller must construct this from the binary's model-release constants,
/// not from downloaded metadata. The metadata is separately signature-checked
/// below so a compromised download cannot choose a new digest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinnedModelArtifact {
    pub sha256: [u8; 32],
    pub size: u64,
}

pub struct ModelPackInstallPlan {
    pub artifact_url: String,
    pub destination: PathBuf,
    pub pinned: PinnedModelArtifact,
    pub metadata: ModelPackMetadata,
}

/// The download backend must honour a byte-range request. Keeping networking
/// behind this trait leaves the integrity boundary independent of HTTP client
/// choice and lets the installer be exercised without a network.
pub trait ModelPackDownload {
    fn open_range(&self, url: &str, start: u64) -> Result<RangeResponse, String>;
}

pub struct RangeResponse {
    /// The start offset asserted by the server's `Content-Range` response.
    pub start: u64,
    pub body: Box<dyn Read>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum InstallFallback {
    DownloadFailed,
    RangeNotHonoured,
    ArtifactTooLarge,
    IntegrityRejected,
    StorageFailed,
}

/// A successful installation is safe to give to `LlamaCppLocalCoverModel`.
/// Every other outcome deliberately retains the always-available word bank.
#[derive(Debug, PartialEq, Eq)]
pub enum ModelPackInstallOutcome {
    Installed {
        artifact_path: PathBuf,
        trusted_pack: TrustedModelPack,
    },
    WordBankFallback(InstallFallback),
}

/// The only disclosure emitted when a user removes the optional AI model.
///
/// This is an event, rather than an "AI installed" preference bit: callers
/// show it for the completed removal and do not repeat it on later sends.
/// Later sends discover availability from the artifact on disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WordBankFallbackNotice;

impl WordBankFallbackNotice {
    pub const MESSAGE: &'static str =
        "Local AI cover text was removed. Future sends will use the word-bank carrier.";
}

/// The result of removing the optional model artifact.
///
/// A failed removal is deliberately not reported as a fallback: the artifact
/// may still be usable, and claiming otherwise would be misleading.
#[derive(Debug, PartialEq, Eq)]
pub enum ModelPackRemovalOutcome {
    Removed {
        word_bank_notice: WordBankFallbackNotice,
    },
    Retained {
        error_kind: io::ErrorKind,
    },
}

/// Whether the optional model can be selected for a send.
///
/// This consults the artifact itself instead of an installed flag. In
/// particular, a successful removal cannot leave later sends believing the
/// model exists.
pub fn model_pack_available(destination: &Path) -> bool {
    fs::metadata(destination).is_ok_and(|metadata| metadata.is_file())
}

/// Remove the optional model without affecting the always-available carrier.
///
/// On success the returned event tells the UI, once and plainly, that the next
/// send uses the word bank. If deletion fails (for example, a locked file),
/// report that honestly and leave selection to `model_pack_available`.
pub fn remove_or_keep_word_bank(destination: &Path) -> ModelPackRemovalOutcome {
    match fs::remove_file(destination) {
        Ok(()) => ModelPackRemovalOutcome::Removed {
            word_bank_notice: WordBankFallbackNotice,
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => ModelPackRemovalOutcome::Removed {
            word_bank_notice: WordBankFallbackNotice,
        },
        Err(error) => ModelPackRemovalOutcome::Retained {
            error_kind: error.kind(),
        },
    }
}

/// Download (or resume), verify, then atomically promote an optional model.
///
/// Interrupted downloads remain as `destination.part`; integrity failures
/// remove that partial file so a later install cannot repeatedly resume known
/// corrupt bytes. No unverified artifact is ever promoted to `destination`.
pub fn install_or_fallback(
    plan: ModelPackInstallPlan,
    downloader: &dyn ModelPackDownload,
    verifier: &dyn ModelPackSignatureVerifier,
) -> ModelPackInstallOutcome {
    match install(plan, downloader, verifier) {
        Ok((artifact_path, trusted_pack)) => ModelPackInstallOutcome::Installed {
            artifact_path,
            trusted_pack,
        },
        Err(fallback) => ModelPackInstallOutcome::WordBankFallback(fallback),
    }
}

fn install(
    plan: ModelPackInstallPlan,
    downloader: &dyn ModelPackDownload,
    verifier: &dyn ModelPackSignatureVerifier,
) -> Result<(PathBuf, TrustedModelPack), InstallFallback> {
    if plan.metadata.artifact_digest != plan.pinned.sha256
        || plan.metadata.artifact_size != plan.pinned.size
    {
        return Err(InstallFallback::IntegrityRejected);
    }
    if let Some(parent) = plan.destination.parent() {
        fs::create_dir_all(parent).map_err(|_| InstallFallback::StorageFailed)?;
    }

    let partial = partial_path(&plan.destination);
    let offset = partial_size(&partial)?;
    if offset > plan.pinned.size {
        discard_partial(&partial);
        return Err(InstallFallback::ArtifactTooLarge);
    }

    if offset < plan.pinned.size {
        let mut response = downloader
            .open_range(&plan.artifact_url, offset)
            .map_err(|_| InstallFallback::DownloadFailed)?;
        if response.start != offset {
            return Err(InstallFallback::RangeNotHonoured);
        }
        match append_bounded(&partial, &mut response.body, plan.pinned.size - offset) {
            Ok(()) => {}
            Err(InstallFallback::ArtifactTooLarge) => {
                discard_partial(&partial);
                return Err(InstallFallback::ArtifactTooLarge);
            }
            Err(fallback) => return Err(fallback),
        }
    }

    let observed = observe_artifact(&partial)?;
    if observed.digest != plan.pinned.sha256 || observed.size != plan.pinned.size {
        discard_partial(&partial);
        return Err(InstallFallback::IntegrityRejected);
    }
    let trusted_pack = TrustedModelPack::verify(plan.metadata, observed, verifier)
        .map_err(|_| InstallFallback::IntegrityRejected)?;

    fs::rename(&partial, &plan.destination).map_err(|_| InstallFallback::StorageFailed)?;
    Ok((plan.destination, trusted_pack))
}

fn partial_path(destination: &Path) -> PathBuf {
    let mut partial = destination.as_os_str().to_owned();
    partial.push(".part");
    PathBuf::from(partial)
}

fn partial_size(path: &Path) -> Result<u64, InstallFallback> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(_) => Err(InstallFallback::StorageFailed),
    }
}

fn append_bounded(path: &Path, body: &mut dyn Read, remaining: u64) -> Result<(), InstallFallback> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|_| InstallFallback::StorageFailed)?;
    let mut written = 0_u64;
    let mut buffer = [0_u8; COPY_BUFFER_BYTES];
    loop {
        let read = body
            .read(&mut buffer)
            .map_err(|_| InstallFallback::DownloadFailed)?;
        if read == 0 {
            break;
        }
        written = written
            .checked_add(read as u64)
            .ok_or(InstallFallback::ArtifactTooLarge)?;
        if written > remaining {
            return Err(InstallFallback::ArtifactTooLarge);
        }
        file.write_all(&buffer[..read])
            .map_err(|_| InstallFallback::StorageFailed)?;
    }
    file.sync_all()
        .map_err(|_| InstallFallback::StorageFailed)?;
    if written == remaining {
        Ok(())
    } else {
        Err(InstallFallback::DownloadFailed)
    }
}

fn observe_artifact(path: &Path) -> Result<ObservedArtifact, InstallFallback> {
    let mut file = File::open(path).map_err(|_| InstallFallback::StorageFailed)?;
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; COPY_BUFFER_BYTES];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| InstallFallback::StorageFailed)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size = size
            .checked_add(read as u64)
            .ok_or(InstallFallback::ArtifactTooLarge)?;
    }
    Ok(ObservedArtifact {
        digest: hasher.finalize().into(),
        size,
    })
}

fn discard_partial(path: &Path) {
    if fs::remove_file(path).is_err() {
        // Failure to discard must not promote the file. A later attempt will
        // re-check it before use and remain on the word bank if it is corrupt.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::io::Cursor;

    struct Download {
        bytes: Vec<u8>,
        starts: RefCell<Vec<u64>>,
    }

    impl ModelPackDownload for Download {
        fn open_range(&self, _url: &str, start: u64) -> Result<RangeResponse, String> {
            self.starts.borrow_mut().push(start);
            Ok(RangeResponse {
                start,
                body: Box::new(Cursor::new(self.bytes[start as usize..].to_vec())),
            })
        }
    }

    struct AcceptingVerifier;

    impl ModelPackSignatureVerifier for AcceptingVerifier {
        fn verify(&self, _metadata_digest: &[u8; 32], _signature: &[u8]) -> bool {
            true
        }
    }

    fn plan(destination: PathBuf, bytes: &[u8]) -> ModelPackInstallPlan {
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        ModelPackInstallPlan {
            artifact_url: "https://osl-installers.example/model.gguf".to_owned(),
            destination,
            pinned: PinnedModelArtifact {
                sha256: digest,
                size: bytes.len() as u64,
            },
            metadata: ModelPackMetadata {
                model_id: "osl-cover-small.gguf".to_owned(),
                version_digest: [7; 32],
                artifact_digest: digest,
                artifact_size: bytes.len() as u64,
                max_working_set_bytes: 1024 * 1024,
                signature: vec![9; 64],
            },
        }
    }

    #[test]
    fn resumes_partial_download_then_promotes_only_a_verified_artifact() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("model.gguf");
        let bytes = b"small but real model bytes";
        fs::write(partial_path(&destination), &bytes[..8]).unwrap();
        let download = Download {
            bytes: bytes.to_vec(),
            starts: RefCell::new(Vec::new()),
        };

        let outcome = install_or_fallback(
            plan(destination.clone(), bytes),
            &download,
            &AcceptingVerifier,
        );

        assert!(matches!(outcome, ModelPackInstallOutcome::Installed { .. }));
        assert_eq!(*download.starts.borrow(), vec![8]);
        assert_eq!(fs::read(&destination).unwrap(), bytes);
        assert!(!partial_path(&destination).exists());
    }

    #[test]
    fn corrupt_artifact_falls_back_without_promoting_it() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("model.gguf");
        let intended = b"small but real model bytes";
        let mut corrupt = intended.to_vec();
        corrupt[3] ^= 0xff;
        let download = Download {
            bytes: corrupt,
            starts: RefCell::new(Vec::new()),
        };

        let outcome = install_or_fallback(
            plan(destination.clone(), intended),
            &download,
            &AcceptingVerifier,
        );

        assert_eq!(
            outcome,
            ModelPackInstallOutcome::WordBankFallback(InstallFallback::IntegrityRejected)
        );
        assert!(!destination.exists());
        assert!(!partial_path(&destination).exists());
    }

    #[test]
    fn removal_makes_the_next_send_use_the_word_bank_with_one_plain_notice() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("model.gguf");
        fs::write(&destination, b"installed model bytes").unwrap();
        assert!(model_pack_available(&destination));

        let outcome = remove_or_keep_word_bank(&destination);

        assert_eq!(
            outcome,
            ModelPackRemovalOutcome::Removed {
                word_bank_notice: WordBankFallbackNotice,
            }
        );
        assert_eq!(
            WordBankFallbackNotice::MESSAGE,
            "Local AI cover text was removed. Future sends will use the word-bank carrier."
        );
        assert!(
            !model_pack_available(&destination),
            "selection must read the artifact, not a stale installed flag"
        );
    }

    #[test]
    fn failed_removal_does_not_falsely_claim_a_word_bank_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("model.gguf");
        fs::create_dir(&destination).unwrap();

        let outcome = remove_or_keep_word_bank(&destination);

        assert!(matches!(outcome, ModelPackRemovalOutcome::Retained { .. }));
        assert!(destination.exists());
    }
}
