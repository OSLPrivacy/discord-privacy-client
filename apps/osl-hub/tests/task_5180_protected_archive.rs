//! TASK 5180 - unpack protected archives within hard quarantine limits.
//!
//! Every fixture here goes through the *shipping* door, not a test-local
//! reimplementation of it: the archive is sealed with
//! `peer_attachment_io::encrypt_file`, opened only into the TASK 5166
//! quarantine with `peer_attachment_io::decrypt_file`, adopted by
//! `ProtectedDownloadQuarantine`, and then handed to
//! `protected_download_final_save::deliver_after_clean_scan` - the production
//! function that scans, releases and performs the final save. The archive
//! expansion is reached from inside that call, so a fixture that never reaches
//! the real unpacker cannot pass.
//!
//! The clean control must release only after every entry has earned its own
//! local receipt. The over-byte, over-entry, over-depth and timeout fixtures
//! must each write zero bytes outside quarantine, release zero bytes to the
//! caller, and name the bound that stopped them.

use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use osl_privacy_hub::download_zone_handoff::{
    AttachmentServicesSaver, PlatformLimitation, ZoneHandoffFailure, ZoneHandoffOutcome,
    ZoneHandoffRequest, ZoneLimitation,
};
use osl_privacy_hub::protected_archive::{
    detect_archive_format, ArchiveFormat, ArchiveLimits, MAX_DEPTH, MAX_ENTRIES,
    MAX_EXPANDED_BYTES, MAX_SCAN_TIME_MS,
};
use osl_privacy_hub::protected_download_final_save::{deliver_after_clean_scan, FinalSaveOutcome};
use osl_privacy_hub::protected_download_quarantine::{
    now_unix_seconds, sha256_hex, AmsiFailure, AmsiProvider, AmsiReport, AmsiSubmission,
    ProtectedDownloadQuarantine, QuarantinedDownload, AMSI_RESULT_NOT_DETECTED,
};

// ---------------------------------------------------------------------------
// Throwaway roots
// ---------------------------------------------------------------------------

struct TempRoot(PathBuf);

impl TempRoot {
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_root(label: &str) -> TempRoot {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "osl-task5180-{label}-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temp root");
    TempRoot(root)
}

fn directory_bytes(root: &Path) -> u64 {
    let mut total = 0u64;
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
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

/// Every `archive-expansion-*` directory anywhere under the system temp root
/// that is *not* inside the quarantine, and the bytes in it. This is the
/// independent observation that expansion never happens outside quarantine: it
/// looks at the filesystem, not at what the boundary says about itself.
fn expansion_bytes_outside_quarantine(quarantine_root: &Path) -> u64 {
    let mut total = 0u64;
    let mut pending = vec![std::env::temp_dir()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                continue;
            }
            let is_expansion = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("archive-expansion-"))
                .unwrap_or(false);
            if is_expansion {
                if !path.starts_with(quarantine_root) {
                    total = total.saturating_add(directory_bytes(&path));
                }
                continue;
            }
            // Only descend through this test's own throwaway roots, so the
            // sweep is bounded and cannot wander the machine's temp dir.
            let ours = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("osl-task5180-"))
                .unwrap_or(false);
            if ours || path.starts_with(quarantine_root) {
                pending.push(path);
            }
        }
    }
    total
}

// ---------------------------------------------------------------------------
// The local scanner double
// ---------------------------------------------------------------------------

/// A local provider that answers clean, counts every call, and - on every
/// single call - re-measures what is visible outside quarantine. A boundary
/// that leaked a byte, or expanded outside the quarantine, is caught while the
/// expansion is still running, not after it has been cleaned up.
struct RecordingScanner {
    calls: AtomicU32,
    quarantine_root: PathBuf,
    exposure_root: PathBuf,
    destination: PathBuf,
    max_outside_bytes: AtomicU64,
    submitted: Mutex<Vec<(String, u64)>>,
    delay: Duration,
}

impl RecordingScanner {
    fn new(quarantine_root: &Path, exposure_root: &Path, destination: &Path) -> Self {
        Self {
            calls: AtomicU32::new(0),
            quarantine_root: quarantine_root.to_owned(),
            exposure_root: exposure_root.to_owned(),
            destination: destination.to_owned(),
            max_outside_bytes: AtomicU64::new(0),
            submitted: Mutex::new(Vec::new()),
            delay: Duration::ZERO,
        }
    }

    fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    fn calls(&self) -> u32 {
        self.calls.load(Ordering::Acquire)
    }

    fn max_outside_bytes(&self) -> u64 {
        self.max_outside_bytes.load(Ordering::Acquire)
    }

    fn submitted_bytes(&self) -> u64 {
        self.submitted
            .lock()
            .map(|entries| entries.iter().map(|(_, len)| *len).sum())
            .unwrap_or(0)
    }
}

impl AmsiProvider for RecordingScanner {
    fn scan(&self, submission: &AmsiSubmission<'_>) -> Result<AmsiReport, AmsiFailure> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        let outside = directory_bytes(&self.exposure_root)
            .saturating_add(
                std::fs::metadata(&self.destination)
                    .map(|meta| meta.len())
                    .unwrap_or(0),
            )
            .saturating_add(expansion_bytes_outside_quarantine(&self.quarantine_root));
        self.max_outside_bytes.fetch_max(outside, Ordering::AcqRel);
        if let Ok(mut entries) = self.submitted.lock() {
            entries.push((
                submission.content_sha256.to_owned(),
                submission.plaintext.len() as u64,
            ));
        }
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
        Ok(AmsiReport {
            result_code: AMSI_RESULT_NOT_DETECTED,
            provider_identity: "OSL TASK 5180 local fixture scanner".to_owned(),
            engine_version: "1.1.0.0".to_owned(),
            signature_version: "1.457.130.0".to_owned(),
            signature_updated_unix: now_unix_seconds(),
            scanned_sha256: sha256_hex(submission.plaintext),
            scanned_len: submission.plaintext.len() as u64,
        })
    }
}

/// Stands in for Windows Attachment Services. Records how many scans had
/// already happened when the final save was reached, so "released only after
/// every entry scans" is measured at the release, not asserted afterwards.
struct RecordingSaver<'a> {
    scanner: &'a RecordingScanner,
    calls: AtomicU32,
    scans_at_save: AtomicU32,
}

impl<'a> RecordingSaver<'a> {
    fn new(scanner: &'a RecordingScanner) -> Self {
        Self {
            scanner,
            calls: AtomicU32::new(0),
            scans_at_save: AtomicU32::new(0),
        }
    }

    fn calls(&self) -> u32 {
        self.calls.load(Ordering::Acquire)
    }

    fn scans_at_save(&self) -> u32 {
        self.scans_at_save.load(Ordering::Acquire)
    }
}

impl AttachmentServicesSaver for RecordingSaver<'_> {
    fn save(
        &self,
        request: &ZoneHandoffRequest,
    ) -> Result<ZoneHandoffOutcome, ZoneHandoffFailure> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        self.scans_at_save
            .store(self.scanner.calls(), Ordering::Release);
        Ok(ZoneHandoffOutcome::PlatformLimited(PlatformLimitation {
            reason: ZoneLimitation::FilesystemCannotRetainMark,
            filesystem: "ext4".to_owned(),
            windows_path: request.local_path.display().to_string(),
            save_calls: 1,
            message: "TASK 5180 fixture saver: this Linux lane cannot carry a zone mark"
                .to_owned(),
        }))
    }
}

// ---------------------------------------------------------------------------
// Fixtures, built as real archives
// ---------------------------------------------------------------------------

fn zip_bytes(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in entries {
            writer.start_file(*name, options).expect("zip entry");
            writer.write_all(bytes).expect("zip entry bytes");
        }
        writer.finish().expect("zip finish");
    }
    cursor.into_inner()
}

fn tar_bytes(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (name, bytes) in entries {
        let mut header = tar::Header::new_ustar();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        builder
            .append_data(&mut header, *name, &bytes[..])
            .expect("tar entry");
    }
    builder.into_inner().expect("tar finish")
}

fn gzip_bytes(payload: &[u8]) -> Vec<u8> {
    let mut encoder =
        flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(payload).expect("gzip write");
    encoder.finish().expect("gzip finish")
}

