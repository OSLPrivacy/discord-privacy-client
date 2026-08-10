use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::SigningKey;
use task_3096_build_proof::{
    make_build_proof, sign_build_proof, BuildProofInput, CANNOT_TELL_EXPIRED_PROOF,
    CANNOT_TELL_MISSING_PROOF, CANNOT_TELL_OLD_BUILD,
};

const FINGERPRINT: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("osl-task-3099-{}-{sequence}", std::process::id()));
        fs::create_dir(&path).expect("create task 3099 temporary directory");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_proof_and_key(directory: &Path) -> (PathBuf, PathBuf) {
    let seed = [29; 32];
    let signing_key = SigningKey::from_bytes(&seed);
    let proof = make_build_proof(BuildProofInput {
        build_fingerprint: FINGERPRINT.to_owned(),
        device_id: "device:qa-laptop-3099".to_owned(),
        person_id: "person:liam-3099".to_owned(),
        made_at_unix_seconds: 1_786_000_000,
        stops_counting_at_unix_seconds: 1_788_000_000,
    })
    .expect("valid proof");
    let signed = sign_build_proof(proof, seed).expect("sign proof");
    let proof_path = directory.join("expired.proof.json");
    let key_path = directory.join("trusted-public.base64");
    fs::write(&proof_path, serde_json::to_vec_pretty(&signed).unwrap()).expect("write proof");
    fs::write(
        &key_path,
        STANDARD.encode(signing_key.verifying_key().to_bytes()),
    )
    .expect("write key");
    (proof_path, key_path)
}

fn invoke(extra_args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_osl-check-build-proof"))
        .args(["--build-fingerprint", FINGERPRINT])
        .args(extra_args)
        .output()
        .expect("run direct build-proof command")
}

fn answer(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("UTF-8 command output")
        .trim()
        .to_owned()
}

#[test]
fn direct_command_returns_plain_cannot_tell_words_for_all_three_cases() {
    let temporary = TempDir::new();
    let (proof_path, key_path) = write_proof_and_key(&temporary.0);
    let mut old_schema: serde_json::Value =
        serde_json::from_slice(&fs::read(&proof_path).unwrap()).unwrap();
    old_schema["schemaVersion"] = 0.into();
    let old_schema_path = temporary.0.join("old-build.proof.json");
    fs::write(
        &old_schema_path,
        serde_json::to_vec_pretty(&old_schema).unwrap(),
    )
    .expect("write old-build proof");

    let missing = answer(invoke(&[]));
    let expired = answer(invoke(&[
        "--proof-file",
        proof_path.to_str().unwrap(),
        "--trusted-public-key-file",
        key_path.to_str().unwrap(),
        "--at-unix-seconds",
        "1788000000",
    ]));
    let old_build = answer(invoke(&[
        "--proof-file",
        old_schema_path.to_str().unwrap(),
        "--trusted-public-key-file",
        key_path.to_str().unwrap(),
        "--at-unix-seconds",
        "1787000000",
    ]));

    assert_eq!(missing, CANNOT_TELL_MISSING_PROOF);
    assert_eq!(expired, CANNOT_TELL_EXPIRED_PROOF);
    assert_eq!(old_build, CANNOT_TELL_OLD_BUILD);

    for wording in [&missing, &expired, &old_build] {
        assert!(
            !wording.to_ascii_lowercase().contains("unmodified"),
            "cannot-tell wording must not claim unmodified: {wording}"
        );
    }

    println!("TASK3099_MISSING_PROOF wording=\"{missing}\"");
    println!("TASK3099_EXPIRED_PROOF wording=\"{expired}\"");
    println!("TASK3099_OLD_BUILD wording=\"{old_build}\"");
    println!("TASK3099_SUMMARY strings=3 forbidden_word_unmodified_count=0");
}
