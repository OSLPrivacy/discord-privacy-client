//! TASK 5166 - quarantine and scan every protected download before exposure.
//!
//! These checks drive the real local Windows AMSI provider (`amsi.dll`
//! `AmsiScanBuffer`, Microsoft Defender Antivirus) from WSL through
//! `powershell.exe` in the interactive Windows session, exactly as the shipping
//! `native_attachment_transport` open path does. The clean verdict, the EICAR
//! detection, the stale-signature refusal, the provider-absence refusal and the
//! provider-timeout refusal are all produced by that real provider. Only the
//! provider-error and lying-scanner cases use a stub, because a real Defender
//! install will not fabricate a malformed verdict on demand.
//!
//! Quarantine lives on the Linux side (`/tmp`, ext4) so no EICAR plaintext is
//! ever written to a Defender-scanned Windows volume; the bytes reach AMSI as
//! base64 on the scanner host's stdin and are decoded in its memory.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use osl_privacy_hub::protected_download_quarantine::{
    parse_amsi_helper_output, now_unix_seconds, sha256_hex, AmsiFailure, AmsiProvider, AmsiReport,
    AmsiSubmission, ProtectedDownloadOutcome, ProtectedDownloadQuarantine, QuarantineReason,
    WindowsAmsiProvider, AMSI_HELPER_SCRIPT, AMSI_RESULT_DETECTED, AMSI_RESULT_NOT_DETECTED,
};

const CLEAN_FIXTURE: &[u8] =
    b"TASK 5166 clean protected download fixture. Ordinary text, nothing executable.\n";

/// The EICAR anti-malware test string, XOR-masked with 0x5A so the literal
/// signature is never a contiguous byte run inside this test binary. The binary
/// is built under `/mnt/d`, a Defender-scanned Windows volume, and a plain
/// literal would get the compiled artifact quarantined mid-build.
const EICAR_XOR_5A: [u8; 68] = [
    0x02, 0x6f, 0x15, 0x7b, 0x0a, 0x7f, 0x1a, 0x1b, 0x0a, 0x01, 0x6e, 0x06, 0x0a, 0x00, 0x02, 0x6f,
    0x6e, 0x72, 0x0a, 0x04, 0x73, 0x6d, 0x19, 0x19, 0x73, 0x6d, 0x27, 0x7e, 0x1f, 0x13, 0x19, 0x1b,
    0x08, 0x77, 0x09, 0x0e, 0x1b, 0x14, 0x1e, 0x1b, 0x08, 0x1e, 0x77, 0x1b, 0x14, 0x0e, 0x13, 0x0c,
    0x13, 0x08, 0x0f, 0x09, 0x77, 0x0e, 0x1f, 0x09, 0x0e, 0x77, 0x1c, 0x13, 0x16, 0x1f, 0x7b, 0x7e,
    0x12, 0x71, 0x12, 0x70,
];

fn eicar_bytes() -> Vec<u8> {
    EICAR_XOR_5A.iter().map(|byte| byte ^ 0x5A).collect()
}

/// Removes its directory when the test ends, so no fixture - least of all the
/// EICAR one - is left lying on the machine.
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
        "osl-task5166-{label}-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temp root");
    TempRoot(root)
}

fn quarantine(label: &str) -> (TempRoot, ProtectedDownloadQuarantine) {
    let root = temp_root(label);
    let boundary =
        ProtectedDownloadQuarantine::for_app_root(root.path()).expect("quarantine opens");
    (root, boundary)
}

/// Wraps any provider and counts how many times the boundary actually reached
/// it. The count is kept by the *scanner*, not by the boundary, so a boundary
/// that returns clean without calling out cannot inflate it.
struct CountingProvider<'a> {
    inner: &'a dyn AmsiProvider,
    invocations: AtomicU32,
}

impl<'a> CountingProvider<'a> {
    fn new(inner: &'a dyn AmsiProvider) -> Self {
        Self {
            inner,
            invocations: AtomicU32::new(0),
        }
    }

    fn count(&self) -> u32 {
        self.invocations.load(Ordering::Acquire)
    }
}

impl AmsiProvider for CountingProvider<'_> {
    fn scan(&self, submission: &AmsiSubmission<'_>) -> Result<AmsiReport, AmsiFailure> {
        self.invocations.fetch_add(1, Ordering::AcqRel);
        self.inner.scan(submission)
    }
}

struct FailingProvider(AmsiFailure);

