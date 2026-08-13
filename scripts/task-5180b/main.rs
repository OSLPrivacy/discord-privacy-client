//! TASK 5180b - prove the hard quarantine limits on archive expansion cannot
//! be starved silently.
//!
//! Drives the VERBATIM production sources
//! (`apps/osl-hub/src/protected_archive.rs` and
//! `apps/osl-hub/src/protected_download_quarantine.rs`, copied in beside this
//! file) through the real release door, `scan_and_release`. The harness script
//! runs it once untouched and once per starved bound:
//!
//!   real              - nothing touched. The clean nested archive must
//!                       release only after every entry has scanned, and each
//!                       bound fixture must be withheld with zero bytes
//!                       outside quarantine. Exit 0.
//!   expanded-bytes    - the byte ceiling disabled -> bomb.zip escapes.
//!   entry-count       - the entry ceiling disabled -> swarm.zip escapes.
//!   nesting-depth     - the depth ceiling disabled -> deep.zip escapes.
//!   scan-time         - the deadline disabled -> slow.zip escapes.
//!   clean-control     - the per-entry receipt skipped -> bundle.zip is
//!                       released without its entries ever being scanned.
//!   quarantine-root   - the expansion workspace moved out of the quarantine
//!                       -> bundle.zip is expanded outside the boundary.
//!
//! TASK 5180a adds the hostile-entry fixtures and the guards behind them:
//!
//!   parent-traversal  - the `..` rejection disabled -> traversal.zip writes
//!                       its entry outside the quarantine.
//!   absolute-path     - the absolute-path rejection disabled -> absolute.tar
//!                       writes its entry outside the quarantine.
//!   symbolic-link     - the symlink rejection disabled -> symlink.tar is
//!                       unpacked and released.
//!   hard-link         - the hard-link rejection disabled -> hardlink.tar is
//!                       unpacked and released.
//!   special-file      - the special-file rejection disabled -> special.tar is
//!                       unpacked and released.
//!
//! The two path modes are the sharpest of the set: the boundary still refuses
//! the archive afterwards, because `adopt` re-checks the quarantine root before
//! an entry is scanned - but by then the bytes are already on disk outside the
//! quarantine. A rejection applied after the write is exactly what this harness
//! has to catch, so the escape is measured from the filesystem, by name.
//!
//! Every starved run must exit 1 naming the escaped fixture. The escape is
//! measured from the filesystem and from the outcome the boundary returned -
//! including a watcher thread that polls the system temp root for
//! `archive-expansion-*` directories while the expansion is running - so a
//! starved boundary cannot talk its way to a pass.

// The copied production modules carry the whole boundary API; this harness
// drives part of it.
#[allow(dead_code)]
mod protected_archive;
#[allow(dead_code)]
mod protected_download_quarantine;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use protected_archive::ArchiveLimits;
use protected_download_quarantine::{
    now_unix_seconds, sha256_hex, AmsiFailure, AmsiProvider, AmsiReport, AmsiSubmission,
    ProtectedDownloadOutcome, ProtectedDownloadQuarantine, AMSI_RESULT_NOT_DETECTED,
};

const BOMB_UNCOMPRESSED_BYTES: usize = 4 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 1024 * 1024;
const MAX_ENTRIES: u32 = 8;
const MAX_DEPTH: u32 = 2;

// ---------------------------------------------------------------------------
// Local scanner double
// ---------------------------------------------------------------------------

struct Scanner {
    calls: AtomicU32,
    delay: Duration,
}

impl Scanner {
    fn new(delay: Duration) -> Self {
        Self {
            calls: AtomicU32::new(0),
            delay,
        }
    }

    fn calls(&self) -> u32 {
        self.calls.load(Ordering::Acquire)
    }
}

impl AmsiProvider for Scanner {
    fn scan(&self, submission: &AmsiSubmission<'_>) -> Result<AmsiReport, AmsiFailure> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
        Ok(AmsiReport {
            result_code: AMSI_RESULT_NOT_DETECTED,
            provider_identity: "OSL TASK 5180b local harness scanner".to_owned(),
            engine_version: "1.1.0.0".to_owned(),
            signature_version: "1.457.130.0".to_owned(),
            signature_updated_unix: now_unix_seconds(),
            scanned_sha256: sha256_hex(submission.plaintext),
            scanned_len: submission.plaintext.len() as u64,
        })
    }
}

// ---------------------------------------------------------------------------
// Fixtures, built as real archives
// ---------------------------------------------------------------------------

