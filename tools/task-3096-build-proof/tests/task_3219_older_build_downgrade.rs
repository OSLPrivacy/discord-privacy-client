use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use task_3096_build_proof::CANNOT_TELL_OLD_BUILD;

const FINGERPRINT: &str = "3219aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OLD_ACTUAL_FINGERPRINT: &str =
    "3219bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const DEVICE: &str = "device:copy-a-3219";
const PERSON: &str = "person:copy-a-3219";
const MADE_AT: &str = "1786000000";
const STOPS_AT: &str = "1788000000";
const CHECKED_AT: &str = "1787000000";

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("osl-task-3219-{}-{sequence}", std::process::id()));
        fs::create_dir(&path).expect("create task 3219 temporary directory");
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
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn proof_args() -> [&'static str; 10] {
    [
        "--build-fingerprint",
        FINGERPRINT,
        "--device-id",
        DEVICE,
        "--person-id",
        PERSON,
        "--made-at-unix-seconds",
        MADE_AT,
        "--stops-counting-at-unix-seconds",
        STOPS_AT,
    ]
}

fn require_success(output: Output, label: &str) -> Vec<u8> {
    assert!(
        output.status.success(),
        "{label} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn produce_old_proof() -> Vec<u8> {
    let output = Command::new(env!("CARGO_BIN_EXE_task-3219-old-weak-build"))
        .args([
            "--app-name",
            "OSL Privacy",
            "--app-version",
            "0.9.0",
            "--actual-build-fingerprint",
            OLD_ACTUAL_FINGERPRINT,
        ])
        .args(proof_args())
        .args([
            "--signing-key-file",
            fixture("task3097-trusted-secret.base64").to_str().unwrap(),
        ])
        .output()
        .expect("run old weak build on side A");
    require_success(output, "old weak build")
}

fn produce_current_proof() -> Vec<u8> {
    let output = Command::new(env!("CARGO_BIN_EXE_osl-sign-build-proof"))
        .args(proof_args())
        .args([
            "--signing-key-file",
            fixture("task3097-trusted-secret.base64").to_str().unwrap(),
        ])
        .output()
        .expect("run current build-proof signer");
    require_success(output, "current build-proof signer")
}

fn current_side_answer(proof_path: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_osl-check-build-proof"))
        .args([
            "--proof-file",
            proof_path.to_str().unwrap(),
            "--trusted-public-key-file",
            fixture("task3097-trusted-public.base64").to_str().unwrap(),
            "--build-fingerprint",
            FINGERPRINT,
            "--at-unix-seconds",
            CHECKED_AT,
        ])
        .output()
        .expect("ask current checker on side B");
    String::from_utf8(require_success(output, "current checker"))
        .expect("checker answer is UTF-8")
        .trim()
        .to_owned()
}

#[test]
fn current_side_calls_the_old_weak_build_unknown_and_the_current_build_unmodified() {
    let temporary = TempDir::new();
    let old_side = temporary.0.join("copy-a-old-build");
    let current_side = temporary.0.join("copy-b-current-build");
    fs::create_dir(&old_side).expect("create old-build side A");
    fs::create_dir(&current_side).expect("create current-build side B");
    assert_ne!(
        OLD_ACTUAL_FINGERPRINT, FINGERPRINT,
        "the downgrade fixture must present a fingerprint different from its actual build"
    );

    let old_proof_path = old_side.join("build-proof.json");
    let old_proof = produce_old_proof();
    fs::write(&old_proof_path, &old_proof).expect("write side A old-build proof");
    let old_document: serde_json::Value =
        serde_json::from_slice(&old_proof).expect("old side produced JSON proof");
    assert_eq!(old_document["schemaVersion"], 0);

    let old_answer = current_side_answer(&old_proof_path);
    assert_eq!(
        old_answer, CANNOT_TELL_OLD_BUILD,
        "old-build result should have been refused, got {old_answer:?}"
    );
    let old_unmodified_count = old_answer
        .to_ascii_lowercase()
        .matches("unmodified")
        .count();
    assert_eq!(
        old_unmodified_count, 0,
        "old-build wording made a clean-build claim: {old_answer}"
    );

    let current_proof_path = current_side.join("build-proof.json");
    let current_proof = produce_current_proof();
    fs::write(&current_proof_path, &current_proof).expect("write side B current-build proof");
    let current_document: serde_json::Value =
        serde_json::from_slice(&current_proof).expect("current side produced JSON proof");
    assert_eq!(current_document["schemaVersion"], 1);

    let current_answer = current_side_answer(&current_proof_path);
    assert_eq!(current_answer, "unmodified");

    println!(
        "TASK3219_OLD_SIDE copy=A schema_version=0 weak_check_field_count=2 weak_check_fields=name,version actual_fingerprint={OLD_ACTUAL_FINGERPRINT} claimed_fingerprint={FINGERPRINT} fingerprints_different=true"
    );
    println!(
        "TASK3219_OLD_BUILD_ANSWER copy=B reason=old-build wording=\"{old_answer}\" forbidden_word_unmodified_count={old_unmodified_count}"
    );
    println!(
        "TASK3219_CURRENT_BUILD_ANSWER copy=B schema_version=1 answer={current_answer} unmodified_answer_count=1"
    );
}
