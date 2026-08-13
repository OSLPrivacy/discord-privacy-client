//! TASK 5180a - reject traversal, absolute-path, link and special-file archive
//! entries.
//!
//! Every fixture here goes through the *shipping* door, exactly as TASK 5180's
//! own check does: the archive is sealed with
//! `peer_attachment_io::encrypt_file`, opened only into the TASK 5166
//! quarantine with `peer_attachment_io::decrypt_file`, adopted by
//! `ProtectedDownloadQuarantine`, and handed to
//! `protected_download_final_save::deliver_after_clean_scan` - the production
//! function that scans, releases and performs the final save. The archive
//! expansion is reached from inside that call, so a fixture that never reaches
//! the real unpacker cannot pass here.
//!
//! The hostile archives are built byte by byte. `tar::Builder` refuses to
//! *write* a `..` or an absolute member name ("paths in archives must be
//! relative"), and a real attacker is under no such obligation, so the ustar
//! headers below are assembled by hand - name, type flag, link name and all.
//! The zip fixtures use `zip::ZipWriter`, whose `start_file` stores whatever
//! name it is given.
//!
//! What each fixture has to show:
//!
//! * the entry is refused, and the refusal names the entry and the kind;
//! * zero bytes are written outside quarantine - measured from the filesystem,
//!   including the exact path the hostile entry aimed at;
//! * zero bytes are released to the caller and the final save is never reached;
//! * the refusal fired *before* the entry was counted and before its first byte
//!   was written, which the refusal reports as
//!   `entries-unpacked-before-refusal 0, bytes-unpacked-before-refusal 0`;
//! * and a clean control carrying ordinary nested directories still releases.

use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use osl_privacy_hub::download_zone_handoff::{
    AttachmentServicesSaver, PlatformLimitation, ZoneHandoffFailure, ZoneHandoffOutcome,
    ZoneHandoffRequest, ZoneLimitation,
};
use osl_privacy_hub::protected_archive::{ArchiveLimits, HostileEntryKind};
use osl_privacy_hub::protected_download_final_save::{deliver_after_clean_scan, FinalSaveOutcome};
use osl_privacy_hub::protected_download_quarantine::{
    now_unix_seconds, sha256_hex, AmsiFailure, AmsiProvider, AmsiReport, AmsiSubmission,
    ProtectedDownloadOutcome, ProtectedDownloadQuarantine, QuarantinedDownload,
    AMSI_RESULT_NOT_DETECTED,
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
        "osl-task5180a-{label}-{}-{nonce}",
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
                Ok(kind) if kind.is_file() || kind.is_symlink() => {
                    total = total.saturating_add(entry.metadata().map(|m| m.len()).unwrap_or(0));
                }
                _ => {}
            }
        }
    }
    total
}

/// Every `archive-expansion-*` directory anywhere under the system temp root
/// that is *not* inside the quarantine, and the bytes in it.
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
            // Only descend through this test's own throwaway roots.
            let ours = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("osl-task5180a-"))
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

/// Answers clean, counts every call, and on every call re-measures what is
/// visible outside quarantine, so a leak is caught while the expansion is still
/// running rather than after the workspace has been cleaned up.
struct RecordingScanner {
    calls: AtomicU32,
    quarantine_root: PathBuf,
    exposure_root: PathBuf,
    destination: PathBuf,
    outside_walk_roots: Vec<PathBuf>,
    max_outside_bytes: AtomicU64,
    submitted: Mutex<Vec<(String, u64)>>,
}

