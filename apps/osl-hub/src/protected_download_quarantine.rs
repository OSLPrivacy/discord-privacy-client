//! TASK 5166 - quarantine and scan every protected download before exposure.
//!
//! A protected download is decrypted **only** into an access-controlled,
//! OSL-private quarantine directory. The exact plaintext bytes that landed
//! there are then submitted to the local Windows Antimalware Scan Interface
//! (`amsi.dll`, `AmsiScanBuffer`). The verdict is bound to the content hash,
//! the provider identity, the signature version and the scan time. Only an
//! explicit clean result, still matching that binding at move time, lets the
//! file leave quarantine, and it leaves by a single atomic `rename`.
//!
//! Everything else fails closed and exposes zero bytes: provider absence, a
//! provider error, a scan timeout, a detection, a stale signature set, a
//! content hash that changed after the scan, a scanner that attests to
//! different bytes than the ones held, and an invocation count that is not
//! exactly one.
//!
//! No plaintext and no sample leaves the device. The scan is a local
//! in-process AMSI call; this module contains no network client and the
//! embedded helper script (`protected_download_quarantine_amsi.ps1`) contains
//! no upload or cloud-submission verb. `tests/task_5166_...` asserts both.
//!
//! This module deliberately depends on nothing but `std`, `sha2`, `hex` and
//! `base64`, so the TASK 5166b bypass harness can compile this exact source
//! file, sabotage one line of it, and watch the check go red.

use std::fmt;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use sha2::{Digest, Sha256};

use crate::protected_archive::{ArchiveInspection, ArchiveLimits};

/// Directory, under the app-local data root, that holds not-yet-cleared
/// plaintext. Created `0o700` and re-checked on every open.
pub const QUARANTINE_DIRECTORY: &str = "protected-download-quarantine";

/// Directory a cleared download is atomically moved into. This is the existing
/// `peer_attachment_io::STAGING_DIRECTORY`, so a released file lands exactly
/// where the rest of the open path already expects it.
pub const EXPOSURE_DIRECTORY: &str = "peer-attachment-staging";

/// A verdict older than this is not evidence about today's malware. Treated as
/// unable to verify, not as clean.
pub const MAX_SIGNATURE_AGE_SECONDS: u64 = 7 * 24 * 60 * 60;

/// A signature set stamped further than this into the future means the local
/// clock and the provider disagree; that is also unable to verify.
pub const MAX_SIGNATURE_FUTURE_SKEW_SECONDS: u64 = 24 * 60 * 60;

/// The one AMSI call a release is allowed to rest on.
pub const REQUIRED_AMSI_INVOCATIONS: u32 = 1;

/// `AMSI_RESULT_CLEAN` (amsi.h).
pub const AMSI_RESULT_CLEAN: i32 = 0;
/// `AMSI_RESULT_NOT_DETECTED` (amsi.h).
pub const AMSI_RESULT_NOT_DETECTED: i32 = 1;
/// `AMSI_RESULT_DETECTED` (amsi.h). `AmsiResultIsMalware` is `result >= 32768`.
pub const AMSI_RESULT_DETECTED: i32 = 32768;

/// Default ceiling on one AMSI round trip. Past it the provider is unable to
/// verify, not silently clean.
pub const DEFAULT_SCAN_TIMEOUT_SECONDS: u64 = 30;

/// The local AMSI helper, embedded rather than dropped on disk so no other
/// process can swap the scanner out from under the boundary.
pub const AMSI_HELPER_SCRIPT: &str = include_str!("protected_download_quarantine_amsi.ps1");

// ---------------------------------------------------------------------------
// Scanner boundary
// ---------------------------------------------------------------------------

/// The exact bytes handed to a scanner, with the hash the boundary computed
/// over them before the call.
pub struct AmsiSubmission<'a> {
    pub content_name: &'a str,
    pub plaintext: &'a [u8],
    pub content_sha256: &'a str,
}

/// What a local AMSI provider reported. `scanned_sha256` and `scanned_len` are
/// the provider's own measurement of the buffer it scanned, so the boundary can
/// refuse a verdict that is about different bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AmsiReport {
    pub result_code: i32,
    pub provider_identity: String,
    pub engine_version: String,
    pub signature_version: String,
    pub signature_updated_unix: u64,
    pub scanned_sha256: String,
    pub scanned_len: u64,
}

