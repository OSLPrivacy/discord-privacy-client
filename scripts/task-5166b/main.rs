//! TASK 5166b - prove the protected-download quarantine cannot be bypassed.
//!
//! Drives the VERBATIM production boundary
//! (`apps/osl-hub/src/protected_download_quarantine.rs`, copied in beside this
//! file) against the real local Windows AMSI provider, with a clean fixture and
//! the EICAR test string. The harness script runs it three times:
//!
//!   real     - the untouched boundary. Must exit 0 and expose only the
//!              unchanged clean fixture.
//!   starved  - the boundary sabotaged to return clean WITHOUT invoking the
//!              scanner. Must exit 1 naming invocation count 0.
//!   swapped  - the boundary sabotaged to drop the before-move re-hash. Must
//!              exit 1 naming the before-move hash mismatch.
//!
//! The scanner invocation count is kept by the scanner, not by the boundary, so
//! a boundary that never calls out cannot inflate it.

// The copied production module carries the whole boundary API; this harness
// only drives part of it.
#[allow(dead_code)]
mod protected_download_quarantine;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use protected_download_quarantine::{
    now_unix_seconds, sha256_hex, AmsiFailure, AmsiProvider, AmsiReport, AmsiSubmission,
    ProtectedDownloadOutcome, ProtectedDownloadQuarantine, QuarantineReason, WindowsAmsiProvider,
};

const CLEAN_FIXTURE: &[u8] =
    b"TASK 5166b clean protected download fixture. Ordinary text, nothing executable.\n";

/// EICAR, XOR-masked with 0x5A so the literal signature is never a contiguous
/// run inside this binary.
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

fn temp_root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "osl-task5166b-{label}-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temp root");
    root
}

