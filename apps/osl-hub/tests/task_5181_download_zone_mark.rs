//! TASK 5181 - preserve Windows download-zone protection after a clean scan.
//!
//! These checks drive the REAL Windows Attachment Services COM server
//! (`CLSID_AttachmentServices` / `IAttachmentExecute`) in the interactive
//! Windows session from WSL, behind the REAL local Windows AMSI provider from
//! TASK 5166. Nothing here is stubbed: a clean executable and a clean document
//! are scanned by Microsoft Defender through `AmsiScanBuffer`, released, saved
//! to a chosen NTFS path, handed to `IAttachmentExecute::Save`, and the
//! `Zone.Identifier` mark is then read back off the destination TWICE - once by
//! the shipping helper and once by an independent `powershell.exe` invocation
//! this test makes itself.
//!
//! Quarantine always lives on the Linux side (`/tmp`, ext4), so no EICAR
//! plaintext is ever written to a Defender-scanned Windows volume.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use osl_privacy_hub::download_zone_handoff::{
    filesystem_can_retain_mark, is_success_hresult, parse_zone_handoff_output, windows_path_for,
    AttachmentServicesSaver, WindowsAttachmentServicesSaver, ZoneHandoffFailure,
    ZoneHandoffOutcome, ZoneHandoffRequest, INTERNET_ZONE_ID, REQUIRED_SAVE_CALLS,
    ZONE_HANDOFF_HELPER_SCRIPT,
};
use osl_privacy_hub::protected_download_final_save::{deliver_after_clean_scan, FinalSaveOutcome};
use osl_privacy_hub::protected_download_quarantine::{
    now_unix_seconds, sha256_hex, AmsiProvider, ProtectedDownloadQuarantine, QuarantineReason,
    WindowsAmsiProvider,
};

const SOURCE_URL: &str = "https://downloads.oslprivacy.example/attachments/osl-report";
const REFERRER_URL: &str = "https://downloads.oslprivacy.example/attachments/";

/// A real, signed, unambiguously clean Windows executable. Copying one that is
/// already on this machine is what makes "a clean executable" a real PE rather
/// than a text file with an `.exe` name.
const CLEAN_EXECUTABLE_SOURCE: &str = "/mnt/c/Windows/System32/hostname.exe";

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

/// A minimal but structurally real PDF document.
fn clean_document_bytes() -> Vec<u8> {
    let body = concat!(
        "%PDF-1.4\n",
        "1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n",
        "2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n",
        "3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]/Contents 4 0 R>>endobj\n",
        "4 0 obj<</Length 58>>stream\n",
        "BT /F1 12 Tf 72 720 Td (OSL TASK 5181 clean document) Tj ET\n",
        "endstream endobj\n",
        "trailer<</Root 1 0 R>>\n",
        "%%EOF\n",
    );
    body.as_bytes().to_vec()
}

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

fn unique(tag: &str) -> String {
    format!("osl-task-5181-{tag}-{}", std::process::id())
}

/// Quarantine always on ext4.
fn linux_root(tag: &str) -> TempRoot {
    let path = std::env::temp_dir().join(unique(tag));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("linux scratch root");
    TempRoot(path)
}

/// A destination on a real NTFS Windows volume, reachable from both sides.
fn ntfs_root(tag: &str) -> TempRoot {
    let path = PathBuf::from("/mnt/d").join(unique(tag));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("ntfs scratch root");
    TempRoot(path)
}

/// Counts the calls that actually reach Attachment Services. The count is kept
/// out here, so a boundary that never hands anything off cannot inflate it.
struct CountingSaver<'a> {
    inner: &'a dyn AttachmentServicesSaver,
    calls: AtomicU32,
}

impl<'a> CountingSaver<'a> {
    fn new(inner: &'a dyn AttachmentServicesSaver) -> Self {
        Self {
            inner,
            calls: AtomicU32::new(0),
        }
    }

    fn calls(&self) -> u32 {
        self.calls.load(Ordering::SeqCst)
    }
}

impl AttachmentServicesSaver for CountingSaver<'_> {
    fn save(
        &self,
        request: &ZoneHandoffRequest,
    ) -> Result<ZoneHandoffOutcome, ZoneHandoffFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.save(request)
    }
}

