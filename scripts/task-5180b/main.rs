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
use std::path::Path;
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

fn tar_bytes(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (name, bytes) in entries {
        let mut header = tar::Header::new_ustar();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        builder
            .append_data(&mut header, name.as_str(), &bytes[..])
            .expect("tar entry");
    }
    builder.into_inner().expect("tar finish")
}

fn gzip_bytes(payload: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(payload).expect("gzip write");
    encoder.finish().expect("gzip finish")
}

fn clean_nested_zip() -> Vec<u8> {
    let inner_tar = tar_bytes(&[
        (
            "beta.txt".to_owned(),
            b"TASK 5180 inner tar entry beta.\n".to_vec(),
        ),
        ("deep/gamma.bin".to_owned(), vec![0x2au8; 96]),
    ]);
    let delta_gz = gzip_bytes(b"TASK 5180 gzip member delta.\n");
    zip_bytes(&[
        (
            "notes/alpha.txt".to_owned(),
            b"TASK 5180 clean nested archive, entry alpha.\n".to_vec(),
        ),
        ("inner.tar".to_owned(), inner_tar),
        ("delta.txt.gz".to_owned(), delta_gz),
    ])
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
            "TASK5180B_VERDICT=PASS the clean control released after all 6 entry receipts and \
             every bound fixture was withheld with 0 bytes outside quarantine"
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