impl RecordingScanner {
    fn new(bench: &Bench) -> Self {
        Self {
            calls: AtomicU32::new(0),
            quarantine_root: bench.quarantine_root.clone(),
            exposure_root: bench.exposure_root.clone(),
            destination: bench.destination.clone(),
            outside_walk_roots: bench.outside_walk_roots(),
            max_outside_bytes: AtomicU64::new(0),
            submitted: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> u32 {
        self.calls.load(Ordering::Acquire)
    }

    fn max_outside_bytes(&self) -> u64 {
        self.max_outside_bytes.load(Ordering::Acquire)
    }

    fn measure_outside(&self) -> u64 {
        let mut outside = directory_bytes(&self.exposure_root)
            .saturating_add(
                std::fs::metadata(&self.destination)
                    .map(|meta| meta.len())
                    .unwrap_or(0),
            )
            .saturating_add(expansion_bytes_outside_quarantine(&self.quarantine_root));
        for root in &self.outside_walk_roots {
            outside = outside.saturating_add(directory_bytes(root));
        }
        outside
    }
}

impl AmsiProvider for RecordingScanner {
    fn scan(&self, submission: &AmsiSubmission<'_>) -> Result<AmsiReport, AmsiFailure> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        let outside = self.measure_outside();
        self.max_outside_bytes.fetch_max(outside, Ordering::AcqRel);
        if let Ok(mut entries) = self.submitted.lock() {
            entries.push((
                submission.content_sha256.to_owned(),
                submission.plaintext.len() as u64,
            ));
        }
        Ok(AmsiReport {
            result_code: AMSI_RESULT_NOT_DETECTED,
            provider_identity: "OSL TASK 5180a local fixture scanner".to_owned(),
            engine_version: "1.1.0.0".to_owned(),
            signature_version: "1.457.130.0".to_owned(),
            signature_updated_unix: now_unix_seconds(),
            scanned_sha256: sha256_hex(submission.plaintext),
            scanned_len: submission.plaintext.len() as u64,
        })
    }
}

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
    fn save(&self, request: &ZoneHandoffRequest) -> Result<ZoneHandoffOutcome, ZoneHandoffFailure> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        self.scans_at_save
            .store(self.scanner.calls(), Ordering::Release);
        Ok(ZoneHandoffOutcome::PlatformLimited(PlatformLimitation {
            reason: ZoneLimitation::FilesystemCannotRetainMark,
            filesystem: "ext4".to_owned(),
            windows_path: request.local_path.display().to_string(),
            save_calls: 1,
            message: "TASK 5180a fixture saver: this Linux lane cannot carry a zone mark"
                .to_owned(),
        }))
    }
}

// ---------------------------------------------------------------------------
// Hand-built ustar - the only way to author a hostile tar
// ---------------------------------------------------------------------------

const TAR_REGULAR: u8 = b'0';
const TAR_HARD_LINK: u8 = b'1';
const TAR_SYMLINK: u8 = b'2';
const TAR_CHAR_DEVICE: u8 = b'3';
const TAR_BLOCK_DEVICE: u8 = b'4';
const TAR_DIRECTORY: u8 = b'5';
const TAR_FIFO: u8 = b'6';

fn put(field: &mut [u8], value: &[u8]) {
    let taken = value.len().min(field.len());
    field[..taken].copy_from_slice(&value[..taken]);
}

/// One 512-byte ustar header plus its padded payload. Nothing about the name,
/// the type flag or the link name is validated: that is the point.
fn ustar_entry(name: &str, type_flag: u8, link_name: &str, data: &[u8]) -> Vec<u8> {
    let mut header = [0u8; 512];
    put(&mut header[0..100], name.as_bytes());
    put(&mut header[100..108], b"0000644\0");
    put(&mut header[108..116], b"0000000\0");
    put(&mut header[116..124], b"0000000\0");
    put(
        &mut header[124..136],
        format!("{:011o}\0", data.len()).as_bytes(),
    );
    put(&mut header[136..148], b"00000000000\0");
    // The checksum is computed with this field read as eight spaces.
    put(&mut header[148..156], b"        ");
    header[156] = type_flag;
    put(&mut header[157..257], link_name.as_bytes());
    put(&mut header[257..263], b"ustar\0");
    put(&mut header[263..265], b"00");
    put(&mut header[265..297], b"osl\0");
    put(&mut header[297..329], b"osl\0");
    put(&mut header[329..337], b"0000000\0");
    put(&mut header[337..345], b"0000000\0");
    let checksum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
    put(&mut header[148..156], format!("{checksum:06o}\0 ").as_bytes());

    let mut out = header.to_vec();
    out.extend_from_slice(data);
    let padding = (512 - data.len() % 512) % 512;
    out.extend(std::iter::repeat_n(0u8, padding));
    out
}