fn powershell() -> PathBuf {
    PathBuf::from("/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe")
}

/// A second, independent observation of the mark: a separate `powershell.exe`
/// process, not the shipping helper, reading the stream off the destination.
fn independent_zone_identifier(path: &Path) -> String {
    let windows_path = windows_path_for(path).expect("windows path");
    let command = format!(
        "$ErrorActionPreference='Stop'; try {{ (Get-Content -LiteralPath '{windows_path}' \
         -Stream 'Zone.Identifier' -Raw) -replace \"`r|`n\", '|' }} catch {{ 'ABSENT' }}"
    );
    let output = Command::new(powershell())
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(command)
        .output()
        .expect("independent zone reader");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn read_back(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// 1. A clean executable and a clean document each reach the chosen path with
//    the expected Zone.Identifier and exactly one Attachment Services save.
// ---------------------------------------------------------------------------

#[test]
fn task_5181_a_clean_executable_and_document_reach_the_chosen_path_with_the_internet_zone_mark() {
    let executable = std::fs::read(CLEAN_EXECUTABLE_SOURCE).unwrap_or_else(|error| {
        panic!("the clean executable fixture {CLEAN_EXECUTABLE_SOURCE} is unreadable: {error}")
    });
    let fixtures: [(&str, &str, Vec<u8>); 2] = [
        ("EXECUTABLE", "osl-report.exe", executable),
        ("DOCUMENT", "osl-report.pdf", clean_document_bytes()),
    ];

    let mut marked = 0u32;
    for (label, file_name, bytes) in fixtures {
        let quarantine_root = linux_root(&format!("clean-{}", label.to_lowercase()));
        let destination_root = ntfs_root(&format!("dest-{}", label.to_lowercase()));
        let quarantine = ProtectedDownloadQuarantine::with_roots(
            quarantine_root.path().join("quarantine"),
            quarantine_root.path().join("exposure"),
        )
        .expect("quarantine");
        let held = quarantine
            .admit_bytes(&format!("task-5181-{label}"), &bytes)
            .expect("admit");
        let chosen_path = destination_root.path().join(file_name);
        let source_url = format!("{SOURCE_URL}{}", Path::new(file_name).extension().map_or(
            String::new(),
            |extension| format!(".{}", extension.to_string_lossy())
        ));

        let provider = WindowsAmsiProvider::new();
        let real_saver = WindowsAttachmentServicesSaver::new();
        let saver = CountingSaver::new(&real_saver);

        let outcome = deliver_after_clean_scan(
            &quarantine,
            held,
            &provider as &dyn AmsiProvider,
            &saver,
            &chosen_path,
            &source_url,
            REFERRER_URL,
            now_unix_seconds(),
        );

        let delivered = match outcome {
            FinalSaveOutcome::Delivered(delivered) => delivered,
            other => panic!("{label} was not delivered: {} {}", other.name(), other.message()),
        };
        let mark = match &delivered.zone {
            ZoneHandoffOutcome::Marked(mark) => mark.clone(),
            ZoneHandoffOutcome::PlatformLimited(limited) => panic!(
                "{label} landed on {} which cannot retain a mark: {}",
                limited.filesystem, limited.message
            ),
        };

        let landed = read_back(&chosen_path);
        let independent = independent_zone_identifier(&chosen_path);

        println!("TASK5181_{label}_CHOSEN_PATH={}", chosen_path.display());
        println!("TASK5181_{label}_WINDOWS_PATH={}", mark.windows_path);
        println!("TASK5181_{label}_FILESYSTEM={}", mark.filesystem);
        println!("TASK5181_{label}_AMSI_RESULT={}", delivered.scan.amsi_result_code);
        println!("TASK5181_{label}_AMSI_PROVIDER={}", delivered.scan.provider_identity);
        println!(
            "TASK5181_{label}_AMSI_SIGNATURE_VERSION={}",
            delivered.scan.signature_version
        );
        println!("TASK5181_{label}_SCANNED_SHA256={}", delivered.scan.content_sha256);
        println!("TASK5181_{label}_LANDED_SHA256={}", sha256_hex(&landed));
        println!("TASK5181_{label}_LANDED_BYTES={}", landed.len());
        println!("TASK5181_{label}_SAVE_CALLS_REPORTED_BY_WINDOWS={}", mark.save_calls);
        println!("TASK5181_{label}_SAVE_CALLS_OBSERVED_BY_TEST={}", saver.calls());
        println!("TASK5181_{label}_HR_SAVE={}", mark.save_hresult);
        println!("TASK5181_{label}_HR_CHECK_POLICY={}", mark.check_policy_hresult);
        println!("TASK5181_{label}_ZONE_ID={}", mark.zone_id);
        println!("TASK5181_{label}_ZONE_HOST_URL={}", mark.host_url);
        println!("TASK5181_{label}_ZONE_REFERRER_URL={}", mark.referrer_url);
        println!(
            "TASK5181_{label}_ZONE_IDENTIFIER={}",
            mark.zone_identifier_text.replace(['\r', '\n'], "|")
        );
        println!("TASK5181_{label}_ZONE_IDENTIFIER_INDEPENDENT={independent}");
        println!("TASK5181_{label}_QUARANTINE_BYTES_AFTER={}", quarantine.quarantined_bytes());
        println!("TASK5181_{label}_EXPOSURE_BYTES_AFTER={}", quarantine.exposed_bytes());

        assert!(chosen_path.exists(), "{label} is not at the chosen path");
        assert_eq!(
            sha256_hex(&landed),
            delivered.scan.content_sha256,
            "{label} at the chosen path is not the bytes that were scanned"
        );
        assert_eq!(landed.len() as u64, delivered.bytes, "{label} length");
        assert!(delivered.zone_marked(), "{label} was not marked");
        assert_eq!(mark.zone_id, INTERNET_ZONE_ID, "{label} zone id");
        assert_eq!(mark.host_url, source_url, "{label} HostUrl");
        assert_eq!(mark.referrer_url, REFERRER_URL, "{label} ReferrerUrl");
        assert_eq!(
            mark.save_calls, REQUIRED_SAVE_CALLS,
            "{label} did not rest on exactly one Attachment Services save call"
        );
        assert_eq!(
            saver.calls(),
            REQUIRED_SAVE_CALLS,
            "{label} reached Attachment Services a number of times other than once"
        );
        assert!(
            filesystem_can_retain_mark(&mark.filesystem),
            "{label} destination filesystem {} cannot retain a mark",
            mark.filesystem
        );
        assert!(
            independent.contains(&format!("ZoneId={INTERNET_ZONE_ID}")),
            "{label}: an independent reader did not find ZoneId={INTERNET_ZONE_ID}: {independent}"
        );
        assert_eq!(
            quarantine.exposed_bytes(),
            0,
            "{label} left bytes behind in the exposure directory"
        );
        marked += 1;
    }

    println!("TASK5181_CLEAN_FIXTURES_MARKED={marked}");
    assert_eq!(marked, 2);
}

// ---------------------------------------------------------------------------
// 2. EICAR reaches neither the save call nor the destination.
// ---------------------------------------------------------------------------

#[test]
fn task_5181_eicar_reaches_neither_the_attachment_services_save_call_nor_the_destination() {
    // Destination stays on ext4: a detection must never put EICAR anywhere, and
    // certainly not on a Windows volume.
    let root = linux_root("eicar");
    let quarantine = ProtectedDownloadQuarantine::with_roots(
        root.path().join("quarantine"),
        root.path().join("exposure"),
    )
    .expect("quarantine");
    let sample = eicar_bytes();
    let held = quarantine.admit_bytes("task-5181-eicar", &sample).expect("admit");
    let chosen_path = root.path().join("downloads").join("osl-report.com");

    let provider = WindowsAmsiProvider::new();
    let real_saver = WindowsAttachmentServicesSaver::new();
    let saver = CountingSaver::new(&real_saver);

    let outcome = deliver_after_clean_scan(
        &quarantine,
        held,
        &provider as &dyn AmsiProvider,
        &saver,
        &chosen_path,
        SOURCE_URL,
        REFERRER_URL,
        now_unix_seconds(),
    );

    let withheld = match outcome {
        FinalSaveOutcome::Withheld(withheld) => withheld,
        other => panic!(
            "EICAR was not withheld: {} {}",
            other.name(),
            other.message()
        ),
    };
    let destination_bytes = std::fs::metadata(&chosen_path).map(|m| m.len()).unwrap_or(0);

    println!("TASK5181_EICAR_SHA256={}", sha256_hex(&sample));
    println!("TASK5181_EICAR_LEN={}", sample.len());
    println!("TASK5181_EICAR_REASON={}", withheld.reason.name());
    println!("TASK5181_EICAR_AMSI_INVOCATIONS={}", withheld.amsi_invocations);
    println!("TASK5181_EICAR_SAVE_CALLS_OBSERVED_BY_TEST={}", saver.calls());
    println!("TASK5181_EICAR_DESTINATION={}", chosen_path.display());
    println!("TASK5181_EICAR_DESTINATION_EXISTS={}", chosen_path.exists());
    println!("TASK5181_EICAR_DESTINATION_BYTES={destination_bytes}");
    println!("TASK5181_EICAR_EXPOSURE_BYTES={}", quarantine.exposed_bytes());
    println!("TASK5181_EICAR_STILL_IN_QUARANTINE={}", quarantine.quarantined_bytes() > 0);
    println!("TASK5181_EICAR_LOCAL_REASON_TEXT={}", withheld.local_reason_text);

    assert_eq!(withheld.reason, QuarantineReason::MalwareDetected);
    assert_eq!(saver.calls(), 0, "EICAR reached the Attachment Services save call");
    assert!(!chosen_path.exists(), "EICAR reached the destination");
    assert_eq!(destination_bytes, 0);
    assert_eq!(withheld.bytes_exposed, 0);
    assert_eq!(quarantine.exposed_bytes(), 0);

    quarantine.discard_withheld(&withheld).expect("discard");
}

// ---------------------------------------------------------------------------
// 3. A non-NTFS destination reports the platform limitation and claims nothing.
// ---------------------------------------------------------------------------

#[test]
fn task_5181_a_non_ntfs_destination_reports_the_platform_limitation_without_claiming_a_mark() {
    let root = linux_root("non-ntfs");
    let quarantine = ProtectedDownloadQuarantine::with_roots(
        root.path().join("quarantine"),
        root.path().join("exposure"),
    )
    .expect("quarantine");
    let bytes = clean_document_bytes();
    let held = quarantine
        .admit_bytes("task-5181-non-ntfs", &bytes)
        .expect("admit");
    // ext4, seen from Windows as the 9P WSL volume: a real destination a user
    // can choose, and one that cannot carry an alternate data stream.
    let chosen_path = root.path().join("downloads").join("osl-report.pdf");

    let provider = WindowsAmsiProvider::new();
    let real_saver = WindowsAttachmentServicesSaver::new();
    let saver = CountingSaver::new(&real_saver);

    let outcome = deliver_after_clean_scan(
        &quarantine,
        held,
        &provider as &dyn AmsiProvider,
        &saver,
        &chosen_path,
        SOURCE_URL,
        REFERRER_URL,
        now_unix_seconds(),
    );

    let delivered = match outcome {
        FinalSaveOutcome::Delivered(delivered) => delivered,
        other => panic!(
            "the non-NTFS save did not complete: {} {}",
            other.name(),
            other.message()
        ),
    };
    let limited = match &delivered.zone {
        ZoneHandoffOutcome::PlatformLimited(limited) => limited.clone(),
        ZoneHandoffOutcome::Marked(mark) => panic!(
            "a non-NTFS destination claimed ZoneId={} on {}",
            mark.zone_id, mark.filesystem
        ),
    };
    // A mark that does not exist must not be faked by a sidecar file either.
    let sidecar = chosen_path.with_file_name(format!(
        "{}:Zone.Identifier",
        chosen_path.file_name().unwrap().to_string_lossy()
    ));
    let independent = independent_zone_identifier(&chosen_path);

    println!("TASK5181_NONNTFS_CHOSEN_PATH={}", chosen_path.display());
    println!("TASK5181_NONNTFS_WINDOWS_PATH={}", limited.windows_path);
    println!("TASK5181_NONNTFS_FILESYSTEM={}", limited.filesystem);
    println!("TASK5181_NONNTFS_REASON={}", limited.reason.name());
    println!("TASK5181_NONNTFS_SAVE_CALLS={}", limited.save_calls);
    println!("TASK5181_NONNTFS_ZONE_MARKED={}", limited.zone_marked());
    println!("TASK5181_NONNTFS_ZONE_ID_CLAIMED={:?}", delivered.zone.zone_id());
    println!("TASK5181_NONNTFS_DESTINATION_BYTES={}", delivered.bytes);
    println!("TASK5181_NONNTFS_LANDED_SHA256={}", sha256_hex(&read_back(&chosen_path)));
    println!("TASK5181_NONNTFS_SIDECAR_EXISTS={}", sidecar.exists());
    println!("TASK5181_NONNTFS_ZONE_IDENTIFIER_INDEPENDENT={independent}");
    println!("TASK5181_NONNTFS_MESSAGE={}", limited.message);

    assert!(chosen_path.exists(), "the clean document did not reach the chosen path");
    assert_eq!(
        sha256_hex(&read_back(&chosen_path)),
        delivered.scan.content_sha256
    );
    assert!(!delivered.zone_marked(), "a non-NTFS destination claimed a mark");
    assert!(!limited.zone_marked());
    assert_eq!(delivered.zone.zone_id(), None, "a zone id was invented");
    assert_eq!(limited.save_calls, 0, "the handoff was attempted anyway");
    assert_eq!(saver.calls(), 1, "the boundary did not consult the saver");
    assert!(
        !filesystem_can_retain_mark(&limited.filesystem),
        "the reported filesystem {} can retain a mark after all",
        limited.filesystem
    );
    assert!(
        limited.message.contains("cannot keep an Internet-zone mark")
            && limited.message.contains("No mark exists on this file"),
        "the limitation does not say plainly that no mark exists: {}",
        limited.message
    );
    assert!(
        !limited.message.contains("ZoneId"),
        "the limitation claims a zone id: {}",
        limited.message
    );
    assert!(
        !sidecar.exists(),
        "a stray Zone.Identifier sidecar was left at {}",
        sidecar.display()
    );
    assert!(
        !independent.contains("ZoneId="),
        "an independent reader found a mark on a non-NTFS destination: {independent}"
    );
}

// ---------------------------------------------------------------------------
// 4. A starved Attachment Services host names the missing zone handoff and
//    leaves nothing at the destination.
// ---------------------------------------------------------------------------

#[test]
fn task_5181_a_starved_attachment_services_host_names_the_missing_zone_handoff() {
    let quarantine_root = linux_root("starved");
    let destination_root = ntfs_root("starved-dest");
    let quarantine = ProtectedDownloadQuarantine::with_roots(
        quarantine_root.path().join("quarantine"),
        quarantine_root.path().join("exposure"),
    )
    .expect("quarantine");
    let bytes = clean_document_bytes();
    let held = quarantine
        .admit_bytes("task-5181-starved", &bytes)
        .expect("admit");
    let chosen_path = destination_root.path().join("osl-report.pdf");

    let provider = WindowsAmsiProvider::new();
    // Starve the handoff: the Attachment Services host is not there.
    let starved = WindowsAttachmentServicesSaver::new()
        .with_powershell(PathBuf::from("/mnt/c/Windows/System32/osl-no-such-attachment-host.exe"));
    let saver = CountingSaver::new(&starved);

    let outcome = deliver_after_clean_scan(
        &quarantine,
        held,
        &provider as &dyn AmsiProvider,
        &saver,
        &chosen_path,
        SOURCE_URL,
        REFERRER_URL,
        now_unix_seconds(),
    );

    let missing = match outcome {
        FinalSaveOutcome::ZoneHandoffMissing(missing) => missing,
        other => panic!(
            "a starved zone handoff was not reported: {} {}",
            other.name(),
            other.message()
        ),
    };

    println!("TASK5181_STARVED_REASON={}", missing.failure.name());
    println!("TASK5181_STARVED_SAVE_CALLS_OBSERVED_BY_TEST={}", saver.calls());
    println!("TASK5181_STARVED_DESTINATION={}", missing.destination.display());
    println!("TASK5181_STARVED_DESTINATION_EXISTS={}", chosen_path.exists());
    println!("TASK5181_STARVED_DESTINATION_BYTES={}", missing.bytes_at_destination);
    println!("TASK5181_STARVED_MESSAGE={}", missing.message);

    assert_eq!(missing.failure.name(), "zone_handoff_absent");
    assert!(matches!(missing.failure, ZoneHandoffFailure::HandoffAbsent(_)));
    assert!(
        missing.message.contains("The Windows zone handoff is missing"),
        "the failure does not name the missing zone handoff: {}",
        missing.message
    );
    assert!(!chosen_path.exists(), "an unmarked download was left on disk");
    assert_eq!(missing.bytes_at_destination, 0);
}

// ---------------------------------------------------------------------------
// 5. The honesty rules, at the parse boundary: nothing but a real ZoneId=3 read
//    back off the file counts as a mark.
// ---------------------------------------------------------------------------

#[test]
fn task_5181_a_save_that_left_no_real_internet_zone_mark_is_never_reported_as_marked() {
    let marked_stream = "[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://example.test/a.exe\r\n";
    let encode = |text: &str| {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(text.as_bytes())
    };
    let helper = |status: &str, save_calls: &str, hr_save: &str, zone: &str| {
        format!(
            "OSL5181_FILESYSTEM=NTFS\nOSL5181_WINDOWS_PATH=D:\\downloads\\a.exe\n\
             OSL5181_FILE_EXISTS_BEFORE=true\nOSL5181_SAVE_CALLS={save_calls}\n\
             OSL5181_HR_SAVE={hr_save}\nOSL5181_HR_CHECK_POLICY=0x00000001\n\
             OSL5181_FILE_EXISTS_AFTER=true\nOSL5181_FILE_LEN_AFTER=10\n\
             OSL5181_ZONE_B64={}\nOSL5181_STATUS={status}\n",
            encode(zone)
        )
    };

    let good = parse_zone_handoff_output(&helper("handed_off", "1", "0x00000000", marked_stream))
        .expect("a real mark parses");
    assert_eq!(good.zone_id(), Some(INTERNET_ZONE_ID));
    assert_eq!(good.save_calls(), 1);

    let cases: [(&str, String, &str); 5] = [
        (
            "EMPTY_STREAM",
            helper("handed_off", "1", "0x00000000", ""),
            "zone_handoff_mark_missing",
        ),
        (
            "WRONG_ZONE",
            helper("handed_off", "1", "0x00000000", "[ZoneTransfer]\r\nZoneId=0\r\n"),
            "zone_handoff_wrong_zone",
        ),
        (
            "ZERO_SAVE_CALLS",
            helper("handed_off", "0", "0x00000000", marked_stream),
            "zone_handoff_save_call_count",
        ),
        (
            "TWO_SAVE_CALLS",
            helper("handed_off", "2", "0x00000000", marked_stream),
            "zone_handoff_save_call_count",
        ),
        (
            "SAVE_REFUSED",
            helper("handed_off", "1", "0x80070005", marked_stream),
            "zone_handoff_save_refused",
        ),
    ];
    let mut refused = 0u32;
    for (label, output, expected) in cases {
        let failure = parse_zone_handoff_output(&output)
            .err()
            .unwrap_or_else(|| panic!("{label} was accepted as a mark"));
        println!("TASK5181_PARSE {label} reason={} ", failure.name());
        assert_eq!(failure.name(), expected, "{label}");
        assert!(
            failure.message().contains("The Windows zone handoff is missing"),
            "{label} does not name the missing zone handoff"
        );
        refused += 1;
    }

    // A helper that reports nothing at all is absence, never a silent success.
    let silent = parse_zone_handoff_output("").expect_err("silence is not a mark");
    println!("TASK5181_PARSE SILENT reason={}", silent.name());
    assert_eq!(silent.name(), "zone_handoff_absent");
    refused += 1;

    println!("TASK5181_PARSE_REFUSALS={refused}");
    println!(
        "TASK5181_HELPER_IS_EMBEDDED={}",
        ZONE_HANDOFF_HELPER_SCRIPT.contains("IAttachmentExecute")
    );
    println!("TASK5181_HELPER_SAVE_CALL_SITES={}", ZONE_HANDOFF_HELPER_SCRIPT.matches("ae.Save()").count());
    assert_eq!(refused, 6);
    assert!(is_success_hresult("0x00000001"));
    assert!(!is_success_hresult("0x80004005"));
    // Exactly one Save() call site exists in the embedded helper.
    assert_eq!(ZONE_HANDOFF_HELPER_SCRIPT.matches("ae.Save()").count(), 1);
}
