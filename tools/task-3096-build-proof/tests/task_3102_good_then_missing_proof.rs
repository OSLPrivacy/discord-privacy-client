use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::SigningKey;
use task_3096_build_proof::{
    make_build_proof, sign_build_proof, BuildProofInput, SignedBuildProof,
};

const NAMED_BUILD_FINGERPRINT: &str =
    "3102aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CHECKED_AT: &str = "1787000000";

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("osl-task-3102-{}-{sequence}", std::process::id()));
        fs::create_dir(&path).expect("create task 3102 temporary directory");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_good_proof(directory: &Path) -> (PathBuf, PathBuf) {
    let seed = [31; 32];
    let signing_key = SigningKey::from_bytes(&seed);
    let proof = make_build_proof(BuildProofInput {
        build_fingerprint: NAMED_BUILD_FINGERPRINT.to_owned(),
        device_id: "device:qa-laptop-3102".to_owned(),
        person_id: "person:liam-3102".to_owned(),
        made_at_unix_seconds: 1_786_000_000,
        stops_counting_at_unix_seconds: 1_788_000_000,
    })
    .expect("make valid build proof");
    let signed: SignedBuildProof = sign_build_proof(proof, seed).expect("sign build proof");

    let proof_path = directory.join(format!("{NAMED_BUILD_FINGERPRINT}.proof.json"));
    let key_path = directory.join("trusted-public.base64");
    fs::write(&proof_path, serde_json::to_vec_pretty(&signed).unwrap())
        .expect("write good signed proof");
    fs::write(
        &key_path,
        STANDARD.encode(signing_key.verifying_key().to_bytes()),
    )
    .expect("write trusted public key");
    (proof_path, key_path)
}

fn invoke_checker(proof_path: &Path, key_path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_osl-check-build-proof"))
        .args([
            "--proof-file",
            proof_path.to_str().unwrap(),
            "--trusted-public-key-file",
            key_path.to_str().unwrap(),
            "--build-fingerprint",
            NAMED_BUILD_FINGERPRINT,
            "--at-unix-seconds",
            CHECKED_AT,
        ])
        .output()
        .expect("run build proof checker")
}

fn answer(output: &Output) -> String {
    assert!(
        output.status.success(),
        "checker failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone())
        .expect("checker answer is UTF-8")
        .trim()
        .to_owned()
}

#[test]
fn good_proof_is_unmodified_once_then_missing_proof_is_cannot_tell() {
    let temporary = TempDir::new();
    let (proof_path, key_path) = write_good_proof(&temporary.0);

    let first_answer = answer(&invoke_checker(&proof_path, &key_path));
    assert_eq!(first_answer, "unmodified");

    fs::remove_file(&proof_path).expect("delete proof between checker runs");
    assert!(
        !proof_path.exists(),
        "proof must be absent before second run"
    );

    let second_answer = answer(&invoke_checker(&proof_path, &key_path));
    assert_eq!(second_answer, "cannot tell");

    let unmodified_answer_count = [&first_answer, &second_answer]
        .into_iter()
        .filter(|answer| answer.as_str() == "unmodified")
        .count();
    assert_eq!(
        unmodified_answer_count, 1,
        "the two-run sequence must not return unmodified twice"
    );

    println!(
        "TASK3102 named_build_fingerprint={NAMED_BUILD_FINGERPRINT} first_answer={first_answer} proof_after_delete_count={} second_answer={} unmodified_answer_count={unmodified_answer_count}",
        usize::from(proof_path.exists()),
        second_answer.replace(' ', "_")
    );
}