/// Every way a local scan can fail to produce a verdict. None of these is
/// clean.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AmsiFailure {
    /// No AMSI provider is registered, or `amsi.dll` / the host is missing.
    ProviderAbsent(String),
    /// The provider was reached but did not return a usable verdict.
    ProviderError(String),
    /// The provider did not answer inside the scan budget.
    Timeout(String),
}

impl fmt::Display for AmsiFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AmsiFailure::ProviderAbsent(detail) => write!(formatter, "provider absent: {detail}"),
            AmsiFailure::ProviderError(detail) => write!(formatter, "provider error: {detail}"),
            AmsiFailure::Timeout(detail) => write!(formatter, "provider timeout: {detail}"),
        }
    }
}

/// A local malware scanner. The only shipping implementation is
/// [`WindowsAmsiProvider`]; the trait exists so the bypass and fail-closed
/// checks can drive absence, error, timeout, detection and stale signatures
/// without needing five machines.
pub trait AmsiProvider {
    fn scan(&self, submission: &AmsiSubmission<'_>) -> Result<AmsiReport, AmsiFailure>;
}

// ---------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------

/// Why a protected download stayed in quarantine. Every one of these is a
/// local decision made on this machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuarantineReason {
    ProviderAbsent,
    ProviderError,
    ProviderTimeout,
    MalwareDetected,
    StaleSignatures,
    ContentHashMismatch,
    ScanBindingMismatch,
    AmsiInvocationCount,
    QuarantineUnreadable,
    // TASK 5180: a container is only as clean as its entries, so an archive
    // that could not be fully expanded and scanned inside quarantine is
    // withheld, and the refusal names the bound that stopped it.
    ArchiveExpandedBytes,
    ArchiveEntryCount,
    ArchiveNestingDepth,
    ArchiveScanTime,
    ArchiveUnsafeEntry,
    ArchiveUnsupported,
    ArchiveUnreadable,
    ArchiveEntryNotClean,
    ArchiveOutsideQuarantine,
}

impl QuarantineReason {
    pub fn name(self) -> &'static str {
        match self {
            QuarantineReason::ProviderAbsent => "provider_absent",
            QuarantineReason::ProviderError => "provider_error",
            QuarantineReason::ProviderTimeout => "provider_timeout",
            QuarantineReason::MalwareDetected => "malware_detected",
            QuarantineReason::StaleSignatures => "stale_signatures",
            QuarantineReason::ContentHashMismatch => "content_hash_mismatch",
            QuarantineReason::ScanBindingMismatch => "scan_binding_mismatch",
            QuarantineReason::AmsiInvocationCount => "amsi_invocation_count",
            QuarantineReason::QuarantineUnreadable => "quarantine_unreadable",
            QuarantineReason::ArchiveExpandedBytes => "archive_expanded_bytes",
            QuarantineReason::ArchiveEntryCount => "archive_entry_count",
            QuarantineReason::ArchiveNestingDepth => "archive_nesting_depth",
            QuarantineReason::ArchiveScanTime => "archive_scan_time",
            QuarantineReason::ArchiveUnsafeEntry => "archive_unsafe_entry",
            QuarantineReason::ArchiveUnsupported => "archive_unsupported",
            QuarantineReason::ArchiveUnreadable => "archive_unreadable",
            QuarantineReason::ArchiveEntryNotClean => "archive_entry_not_clean",
            QuarantineReason::ArchiveOutsideQuarantine => "archive_outside_quarantine",
        }
    }
}

impl fmt::Display for QuarantineReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// A download that did not leave quarantine. `bytes_exposed` is structurally
/// zero: nothing is written outside quarantine on this path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WithheldDownload {
    pub reason: QuarantineReason,
    pub local_reason_text: String,
    pub bytes_exposed: u64,
    pub quarantine_path: PathBuf,
    pub amsi_invocations: u32,
}

impl WithheldDownload {
    fn new(
        reason: QuarantineReason,
        detail: &str,
        quarantine_path: &Path,
        amsi_invocations: u32,
    ) -> Self {
        Self {
            reason,
            local_reason_text: format!(
                "This protected download is held in OSL's private quarantine on this device \
                 and has not been opened. Local reason: {} ({detail}). Nothing was sent to a \
                 cloud scanner.",
                reason.name()
            ),
            bytes_exposed: 0,
            quarantine_path: quarantine_path.to_owned(),
            amsi_invocations,
        }
    }
}