/// Concatenate entries and close the archive with the two zero blocks.
fn ustar_archive(entries: Vec<Vec<u8>>) -> Vec<u8> {
    let mut out: Vec<u8> = entries.into_iter().flatten().collect();
    out.extend(std::iter::repeat_n(0u8, 1024));
    out
}

// ---------------------------------------------------------------------------
// Zip fixtures
// ---------------------------------------------------------------------------

fn zip_options() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)
}

fn zip_bytes(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        for (name, bytes) in entries {
            writer.start_file(*name, zip_options()).expect("zip entry");
            writer.write_all(bytes).expect("zip entry bytes");
        }
        writer.finish().expect("zip finish");
    }
    cursor.into_inner()
}

fn gzip_bytes(payload: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(payload).expect("gzip write");
    encoder.finish().expect("gzip finish")
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const TRAVERSAL_ENTRY: &str = "../../osl-5180a-traversal-escape.txt";
const NESTED_TRAVERSAL_ENTRY: &str = "../../osl-5180a-nested-traversal-escape.txt";
const TRAVERSAL_PAYLOAD: &[u8] = b"TASK 5180a: this entry tried to climb out of the workspace.\n";

/// The clean control: ordinary nested directories at both levels, including
/// explicit directory entries in the zip and in the inner tar.
fn clean_nested_directory_zip() -> Vec<u8> {
    let inner_tar = ustar_archive(vec![
        ustar_entry(
            "beta.txt",
            TAR_REGULAR,
            "",
            b"TASK 5180a inner tar entry beta.\n",
        ),
        ustar_entry("deep/", TAR_DIRECTORY, "", b""),
        ustar_entry("deep/gamma.bin", TAR_REGULAR, "", &[0x2au8; 96]),
    ]);
    let delta_gz = gzip_bytes(b"TASK 5180a gzip member delta.\n");
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        writer
            .add_directory("notes", zip_options())
            .expect("zip directory entry");
        writer
            .start_file("notes/alpha.txt", zip_options())
            .expect("zip entry");
        writer
            .write_all(b"TASK 5180a clean nested archive, entry alpha.\n")
            .expect("zip entry bytes");
        writer
            .start_file("inner.tar", zip_options())
            .expect("zip entry");
        writer.write_all(&inner_tar).expect("zip entry bytes");
        writer
            .start_file("delta.txt.gz", zip_options())
            .expect("zip entry");
        writer.write_all(&delta_gz).expect("zip entry bytes");
        writer.finish().expect("zip finish");
    }
    cursor.into_inner()
}

fn traversal_zip() -> Vec<u8> {
    zip_bytes(&[
        (TRAVERSAL_ENTRY, TRAVERSAL_PAYLOAD.to_vec()),
        ("notes/alpha.txt", b"TASK 5180a decoy entry.\n".to_vec()),
    ])
}

fn traversal_tar() -> Vec<u8> {
    ustar_archive(vec![ustar_entry(
        TRAVERSAL_ENTRY,
        TAR_REGULAR,
        "",
        TRAVERSAL_PAYLOAD,
    )])
}

/// zip > tar > `../../escape`: the hostile entry only exists at nesting level
/// two, behind a clean first entry, so a guard that only ran on the outermost
/// container would miss it.
fn nested_traversal_zip() -> Vec<u8> {
    let inner_tar = ustar_archive(vec![ustar_entry(
        NESTED_TRAVERSAL_ENTRY,
        TAR_REGULAR,
        "",
        TRAVERSAL_PAYLOAD,
    )]);
    zip_bytes(&[
        (
            "notes/alpha.txt",
            b"TASK 5180a clean first entry.\n".to_vec(),
        ),
        ("inner.tar", inner_tar),
    ])
}

fn absolute_entry_name(root: &Path) -> String {
    root.join("osl-5180a-absolute-escape.txt").display().to_string()
}

fn absolute_zip(root: &Path) -> Vec<u8> {
    zip_bytes(&[(
        absolute_entry_name(root).as_str(),
        b"TASK 5180a: this entry named an absolute path.\n".to_vec(),
    )])
}

