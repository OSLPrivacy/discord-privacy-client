use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use task_3096_build_proof::{
    CANNOT_TELL_DIFFERENT_DEVICE_PROOF, CANNOT_TELL_EXPIRED_PROOF, CANNOT_TELL_MISSING_PROOF,
    MODIFIED_BUILD_FINGERPRINT_MISMATCH,
};

const GOOD_BUILD: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const CHANGED_BUILD: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
const OWNER_DEVICE: &str = "device:owner-3217";
const COPIER_DEVICE: &str = "device:copier-3217";
const PERSON: &str = "person:owner-3217";
const MADE_AT: &str = "1786000000";
const STOPS_AT: &str = "1787000000";
const DURING_PROOF: &str = "1786500000";

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("osl-task-3217-{}-{sequence}", std::process::id()));
        fs::create_dir(&path).expect("create task 3217 temporary directory");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn require_success(output: Output, label: &str) -> Vec<u8> {
    assert!(
        output.status.success(),
        "{label} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn sign_good_build_proof() -> Vec<u8> {
    require_success(
        Command::new(env!("CARGO_BIN_EXE_osl-sign-build-proof"))
            .args([
                "--signing-key-file",
                fixture("task3097-trusted-secret.base64").to_str().unwrap(),
                "--build-fingerprint",
                GOOD_BUILD,
                "--device-id",
                OWNER_DEVICE,
                "--person-id",
                PERSON,
                "--made-at-unix-seconds",
                MADE_AT,
                "--stops-counting-at-unix-seconds",
                STOPS_AT,
            ])
            .output()
            .expect("run shipped build-proof signer"),
        "shipped build-proof signer",
    )
}

fn check(proof_path: &Path, fingerprint: &str, device: &str, at: &str) -> String {
    let stdout = require_success(
        Command::new(env!("CARGO_BIN_EXE_osl-check-build-proof"))
            .args([
                "--proof-file",
                proof_path.to_str().unwrap(),
                "--trusted-public-key-file",
                fixture("task3097-trusted-public.base64").to_str().unwrap(),
                "--build-fingerprint",
                fingerprint,
                "--device-id",
                device,
                "--at-unix-seconds",
                at,
            ])
            .output()
            .expect("run shipped build-proof checker"),
        "shipped build-proof checker",
    );
    String::from_utf8(stdout)
        .expect("checker answer is UTF-8")
        .trim()
        .to_owned()
}

#[test]
fn copied_proof_never_authenticates_changed_or_other_device_builds() {
    let temporary = TempDir::new();
    let owner_proof = temporary.0.join("owner-real-build-proof.json");
    fs::write(&owner_proof, sign_good_build_proof()).expect("save signed proof from good build");

    // The attacker copies this exact signed proof; it is not recreated or edited.
    let copied_proof = temporary.0.join("copied-build-proof.json");
    fs::copy(&owner_proof, &copied_proof).expect("copy good build proof verbatim");
    assert_eq!(
        fs::read(&owner_proof).unwrap(),
        fs::read(&copied_proof).unwrap(),
        "attack must present the exact copied proof"
    );

    let proofless_fixture = temporary.0.join("fixture-without-copied-build-proof");
    fs::create_dir(&proofless_fixture).expect("create fixture without copied build proof");

    let changed_build = check(&copied_proof, CHANGED_BUILD, OWNER_DEVICE, DURING_PROOF);
    let other_device = check(&copied_proof, GOOD_BUILD, COPIER_DEVICE, DURING_PROOF);
    let expired = check(&copied_proof, GOOD_BUILD, OWNER_DEVICE, STOPS_AT);
    let owner_good = check(&owner_proof, GOOD_BUILD, OWNER_DEVICE, DURING_PROOF);
    let no_copied_proof = check(
        &proofless_fixture.join("build-proof.json"),
        GOOD_BUILD,
        OWNER_DEVICE,
        DURING_PROOF,
    );

    assert_eq!(changed_build, MODIFIED_BUILD_FINGERPRINT_MISMATCH);
    assert_eq!(
        other_device, CANNOT_TELL_DIFFERENT_DEVICE_PROOF,
        "TASK3217_DEVICE_REPLAY forbidden externally observable state={other_device}"
    );
    assert_eq!(expired, CANNOT_TELL_EXPIRED_PROOF);
    assert_eq!(owner_good, "unmodified");
    assert_eq!(no_copied_proof, CANNOT_TELL_MISSING_PROOF);

    for answer in [&changed_build, &other_device, &expired] {
        assert_ne!(
            answer, "unmodified",
            "copied-proof attack claimed unmodified"
        );
    }

    println!(
        "TASK3217_CHANGED_BUILD copied_proof=1 answer=\"{changed_build}\" reason=build-fingerprint-mismatch"
    );
    println!(
        "TASK3217_OTHER_DEVICE copied_proof=1 answer=\"{other_device}\" reason=device-binding-mismatch"
    );
    println!("TASK3217_EXPIRED copied_proof=1 answer=\"{expired}\" reason=expired-proof");
    println!("TASK3217_OWNER_GOOD own_device=1 answer={owner_good}");
    println!(
        "TASK3217_NO_COPIED_PROOF fixture_without_copied_proof=1 checked_path=fixture-without-copied-build-proof/build-proof.json answer=\"{no_copied_proof}\""
    );
    println!("TASK3217_SUMMARY attack_attempts=3 unmodified_attempt_count=0 good_owner_unmodified_count=1");
}