/// The clean verdict, bound to the bytes it is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CleanScan {
    pub content_sha256: String,
    pub content_len: u64,
    pub provider_identity: String,
    pub engine_version: String,
    pub signature_version: String,
    pub signature_updated_unix: u64,
    pub scanned_at_unix: u64,
    pub amsi_result_code: i32,
    pub amsi_invocations: u32,
}

/// A download that left quarantine by one atomic rename.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExposedDownload {
    pub path: PathBuf,
    pub bytes_exposed: u64,
    pub scan: CleanScan,
    /// TASK 5180: present when the released file was a container. It is the
    /// measurement of the expansion that had to finish, inside quarantine and
    /// inside every bound, before this release was allowed.
    pub archive: Option<ArchiveInspection>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtectedDownloadOutcome {
    Exposed(ExposedDownload),
    Withheld(WithheldDownload),
}

impl ProtectedDownloadOutcome {
    pub fn bytes_exposed(&self) -> u64 {
        match self {
            ProtectedDownloadOutcome::Exposed(exposed) => exposed.bytes_exposed,
            ProtectedDownloadOutcome::Withheld(withheld) => withheld.bytes_exposed,
        }
    }

    pub fn reason_name(&self) -> &'static str {
        match self {
            ProtectedDownloadOutcome::Exposed(_) => "clean",
            ProtectedDownloadOutcome::Withheld(withheld) => withheld.reason.name(),
        }
    }
}

/// A plaintext file that exists only inside the quarantine root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuarantinedDownload {
    path: PathBuf,
    len: u64,
}

impl QuarantinedDownload {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

// ---------------------------------------------------------------------------
// The quarantine
// ---------------------------------------------------------------------------

pub struct ProtectedDownloadQuarantine {
    quarantine_root: PathBuf,
    exposure_root: PathBuf,
    max_signature_age_seconds: u64,
    archive_limits: ArchiveLimits,
}

impl ProtectedDownloadQuarantine {
    /// Open (creating if needed) the quarantine and exposure directories under
    /// one app-local data root.
    pub fn for_app_root(app_local_data_dir: &Path) -> Result<Self, String> {
        Self::with_roots(
            app_local_data_dir.join(QUARANTINE_DIRECTORY),
            app_local_data_dir.join(EXPOSURE_DIRECTORY),
        )
    }

    pub fn with_roots(quarantine_root: PathBuf, exposure_root: PathBuf) -> Result<Self, String> {
        reject_unsafe_root(&quarantine_root)?;
        reject_unsafe_root(&exposure_root)?;
        if quarantine_root == exposure_root {
            return Err("OSL quarantine and exposure roots must differ".to_owned());
        }
        create_private_directory(&quarantine_root)?;
        fs::create_dir_all(&exposure_root)
            .map_err(|_| "OSL exposure directory could not be created".to_owned())?;
        Ok(Self {
            quarantine_root,
            exposure_root,
            max_signature_age_seconds: MAX_SIGNATURE_AGE_SECONDS,
            archive_limits: ArchiveLimits::shipping(),
        })
    }

    /// Override the signature-freshness budget. Used by the stale-signature
    /// check so it does not have to wait a week.
    pub fn with_max_signature_age_seconds(mut self, seconds: u64) -> Self {
        self.max_signature_age_seconds = seconds;
        self
    }

    /// TASK 5180: tighten the archive-expansion bounds. `clamped` means a
    /// caller can only ever ask for *less* than the shipping ceiling; a wider
    /// request silently gets the shipping ceiling back.
    pub fn with_archive_limits(mut self, limits: ArchiveLimits) -> Self {
        self.archive_limits = limits.clamped();
        self
    }

    pub fn archive_limits(&self) -> ArchiveLimits {
        self.archive_limits
    }

    pub fn quarantine_root(&self) -> &Path {
        &self.quarantine_root
    }

    pub fn exposure_root(&self) -> &Path {
        &self.exposure_root
    }