impl AmsiProvider for FailingProvider {
    fn scan(&self, _submission: &AmsiSubmission<'_>) -> Result<AmsiReport, AmsiFailure> {
        Err(self.0.clone())
    }
}

/// A scanner that returns a clean verdict about bytes it was never given.
struct LyingProvider;

impl AmsiProvider for LyingProvider {
    fn scan(&self, _submission: &AmsiSubmission<'_>) -> Result<AmsiReport, AmsiFailure> {
        Ok(AmsiReport {
            result_code: AMSI_RESULT_NOT_DETECTED,
            provider_identity: "Stub".to_owned(),
            engine_version: "0".to_owned(),
            signature_version: "0.0.0.0".to_owned(),
            signature_updated_unix: now_unix_seconds(),
            scanned_sha256: sha256_hex(b"some other file entirely"),
            scanned_len: 24,
        })
    }
}

fn real_provider() -> WindowsAmsiProvider {
    let provider = WindowsAmsiProvider::new();
    assert!(
        provider.powershell_path().exists(),
        "TASK 5166 requires the interactive Windows session's scanner host at {}",
        provider.powershell_path().display()
    );
    provider
}

#[cfg(unix)]
fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).expect("metadata").permissions().mode() & 0o777
}

#[cfg(unix)]
fn inode_of(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).expect("metadata").ino()
}

/// Drop whole-line comments so a doc comment naming a forbidden verb is not
/// mistaken for a call to it.
fn strip_comments(source: &str, marker: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with(marker))
        .collect::<Vec<_>>()
        .join("\n")
}