fn zip_bytes(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in entries {
            writer.start_file(name.as_str(), options).expect("zip entry");
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

/// The clean control. TASK 5180a adds the ordinary nested directory entries -
/// `notes/` in the zip and `deep/` in the inner tar - which must be walked,
/// validated and skipped without ever being counted as entries.
fn clean_nested_zip() -> Vec<u8> {
    let inner_tar = ustar_archive(vec![
        ustar_entry(
            "beta.txt",
            TAR_REGULAR,
            "",
            b"TASK 5180 inner tar entry beta.\n",
        ),
        ustar_entry("deep/", TAR_DIRECTORY, "", b""),
        ustar_entry("deep/gamma.bin", TAR_REGULAR, "", &[0x2au8; 96]),
    ]);
    let delta_gz = gzip_bytes(b"TASK 5180 gzip member delta.\n");
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer.add_directory("notes", options).expect("zip dir");
        writer
            .start_file("notes/alpha.txt", options)
            .expect("zip entry");
        writer
            .write_all(b"TASK 5180 clean nested archive, entry alpha.\n")
            .expect("zip entry bytes");
        writer.start_file("inner.tar", options).expect("zip entry");
        writer.write_all(&inner_tar).expect("zip entry bytes");
        writer
            .start_file("delta.txt.gz", options)
            .expect("zip entry");
        writer.write_all(&delta_gz).expect("zip entry bytes");
        writer.finish().expect("zip finish");
    }
    cursor.into_inner()
}

// ---------------------------------------------------------------------------
// TASK 5180a - hand-built hostile archives
// ---------------------------------------------------------------------------
//
// `tar::Builder` refuses to write a `..` or an absolute member name ("paths in
// archives must be relative"), and an attacker is under no such obligation, so
// these ustar headers are assembled byte by byte.

const TAR_REGULAR: u8 = b'0';
const TAR_HARD_LINK: u8 = b'1';
const TAR_SYMLINK: u8 = b'2';
const TAR_DIRECTORY: u8 = b'5';
const TAR_FIFO: u8 = b'6';

const TRAVERSAL_ESCAPE_NAME: &str = "osl-5180b-traversal-escape.txt";
const ABSOLUTE_ESCAPE_NAME: &str = "osl-5180b-absolute-escape.txt";

fn put(field: &mut [u8], value: &[u8]) {
    let taken = value.len().min(field.len());
    field[..taken].copy_from_slice(&value[..taken]);
}

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

fn ustar_archive(entries: Vec<Vec<u8>>) -> Vec<u8> {
    let mut out: Vec<u8> = entries.into_iter().flatten().collect();
    out.extend(std::iter::repeat_n(0u8, 1024));
    out
}

/// `../../<name>`: two levels up from the expansion workspace is the directory
/// that holds the quarantine, i.e. outside it.
fn traversal_zip() -> Vec<u8> {
    zip_bytes(&[(
        format!("../../{TRAVERSAL_ESCAPE_NAME}"),
        b"TASK 5180b: this entry climbed out of the workspace.\n".to_vec(),
    )])
}

/// An absolute member name pointing at this run's own throwaway root, so a
/// starved guard writes somewhere this harness owns and cleans up.
fn absolute_tar(root: &Path) -> Vec<u8> {
    let name = root.join(ABSOLUTE_ESCAPE_NAME).display().to_string();
    ustar_archive(vec![ustar_entry(
        &name,
        TAR_REGULAR,
        "",
        b"TASK 5180b: this entry named an absolute path.\n",
    )])
}

fn symlink_tar() -> Vec<u8> {
    ustar_archive(vec![
        ustar_entry("escape-link", TAR_SYMLINK, "../..", b""),
        ustar_entry(
            "escape-link/osl-5180b-link-escape.txt",
            TAR_REGULAR,
            "",
            b"TASK 5180b: written through a symlink.\n",
        ),
    ])
}

fn hardlink_tar() -> Vec<u8> {
    ustar_archive(vec![ustar_entry(
        "hard-link.txt",
        TAR_HARD_LINK,
        "../../../etc/shadow",
        b"",
    )])
}

fn special_tar() -> Vec<u8> {
    ustar_archive(vec![ustar_entry("pipe.fifo", TAR_FIFO, "", b"")])
}

fn over_byte_zip() -> Vec<u8> {
    zip_bytes(&[("filler.bin".to_owned(), vec![0u8; BOMB_UNCOMPRESSED_BYTES])])
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
    zip_bytes(&entries)
}

fn over_depth_zip() -> Vec<u8> {
    let level3 = zip_bytes(&[(
        "payload.txt".to_owned(),
        b"TASK 5180 fourth-level payload.\n".to_vec(),
    )]);
    let level2 = zip_bytes(&[("level3.zip".to_owned(), level3)]);
    zip_bytes(&[("level2.zip".to_owned(), level2)])
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
    zip_bytes(&built)
}

// ---------------------------------------------------------------------------
// Filesystem measurement
// ---------------------------------------------------------------------------

fn directory_bytes(root: &Path) -> u64 {
    let mut total = 0u64;
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        let entries = match std::fs::read_dir(&directory) {
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

/// Expansion workspaces sitting directly in the system temp root, i.e. outside
/// every quarantine this harness created. Polled while the boundary runs.
fn stray_expansion_bytes() -> u64 {
    let mut total = 0u64;
    let entries = match std::fs::read_dir(std::env::temp_dir()) {
        Ok(entries) => entries,
        Err(_) => return 0,
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("archive-expansion-") {
            continue;
        }
        total = total.saturating_add(directory_bytes(&entry.path()).max(1));
    }
    total
}

/// Every file under this run's root that is neither in a `quarantine`
/// directory nor in an `exposure` directory - that is, every byte an archive
/// entry managed to put outside the OSL quarantine. TASK 5180a: a starved path
/// guard shows up here even though the boundary still refuses the archive
/// afterwards, because the bytes are already written by then.
fn files_outside_quarantine(root: &Path) -> Vec<(PathBuf, u64)> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => {
                    if name != "quarantine" && name != "exposure" {
                        pending.push(path);
                    }
                }
                Ok(_) => {
                    let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
                    found.push((path, size));
                }
                Err(_) => {}
            }
        }
    }
    found.sort();
    found
}