    /// The single directory a streaming decryptor is allowed to write into.
    /// Callers that decrypt to a file hand this to their output allocator, so
    /// the plaintext's first and only home is inside quarantine.
    pub fn decrypt_target_root(&self) -> &Path {
        &self.quarantine_root
    }

    /// Write already-decrypted plaintext into quarantine, `0o600`. This is the
    /// in-memory ingress; streaming ingresses use [`Self::adopt`] instead.
    pub fn admit_bytes(
        &self,
        download_id: &str,
        plaintext: &[u8],
    ) -> Result<QuarantinedDownload, String> {
        create_private_directory(&self.quarantine_root)?;
        let path = self.quarantine_root.join(quarantine_file_name(download_id));
        if path.exists() {
            return Err("OSL quarantine slot is already occupied".to_owned());
        }
        write_private_file(&path, plaintext)?;
        self.adopt(path)
    }

    /// Take ownership of a plaintext file a decryptor already wrote inside the
    /// quarantine root. Refuses anything that is not really in there.
    pub fn adopt(&self, path: PathBuf) -> Result<QuarantinedDownload, String> {
        let canonical_root = fs::canonicalize(&self.quarantine_root)
            .map_err(|_| "OSL quarantine directory could not be checked".to_owned())?;
        let canonical_path = fs::canonicalize(&path)
            .map_err(|_| "quarantined download could not be checked".to_owned())?;
        if !canonical_path.starts_with(&canonical_root) {
            return Err("quarantined download is outside the OSL quarantine".to_owned());
        }
        let metadata = fs::symlink_metadata(&canonical_path)
            .map_err(|_| "quarantined download could not be checked".to_owned())?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("quarantined download is not a regular file".to_owned());
        }
        Ok(QuarantinedDownload {
            path: canonical_path,
            len: metadata.len(),
        })
    }

    /// Read the held bytes, hash them, submit those exact bytes to the local
    /// provider once, and bind the verdict to hash, provider, signature
    /// version and time.
    pub fn scan(
        &self,
        held: &QuarantinedDownload,
        provider: &dyn AmsiProvider,
        now_unix: u64,
    ) -> Result<CleanScan, WithheldDownload> {
        let plaintext = match fs::read(&held.path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return Err(WithheldDownload::new(
                    QuarantineReason::QuarantineUnreadable,
                    &error.to_string(),
                    &held.path,
                    0,
                ))
            }
        };
        let content_sha256 = sha256_hex(&plaintext);
        let content_len = plaintext.len() as u64;

        // The one invocation a release may rest on. Counted here, at the
        // boundary, and re-checked at move time.
        let mut amsi_invocations = 0u32;
        let submission = AmsiSubmission {
            content_name: "osl-protected-download",
            plaintext: &plaintext,
            content_sha256: &content_sha256,
        };
        amsi_invocations = amsi_invocations.saturating_add(1);
        let report = match provider.scan(&submission) {
            Ok(report) => report,
            Err(AmsiFailure::ProviderAbsent(detail)) => {
                return Err(WithheldDownload::new(
                    QuarantineReason::ProviderAbsent,
                    &detail,
                    &held.path,
                    amsi_invocations,
                ))
            }
            Err(AmsiFailure::ProviderError(detail)) => {
                return Err(WithheldDownload::new(
                    QuarantineReason::ProviderError,
                    &detail,
                    &held.path,
                    amsi_invocations,
                ))
            }
            Err(AmsiFailure::Timeout(detail)) => {
                return Err(WithheldDownload::new(
                    QuarantineReason::ProviderTimeout,
                    &detail,
                    &held.path,
                    amsi_invocations,
                ))
            }
        };

        // A verdict about other bytes is not a verdict about this download.
        if !report.scanned_sha256.eq_ignore_ascii_case(&content_sha256)
            || report.scanned_len != content_len
        {
            return Err(WithheldDownload::new(
                QuarantineReason::ScanBindingMismatch,
                &format!(
                    "scanner attested sha256={} len={} but quarantine holds sha256={content_sha256} len={content_len}",
                    report.scanned_sha256, report.scanned_len
                ),
                &held.path,
                amsi_invocations,
            ));
        }