/// zip > tar > (files) and zip > gz > (file). Six entries, deepest level 2.
fn clean_nested_zip() -> Vec<u8> {
    let inner_tar = tar_bytes(&[
        ("beta.txt", b"TASK 5180 inner tar entry beta.\n".to_vec()),
        ("deep/gamma.bin", vec![0x2au8; 96]),
    ]);
    let delta_gz = gzip_bytes(b"TASK 5180 gzip member delta.\n");
    zip_bytes(&[
        (
            "notes/alpha.txt",
            b"TASK 5180 clean nested archive, entry alpha.\n".to_vec(),
        ),
        ("inner.tar", inner_tar),
        ("delta.txt.gz", delta_gz),
    ])
}

const BOMB_UNCOMPRESSED_BYTES: usize = 4 * 1024 * 1024;

fn over_byte_zip() -> Vec<u8> {
    zip_bytes(&[("filler.bin", vec![0u8; BOMB_UNCOMPRESSED_BYTES])])
}

fn over_byte_gzip() -> Vec<u8> {
    gzip_bytes(&vec![0u8; BOMB_UNCOMPRESSED_BYTES])
}

fn over_entry_zip(count: usize) -> Vec<u8> {
    let entries: Vec<(String, Vec<u8>)> = (0..count)
        .map(|index| {
            (
                format!("swarm/entry-{index:02}.txt"),
                format!("TASK 5180 swarm entry {index}\n").into_bytes(),
            )
        })
        .collect();
    let borrowed: Vec<(&str, Vec<u8>)> = entries
        .iter()
        .map(|(name, bytes)| (name.as_str(), bytes.clone()))
        .collect();
    zip_bytes(&borrowed)
}

/// zip > zip > zip > file: four levels of container.
fn over_depth_zip() -> Vec<u8> {
    let level3 = zip_bytes(&[("payload.txt", b"TASK 5180 fourth-level payload.\n".to_vec())]);
    let level2 = zip_bytes(&[("level3.zip", level3)]);
    zip_bytes(&[("level2.zip", level2)])
}

fn slow_zip(entries: usize) -> Vec<u8> {
    let built: Vec<(String, Vec<u8>)> = (0..entries)
        .map(|index| {
            (
                format!("slow-{index:02}.txt"),
                format!("TASK 5180 slow entry {index}\n").into_bytes(),
            )
        })
        .collect();
    let borrowed: Vec<(&str, Vec<u8>)> = built
        .iter()
        .map(|(name, bytes)| (name.as_str(), bytes.clone()))
        .collect();
    zip_bytes(&borrowed)
}

// ---------------------------------------------------------------------------
// The shipping ingress: seal, decrypt into quarantine, adopt
// ---------------------------------------------------------------------------

struct Bench {
    _root: TempRoot,
    quarantine: ProtectedDownloadQuarantine,
    quarantine_root: PathBuf,
    exposure_root: PathBuf,
    destination: PathBuf,
    sealed_root: PathBuf,
}

fn bench(label: &str, limits: ArchiveLimits) -> Bench {
    let root = temp_root(label);
    let quarantine_root = root.path().join("quarantine");
    let exposure_root = root.path().join("exposure");
    let sealed_root = root.path().join("sealed");
    let destination = root.path().join("downloads").join("delivered.bin");
    std::fs::create_dir_all(&sealed_root).expect("sealed root");
    let quarantine =
        ProtectedDownloadQuarantine::with_roots(quarantine_root.clone(), exposure_root.clone())
            .expect("quarantine opens")
            .with_archive_limits(limits);
    Bench {
        _root: root,
        quarantine,
        quarantine_root,
        exposure_root,
        destination,
        sealed_root,
    }
}