/// Bytes of a specific protected payload visible anywhere outside quarantine.
fn payload_bytes_exposed(exposure_root: &Path, payload: &[u8]) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(exposure_root) {
        for entry in entries.flatten() {
            if let Ok(bytes) = std::fs::read(entry.path()) {
                if bytes == payload {
                    total = total.saturating_add(bytes.len() as u64);
                }
            }
        }
    }
    total
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "real".to_owned());
    println!("TASK5166B_MODE={mode}");

    let provider = WindowsAmsiProvider::new();
    if !provider.powershell_path().exists() {
        println!(
            "TASK5166B_FAIL the interactive Windows scanner host {} is not present",
            provider.powershell_path().display()
        );
        std::process::exit(1);
    }

    let mut failures: Vec<String> = Vec::new();
    let eicar = eicar_bytes();

    // ---- case 1: the clean fixture, one matching scanner call ---------------
    let clean_root = temp_root("clean");
    let clean_boundary =
        ProtectedDownloadQuarantine::for_app_root(&clean_root).expect("quarantine opens");
    let held = clean_boundary
        .admit_bytes("task-5166b-clean", CLEAN_FIXTURE)
        .expect("clean fixture admitted");
    let counting = CountingProvider::new(&provider);
    let clean_outcome = clean_boundary.scan_and_release(held, &counting, now_unix_seconds());
    let clean_invocations = counting.count();
    println!("TASK5166B_CLEAN_SCANNER_INVOCATIONS={clean_invocations}");
    println!("TASK5166B_CLEAN_OUTCOME={}", clean_outcome.reason_name());
    println!("TASK5166B_CLEAN_EXPOSED_BYTES={}", clean_outcome.bytes_exposed());
    if clean_invocations != 1 {
        failures.push(format!(
            "the clean release rests on scanner invocation count {clean_invocations}, not 1"
        ));
    }
    match &clean_outcome {
        ProtectedDownloadOutcome::Exposed(exposed) => {
            let released = std::fs::read(&exposed.path).unwrap_or_default();
            println!(
                "TASK5166B_CLEAN_RELEASED_UNCHANGED={}",
                released == CLEAN_FIXTURE
            );
            if released != CLEAN_FIXTURE {
                failures.push("the released clean fixture is not the fixture that was scanned".to_owned());
            }
        }
        ProtectedDownloadOutcome::Withheld(withheld) => {
            println!("TASK5166B_CLEAN_REFUSAL={}", withheld.local_reason_text);
            failures.push(format!(
                "the clean fixture was refused: {} (invocation count {})",
                withheld.reason.name(),
                withheld.amsi_invocations
            ));
        }
    }

    // ---- case 2: EICAR must never leave quarantine --------------------------
    let eicar_root = temp_root("eicar");
    let eicar_boundary =
        ProtectedDownloadQuarantine::for_app_root(&eicar_root).expect("quarantine opens");
    let held = eicar_boundary
        .admit_bytes("task-5166b-eicar", &eicar)
        .expect("EICAR fixture admitted");
    let counting = CountingProvider::new(&provider);
    let eicar_outcome = eicar_boundary.scan_and_release(held, &counting, now_unix_seconds());
    let eicar_exposed = payload_bytes_exposed(eicar_boundary.exposure_root(), &eicar);
    println!("TASK5166B_EICAR_SCANNER_INVOCATIONS={}", counting.count());
    println!("TASK5166B_EICAR_OUTCOME={}", eicar_outcome.reason_name());
    println!("TASK5166B_EICAR_PROTECTED_BYTES_EXPOSED={eicar_exposed}");
    if eicar_exposed != 0 {
        failures.push(format!("{eicar_exposed} EICAR bytes were exposed outside quarantine"));
    }
    if !matches!(&eicar_outcome, ProtectedDownloadOutcome::Withheld(w) if w.reason == QuarantineReason::MalwareDetected)
    {
        failures.push(format!(
            "EICAR was not held for malware_detected, it was {}",
            eicar_outcome.reason_name()
        ));
    }

    // ---- case 3: one byte swapped after the clean result, before the move ---
    let swap_root = temp_root("swap");
    let swap_boundary =
        ProtectedDownloadQuarantine::for_app_root(&swap_root).expect("quarantine opens");
    let held = swap_boundary
        .admit_bytes("task-5166b-swap", CLEAN_FIXTURE)
        .expect("clean fixture admitted");
    let quarantine_path = held.path().to_owned();
    let counting = CountingProvider::new(&provider);
    let scan = swap_boundary
        .scan(&held, &counting, now_unix_seconds())
        .expect("the clean fixture scans clean");
    let mut tampered = CLEAN_FIXTURE.to_vec();
    tampered[0] ^= 0x01;
    std::fs::write(&quarantine_path, &tampered).expect("swap one byte in quarantine");
    println!("TASK5166B_SWAP_SCANNED_SHA256={}", scan.content_sha256);
    println!("TASK5166B_SWAP_BEFORE_MOVE_SHA256={}", sha256_hex(&tampered));
    match swap_boundary.release(held, &scan) {
        Ok(exposed) => {
            println!("TASK5166B_SWAP_OUTCOME=released");
            println!("TASK5166B_SWAP_EXPOSED_BYTES={}", exposed.bytes_exposed);
            failures.push(format!(
                "the before-move hash mismatch was not detected: sha256={} was scanned but sha256={} was moved out of quarantine",
                scan.content_sha256,
                sha256_hex(&tampered)
            ));
        }
        Err(withheld) => {
            println!("TASK5166B_SWAP_OUTCOME={}", withheld.reason.name());
            println!("TASK5166B_SWAP_EXPOSED_BYTES={}", withheld.bytes_exposed);
            if withheld.reason != QuarantineReason::ContentHashMismatch {
                failures.push(format!(
                    "the swapped file was refused as {} rather than a before-move hash mismatch",
                    withheld.reason.name()
                ));
            }
        }
    }
    let swap_protected_exposed = payload_bytes_exposed(swap_boundary.exposure_root(), CLEAN_FIXTURE);
    println!("TASK5166B_SWAP_PROTECTED_BYTES_EXPOSED={swap_protected_exposed}");
    if swap_protected_exposed != 0 {
        failures.push(format!(
            "{swap_protected_exposed} scanned protected bytes were exposed after the swap"
        ));
    }

    let _ = std::fs::remove_dir_all(&clean_root);
    let _ = std::fs::remove_dir_all(&eicar_root);
    let _ = std::fs::remove_dir_all(&swap_root);

    for failure in &failures {
        println!("TASK5166B_FAIL {failure}");
    }
    println!("TASK5166B_FAILURE_COUNT={}", failures.len());
    if failures.is_empty() {
        println!("TASK5166B_EXIT=0");
        std::process::exit(0);
    }
    println!("TASK5166B_EXIT=1");
    std::process::exit(1);
}
