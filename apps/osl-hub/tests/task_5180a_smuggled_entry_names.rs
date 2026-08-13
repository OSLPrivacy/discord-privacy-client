//! TASK 5180a (lane i) - a hostile entry name smuggled past the 100-byte ustar
//! name field is still refused.
//!
//! `apps/osl-hub/tests/task_5180a_hostile_archive_entries.rs` puts the hostile
//! name in the plain ustar header, where it is easy to see. Tar has two other
//! channels that carry an entry's real name, and both of them are how a real
//! attacker gets a long or awkward path into an archive:
//!
//! * a **GNU long name** - a `L` entry called `././@LongLink` whose *payload*
//!   is the next entry's true path, and
//! * a **pax extended header** - an `x` entry whose payload is a
//!   `len path=...` record that overrides the next entry's path.
//!
//! In both cases the ustar name field of the entry that follows is short and
//! entirely innocent, so a guard that only looked at the header name would let
//! the traversal through. This check drives both through the same shipping door
//! the rest of TASK 5180a uses - sealed, opened only into the TASK 5166
//! quarantine, then `deliver_after_clean_scan` - and requires a refusal with
//! zero bytes outside quarantine, exactly as for the plain fixtures.
//!
//! A third case covers a `..` that sits in the *middle* of an otherwise
//! ordinary-looking path (`notes/../../escape.txt`), which a guard that only
//! inspected the first component would miss.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use osl_privacy_hub::download_zone_handoff::{
    AttachmentServicesSaver, PlatformLimitation, ZoneHandoffFailure, ZoneHandoffOutcome,
    ZoneHandoffRequest, ZoneLimitation,
};
use osl_privacy_hub::protected_archive::ArchiveLimits;
use osl_privacy_hub::protected_download_final_save::{deliver_after_clean_scan, FinalSaveOutcome};
use osl_privacy_hub::protected_download_quarantine::{
    now_unix_seconds, sha256_hex, AmsiFailure, AmsiProvider, AmsiReport, AmsiSubmission,
    ProtectedDownloadQuarantine, QuarantinedDownload, AMSI_RESULT_NOT_DETECTED,
};

// ---------------------------------------------------------------------------
// Throwaway root
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
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!(
        "osl-task5180a-smuggled-{label}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).expect("throwaway root");
    TempRoot(path)
}

fn directory_bytes(root: &Path) -> u64 {
    let mut total = 0u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => pending.push(entry.path()),
                Ok(_) => {
                    total = total.saturating_add(entry.metadata().map(|m| m.len()).unwrap_or(0))
                }
                Err(_) => {}
            }
        }
    }
    total
}

// ---------------------------------------------------------------------------
// Hand-built tar - ustar, GNU long name and pax extended header
// ---------------------------------------------------------------------------

const TAR_REGULAR: u8 = b'0';
const TAR_GNU_LONG_NAME: u8 = b'L';
const TAR_PAX_HEADER: u8 = b'x';

fn put(field: &mut [u8], value: &[u8]) {
    let taken = value.len().min(field.len());
    field[..taken].copy_from_slice(&value[..taken]);
}

/// One 512-byte ustar header plus its padded payload. Nothing about the name or
/// the type flag is validated here: that is the point.
fn tar_entry(name: &str, type_flag: u8, data: &[u8]) -> Vec<u8> {
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
    put(&mut header[257..263], b"ustar\0");
    put(&mut header[263..265], b"00");
    put(&mut header[265..297], b"osl\0");
    put(&mut header[297..329], b"osl\0");
    let checksum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
    put(&mut header[148..156], format!("{checksum:06o}\0 ").as_bytes());

    let mut out = header.to_vec();
    out.extend_from_slice(data);
    let padding = (512 - data.len() % 512) % 512;
    out.extend(std::iter::repeat_n(0u8, padding));
    out
}

fn tar_archive(entries: Vec<Vec<u8>>) -> Vec<u8> {
    let mut out: Vec<u8> = entries.into_iter().flatten().collect();
    out.extend(std::iter::repeat_n(0u8, 1024));
    out
}