// ---------------------------------------------------------------------------
// One fixture through the real release door
// ---------------------------------------------------------------------------

struct RunOutcome {
    released_bytes: u64,
    reason: String,
    text: String,
    entries_seen: u32,
    entries_scanned: u32,
    workspace_in_quarantine: bool,
    exposure_bytes: u64,
    scan_calls: u32,
}

fn run_fixture(
    root: &Path,
    label: &str,
    fixture: &[u8],
    limits: ArchiveLimits,
    entry_delay: Duration,
    stray_peak: &Arc<AtomicU64>,
) -> RunOutcome {
    let quarantine_root = root.join(label).join("quarantine");
    let exposure_root = root.join(label).join("exposure");
    let quarantine =
        ProtectedDownloadQuarantine::with_roots(quarantine_root.clone(), exposure_root.clone())
            .expect("quarantine opens")
            .with_archive_limits(limits);
    let held = quarantine
        .admit_bytes(label, fixture)
        .expect("admit the fixture into quarantine");
    let scanner = Scanner::new(entry_delay);
    let outcome = quarantine.scan_and_release(held, &scanner, now_unix_seconds());
    let canonical_quarantine =
        std::fs::canonicalize(&quarantine_root).unwrap_or_else(|_| quarantine_root.clone());
    match outcome {
        ProtectedDownloadOutcome::Exposed(exposed) => {
            let archive = exposed.archive.clone();
            RunOutcome {
                released_bytes: exposed.bytes_exposed,
                reason: "released".to_owned(),
                text: String::new(),
                entries_seen: archive.as_ref().map(|a| a.entries_seen).unwrap_or(0),
                entries_scanned: archive.as_ref().map(|a| a.entries_scanned).unwrap_or(0),
                workspace_in_quarantine: archive
                    .as_ref()
                    .map(|a| a.workspace_root.starts_with(&canonical_quarantine))
                    .unwrap_or(false),
                exposure_bytes: directory_bytes(&exposure_root),
                scan_calls: scanner.calls(),
            }
        }
        ProtectedDownloadOutcome::Withheld(withheld) => RunOutcome {
            released_bytes: withheld.bytes_exposed,
            reason: withheld.reason.name().to_owned(),
            text: withheld.local_reason_text.clone(),
            entries_seen: 0,
            entries_scanned: 0,
            workspace_in_quarantine: true,
            exposure_bytes: directory_bytes(&exposure_root),
            scan_calls: scanner.calls(),
        },
    }
    .measured(stray_peak)
}

impl RunOutcome {
    /// Fold in whatever the watcher saw while this fixture was running.
    fn measured(self, stray_peak: &Arc<AtomicU64>) -> Self {
        stray_peak.fetch_max(stray_expansion_bytes(), Ordering::AcqRel);
        self
    }
}