/// Seal the fixture exactly as a sender would, then open it *only* into the
/// quarantine. The plaintext's first and only home is the quarantine root, so
/// the inspection below is by construction post-decryption and in-quarantine.
fn admit_sealed(bench: &Bench, filename: &str, mime: &str, bytes: &[u8]) -> QuarantinedDownload {
    let plain_path = bench.sealed_root.join(format!("plain-{filename}"));
    std::fs::write(&plain_path, bytes).expect("write fixture plaintext");
    let mut source = std::fs::File::open(&plain_path).expect("open fixture plaintext");
    let staged = osl_privacy_hub::peer_attachment_io::encrypt_file(
        &bench.sealed_root,
        &mut source,
        filename,
        mime,
        crypto::aead::Key::from_bytes([0x5du8; 32]),
        vec![0x51; 16],
        0,
    )
    .expect("seal the fixture");
    drop(source);
    std::fs::remove_file(&plain_path).expect("remove the unsealed fixture copy");

    let mut sealed = std::fs::File::open(staged.path()).expect("reopen the sealed fixture");
    let opened = osl_privacy_hub::peer_attachment_io::decrypt_file(
        bench.quarantine.decrypt_target_root(),
        &mut sealed,
        filename,
        mime,
        crypto::aead::Key::from_bytes([0x5du8; 32]),
    )
    .expect("decrypt into quarantine");
    let staged_plaintext = opened
        .release_to_external_reader()
        .expect("the decrypted plaintext has a path");
    let path = staged_plaintext.path().to_owned();
    assert!(
        path.starts_with(&bench.quarantine_root),
        "the decrypted plaintext must land inside quarantine: {}",
        path.display()
    );
    bench.quarantine.adopt(path).expect("adopt into quarantine")
}

fn deliver(
    bench: &Bench,
    held: QuarantinedDownload,
    scanner: &RecordingScanner,
    saver: &RecordingSaver<'_>,
) -> FinalSaveOutcome {
    deliver_after_clean_scan(
        &bench.quarantine,
        held,
        scanner,
        saver,
        &bench.destination,
        "https://osl.invalid/task-5180",
        "https://osl.invalid/",
        now_unix_seconds(),
    )
}

fn destination_bytes(bench: &Bench) -> u64 {
    std::fs::metadata(&bench.destination)
        .map(|meta| meta.len())
        .unwrap_or(0)
}

/// Limits used by the bound fixtures and the clean control. Every one is a
/// *tightening* of the shipping ceiling and goes through the same
/// `ArchiveLimits::clamped()` path production uses.
fn tightened() -> ArchiveLimits {
    ArchiveLimits::shipping()
        .with_max_expanded_bytes(1024 * 1024)
        .with_max_entries(8)
        .with_max_depth(2)
        .with_max_scan_time(Duration::from_secs(30))
}

// ---------------------------------------------------------------------------
// 1. The clean control
// ---------------------------------------------------------------------------

#[test]
fn task_5180_clean_nested_archive_releases_only_after_every_entry_scans() {
    let bench = bench("clean", tightened());
    let fixture = clean_nested_zip();
    let held = admit_sealed(&bench, "bundle.zip", "application/zip", &fixture);
    let scanner = RecordingScanner::new(
        &bench.quarantine_root,
        &bench.exposure_root,
        &bench.destination,
    );
    let saver = RecordingSaver::new(&scanner);

    let outcome = deliver(&bench, held, &scanner, &saver);
    let delivered = match outcome {
        FinalSaveOutcome::Delivered(delivered) => delivered,
        other => panic!("the clean nested archive must deliver, got {}", other.name()),
    };
    // The delivered file is the original archive, moved once.
    println!("TASK5180_CLEAN_DELIVERED_BYTES={}", delivered.bytes);
    println!(
        "TASK5180_CLEAN_DELIVERED_PATH_IN_QUARANTINE={}",
        delivered.path.starts_with(&bench.quarantine_root)
    );
    println!("TASK5180_CLEAN_SAVER_CALLS={}", saver.calls());
    println!("TASK5180_CLEAN_SCAN_CALLS={}", scanner.calls());
    println!(
        "TASK5180_CLEAN_SCANS_BEFORE_SAVE={}",
        saver.scans_at_save()
    );
    println!(
        "TASK5180_CLEAN_OUTSIDE_BYTES_DURING_SCANS={}",
        scanner.max_outside_bytes()
    );
    println!(
        "TASK5180_CLEAN_SUBMITTED_BYTES={}",
        scanner.submitted_bytes()
    );

    assert_eq!(delivered.bytes, fixture.len() as u64);
    assert!(!delivered.path.starts_with(&bench.quarantine_root));
    assert_eq!(saver.calls(), 1);
    // One whole-file receipt plus one per entry, all of them before the save.
    assert_eq!(saver.scans_at_save(), scanner.calls());
    assert_eq!(scanner.max_outside_bytes(), 0);
}