/// A tar whose hostile path rides in a GNU `L` long-name entry. The regular
/// entry that follows carries the innocent name `harmless.txt`.
fn gnu_long_name_tar(hostile: &str) -> Vec<u8> {
    let mut payload = hostile.as_bytes().to_vec();
    payload.push(0);
    tar_archive(vec![
        tar_entry("././@LongLink", TAR_GNU_LONG_NAME, &payload),
        tar_entry("harmless.txt", TAR_REGULAR, b"TASK 5180a GNU long-name escape\n"),
    ])
}

/// A tar whose hostile path rides in a pax `x` extended header record. The pax
/// record format is `<len> path=<value>\n`, where `<len>` counts itself.
fn pax_header_tar(hostile: &str) -> Vec<u8> {
    let body = format!(" path={hostile}\n");
    let mut len = body.len() + 1;
    // The length prefix counts its own digits, so it has to settle.
    loop {
        let settled = body.len() + len.to_string().len();
        if settled == len {
            break;
        }
        len = settled;
    }
    let record = format!("{len}{body}");
    tar_archive(vec![
        tar_entry("PaxHeader/harmless.txt", TAR_PAX_HEADER, record.as_bytes()),
        tar_entry("harmless.txt", TAR_REGULAR, b"TASK 5180a pax escape\n"),
    ])
}

/// An ordinary ustar entry whose `..` sits in the middle of the path.
fn mid_path_traversal_tar(hostile: &str) -> Vec<u8> {
    tar_archive(vec![tar_entry(
        hostile,
        TAR_REGULAR,
        b"TASK 5180a mid-path traversal escape\n",
    )])
}

// ---------------------------------------------------------------------------
// Bench: the shipping door
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

    /// Everything in this bench that is neither the quarantine nor the sender's
    /// own sealed staging copy - where an escaped entry would land.
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

fn bench(label: &str) -> Bench {
    let root = temp_root(label);
    let quarantine_root = root.path().join("quarantine");
    let exposure_root = root.path().join("exposure");
    let sealed_root = root.path().join("sealed");
    let destination = root.path().join("downloads").join("delivered.bin");
    std::fs::create_dir_all(&sealed_root).expect("sealed root");
    let limits = ArchiveLimits::shipping()
        .with_max_expanded_bytes(1024 * 1024)
        .with_max_entries(8)
        .with_max_depth(2)
        .with_max_scan_time(Duration::from_secs(30));
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

/// Seal the fixture, open it *only* into quarantine, and adopt it - the same
/// admission path the rest of TASK 5180a uses.
fn admit_sealed(bench: &Bench, filename: &str, bytes: &[u8]) -> QuarantinedDownload {
    let plain_path = bench.sealed_root.join(format!("plain-{filename}"));
    std::fs::write(&plain_path, bytes).expect("write fixture plaintext");
    let mut source = std::fs::File::open(&plain_path).expect("open fixture plaintext");
    let staged = osl_privacy_hub::peer_attachment_io::encrypt_file(
        &bench.sealed_root,
        &mut source,
        filename,
        "application/x-tar",
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
        "application/x-tar",
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

struct CleanScanner {
    calls: AtomicU32,
}

impl CleanScanner {
    fn new() -> Self {
        Self {
            calls: AtomicU32::new(0),
        }
    }

    fn calls(&self) -> u32 {
        self.calls.load(Ordering::Acquire)
    }
}

impl AmsiProvider for CleanScanner {
    fn scan(&self, submission: &AmsiSubmission<'_>) -> Result<AmsiReport, AmsiFailure> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        Ok(AmsiReport {
            result_code: AMSI_RESULT_NOT_DETECTED,
            provider_identity: "OSL TASK 5180a smuggled-name fixture scanner".to_owned(),
            engine_version: "1.1.0.0".to_owned(),
            signature_version: "1.457.130.0".to_owned(),
            signature_updated_unix: now_unix_seconds(),
            scanned_sha256: sha256_hex(submission.plaintext),
            scanned_len: submission.plaintext.len() as u64,
        })
    }
}

struct CountingSaver {
    calls: AtomicU32,
}

impl CountingSaver {
    fn new() -> Self {
        Self {
            calls: AtomicU32::new(0),
        }
    }

    fn calls(&self) -> u32 {
        self.calls.load(Ordering::Acquire)
    }
}

impl AttachmentServicesSaver for CountingSaver {
    fn save(&self, request: &ZoneHandoffRequest) -> Result<ZoneHandoffOutcome, ZoneHandoffFailure> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        Ok(ZoneHandoffOutcome::PlatformLimited(PlatformLimitation {
            reason: ZoneLimitation::FilesystemCannotRetainMark,
            filesystem: "ext4".to_owned(),
            windows_path: request.local_path.display().to_string(),
            save_calls: 1,
            message: "TASK 5180a smuggled-name fixture saver".to_owned(),
        }))
    }
}