        if report.result_code >= AMSI_RESULT_DETECTED {
            return Err(WithheldDownload::new(
                QuarantineReason::MalwareDetected,
                &format!(
                    "{} reported AMSI_RESULT={} for sha256={content_sha256} using signature version {}",
                    report.provider_identity, report.result_code, report.signature_version
                ),
                &held.path,
                amsi_invocations,
            ));
        }
        if report.result_code != AMSI_RESULT_CLEAN && report.result_code != AMSI_RESULT_NOT_DETECTED
        {
            return Err(WithheldDownload::new(
                QuarantineReason::ProviderError,
                &format!(
                    "{} returned non-clean AMSI_RESULT={}",
                    report.provider_identity, report.result_code
                ),
                &held.path,
                amsi_invocations,
            ));
        }

        if let Err(detail) = self.check_signature_freshness(&report, now_unix) {
            return Err(WithheldDownload::new(
                QuarantineReason::StaleSignatures,
                &detail,
                &held.path,
                amsi_invocations,
            ));
        }

        Ok(CleanScan {
            content_sha256,
            content_len,
            provider_identity: report.provider_identity,
            engine_version: report.engine_version,
            signature_version: report.signature_version,
            signature_updated_unix: report.signature_updated_unix,
            scanned_at_unix: now_unix,
            amsi_result_code: report.result_code,
            amsi_invocations,
        })
    }

    fn check_signature_freshness(&self, report: &AmsiReport, now_unix: u64) -> Result<(), String> {
        if report.provider_identity.trim().is_empty() {
            return Err("provider did not identify itself".to_owned());
        }
        if report.signature_version.trim().is_empty() {
            return Err("provider reported no signature version".to_owned());
        }
        if report.signature_updated_unix == 0 {
            return Err("provider reported no signature timestamp".to_owned());
        }
        if report.signature_updated_unix > now_unix.saturating_add(MAX_SIGNATURE_FUTURE_SKEW_SECONDS)
        {
            return Err(format!(
                "signature version {} is stamped {} seconds in the future",
                report.signature_version,
                report.signature_updated_unix.saturating_sub(now_unix)
            ));
        }
        let age = now_unix.saturating_sub(report.signature_updated_unix);
        if age > self.max_signature_age_seconds {
            return Err(format!(
                "signature version {} is {age} seconds old, older than the {} second budget",
                report.signature_version, self.max_signature_age_seconds
            ));
        }
        Ok(())
    }

    /// Move a cleared download out of quarantine. Re-hashes the file first, so
    /// a byte changed after the scan is refused, and re-checks that exactly one
    /// AMSI call stands behind the verdict.
    pub fn release(
        &self,
        held: QuarantinedDownload,
        scan: &CleanScan,
    ) -> Result<ExposedDownload, WithheldDownload> {
        if scan.amsi_invocations != REQUIRED_AMSI_INVOCATIONS {
            return Err(WithheldDownload::new(
                QuarantineReason::AmsiInvocationCount,
                &format!(
                    "invocation count {} is not the required {REQUIRED_AMSI_INVOCATIONS}",
                    scan.amsi_invocations
                ),
                &held.path,
                scan.amsi_invocations,
            ));
        }
        if scan.amsi_result_code != AMSI_RESULT_CLEAN
            && scan.amsi_result_code != AMSI_RESULT_NOT_DETECTED
        {
            return Err(WithheldDownload::new(
                QuarantineReason::MalwareDetected,
                &format!("AMSI_RESULT={} is not clean", scan.amsi_result_code),
                &held.path,
                scan.amsi_invocations,
            ));
        }

        let current = match fs::read(&held.path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return Err(WithheldDownload::new(
                    QuarantineReason::QuarantineUnreadable,
                    &error.to_string(),
                    &held.path,
                    scan.amsi_invocations,
                ))
            }
        };
        let before_move_sha256 = sha256_hex(&current);
        let before_move_len = current.len() as u64;
        if before_move_sha256 != scan.content_sha256 || before_move_len != scan.content_len {
            return Err(WithheldDownload::new(
                QuarantineReason::ContentHashMismatch,
                &format!(
                    "before-move sha256={before_move_sha256} len={before_move_len} does not match \
                     the scanned sha256={} len={}",
                    scan.content_sha256, scan.content_len
                ),
                &held.path,
                scan.amsi_invocations,
            ));
        }

        let file_name = match held.path.file_name().and_then(|name| name.to_str()) {
            Some(name) => name.to_owned(),
            None => {
                return Err(WithheldDownload::new(
                    QuarantineReason::QuarantineUnreadable,
                    "quarantined download has no file name",
                    &held.path,
                    scan.amsi_invocations,
                ))
            }
        };
        let destination = self.exposure_root.join(&file_name);
        if let Err(error) = fs::create_dir_all(&self.exposure_root) {
            return Err(WithheldDownload::new(
                QuarantineReason::QuarantineUnreadable,
                &error.to_string(),
                &held.path,
                scan.amsi_invocations,
            ));
        }
        // One rename. There is no intermediate copy in the exposure directory,
        // so no partially written protected byte is ever visible outside
        // quarantine.
        if let Err(error) = fs::rename(&held.path, &destination) {
            return Err(WithheldDownload::new(
                QuarantineReason::QuarantineUnreadable,
                &format!("atomic move out of quarantine failed: {error}"),
                &held.path,
                scan.amsi_invocations,
            ));
        }

        let exposed_len = fs::metadata(&destination).map(|meta| meta.len()).unwrap_or(0);
        Ok(ExposedDownload {
            path: destination,
            bytes_exposed: exposed_len,
            scan: scan.clone(),
            archive: None,
        })
    }

    /// The shipping path: scan, then - if the bytes are a container - expand
    /// and scan every entry inside quarantine within the four hard bounds, and
    /// only then release. There is no window for a caller to forget a step and
    /// no second door: an archive that does not survive
    /// [`crate::protected_archive`] never reaches the `rename`.
    pub fn scan_and_release(
        &self,
        held: QuarantinedDownload,
        provider: &dyn AmsiProvider,
        now_unix: u64,
    ) -> ProtectedDownloadOutcome {
        let scan = match self.scan(&held, provider, now_unix) {
            Ok(scan) => scan,
            Err(withheld) => return ProtectedDownloadOutcome::Withheld(withheld),
        };
        // TASK 5180. The whole-file verdict above says nothing about the
        // entries a user would actually open, so a container is expanded and
        // scanned entry by entry before the release door opens.
        let inspection = match crate::protected_archive::inspect_if_archive(
            self,
            &held,
            provider,
            self.archive_limits,
            now_unix,
        ) {
            Ok(inspection) => inspection,
            Err(refusal) => {
                return ProtectedDownloadOutcome::Withheld(WithheldDownload::new(
                    refusal.quarantine_reason(),
                    &refusal.reason(),
                    &held.path,
                    scan.amsi_invocations,
                ))
            }
        };
        match self.release(held, &scan) {
            Ok(mut exposed) => {
                exposed.archive = inspection;
                ProtectedDownloadOutcome::Exposed(exposed)
            }
            Err(withheld) => ProtectedDownloadOutcome::Withheld(withheld),
        }
    }

    /// Total bytes of every file sitting in the exposure directory. Used by the
    /// checks to assert an exposure of exactly zero.
    pub fn exposed_bytes(&self) -> u64 {
        directory_bytes(&self.exposure_root)
    }

    /// Bytes still held inside quarantine.
    pub fn quarantined_bytes(&self) -> u64 {
        directory_bytes(&self.quarantine_root)
    }

    /// Destroy the plaintext behind a refusal. The path came out of a
    /// [`WithheldDownload`] this quarantine produced, and is re-checked against
    /// the quarantine root before anything is removed.
    pub fn discard_withheld(&self, withheld: &WithheldDownload) -> Result<(), String> {
        let canonical_root = fs::canonicalize(&self.quarantine_root)
            .map_err(|_| "OSL quarantine directory could not be checked".to_owned())?;
        if !withheld.quarantine_path.starts_with(&canonical_root) {
            return Err("withheld download is outside the OSL quarantine".to_owned());
        }
        match fs::remove_file(&withheld.quarantine_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("quarantined plaintext could not be removed: {error}")),
        }
    }

    /// Destroy a withheld download's plaintext.
    pub fn discard(&self, held: QuarantinedDownload) -> Result<(), String> {
        match fs::remove_file(&held.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("quarantined plaintext could not be removed: {error}")),
        }
    }
}