#[test]
fn task_5180_clean_control_scans_every_entry_inside_quarantine() {
    let bench = bench("clean-entries", tightened());
    let fixture = clean_nested_zip();
    let held = admit_sealed(&bench, "bundle.zip", "application/zip", &fixture);
    let scanner = RecordingScanner::new(
        &bench.quarantine_root,
        &bench.exposure_root,
        &bench.destination,
    );

    let outcome = bench
        .quarantine
        .scan_and_release(held, &scanner, now_unix_seconds());
    let exposed = match outcome {
        osl_privacy_hub::protected_download_quarantine::ProtectedDownloadOutcome::Exposed(
            exposed,
        ) => exposed,
        osl_privacy_hub::protected_download_quarantine::ProtectedDownloadOutcome::Withheld(
            withheld,
        ) => panic!("clean control withheld: {}", withheld.local_reason_text),
    };
    let archive = exposed
        .archive
        .expect("a container release carries its inspection");

    println!("TASK5180_CLEAN_FORMAT={}", archive.format);
    println!("TASK5180_CLEAN_ENTRIES_SEEN={}", archive.entries_seen);
    println!("TASK5180_CLEAN_ENTRIES_SCANNED={}", archive.entries_scanned);
    println!(
        "TASK5180_CLEAN_ENTRY_SCAN_CALLS={}",
        archive.entry_scan_calls
    );
    println!("TASK5180_CLEAN_EXPANDED_BYTES={}", archive.expanded_bytes);
    println!("TASK5180_CLEAN_SCANNED_BYTES={}", archive.scanned_bytes);
    println!("TASK5180_CLEAN_DEEPEST_LEVEL={}", archive.deepest_level);
    println!(
        "TASK5180_CLEAN_ENTRY_ORDER={}",
        archive.entry_order.join(",")
    );
    println!(
        "TASK5180_CLEAN_WORKSPACE_IN_QUARANTINE={}",
        archive
            .workspace_root
            .starts_with(std::fs::canonicalize(&bench.quarantine_root).expect("canonical root"))
    );
    println!(
        "TASK5180_CLEAN_WORKSPACE_REMOVED={}",
        archive.workspace_removed
    );
    println!(
        "TASK5180_CLEAN_EXPANSION_DIRS_LEFT={}",
        expansion_bytes_outside_quarantine(&bench.quarantine_root)
    );
    println!("TASK5180_CLEAN_RELEASED_BYTES={}", exposed.bytes_exposed);
    println!("TASK5180_CLEAN_TOTAL_SCAN_CALLS={}", scanner.calls());

    assert_eq!(archive.format, "zip");
    assert_eq!(archive.entries_seen, 6);
    assert!(archive.every_entry_scanned());
    assert_eq!(archive.deepest_level, 2);
    assert_eq!(
        archive.entry_order,
        vec![
            "notes/alpha.txt",
            "inner.tar",
            "beta.txt",
            "deep/gamma.bin",
            "delta.txt.gz",
            "delta.txt.gz.gunzip",
        ]
    );
    assert!(archive.workspace_root.starts_with(
        std::fs::canonicalize(&bench.quarantine_root).expect("canonical quarantine root")
    ));
    assert!(archive.workspace_removed);
    assert_eq!(expansion_bytes_outside_quarantine(&bench.quarantine_root), 0);
    // One whole-file receipt plus one per entry.
    assert_eq!(scanner.calls(), 1 + archive.entries_seen);
    assert_eq!(exposed.bytes_exposed, fixture.len() as u64);
    assert_eq!(scanner.max_outside_bytes(), 0);
}

// ---------------------------------------------------------------------------
// 2. The four bounds
// ---------------------------------------------------------------------------

struct BoundResult {
    reason: &'static str,
    text: String,
    outside_bytes: u64,
    released_bytes: u64,
    saver_calls: u32,
}