fn absolute_tar(root: &Path) -> Vec<u8> {
    ustar_archive(vec![ustar_entry(
        &absolute_entry_name(root),
        TAR_REGULAR,
        "",
        b"TASK 5180a: this entry named an absolute path.\n",
    )])
}

/// The classic symlink escape: plant a link out of the workspace, then write
/// through it. The guard has to stop the link entry; the follow-up entry proves
/// what it is protecting.
fn symlink_tar() -> Vec<u8> {
    ustar_archive(vec![
        ustar_entry("escape-link", TAR_SYMLINK, "../..", b""),
        ustar_entry(
            "escape-link/osl-5180a-link-escape.txt",
            TAR_REGULAR,
            "",
            b"TASK 5180a: written through a symlink.\n",
        ),
    ])
}

fn symlink_zip() -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        writer
            .add_symlink("passwd-link", "/etc/passwd", zip_options())
            .expect("zip symlink entry");
        writer.finish().expect("zip finish");
    }
    cursor.into_inner()
}

fn hard_link_tar() -> Vec<u8> {
    ustar_archive(vec![ustar_entry(
        "hard-link.txt",
        TAR_HARD_LINK,
        "../../../etc/shadow",
        b"",
    )])
}

fn special_file_tar(name: &str, type_flag: u8) -> Vec<u8> {
    ustar_archive(vec![ustar_entry(name, type_flag, "", b"")])
}

// ---------------------------------------------------------------------------
// The shipping ingress: seal, decrypt into quarantine, adopt
// ---------------------------------------------------------------------------

struct Bench {
    root: TempRoot,
    quarantine: ProtectedDownloadQuarantine,
    quarantine_root: PathBuf,
    exposure_root: PathBuf,
    destination: PathBuf,
    sealed_root: PathBuf,
}

impl Bench {
    fn root(&self) -> &Path {
        self.root.path()
    }

    /// Everything in this bench that is *not* the quarantine and not the
    /// sender's own sealed staging copy. A hostile entry that escaped the
    /// workspace lands in here, and it must always weigh zero bytes.
    fn outside_walk_roots(&self) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        let Ok(entries) = std::fs::read_dir(self.root()) else {
            return roots;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path == self.quarantine_root || path == self.sealed_root {
                continue;
            }
            roots.push(path);
        }
        roots
    }

    /// Bytes sitting anywhere in this bench outside the quarantine, measured
    /// from the filesystem after the run.
    fn bytes_outside_quarantine(&self) -> u64 {
        let mut total = 0u64;
        let Ok(entries) = std::fs::read_dir(self.root()) else {
            return 0;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path == self.quarantine_root || path == self.sealed_root {
                continue;
            }
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => total = total.saturating_add(directory_bytes(&path)),
                Ok(_) => {
                    total = total.saturating_add(entry.metadata().map(|m| m.len()).unwrap_or(0))
                }
                Err(_) => {}
            }
        }
        total
    }
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
        root,
        quarantine,
        quarantine_root,
        exposure_root,
        destination,
        sealed_root,
    }
}

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
        "https://osl.invalid/task-5180a",
        "https://osl.invalid/",
        now_unix_seconds(),
    )
}

fn destination_bytes(bench: &Bench) -> u64 {
    std::fs::metadata(&bench.destination)
        .map(|meta| meta.len())
        .unwrap_or(0)
}

/// The same tightening TASK 5180 uses, so the resource bounds are in force and
/// cannot be what stopped a hostile fixture.
fn tightened() -> ArchiveLimits {
    ArchiveLimits::shipping()
        .with_max_expanded_bytes(1024 * 1024)
        .with_max_entries(8)
        .with_max_depth(2)
        .with_max_scan_time(Duration::from_secs(30))
}

// ---------------------------------------------------------------------------
// One hostile fixture, through the shipping door
// ---------------------------------------------------------------------------

struct HostileResult {
    reason: String,
    text: String,
    released_bytes: u64,
    saver_calls: u32,
    scan_calls: u32,
    outside_bytes: u64,
    escaped_paths: Vec<String>,
}