// ---------------------------------------------------------------------------
// The shipping provider: local Windows AMSI
// ---------------------------------------------------------------------------

/// Submits the exact plaintext bytes to `amsi.dll` on this machine through a
/// `powershell.exe` host running the embedded helper. Never writes the bytes to
/// a Windows-visible file and never opens a socket.
pub struct WindowsAmsiProvider {
    powershell: PathBuf,
    timeout: Duration,
}

impl Default for WindowsAmsiProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsAmsiProvider {
    /// Absolute System32 path, so a hijacked `PATH` cannot substitute the
    /// scanner host. Matches `native_attachment_transport::system32_binary`.
    pub fn new() -> Self {
        Self {
            powershell: default_powershell_path(),
            timeout: Duration::from_secs(DEFAULT_SCAN_TIMEOUT_SECONDS),
        }
    }

    pub fn with_powershell(mut self, powershell: PathBuf) -> Self {
        self.powershell = powershell;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn powershell_path(&self) -> &Path {
        &self.powershell
    }

    fn encoded_command() -> String {
        let mut utf16 = Vec::with_capacity(AMSI_HELPER_SCRIPT.len() * 2);
        for unit in AMSI_HELPER_SCRIPT.encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        base64::engine::general_purpose::STANDARD.encode(utf16)
    }
}

impl AmsiProvider for WindowsAmsiProvider {
    fn scan(&self, submission: &AmsiSubmission<'_>) -> Result<AmsiReport, AmsiFailure> {
        if !self.powershell.exists() {
            return Err(AmsiFailure::ProviderAbsent(format!(
                "the Windows scanner host {} is not present",
                self.powershell.display()
            )));
        }
        let payload = base64::engine::general_purpose::STANDARD.encode(submission.plaintext);

        let mut child = Command::new(&self.powershell)
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-EncodedCommand")
            .arg(Self::encoded_command())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                AmsiFailure::ProviderAbsent(format!("the Windows scanner host would not start: {error}"))
            })?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| AmsiFailure::ProviderError("scanner stdin was unavailable".to_owned()))?;
        let writer = std::thread::spawn(move || {
            let _ = stdin.write_all(payload.as_bytes());
            let _ = stdin.flush();
            drop(stdin);
        });
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| AmsiFailure::ProviderError("scanner stdout was unavailable".to_owned()))?;
        let reader = std::thread::spawn(move || {
            let mut buffer = String::new();
            let _ = stdout.read_to_string(&mut buffer);
            buffer
        });

        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    return Err(AmsiFailure::ProviderError(format!(
                        "the Windows scanner host could not be waited on: {error}"
                    )));
                }
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AmsiFailure::Timeout(format!(
                    "the local AMSI provider did not answer within {} ms",
                    self.timeout.as_millis()
                )));
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        let output = reader.join().unwrap_or_default();
        let _ = writer.join();
        if !status.success() {
            return Err(AmsiFailure::ProviderError(format!(
                "the Windows scanner host exited with {status}"
            )));
        }
        parse_amsi_helper_output(&output)
    }
}