struct Probe {
    reason: String,
    text: String,
    released_bytes: u64,
    saver_calls: u32,
    scan_calls: u32,
    outside_bytes: u64,
    escaped: Vec<String>,
}

/// Build one hostile tar, run it through `deliver_after_clean_scan`, and
/// measure what it managed to do. `build` is handed the bench root so the
/// fixture can aim at a path inside this test's own throwaway root.
fn run_probe<F>(label: &str, filename: &str, build: F) -> Probe
where
    F: Fn(&Path) -> (Vec<u8>, Vec<PathBuf>),
{
    let bench = bench(label);
    let (fixture, escape_targets) = build(bench.root());
    let held = admit_sealed(&bench, filename, &fixture);
    let scanner = CleanScanner::new();
    let saver = CountingSaver::new();

    let outcome = deliver_after_clean_scan(
        &bench.quarantine,
        held,
        &scanner,
        &saver,
        &bench.destination,
        "https://osl.invalid/task-5180a-smuggled",
        "https://osl.invalid/",
        now_unix_seconds(),
    );
    let withheld = match outcome {
        FinalSaveOutcome::Withheld(withheld) => withheld,
        other => panic!("{label} must be withheld, got {}", other.name()),
    };

    let escaped: Vec<String> = escape_targets
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

    let outside = bench
        .bytes_outside_quarantine()
        .max(directory_bytes(&bench.exposure_root))
        .max(
            std::fs::metadata(&bench.destination)
                .map(|meta| meta.len())
                .unwrap_or(0),
        );

    Probe {
        reason: withheld.reason.name().to_owned(),
        text: withheld.local_reason_text.clone(),
        released_bytes: withheld.bytes_exposed,
        saver_calls: saver.calls(),
        scan_calls: scanner.calls(),
        outside_bytes: outside,
        escaped,
    }
}

fn report(tag: &str, probe: &Probe, hostile_name: &str) {
    println!(
        "{tag}_REASON={} OUTSIDE_BYTES={} RELEASED_BYTES={} SAVER_CALLS={} SCAN_CALLS={} ESCAPED_PATHS={} TEXT={}",
        probe.reason,
        probe.outside_bytes,
        probe.released_bytes,
        probe.saver_calls,
        probe.scan_calls,
        if probe.escaped.is_empty() {
            "none".to_owned()
        } else {
            probe.escaped.join(",")
        },
        probe.text,
    );
    assert_eq!(probe.outside_bytes, 0, "{tag} wrote bytes outside quarantine");
    assert!(
        probe.escaped.is_empty(),
        "{tag} materialised the entry it aimed at: {:?}",
        probe.escaped
    );
    assert_eq!(probe.released_bytes, 0, "{tag} released bytes to the caller");
    assert_eq!(probe.saver_calls, 0, "{tag} reached the final save");
    assert!(
        probe.text.contains(hostile_name),
        "{tag} did not name the rejected entry '{hostile_name}': {}",
        probe.text
    );
    assert!(
        probe.text.contains("before any of its bytes were unpacked"),
        "{tag} did not refuse before the write: {}",
        probe.text
    );
}