/// Run one hostile archive through `deliver_after_clean_scan` and measure what
/// it managed to do. `build` is handed the bench root so a fixture can name an
/// absolute path that would land inside this test's own throwaway root.
fn run_hostile<F>(label: &str, filename: &str, mime: &str, build: F) -> HostileResult
where
    F: Fn(&Path) -> (Vec<u8>, Vec<PathBuf>),
{
    let bench = bench(label, tightened());
    let (fixture, escape_targets) = build(bench.root());
    let held = admit_sealed(&bench, filename, mime, &fixture);
    let scanner = RecordingScanner::new(&bench);
    let saver = RecordingSaver::new(&scanner);

    let outcome = deliver(&bench, held, &scanner, &saver);
    let withheld = match outcome {
        FinalSaveOutcome::Withheld(withheld) => withheld,
        other => panic!("{label} must be withheld, got {}", other.name()),
    };

    let escaped_paths: Vec<String> = escape_targets
        .iter()
        .filter(|path| path.exists())
        .map(|path| {
            format!(
                "{} ({} bytes)",
                path.display(),
                std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
            )
        })
        .collect();

    let outside = scanner
        .max_outside_bytes()
        .max(directory_bytes(&bench.exposure_root))
        .max(destination_bytes(&bench))
        .max(expansion_bytes_outside_quarantine(&bench.quarantine_root))
        .max(bench.bytes_outside_quarantine());

    HostileResult {
        reason: withheld.reason.name().to_owned(),
        text: withheld.local_reason_text.clone(),
        released_bytes: withheld.bytes_exposed,
        saver_calls: saver.calls(),
        scan_calls: scanner.calls(),
        outside_bytes: outside,
        escaped_paths,
    }
}

/// Everything the finish line asks of a hostile fixture, in one place.
fn assert_refused(
    tag: &str,
    result: &HostileResult,
    expected_reason: &str,
    expected_kind: HostileEntryKind,
    expected_entry: &str,
    first_entry_is_hostile: bool,
) {
    println!(
        "TASK5180A_{tag}_REASON={} OUTSIDE_BYTES={} RELEASED_BYTES={} SAVER_CALLS={} \
         SCAN_CALLS={} ESCAPED_PATHS={} TEXT={}",
        result.reason,
        result.outside_bytes,
        result.released_bytes,
        result.saver_calls,
        result.scan_calls,
        if result.escaped_paths.is_empty() {
            "none".to_owned()
        } else {
            result.escaped_paths.join(" | ")
        },
        result.text
    );
    assert_eq!(
        result.escaped_paths.len(),
        0,
        "{tag} wrote the hostile entry outside quarantine: {:?}",
        result.escaped_paths
    );
    assert_eq!(result.outside_bytes, 0, "{tag} wrote bytes outside quarantine");
    assert_eq!(result.released_bytes, 0, "{tag} released bytes to the caller");
    assert_eq!(result.saver_calls, 0, "{tag} reached the final save");
    assert_eq!(
        result.reason, expected_reason,
        "{tag} was refused as the wrong reason"
    );
    assert!(
        result
            .text
            .contains(&format!("limit={}", expected_kind.name())),
        "{tag} did not name the rejection kind: {}",
        result.text
    );
    assert!(
        result.text.contains("archive entry '") && result.text.contains(expected_entry),
        "{tag} did not name the rejected entry: {}",
        result.text
    );
    if first_entry_is_hostile {
        // The guard fired before the entry was counted and before its first
        // byte was written: nothing at all had been unpacked.
        assert!(
            result
                .text
                .contains("entries-unpacked-before-refusal 0, bytes-unpacked-before-refusal 0"),
            "{tag} did not refuse before unpacking: {}",
            result.text
        );
        // One whole-file receipt and no entry receipts: the hostile entry was
        // never even submitted to the scanner.
        assert_eq!(result.scan_calls, 1, "{tag} scanned the hostile entry");
    }
}

// ---------------------------------------------------------------------------
// 1. Parent traversal
// ---------------------------------------------------------------------------