/// Parse the embedded helper's `OSL5166_KEY=value` lines.
pub fn parse_amsi_helper_output(output: &str) -> Result<AmsiReport, AmsiFailure> {
    let mut fields: Vec<(String, String)> = Vec::new();
    for line in output.lines() {
        let line = line.trim_end_matches('\r').trim();
        if let Some(rest) = line.strip_prefix("OSL5166_") {
            if let Some((key, value)) = rest.split_once('=') {
                fields.push((key.to_owned(), value.to_owned()));
            }
        }
    }
    let get = |key: &str| -> Option<&str> {
        fields
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    let detail = get("DETAIL").unwrap_or("no detail reported").to_owned();
    match get("STATUS") {
        Some("scanned") => {}
        Some("provider_absent") => return Err(AmsiFailure::ProviderAbsent(detail)),
        Some("provider_error") => return Err(AmsiFailure::ProviderError(detail)),
        Some(other) => {
            return Err(AmsiFailure::ProviderError(format!(
                "the local AMSI helper reported an unknown status {other}"
            )))
        }
        None => {
            return Err(AmsiFailure::ProviderAbsent(
                "the local AMSI helper reported no status".to_owned(),
            ))
        }
    }
    let result_code = get("AMSI_RESULT")
        .and_then(|value| value.trim().parse::<i32>().ok())
        .ok_or_else(|| {
            AmsiFailure::ProviderError("the local AMSI helper reported no result code".to_owned())
        })?;
    let scanned_len = get("SCANNED_LEN")
        .and_then(|value| value.trim().parse::<u64>().ok())
        .ok_or_else(|| {
            AmsiFailure::ProviderError("the local AMSI helper reported no scanned length".to_owned())
        })?;
    let scanned_sha256 = get("SCANNED_SHA256")
        .map(|value| value.trim().to_owned())
        .filter(|value| value.len() == 64)
        .ok_or_else(|| {
            AmsiFailure::ProviderError("the local AMSI helper reported no scanned hash".to_owned())
        })?;
    Ok(AmsiReport {
        result_code,
        provider_identity: get("PROVIDER").unwrap_or("").trim().to_owned(),
        engine_version: get("ENGINE_VERSION").unwrap_or("").trim().to_owned(),
        signature_version: get("SIGNATURE_VERSION").unwrap_or("").trim().to_owned(),
        signature_updated_unix: get("SIGNATURE_UPDATED_UNIX")
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(0),
        scanned_sha256,
        scanned_len,
    })
}

fn default_powershell_path() -> PathBuf {
    // From WSL the same interactive Windows session is reached through the
    // DrvFs mount; on Windows itself the drive-letter path is the real one.
    let mounted = Path::new("/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe");
    if mounted.exists() {
        return mounted.to_owned();
    }
    PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn quarantine_file_name(download_id: &str) -> String {
    format!("opened-{}.oslatt", &sha256_hex(download_id.as_bytes())[..32])
}

fn reject_unsafe_root(root: &Path) -> Result<(), String> {
    if !root.is_absolute()
        || root.parent().is_none()
        || root
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err("OSL quarantine root is invalid".to_owned());
    }
    Ok(())
}

/// Create a directory only this account can enter, and refuse to use it if the
/// access control did not take.
fn create_private_directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path)
        .map_err(|_| "OSL quarantine directory could not be created".to_owned())?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| "OSL quarantine directory could not be checked".to_owned())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("OSL quarantine directory is unsafe".to_owned());
    }
    harden_directory(path)
}