fn limits() -> ArchiveLimits {
    ArchiveLimits::shipping()
        .with_max_expanded_bytes(MAX_EXPANDED_BYTES)
        .with_max_entries(MAX_ENTRIES)
        .with_max_depth(MAX_DEPTH)
        .with_max_scan_time(Duration::from_secs(30))
}

fn main() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "osl-task5180b-run-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("harness root");
    println!("TASK5180B_ROOT={}", root.display());

    // An independent observer of the quarantine boundary: it polls the system
    // temp root for expansion workspaces while the boundary is expanding, so a
    // workspace opened outside the quarantine is seen even though the boundary
    // destroys it on the way out.
    let stray_peak = Arc::new(AtomicU64::new(0));
    let watching = Arc::new(AtomicBool::new(true));
    let watcher = {
        let stray_peak = Arc::clone(&stray_peak);
        let watching = Arc::clone(&watching);
        std::thread::spawn(move || {
            while watching.load(Ordering::Acquire) {
                stray_peak.fetch_max(stray_expansion_bytes(), Ordering::AcqRel);
                std::thread::sleep(Duration::from_micros(50));
            }
        })
    };

    let mut escapes: Vec<String> = Vec::new();

    // --- the clean control -------------------------------------------------
    let clean = clean_nested_zip();
    let control = run_fixture(
        &root,
        "bundle.zip",
        &clean,
        limits(),
        Duration::ZERO,
        &stray_peak,
    );
    println!(
        "TASK5180B_CLEAN fixture=bundle.zip outcome={} released_bytes={} entries_seen={} \
         entries_scanned={} scan_calls={} workspace_in_quarantine={}",
        control.reason,
        control.released_bytes,
        control.entries_seen,
        control.entries_scanned,
        control.scan_calls,
        control.workspace_in_quarantine
    );
    if control.reason != "released" {
        escapes.push(format!(
            "bundle.zip the clean control did not release: {} ({})",
            control.reason, control.text
        ));
    } else {
        if control.entries_seen != 6 || control.entries_scanned != control.entries_seen {
            escapes.push(format!(
                "bundle.zip was released without every entry scanning: entries_seen={} \
                 entries_scanned={}",
                control.entries_seen, control.entries_scanned
            ));
        }
        if control.scan_calls != 1 + control.entries_seen {
            escapes.push(format!(
                "bundle.zip released on {} local scans, not one whole-file receipt plus one \
                 per entry",
                control.scan_calls
            ));
        }
        if !control.workspace_in_quarantine {
            escapes.push(
                "bundle.zip was expanded in a workspace outside the OSL quarantine".to_owned(),
            );
        }
        if control.released_bytes != clean.len() as u64 {
            escapes.push(format!(
                "bundle.zip released {} bytes, not the {} byte archive",
                control.released_bytes,
                clean.len()
            ));
        }
    }

    // --- the four bounds ---------------------------------------------------
    let bounds: Vec<(&str, Vec<u8>, ArchiveLimits, Duration, &str, &str)> = vec![
        (
            "bomb.zip",
            over_byte_zip(),
            limits(),
            Duration::ZERO,
            "archive_expanded_bytes",
            "limit=expanded-bytes",
        ),
        (
            "swarm.zip",
            over_entry_zip(12),
            limits(),
            Duration::ZERO,
            "archive_entry_count",
            "limit=entry-count",
        ),
        (
            "deep.zip",
            over_depth_zip(),
            limits(),
            Duration::ZERO,
            "archive_nesting_depth",
            "limit=nesting-depth",
        ),
        (
            "slow.zip",
            slow_zip(6),
            limits().with_max_scan_time(Duration::from_millis(40)),
            Duration::from_millis(30),
            "archive_scan_time",
            "limit=scan-time",
        ),
    ];
    for (name, fixture, fixture_limits, delay, expected_reason, expected_limit) in bounds {
        let outcome = run_fixture(
            &root,
            name,
            &fixture,
            fixture_limits,
            delay,
            &stray_peak,
        );
        println!(
            "TASK5180B_BOUND fixture={name} outcome={} released_bytes={} exposure_bytes={} \
             text={}",
            outcome.reason, outcome.released_bytes, outcome.exposure_bytes, outcome.text
        );
        if outcome.reason == "released" {
            escapes.push(format!(
                "{name} escaped the {expected_limit} bound and was released ({} bytes, {} \
                 entries unpacked)",
                outcome.released_bytes, outcome.entries_seen
            ));
            continue;
        }
        if outcome.reason != expected_reason {
            escapes.push(format!(
                "{name} was refused as '{}' instead of naming the {expected_limit} bound",
                outcome.reason
            ));
        }
        if !outcome.text.contains(expected_limit) {
            escapes.push(format!(
                "{name} was refused without naming {expected_limit}: {}",
                outcome.text
            ));
        }
        if outcome.released_bytes != 0 || outcome.exposure_bytes != 0 {
            escapes.push(format!(
                "{name} released {} bytes and left {} bytes outside quarantine",
                outcome.released_bytes, outcome.exposure_bytes
            ));
        }
    }

    // --- TASK 5180a: the hostile entries -----------------------------------
    let baseline = files_outside_quarantine(&root);
    println!("TASK5180B_OUTSIDE_QUARANTINE_BASELINE={}", baseline.len());
    for (path, size) in &baseline {
        escapes.push(format!(
            "bundle.zip left {size} bytes outside the OSL quarantine at {}",
            path.display()
        ));
    }
    let hostiles: Vec<(&str, Vec<u8>, &str, &str)> = vec![
        (
            "traversal.zip",
            traversal_zip(),
            "archive_parent_traversal",
            "limit=parent-traversal",
        ),
        (
            "absolute.tar",
            absolute_tar(&root),
            "archive_absolute_path",
            "limit=absolute-path",
        ),
        (
            "symlink.tar",
            symlink_tar(),
            "archive_symbolic_link",
            "limit=symbolic-link",
        ),
        (
            "hardlink.tar",
            hardlink_tar(),
            "archive_hard_link",
            "limit=hard-link",
        ),
        (
            "special.tar",
            special_tar(),
            "archive_special_file",
            "limit=special-file",
        ),
    ];
    for (name, fixture, expected_reason, expected_limit) in hostiles {
        let outcome = run_fixture(&root, name, &fixture, limits(), Duration::ZERO, &stray_peak);
        // Measured from the filesystem, not from what the boundary said about
        // itself: a guard that rejects *after* writing still leaves these.
        let outside = files_outside_quarantine(&root);
        println!(
            "TASK5180B_HOSTILE fixture={name} outcome={} released_bytes={} \
             outside_quarantine_files={} text={}",
            outcome.reason,
            outcome.released_bytes,
            outside.len(),
            outcome.text
        );
        for (path, size) in &outside {
            escapes.push(format!(
                "{name} wrote {size} bytes outside the OSL quarantine at {}",
                path.display()
            ));
            // Cleared so the next fixture is measured on its own.
            let _ = std::fs::remove_file(path);
        }
        if outcome.reason == "released" {
            escapes.push(format!(
                "{name} escaped the {expected_limit} guard and was released ({} bytes, {} \
                 entries unpacked)",
                outcome.released_bytes, outcome.entries_seen
            ));
            continue;
        }
        if outcome.reason != expected_reason {
            escapes.push(format!(
                "{name} was refused as '{}' instead of naming the {expected_limit} guard",
                outcome.reason
            ));
        }
        if !outcome.text.contains(expected_limit) {
            escapes.push(format!(
                "{name} was refused without naming {expected_limit}: {}",
                outcome.text
            ));
        }
        if !outcome.text.contains("archive entry '") {
            escapes.push(format!(
                "{name} was refused without naming the rejected entry: {}",
                outcome.text
            ));
        }
        if outcome.released_bytes != 0 || outcome.exposure_bytes != 0 {
            escapes.push(format!(
                "{name} released {} bytes and left {} bytes outside quarantine",
                outcome.released_bytes, outcome.exposure_bytes
            ));
        }
    }

    watching.store(false, Ordering::Release);
    let _ = watcher.join();
    let stray = stray_peak.load(Ordering::Acquire);
    println!("TASK5180B_STRAY_EXPANSION_BYTES_OUTSIDE_QUARANTINE={stray}");
    if stray != 0 {
        escapes.push(format!(
            "bundle.zip was expanded outside the OSL quarantine ({stray} bytes seen in {})",
            std::env::temp_dir().display()
        ));
    }

    let _ = std::fs::remove_dir_all(&root);

    if escapes.is_empty() {
        println!(
            "TASK5180B_VERDICT=PASS the clean control released after all 6 entry receipts, every \
             bound fixture was withheld, and all 5 hostile-entry fixtures were refused by name \
             with 0 bytes outside quarantine"
        );
        println!("TASK5180B_EXIT=0");
        std::process::exit(0);
    }
    for escape in &escapes {
        println!("TASK5180B_ESCAPED_FIXTURE={escape}");
    }
    println!("TASK5180B_ESCAPE_COUNT={}", escapes.len());
    println!("TASK5180B_EXIT=1");
    std::process::exit(1);
}