#[test]
fn task_5180a_parent_traversal_entries_are_refused_before_they_are_written() {
    let zip = run_hostile("traversal-zip", "traversal.zip", "application/zip", |root| {
        (
            traversal_zip(),
            vec![root.join("osl-5180a-traversal-escape.txt")],
        )
    });
    assert_refused(
        "TRAVERSAL_ZIP",
        &zip,
        "archive_parent_traversal",
        HostileEntryKind::ParentTraversal,
        TRAVERSAL_ENTRY,
        true,
    );

    let tar = run_hostile(
        "traversal-tar",
        "traversal.tar",
        "application/x-tar",
        |root| {
            (
                traversal_tar(),
                vec![root.join("osl-5180a-traversal-escape.txt")],
            )
        },
    );
    assert_refused(
        "TRAVERSAL_TAR",
        &tar,
        "archive_parent_traversal",
        HostileEntryKind::ParentTraversal,
        TRAVERSAL_ENTRY,
        true,
    );
}

#[test]
fn task_5180a_a_traversal_entry_two_levels_down_is_still_refused() {
    let nested = run_hostile(
        "traversal-nested",
        "nested.zip",
        "application/zip",
        |root| {
            (
                nested_traversal_zip(),
                vec![root.join("osl-5180a-nested-traversal-escape.txt")],
            )
        },
    );
    assert_refused(
        "TRAVERSAL_NESTED",
        &nested,
        "archive_parent_traversal",
        HostileEntryKind::ParentTraversal,
        NESTED_TRAVERSAL_ENTRY,
        false,
    );
    // Two clean entries were unpacked at level one before the hostile entry was
    // reached at level two - so this really is the recursive guard talking, and
    // it still reports the refused entry as contributing nothing.
    assert!(
        nested
            .text
            .contains("entries-unpacked-before-refusal 2,"),
        "the nested refusal must report the entries already unpacked: {}",
        nested.text
    );
}

// ---------------------------------------------------------------------------
// 2. Absolute paths
// ---------------------------------------------------------------------------

#[test]
fn task_5180a_absolute_path_entries_are_refused_before_they_are_written() {
    let zip = run_hostile("absolute-zip", "absolute.zip", "application/zip", |root| {
        (
            absolute_zip(root),
            vec![root.join("osl-5180a-absolute-escape.txt")],
        )
    });
    println!("TASK5180A_ABSOLUTE_ZIP_ENTRY_WAS_ABSOLUTE=true");
    assert_refused(
        "ABSOLUTE_ZIP",
        &zip,
        "archive_absolute_path",
        HostileEntryKind::AbsolutePath,
        "osl-5180a-absolute-escape.txt",
        true,
    );

    let tar = run_hostile("absolute-tar", "absolute.tar", "application/x-tar", |root| {
        (
            absolute_tar(root),
            vec![root.join("osl-5180a-absolute-escape.txt")],
        )
    });
    assert_refused(
        "ABSOLUTE_TAR",
        &tar,
        "archive_absolute_path",
        HostileEntryKind::AbsolutePath,
        "osl-5180a-absolute-escape.txt",
        true,
    );
}

// ---------------------------------------------------------------------------
// 3. Links
// ---------------------------------------------------------------------------

#[test]
fn task_5180a_symbolic_link_entries_are_refused() {
    let tar = run_hostile("symlink-tar", "symlink.tar", "application/x-tar", |root| {
        (
            symlink_tar(),
            vec![root.join("osl-5180a-link-escape.txt")],
        )
    });
    assert_refused(
        "SYMLINK_TAR",
        &tar,
        "archive_symbolic_link",
        HostileEntryKind::SymbolicLink,
        "escape-link",
        true,
    );
    assert!(
        tar.text.contains("-> '../..'"),
        "the refusal must name where the link pointed: {}",
        tar.text
    );

    let zip = run_hostile("symlink-zip", "symlink.zip", "application/zip", |_root| {
        (symlink_zip(), Vec::new())
    });
    assert_refused(
        "SYMLINK_ZIP",
        &zip,
        "archive_symbolic_link",
        HostileEntryKind::SymbolicLink,
        "passwd-link",
        true,
    );
    assert!(
        zip.text.contains("-> '/etc/passwd'"),
        "the refusal must name where the link pointed: {}",
        zip.text
    );
}