#[test]
fn task_5180a_a_traversal_smuggled_in_a_gnu_long_name_is_refused() {
    let hostile = "../../osl-5180a-gnu-longname-escape.txt";
    let probe = run_probe("gnu-longname", "longname.tar", |root| {
        (
            gnu_long_name_tar(hostile),
            vec![root.join("osl-5180a-gnu-longname-escape.txt")],
        )
    });
    report("TASK5180A_GNU_LONGNAME", &probe, hostile);
    assert_eq!(probe.reason, "archive_parent_traversal");
}

#[test]
fn task_5180a_an_absolute_path_smuggled_in_a_gnu_long_name_is_refused() {
    let probe = run_probe("gnu-longname-abs", "longname-abs.tar", |root| {
        let hostile = root
            .join("osl-5180a-gnu-longname-absolute.txt")
            .display()
            .to_string();
        (
            gnu_long_name_tar(&hostile),
            vec![root.join("osl-5180a-gnu-longname-absolute.txt")],
        )
    });
    println!(
        "TASK5180A_GNU_LONGNAME_ABS_REASON={} OUTSIDE_BYTES={} RELEASED_BYTES={} SAVER_CALLS={} ESCAPED_PATHS={} TEXT={}",
        probe.reason,
        probe.outside_bytes,
        probe.released_bytes,
        probe.saver_calls,
        if probe.escaped.is_empty() {
            "none".to_owned()
        } else {
            probe.escaped.join(",")
        },
        probe.text,
    );
    assert_eq!(probe.outside_bytes, 0);
    assert!(probe.escaped.is_empty());
    assert_eq!(probe.released_bytes, 0);
    assert_eq!(probe.saver_calls, 0);
    assert_eq!(probe.reason, "archive_absolute_path");
}

#[test]
fn task_5180a_a_traversal_smuggled_in_a_pax_header_is_refused() {
    let hostile = "../../osl-5180a-pax-escape.txt";
    let probe = run_probe("pax", "pax.tar", |root| {
        (
            pax_header_tar(hostile),
            vec![root.join("osl-5180a-pax-escape.txt")],
        )
    });
    println!(
        "TASK5180A_PAX_REASON={} OUTSIDE_BYTES={} RELEASED_BYTES={} SAVER_CALLS={} SCAN_CALLS={} ESCAPED_PATHS={} TEXT={}",
        probe.reason,
        probe.outside_bytes,
        probe.released_bytes,
        probe.saver_calls,
        probe.scan_calls,
        if probe.escaped.is_empty() {
            "none".to_owned()
        } else {
            probe.escaped.join(",")
        },
        probe.text,
    );
    assert_eq!(probe.outside_bytes, 0, "the pax fixture wrote outside quarantine");
    assert!(probe.escaped.is_empty(), "the pax fixture materialised {:?}", probe.escaped);
    assert_eq!(probe.released_bytes, 0);
    assert_eq!(probe.saver_calls, 0);
    // Either the pax path override reaches the name guard, or the extended
    // header entry itself is refused as an unrecognised (special) entry type.
    // Both are fail-closed; the print above records which one fired.
    assert!(
        probe.reason == "archive_parent_traversal" || probe.reason == "archive_special_file",
        "the pax fixture must be refused by a TASK 5180a guard, got {}",
        probe.reason
    );
}

#[test]
fn task_5180a_a_traversal_in_the_middle_of_a_path_is_refused() {
    let hostile = "notes/deep/../../../osl-5180a-mid-path-escape.txt";
    let probe = run_probe("mid-path", "midpath.tar", |root| {
        (
            mid_path_traversal_tar(hostile),
            vec![root.join("osl-5180a-mid-path-escape.txt")],
        )
    });
    report("TASK5180A_MID_PATH", &probe, hostile);
    assert_eq!(probe.reason, "archive_parent_traversal");
}