#[cfg(unix)]
fn harden_directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| "OSL quarantine directory could not be locked down".to_owned())?;
    let mode = fs::metadata(path)
        .map_err(|_| "OSL quarantine directory could not be checked".to_owned())?
        .permissions()
        .mode()
        & 0o777;
    if mode != 0o700 {
        return Err(format!(
            "OSL quarantine directory is not private: mode {mode:o}"
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn harden_directory(path: &Path) -> Result<(), String> {
    // Break inheritance and grant this account only. A directory OSL cannot
    // lock down is not a quarantine, so a failure here is fatal.
    let status = Command::new(r"C:\Windows\System32\icacls.exe")
        .arg(path)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(format!("{}:(OI)(CI)F", current_windows_account()))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("OSL quarantine directory could not be locked down: {error}"))?;
    if !status.success() {
        return Err("OSL quarantine directory access control could not be applied".to_owned());
    }
    Ok(())
}

#[cfg(windows)]
fn current_windows_account() -> String {
    match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        (Ok(domain), Ok(user)) if !domain.is_empty() && !user.is_empty() => {
            format!("{domain}\\{user}")
        }
        (_, Ok(user)) if !user.is_empty() => user,
        _ => "%USERNAME%".to_owned(),
    }
}

#[cfg(unix)]
fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| format!("quarantined plaintext could not be created: {error}"))?;
    file.write_all(bytes)
        .map_err(|error| format!("quarantined plaintext could not be written: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("quarantined plaintext could not be synchronized: {error}"))?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("quarantined plaintext could not be created: {error}"))?;
    file.write_all(bytes)
        .map_err(|error| format!("quarantined plaintext could not be written: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("quarantined plaintext could not be synchronized: {error}"))?;
    Ok(())
}

fn directory_bytes(root: &Path) -> u64 {
    let mut total = 0u64;
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => pending.push(entry.path()),
                Ok(kind) if kind.is_file() => {
                    total = total.saturating_add(entry.metadata().map(|m| m.len()).unwrap_or(0));
                }
                _ => {}
            }
        }
    }
    total
}