fn run_bound_fixture(
    label: &str,
    limits: ArchiveLimits,
    filename: &str,
    mime: &str,
    fixture: &[u8],
    entry_delay: Duration,
) -> BoundResult {
    let bench = bench(label, limits);
    let held = admit_sealed(&bench, filename, mime, fixture);
    let scanner = RecordingScanner::new(
        &bench.quarantine_root,
        &bench.exposure_root,
        &bench.destination,
    )
    .with_delay(entry_delay);
    let saver = RecordingSaver::new(&scanner);

    let outcome = deliver(&bench, held, &scanner, &saver);
    let withheld = match outcome {
        FinalSaveOutcome::Withheld(withheld) => withheld,
        other => panic!("{label} must be withheld, got {}", other.name()),
    };
    let outside = scanner
        .max_outside_bytes()
        .max(directory_bytes(&bench.exposure_root))
        .max(destination_bytes(&bench))
        .max(expansion_bytes_outside_quarantine(&bench.quarantine_root));
    BoundResult {
        reason: withheld.reason.name(),
        text: withheld.local_reason_text.clone(),
        outside_bytes: outside,
        released_bytes: withheld.bytes_exposed,
        saver_calls: saver.calls(),
    }
}

fn report(tag: &str, result: &BoundResult) {
    println!(
        "TASK5180_{tag}_OUTSIDE_BYTES={} RELEASED_BYTES={} SAVER_CALLS={} REASON={} TEXT={}",
        result.outside_bytes,
        result.released_bytes,
        result.saver_calls,
        result.reason,
        result.text
    );
    assert_eq!(result.outside_bytes, 0, "{tag} wrote bytes outside quarantine");
    assert_eq!(result.released_bytes, 0, "{tag} released bytes to the caller");
    assert_eq!(result.saver_calls, 0, "{tag} reached the final save");
}

#[test]
fn task_5180_over_byte_archive_is_stopped_by_the_expanded_byte_limit() {
    let fixture = over_byte_zip();
    println!("TASK5180_OVER_BYTE_SEALED_ARCHIVE_BYTES={}", fixture.len());
    println!("TASK5180_OVER_BYTE_UNCOMPRESSED_BYTES={BOMB_UNCOMPRESSED_BYTES}");
    let result = run_bound_fixture(
        "over-byte",
        tightened(),
        "bomb.zip",
        "application/zip",
        &fixture,
        Duration::ZERO,
    );
    report("OVER_BYTE", &result);
    assert_eq!(result.reason, "archive_expanded_bytes");
    assert!(
        result.text.contains("limit=expanded-bytes"),
        "the refusal must name the limit: {}",
        result.text
    );
    assert!(result.text.contains("1048576"));
    // Enforced mid-stream: the boundary stopped at the ceiling, far below the
    // 4 MiB the entry actually holds.
    let stopped_at: u64 = result
        .text
        .split("stopped at ")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .and_then(|number| number.parse().ok())
        .expect("the refusal reports where it stopped");
    println!("TASK5180_OVER_BYTE_EXPANDED_BEFORE_STOP={stopped_at}");
    assert!(stopped_at <= 1024 * 1024);
    assert!(stopped_at < BOMB_UNCOMPRESSED_BYTES as u64);
}

#[test]
fn task_5180_over_byte_gzip_is_stopped_without_any_declared_size() {
    // A gzip member declares no usable uncompressed size up front, so this
    // fixture can only be stopped by a bound enforced while the bytes stream.
    let fixture = over_byte_gzip();
    println!("TASK5180_OVER_BYTE_GZ_SEALED_ARCHIVE_BYTES={}", fixture.len());
    let result = run_bound_fixture(
        "over-byte-gz",
        tightened(),
        "bomb.txt.gz",
        "application/gzip",
        &fixture,
        Duration::ZERO,
    );
    report("OVER_BYTE_GZ", &result);
    assert_eq!(result.reason, "archive_expanded_bytes");
    assert!(result.text.contains("limit=expanded-bytes"));
}

