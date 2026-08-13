use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use ed25519_dalek::SigningKey;
use task_3096_build_proof::{
    make_build_proof, sign_build_proof, BuildProofInput, SignedBuildProof,
    CANNOT_TELL_MISSING_PROOF, CANNOT_TELL_UNAVAILABLE_PROOF, MODIFIED_BUILD_FINGERPRINT_MISMATCH,
};

const ORIGINAL: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const CHANGED: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
const CHECKED_AT: &str = "1787000000";
const DEVICE: &str = "device:qa-laptop-3098";

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("osl-task-3098-{}-{sequence}", std::process::id()));
        fs::create_dir(&path).expect("create task 3098 temporary directory");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn signed_proof(fingerprint: &str, seed: [u8; 32]) -> SignedBuildProof {
    let proof = make_build_proof(BuildProofInput {
        build_fingerprint: fingerprint.to_owned(),
        device_id: DEVICE.to_owned(),
        person_id: "person:liam-3098".to_owned(),
        made_at_unix_seconds: 1_786_000_000,
        stops_counting_at_unix_seconds: 1_788_000_000,
    })
    .expect("valid proof");
    sign_build_proof(proof, seed).expect("sign proof")
}

fn write_fixture(directory: &Path, fingerprint: &str) -> (PathBuf, PathBuf) {
    let seed = [23; 32];
    let signing_key = SigningKey::from_bytes(&seed);
    let proof_path = directory.join(format!("{fingerprint}.proof.json"));
    let key_path = directory.join("trusted-public.base64");
    fs::write(
        &proof_path,
        serde_json::to_vec_pretty(&signed_proof(fingerprint, seed)).unwrap(),
    )
    .expect("write signed proof");
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    fs::write(
        &key_path,
        STANDARD.encode(signing_key.verifying_key().to_bytes()),
    )
    .expect("write trusted public key");
    (proof_path, key_path)
}

fn invoke(proof: Option<&Path>, key: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_osl-check-build-proof"));
    command.args([
        "--build-fingerprint",
        ORIGINAL,
        "--device-id",
        DEVICE,
        "--at-unix-seconds",
        CHECKED_AT,
    ]);
    if let Some(path) = proof {
        command.args(["--proof-file", path.to_str().unwrap()]);
    }
    if let Some(path) = key {
        command.args(["--trusted-public-key-file", path.to_str().unwrap()]);
    }
    command.output().expect("run build proof checker")
}

fn answer(output: &Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone())
        .expect("UTF-8 answer")
        .trim()
        .to_owned()
}

#[test]
fn checker_returns_all_three_required_answers() {
    let temporary = TempDir::new();
    let (good_proof, trusted_key) = write_fixture(&temporary.0, ORIGINAL);
    let (changed_proof, _) = write_fixture(&temporary.0, CHANGED);

    let good_answer = answer(&invoke(Some(&good_proof), Some(&trusted_key)));
    let changed_answer = answer(&invoke(Some(&changed_proof), Some(&trusted_key)));
    let missing_answer = answer(&invoke(None, None));

    assert_eq!(good_answer, "unmodified");
    assert_eq!(changed_answer, MODIFIED_BUILD_FINGERPRINT_MISMATCH);
    assert_eq!(missing_answer, CANNOT_TELL_MISSING_PROOF);

    println!("TASK3098_GOOD proof_count=1 answer={good_answer}");
    println!("TASK3098_CHANGED changed_build_fingerprint_count=1 answer={changed_answer}");
    println!("TASK3098_MISSING proof_count=0 answer={missing_answer}");
}

#[test]
fn uncertainty_never_becomes_a_modification_claim() {
    let temporary = TempDir::new();
    let (proof_path, trusted_key) = write_fixture(&temporary.0, ORIGINAL);

    let mut tampered: serde_json::Value =
        serde_json::from_slice(&fs::read(&proof_path).unwrap()).unwrap();
    tampered["proof"]["buildFingerprint"] = CHANGED.into();
    let tampered_path = temporary.0.join("tampered.proof.json");
    fs::write(
        &tampered_path,
        serde_json::to_vec_pretty(&tampered).unwrap(),
    )
    .unwrap();

    let missing_path = temporary.0.join("does-not-exist.proof.json");
    assert_eq!(
        answer(&invoke(Some(&tampered_path), Some(&trusted_key))),
        CANNOT_TELL_UNAVAILABLE_PROOF
    );
    assert_eq!(
        answer(&invoke(Some(&missing_path), Some(&trusted_key))),
        CANNOT_TELL_MISSING_PROOF
    );
    assert_eq!(
        answer(&invoke(Some(&proof_path), None)),
        CANNOT_TELL_UNAVAILABLE_PROOF
    );

    println!(
        "TASK3098_UNCERTAIN tampered=1 missing_file=1 missing_trust_root=1 answer=cannot_tell"
    );
}