#[test]
fn task_5180a_hard_link_entries_are_refused() {
    let tar = run_hostile("hardlink-tar", "hardlink.tar", "application/x-tar", |_root| {
        (hard_link_tar(), Vec::new())
    });
    assert_refused(
        "HARD_LINK_TAR",
        &tar,
        "archive_hard_link",
        HostileEntryKind::HardLink,
        "hard-link.txt",
        true,
    );
    assert!(
        tar.text.contains("-> '../../../etc/shadow'"),
        "the refusal must name where the link pointed: {}",
        tar.text
    );
}

// ---------------------------------------------------------------------------
// 4. Special files
// ---------------------------------------------------------------------------

#[test]
fn task_5180a_special_file_entries_are_refused() {
    for (tag, label, entry, type_flag, expected_text) in [
        ("FIFO", "special-fifo", "pipe.fifo", TAR_FIFO, "FIFO"),
        (
            "CHAR_DEVICE",
            "special-char",
            "dev/tty0",
            TAR_CHAR_DEVICE,
            "character device",
        ),
        (
            "BLOCK_DEVICE",
            "special-block",
            "dev/sda1",
            TAR_BLOCK_DEVICE,
            "block device",
        ),
    ] {
        let result = run_hostile(label, "special.tar", "application/x-tar", |_root| {
            (special_file_tar(entry, type_flag), Vec::new())
        });
        assert_refused(
            tag,
            &result,
            "archive_special_file",
            HostileEntryKind::SpecialFile,
            entry,
            true,
        );
        assert!(
            result.text.contains(expected_text),
            "the refusal must name the special-file kind: {}",
            result.text
        );
    }
}

// ---------------------------------------------------------------------------
// 5. The clean control
// ---------------------------------------------------------------------------

#[test]
fn task_5180a_clean_control_with_nested_directories_still_releases() {
    let bench = bench("clean-control", tightened());
    let fixture = clean_nested_directory_zip();
    let held = admit_sealed(&bench, "bundle.zip", "application/zip", &fixture);
    let scanner = RecordingScanner::new(&bench);
    let saver = RecordingSaver::new(&scanner);

    let outcome = deliver(&bench, held, &scanner, &saver);
    let delivered = match outcome {
        FinalSaveOutcome::Delivered(delivered) => delivered,
        other => panic!(
            "the clean control with nested directories must deliver, got {}",
            other.name()
        ),
    };
    println!("TASK5180A_CLEAN_DELIVERED_BYTES={}", delivered.bytes);
    println!("TASK5180A_CLEAN_SAVER_CALLS={}", saver.calls());
    println!("TASK5180A_CLEAN_SCAN_CALLS={}", scanner.calls());
    println!("TASK5180A_CLEAN_SCANS_BEFORE_SAVE={}", saver.scans_at_save());
    println!(
        "TASK5180A_CLEAN_OUTSIDE_BYTES_DURING_SCANS={}",
        scanner.max_outside_bytes()
    );
    println!(
        "TASK5180A_CLEAN_DELIVERED_PATH_IN_QUARANTINE={}",
        delivered.path.starts_with(&bench.quarantine_root)
    );
    assert_eq!(delivered.bytes, fixture.len() as u64);
    assert!(!delivered.path.starts_with(&bench.quarantine_root));
    assert_eq!(saver.calls(), 1);
    assert_eq!(saver.scans_at_save(), scanner.calls());
}