#[test]
fn task_5180_over_entry_archive_is_stopped_by_the_entry_count_limit() {
    let fixture = over_entry_zip(12);
    let result = run_bound_fixture(
        "over-entry",
        tightened(),
        "swarm.zip",
        "application/zip",
        &fixture,
        Duration::ZERO,
    );
    report("OVER_ENTRY", &result);
    assert_eq!(result.reason, "archive_entry_count");
    assert!(
        result.text.contains("limit=entry-count") && result.text.contains("8 entries"),
        "the refusal must name the limit: {}",
        result.text
    );
}

#[test]
fn task_5180_over_depth_archive_is_stopped_by_the_nesting_depth_limit() {
    let fixture = over_depth_zip();
    let result = run_bound_fixture(
        "over-depth",
        tightened(),
        "deep.zip",
        "application/zip",
        &fixture,
        Duration::ZERO,
    );
    report("OVER_DEPTH", &result);
    assert_eq!(result.reason, "archive_nesting_depth");
    assert!(
        result.text.contains("limit=nesting-depth") && result.text.contains("2 levels"),
        "the refusal must name the limit: {}",
        result.text
    );
}

#[test]
fn task_5180_slow_archive_is_stopped_by_the_scan_time_limit() {
    let fixture = slow_zip(6);
    let limits = tightened().with_max_scan_time(Duration::from_millis(40));
    let result = run_bound_fixture(
        "timeout",
        limits,
        "slow.zip",
        "application/zip",
        &fixture,
        Duration::from_millis(30),
    );
    report("TIMEOUT", &result);
    assert_eq!(result.reason, "archive_scan_time");
    assert!(
        result.text.contains("limit=scan-time") && result.text.contains("40 ms"),
        "the refusal must name the limit: {}",
        result.text
    );
}

// ---------------------------------------------------------------------------
// 3. The boundary itself
// ---------------------------------------------------------------------------

#[test]
fn task_5180_shipping_ceilings_can_only_be_tightened() {
    let shipping = ArchiveLimits::shipping();
    println!("TASK5180_LIMIT_CEILINGS={}", shipping.describe());
    assert_eq!(shipping.max_expanded_bytes, MAX_EXPANDED_BYTES);
    assert_eq!(shipping.max_entries, MAX_ENTRIES);
    assert_eq!(shipping.max_depth, MAX_DEPTH);
    assert_eq!(shipping.max_scan_time.as_millis() as u64, MAX_SCAN_TIME_MS);

    let widened = ArchiveLimits::shipping()
        .with_max_expanded_bytes(u64::MAX)
        .with_max_entries(u32::MAX)
        .with_max_depth(u32::MAX)
        .with_max_scan_time(Duration::from_secs(86_400))
        .clamped();
    println!("TASK5180_WIDENED_CLAMPED_TO={}", widened.describe());
    assert_eq!(widened, shipping);

    let root = temp_root("ceilings");
    let quarantine = ProtectedDownloadQuarantine::with_roots(
        root.path().join("quarantine"),
        root.path().join("exposure"),
    )
    .expect("quarantine opens")
    .with_archive_limits(
        ArchiveLimits::shipping()
            .with_max_expanded_bytes(u64::MAX)
            .with_max_entries(u32::MAX),
    );
    println!(
        "TASK5180_QUARANTINE_LIMITS={}",
        quarantine.archive_limits().describe()
    );
    assert_eq!(quarantine.archive_limits(), shipping);
}

#[test]
fn task_5180_containers_are_recognised_from_content_not_from_the_name() {
    let zip = clean_nested_zip();
    let tar = tar_bytes(&[("a.txt", b"a".to_vec())]);
    let gz = gzip_bytes(b"a");
    let plain = b"TASK 5180 ordinary text, nothing to expand.\n".to_vec();
    for (label, bytes, expected) in [
        ("ZIP", zip, ArchiveFormat::Zip),
        ("TAR", tar, ArchiveFormat::Tar),
        ("GZIP", gz, ArchiveFormat::Gzip),
        ("PLAIN", plain, ArchiveFormat::NotAnArchive),
        (
            "SEVEN_ZIP",
            b"7z\xbc\xaf\x27\x1c\x00\x04".to_vec(),
            ArchiveFormat::Unsupported("7-Zip"),
        ),
        (
            "RAR",
            b"Rar!\x1a\x07\x01\x00".to_vec(),
            ArchiveFormat::Unsupported("RAR"),
        ),
    ] {
        let detected = detect_archive_format(&bytes);
        println!(
            "TASK5180_DETECT_{label}={} INSPECTED={}",
            detected.name(),
            detected.needs_inspection()
        );
        assert_eq!(detected, expected);
    }
}