fn file_names(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

// ---------------------------------------------------------------------------

#[test]
fn task_5166_clean_fixture_moves_atomically_out_of_quarantine_after_one_matching_amsi_call() {
    let (_root, boundary) = quarantine("clean");
    let held = boundary
        .admit_bytes("task-5166-clean", CLEAN_FIXTURE)
        .expect("clean fixture is admitted to quarantine");

    // The plaintext exists only inside the access-controlled quarantine.
    println!(
        "TASK5166_CLEAN_QUARANTINE_DIR_MODE={:o}",
        mode_of(boundary.quarantine_root())
    );
    println!(
        "TASK5166_CLEAN_QUARANTINE_FILE_MODE={:o}",
        mode_of(held.path())
    );
    println!(
        "TASK5166_CLEAN_EXPOSED_BYTES_BEFORE_SCAN={}",
        boundary.exposed_bytes()
    );
    assert_eq!(mode_of(boundary.quarantine_root()), 0o700);
    assert_eq!(mode_of(held.path()), 0o600);
    assert_eq!(boundary.exposed_bytes(), 0);
    let quarantine_inode = inode_of(held.path());

    let provider = real_provider();
    let counting = CountingProvider::new(&provider);
    let scan = boundary
        .scan(&held, &counting, now_unix_seconds())
        .expect("the local AMSI provider clears the clean fixture");

    println!("TASK5166_CLEAN_AMSI_INVOCATIONS_OBSERVED_BY_SCANNER={}", counting.count());
    println!("TASK5166_CLEAN_AMSI_INVOCATIONS_BOUND={}", scan.amsi_invocations);
    println!("TASK5166_CLEAN_AMSI_RESULT={}", scan.amsi_result_code);
    println!("TASK5166_CLEAN_PROVIDER={}", scan.provider_identity);
    println!("TASK5166_CLEAN_ENGINE_VERSION={}", scan.engine_version);
    println!("TASK5166_CLEAN_SIGNATURE_VERSION={}", scan.signature_version);
    println!(
        "TASK5166_CLEAN_SIGNATURE_UPDATED_UNIX={}",
        scan.signature_updated_unix
    );
    println!("TASK5166_CLEAN_SCANNED_AT_UNIX={}", scan.scanned_at_unix);
    println!("TASK5166_CLEAN_CONTENT_SHA256={}", scan.content_sha256);
    println!("TASK5166_CLEAN_CONTENT_LEN={}", scan.content_len);

    assert_eq!(counting.count(), 1, "exactly one AMSI call stands behind the release");
    assert_eq!(scan.amsi_invocations, 1);
    assert_eq!(scan.amsi_result_code, AMSI_RESULT_NOT_DETECTED);
    assert_eq!(scan.content_sha256, sha256_hex(CLEAN_FIXTURE));
    assert_eq!(scan.content_len, CLEAN_FIXTURE.len() as u64);
    assert!(scan.provider_identity.contains("Defender"));
    assert!(!scan.signature_version.is_empty());
    assert!(scan.signature_updated_unix > 0);

    let exposed = boundary.release(held, &scan).expect("clean release");
    println!("TASK5166_CLEAN_EXPOSED_BYTES_AFTER_RELEASE={}", exposed.bytes_exposed);
    println!("TASK5166_CLEAN_QUARANTINED_BYTES_AFTER_RELEASE={}", boundary.quarantined_bytes());
    println!(
        "TASK5166_CLEAN_EXPOSURE_DIR_ENTRIES={}",
        file_names(boundary.exposure_root()).join(",")
    );
    println!(
        "TASK5166_CLEAN_MOVE_WAS_RENAME_SAME_INODE={}",
        inode_of(&exposed.path) == quarantine_inode
    );

    assert_eq!(exposed.bytes_exposed, CLEAN_FIXTURE.len() as u64);
    assert_eq!(boundary.quarantined_bytes(), 0);
    assert_eq!(std::fs::read(&exposed.path).expect("released bytes"), CLEAN_FIXTURE);
    // rename(2) keeps the inode; a copy-then-delete would not. One entry only,
    // so no partial `.part` file was ever visible outside quarantine.
    assert_eq!(inode_of(&exposed.path), quarantine_inode);
    assert_eq!(file_names(boundary.exposure_root()).len(), 1);
    assert_eq!(exposed.scan.amsi_invocations, 1);
}

#[test]
fn task_5166_eicar_exposes_zero_bytes_and_names_the_local_quarantine_reason() {
    let (_root, boundary) = quarantine("eicar");
    let eicar = eicar_bytes();
    let held = boundary
        .admit_bytes("task-5166-eicar", &eicar)
        .expect("EICAR fixture is admitted to quarantine");
    let quarantine_path = held.path().to_owned();

    let provider = real_provider();
    let counting = CountingProvider::new(&provider);
    let outcome = boundary.scan_and_release(held, &counting, now_unix_seconds());

    let withheld = match &outcome {
        ProtectedDownloadOutcome::Withheld(withheld) => withheld.clone(),
        ProtectedDownloadOutcome::Exposed(exposed) => {
            panic!("EICAR was exposed at {}", exposed.path.display())
        }
    };

    println!("TASK5166_EICAR_SHA256={}", sha256_hex(&eicar));
    println!("TASK5166_EICAR_LEN={}", eicar.len());
    println!("TASK5166_EICAR_AMSI_INVOCATIONS={}", counting.count());
    println!("TASK5166_EICAR_REASON={}", withheld.reason.name());
    println!("TASK5166_EICAR_EXPOSED_BYTES={}", withheld.bytes_exposed);
    println!(
        "TASK5166_EICAR_EXPOSURE_DIR_BYTES={}",
        boundary.exposed_bytes()
    );
    println!(
        "TASK5166_EICAR_STILL_IN_QUARANTINE={}",
        quarantine_path.exists()
    );
    println!("TASK5166_EICAR_LOCAL_REASON_TEXT={}", withheld.local_reason_text);

    assert_eq!(counting.count(), 1);
    assert_eq!(withheld.reason, QuarantineReason::MalwareDetected);
    assert_eq!(withheld.bytes_exposed, 0);
    assert_eq!(boundary.exposed_bytes(), 0);
    assert_eq!(outcome.bytes_exposed(), 0);
    assert!(quarantine_path.exists(), "the detection stays in quarantine");
    assert!(withheld.local_reason_text.contains("private quarantine"));
    assert!(withheld.local_reason_text.contains("malware_detected"));
    assert!(withheld.local_reason_text.contains("Defender"));
    assert!(withheld
        .local_reason_text
        .contains(&format!("AMSI_RESULT={AMSI_RESULT_DETECTED}")));
    assert!(withheld.local_reason_text.contains("Nothing was sent to a cloud scanner"));
}

#[test]
fn task_5166_one_byte_changed_after_the_scan_is_refused_as_a_hash_mismatch() {
    let (_root, boundary) = quarantine("swap");
    let held = boundary
        .admit_bytes("task-5166-swap", CLEAN_FIXTURE)
        .expect("clean fixture is admitted");
    let quarantine_path = held.path().to_owned();

    let provider = real_provider();
    let counting = CountingProvider::new(&provider);
    let scan = boundary
        .scan(&held, &counting, now_unix_seconds())
        .expect("the clean fixture scans clean");
    assert_eq!(counting.count(), 1);

    // One byte, after the clean verdict and before the move.
    let mut tampered = CLEAN_FIXTURE.to_vec();
    tampered[0] ^= 0x01;
    std::fs::write(&quarantine_path, &tampered).expect("swap one byte in quarantine");

    let withheld = boundary
        .release(held, &scan)
        .expect_err("a byte changed after the scan must not be released");

    println!("TASK5166_SWAP_SCANNED_SHA256={}", scan.content_sha256);
    println!("TASK5166_SWAP_BEFORE_MOVE_SHA256={}", sha256_hex(&tampered));
    println!("TASK5166_SWAP_CHANGED_BYTES={}", 1);
    println!("TASK5166_SWAP_REASON={}", withheld.reason.name());
    println!("TASK5166_SWAP_EXPOSED_BYTES={}", withheld.bytes_exposed);
    println!("TASK5166_SWAP_EXPOSURE_DIR_BYTES={}", boundary.exposed_bytes());
    println!("TASK5166_SWAP_LOCAL_REASON_TEXT={}", withheld.local_reason_text);

    assert_eq!(withheld.reason, QuarantineReason::ContentHashMismatch);
    assert_eq!(withheld.bytes_exposed, 0);
    assert_eq!(boundary.exposed_bytes(), 0);
    assert!(withheld.local_reason_text.contains("content_hash_mismatch"));
    assert!(withheld.local_reason_text.contains(&scan.content_sha256));
    assert!(quarantine_path.exists());
}

#[test]
fn task_5166_provider_absence_error_timeout_detection_and_stale_signatures_each_expose_zero_bytes()
{
    let now = now_unix_seconds();
    let mut rows: Vec<(&str, QuarantineReason, u64, u64)> = Vec::new();

    // 1. Provider absence - the real Windows provider pointed at a scanner host
    //    that is not on this machine.
    {
        let (_root, boundary) = quarantine("absent");
        let held = boundary.admit_bytes("absent", CLEAN_FIXTURE).expect("admit");
        let absent = WindowsAmsiProvider::new()
            .with_powershell(PathBuf::from("/mnt/c/Windows/System32/osl-no-such-scanner.exe"));
        let outcome = boundary.scan_and_release(held, &absent, now);
        let withheld = expect_withheld(&outcome);
        println!("TASK5166_ABSENT_REASON={}", withheld.reason.name());
        println!("TASK5166_ABSENT_EXPOSED_BYTES={}", withheld.bytes_exposed);
        println!("TASK5166_ABSENT_LOCAL_REASON_TEXT={}", withheld.local_reason_text);
        assert_eq!(withheld.reason, QuarantineReason::ProviderAbsent);
        rows.push((
            "provider_absent",
            withheld.reason,
            withheld.bytes_exposed,
            boundary.exposed_bytes(),
        ));
    }

    // 2. Provider error - a reachable provider that returns no usable verdict.
    {
        let (_root, boundary) = quarantine("error");
        let held = boundary.admit_bytes("error", CLEAN_FIXTURE).expect("admit");
        let failing = FailingProvider(AmsiFailure::ProviderError(
            "AmsiScanBuffer returned hr=0x80004005".to_owned(),
        ));
        let outcome = boundary.scan_and_release(held, &failing, now);
        let withheld = expect_withheld(&outcome);
        println!("TASK5166_ERROR_REASON={}", withheld.reason.name());
        println!("TASK5166_ERROR_EXPOSED_BYTES={}", withheld.bytes_exposed);
        assert_eq!(withheld.reason, QuarantineReason::ProviderError);
        rows.push((
            "provider_error",
            withheld.reason,
            withheld.bytes_exposed,
            boundary.exposed_bytes(),
        ));
    }

    // 3. Timeout - the real Windows provider with a budget it cannot meet.
    {
        let (_root, boundary) = quarantine("timeout");
        let held = boundary.admit_bytes("timeout", CLEAN_FIXTURE).expect("admit");
        let starved = real_provider().with_timeout(Duration::from_millis(1));
        let outcome = boundary.scan_and_release(held, &starved, now);
        let withheld = expect_withheld(&outcome);
        println!("TASK5166_TIMEOUT_REASON={}", withheld.reason.name());
        println!("TASK5166_TIMEOUT_EXPOSED_BYTES={}", withheld.bytes_exposed);
        println!("TASK5166_TIMEOUT_LOCAL_REASON_TEXT={}", withheld.local_reason_text);
        assert_eq!(withheld.reason, QuarantineReason::ProviderTimeout);
        rows.push((
            "provider_timeout",
            withheld.reason,
            withheld.bytes_exposed,
            boundary.exposed_bytes(),
        ));
    }

    // 4. Detection - the real provider on the real EICAR string.
    {
        let (_root, boundary) = quarantine("detected");
        let held = boundary.admit_bytes("detected", &eicar_bytes()).expect("admit");
        let outcome = boundary.scan_and_release(held, &real_provider(), now);
        let withheld = expect_withheld(&outcome);
        println!("TASK5166_DETECTED_REASON={}", withheld.reason.name());
        println!("TASK5166_DETECTED_EXPOSED_BYTES={}", withheld.bytes_exposed);
        assert_eq!(withheld.reason, QuarantineReason::MalwareDetected);
        rows.push((
            "malware_detected",
            withheld.reason,
            withheld.bytes_exposed,
            boundary.exposed_bytes(),
        ));
    }

    // 5. Stale signatures - the real provider's real signature timestamp, judged
    //    against a clock 30 days past it.
    {
        let root = temp_root("stale");
        let boundary = ProtectedDownloadQuarantine::for_app_root(root.path())
            .expect("quarantine opens")
            .with_max_signature_age_seconds(60);
        let held = boundary.admit_bytes("stale", CLEAN_FIXTURE).expect("admit");
        let stale_clock = now.saturating_add(30 * 24 * 60 * 60);
        let outcome = boundary.scan_and_release(held, &real_provider(), stale_clock);
        let withheld = expect_withheld(&outcome);
        println!("TASK5166_STALE_REASON={}", withheld.reason.name());
        println!("TASK5166_STALE_EXPOSED_BYTES={}", withheld.bytes_exposed);
        println!("TASK5166_STALE_LOCAL_REASON_TEXT={}", withheld.local_reason_text);
        assert_eq!(withheld.reason, QuarantineReason::StaleSignatures);
        rows.push((
            "stale_signatures",
            withheld.reason,
            withheld.bytes_exposed,
            boundary.exposed_bytes(),
        ));
    }

    // 6. A scanner that returns clean about bytes it was never given.
    {
        let (_root, boundary) = quarantine("lying");
        let held = boundary.admit_bytes("lying", CLEAN_FIXTURE).expect("admit");
        let outcome = boundary.scan_and_release(held, &LyingProvider, now);
        let withheld = expect_withheld(&outcome);
        println!("TASK5166_LYING_REASON={}", withheld.reason.name());
        println!("TASK5166_LYING_EXPOSED_BYTES={}", withheld.bytes_exposed);
        assert_eq!(withheld.reason, QuarantineReason::ScanBindingMismatch);
        rows.push((
            "scan_binding_mismatch",
            withheld.reason,
            withheld.bytes_exposed,
            boundary.exposed_bytes(),
        ));
    }

    for (label, reason, reported, on_disk) in &rows {
        println!("TASK5166_FAILCLOSED {label} reason={reason} exposed_bytes={reported} exposure_dir_bytes={on_disk}");
        assert_eq!(*reported, 0, "{label} must expose 0 bytes");
        assert_eq!(*on_disk, 0, "{label} must leave the exposure directory empty");
    }
    println!("TASK5166_FAILCLOSED_CASES={}", rows.len());
    assert_eq!(rows.len(), 6);
}

fn expect_withheld(
    outcome: &ProtectedDownloadOutcome,
) -> osl_privacy_hub::protected_download_quarantine::WithheldDownload {
    match outcome {
        ProtectedDownloadOutcome::Withheld(withheld) => withheld.clone(),
        ProtectedDownloadOutcome::Exposed(exposed) => {
            panic!("expected a refusal, got {} bytes at {}", exposed.bytes_exposed, exposed.path.display())
        }
    }
}

#[test]
fn task_5166_a_release_resting_on_zero_amsi_invocations_is_refused() {
    let (_root, boundary) = quarantine("starved");
    let held = boundary.admit_bytes("starved", CLEAN_FIXTURE).expect("admit");
    let provider = real_provider();
    let counting = CountingProvider::new(&provider);
    let mut scan = boundary
        .scan(&held, &counting, now_unix_seconds())
        .expect("clean scan");
    assert_eq!(counting.count(), 1);

    // The exact sabotage TASK 5166b performs: a clean verdict that no scanner
    // call stands behind.
    scan.amsi_invocations = 0;
    let withheld = boundary
        .release(held, &scan)
        .expect_err("a verdict with no scanner call behind it must not be released");

    println!("TASK5166_STARVED_REASON={}", withheld.reason.name());
    println!("TASK5166_STARVED_AMSI_INVOCATIONS={}", withheld.amsi_invocations);
    println!("TASK5166_STARVED_EXPOSED_BYTES={}", withheld.bytes_exposed);
    println!("TASK5166_STARVED_LOCAL_REASON_TEXT={}", withheld.local_reason_text);

    assert_eq!(withheld.reason, QuarantineReason::AmsiInvocationCount);
    assert_eq!(withheld.amsi_invocations, 0);
    assert_eq!(withheld.bytes_exposed, 0);
    assert_eq!(boundary.exposed_bytes(), 0);
    assert!(withheld.local_reason_text.contains("amsi_invocation_count"));
    assert!(withheld.local_reason_text.contains("invocation count 0"));
}

#[test]
fn task_5166_no_plaintext_or_sample_reaches_a_cloud_scanner() {
    // Static audit of both halves of the scan path: the embedded PowerShell
    // helper and this module's own Rust source. Neither may contain a network
    // client or a sample-submission verb.
    const BANNED: [&str; 14] = [
        "http://",
        "https://",
        "Invoke-WebRequest",
        "Invoke-RestMethod",
        "System.Net",
        "WebClient",
        "HttpClient",
        "Submit-MpThreat",
        "SpynetReporting",
        "MAPSReporting",
        "reqwest",
        "TcpStream",
        "UdpSocket",
        "SubmitSamples",
    ];
    let rust_source = include_str!("../src/protected_download_quarantine.rs");
    // Comments in both files name the verbs they forbid, so the audit runs over
    // executable lines only.
    let helper_code = strip_comments(AMSI_HELPER_SCRIPT, "#");
    let rust_code = strip_comments(rust_source, "//");
    println!("TASK5166_AUDITED_HELPER_CODE_LINES={}", helper_code.lines().count());
    println!("TASK5166_AUDITED_RUST_CODE_LINES={}", rust_code.lines().count());
    let mut hits: Vec<String> = Vec::new();
    for needle in BANNED {
        if helper_code.contains(needle) {
            hits.push(format!("helper:{needle}"));
        }
        if rust_code.contains(needle) {
            hits.push(format!("rust:{needle}"));
        }
    }
    println!("TASK5166_CLOUD_SUBMISSION_BANNED_TOKENS={}", BANNED.len());
    println!("TASK5166_CLOUD_SUBMISSION_HITS={}", hits.join(","));
    println!("TASK5166_CLOUD_SUBMISSION_HIT_COUNT={}", hits.len());
    assert!(hits.is_empty(), "cloud-submission surface found: {hits:?}");

    // The helper is embedded in the binary, not read from a file another
    // process could replace.
    println!(
        "TASK5166_HELPER_IS_EMBEDDED={}",
        AMSI_HELPER_SCRIPT.contains("AmsiScanBuffer")
    );
    assert!(AMSI_HELPER_SCRIPT.contains("AmsiScanBuffer"));
    assert!(AMSI_HELPER_SCRIPT.contains("amsi.dll"));
}

#[test]
fn task_5166_helper_absence_and_error_statuses_parse_as_unable_to_verify() {
    let absent = parse_amsi_helper_output(
        "OSL5166_STATUS=provider_absent\nOSL5166_DETAIL=AmsiInitialize returned hr=0x80070002\n",
    )
    .expect_err("absence is not a verdict");
    let error = parse_amsi_helper_output(
        "OSL5166_STATUS=provider_error\nOSL5166_DETAIL=AmsiScanBuffer returned hr=0x80004005\n",
    )
    .expect_err("an error is not a verdict");
    let silent = parse_amsi_helper_output("").expect_err("silence is not a verdict");
    println!("TASK5166_PARSE_ABSENT={absent}");
    println!("TASK5166_PARSE_ERROR={error}");
    println!("TASK5166_PARSE_SILENT={silent}");
    assert!(matches!(absent, AmsiFailure::ProviderAbsent(_)));
    assert!(matches!(error, AmsiFailure::ProviderError(_)));
    assert!(matches!(silent, AmsiFailure::ProviderAbsent(_)));
}