#[test]
fn task_5180a_clean_control_directories_are_walked_but_never_counted_as_entries() {
    let bench = bench("clean-entries", tightened());
    let fixture = clean_nested_directory_zip();
    let held = admit_sealed(&bench, "bundle.zip", "application/zip", &fixture);
    let scanner = RecordingScanner::new(&bench);

    let outcome = bench
        .quarantine
        .scan_and_release(held, &scanner, now_unix_seconds());
    let exposed = match outcome {
        ProtectedDownloadOutcome::Exposed(exposed) => exposed,
        ProtectedDownloadOutcome::Withheld(withheld) => {
            panic!("clean control withheld: {}", withheld.local_reason_text)
        }
    };
    let archive = exposed
        .archive
        .expect("a container release carries its inspection");

    println!("TASK5180A_CLEAN_FORMAT={}", archive.format);
    println!("TASK5180A_CLEAN_ENTRIES_SEEN={}", archive.entries_seen);
    println!("TASK5180A_CLEAN_ENTRIES_SCANNED={}", archive.entries_scanned);
    println!(
        "TASK5180A_CLEAN_ENTRY_SCAN_CALLS={}",
        archive.entry_scan_calls
    );
    println!("TASK5180A_CLEAN_DEEPEST_LEVEL={}", archive.deepest_level);
    println!(
        "TASK5180A_CLEAN_ENTRY_ORDER={}",
        archive.entry_order.join(",")
    );
    println!(
        "TASK5180A_CLEAN_WORKSPACE_REMOVED={}",
        archive.workspace_removed
    );
    println!("TASK5180A_CLEAN_RELEASED_BYTES={}", exposed.bytes_exposed);
    println!(
        "TASK5180A_CLEAN_OUTSIDE_BYTES_AFTER_RELEASE={}",
        bench.bytes_outside_quarantine()
    );

    // `notes/` in the zip and `deep/` in the inner tar are ordinary nested
    // directories: they are validated like every other entry, then skipped.
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
    assert!(archive.workspace_removed);
    assert_eq!(scanner.calls(), 1 + archive.entries_seen);
    assert_eq!(exposed.bytes_exposed, fixture.len() as u64);
    assert_eq!(scanner.max_outside_bytes(), 0);
    // Nothing was outside quarantine while the entries were being scanned, and
    // afterwards the only thing outside it is the released archive itself -
    // not one byte that came out of a container.
    assert_eq!(bench.bytes_outside_quarantine(), exposed.bytes_exposed);
}

// ---------------------------------------------------------------------------
// 6. The guards are where they have to be
// ---------------------------------------------------------------------------

#[test]
fn task_5180a_every_guard_runs_before_the_entry_is_counted_or_written() {
    // The fixtures above are what proves the behaviour; this reads the shipping
    // source to prove the *ordering* is structural rather than incidental, and
    // that the starvation markers TASK 5180b drives are still where it looks.
    let source = include_str!("../src/protected_archive.rs");
    for marker in [
        "TASK5180A-GUARD-ABSOLUTE-PATH",
        "TASK5180A-GUARD-PARENT-TRAVERSAL",
        "TASK5180A-GUARD-SYMBOLIC-LINK",
        "TASK5180A-GUARD-HARD-LINK",
        "TASK5180A-GUARD-SPECIAL-FILE",
    ] {
        let count = source.matches(marker).count();
        println!("TASK5180A_GUARD_MARKER_{marker}={count}");
        assert!(count >= 1, "the {marker} guard marker is missing");
    }

    for (walker, next) in [("fn walk_zip", "fn walk_tar"), ("fn walk_tar", "fn walk_gzip")] {
        let start = source.find(walker).expect("walker present");
        let end = source.find(next).expect("next walker present");
        let body = &source[start..end];
        let validate = body
            .find("let entry_path = self.entry_path(&name)?;")
            .unwrap_or_else(|| panic!("{walker} validates the entry name"));
        let count = body
            .find("self.count_entry(&name)?;")
            .unwrap_or_else(|| panic!("{walker} counts the entry"));
        let copy = body
            .find("self.copy_entry_bounded(")
            .unwrap_or_else(|| panic!("{walker} writes the entry"));
        let guard = body
            .find("TASK5180A-GUARD-")
            .unwrap_or_else(|| panic!("{walker} carries a TASK 5180a guard"));
        println!(
            "TASK5180A_ORDER[{walker}] validate={validate} guard={guard} count={count} copy={copy}"
        );
        assert!(validate < count && validate < copy);
        assert!(guard < count && guard < copy);
    }

    // The destination is built by joining validated components onto the
    // workspace - there is no path in the module that joins a raw archive name.
    assert!(source.contains("Ok(self.workspace.join(relative))"));
    println!("TASK5180A_DESTINATION_IS_BUILT_FROM_VALIDATED_COMPONENTS=true");
}