#[test]
fn task_5180_a_plain_download_is_not_expanded_and_still_releases() {
    let bench = bench("plain", tightened());
    let payload = b"TASK 5180 ordinary protected download, not a container.\n".to_vec();
    let held = admit_sealed(&bench, "notes.txt", "text/plain", &payload);
    let scanner = RecordingScanner::new(
        &bench.quarantine_root,
        &bench.exposure_root,
        &bench.destination,
    );
    let saver = RecordingSaver::new(&scanner);
    let outcome = deliver(&bench, held, &scanner, &saver);
    let delivered = match outcome {
        FinalSaveOutcome::Delivered(delivered) => delivered,
        other => panic!("a plain download must deliver, got {}", other.name()),
    };
    println!("TASK5180_PLAIN_SCAN_CALLS={}", scanner.calls());
    println!("TASK5180_PLAIN_DELIVERED_BYTES={}", delivered.bytes);
    assert_eq!(scanner.calls(), 1);
    assert_eq!(delivered.bytes, payload.len() as u64);
}

#[test]
fn task_5180_an_unwalkable_container_is_never_released() {
    let bench = bench("unsupported", tightened());
    // A real 7-Zip signature. This build cannot walk its entries, so it is
    // unable to verify - never "not an archive", never released.
    let mut fixture = b"7z\xbc\xaf\x27\x1c\x00\x04".to_vec();
    fixture.extend_from_slice(&[0u8; 64]);
    let held = admit_sealed(&bench, "vault.zip", "application/zip", &fixture);
    let scanner = RecordingScanner::new(
        &bench.quarantine_root,
        &bench.exposure_root,
        &bench.destination,
    );
    let saver = RecordingSaver::new(&scanner);
    let outcome = deliver(&bench, held, &scanner, &saver);
    let withheld = match outcome {
        FinalSaveOutcome::Withheld(withheld) => withheld,
        other => panic!("an unwalkable container must be withheld, got {}", other.name()),
    };
    println!(
        "TASK5180_UNSUPPORTED_OUTSIDE_BYTES={} REASON={} TEXT={}",
        directory_bytes(&bench.exposure_root).max(destination_bytes(&bench)),
        withheld.reason.name(),
        withheld.local_reason_text
    );
    assert_eq!(withheld.reason.name(), "archive_unsupported");
    assert_eq!(withheld.bytes_exposed, 0);
    assert_eq!(saver.calls(), 0);
    assert_eq!(directory_bytes(&bench.exposure_root), 0);
    assert_eq!(destination_bytes(&bench), 0);
}

#[test]
fn task_5180_the_release_door_is_the_only_way_past_the_unpacker() {
    // The archive expansion is not an optional extra step a caller could skip:
    // it is inside `scan_and_release`, which is the only function that reaches
    // `release`, and `deliver_after_clean_scan` is built on it.
    let quarantine_source = include_str!("../src/protected_download_quarantine.rs");
    let final_save_source = include_str!("../src/protected_download_final_save.rs");
    let inspect_calls = quarantine_source
        .matches("protected_archive::inspect_if_archive")
        .count();
    let release_calls = final_save_source.matches("scan_and_release").count();
    println!("TASK5180_RELEASE_DOOR_INSPECT_CALLS={inspect_calls}");
    println!("TASK5180_FINAL_SAVE_USES_SCAN_AND_RELEASE={release_calls}");
    assert!(inspect_calls >= 1);
    assert!(release_calls >= 1);
    // The inspection happens before the release, not after it.
    let inspect_at = quarantine_source
        .find("protected_archive::inspect_if_archive")
        .expect("the release door inspects archives");
    let release_at = quarantine_source
        .find("match self.release(held, &scan)")
        .expect("the release door releases");
    println!("TASK5180_INSPECT_BEFORE_RELEASE={}", inspect_at < release_at);
    assert!(inspect_at < release_at);
}
