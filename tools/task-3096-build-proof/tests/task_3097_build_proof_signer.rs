use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::SigningKey;
use serde_json::Value;

const FINGERPRINT: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const DEVICE: &str = "device:qa-laptop-3097";
const PERSON: &str = "person:liam-3097";
const MADE_AT: &str = "1786399230";
const STOPS_AT: &str = "1789077630";

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("osl-task-3097-{}-{sequence}", std::process::id()));
        fs::create_dir(&path).expect("create task 3097 temporary directory");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_keypair(directory: &Path, name: &str, seed: [u8; 32]) -> (PathBuf, PathBuf) {
    let signing = SigningKey::from_bytes(&seed);
    let secret_path = directory.join(format!("{name}.secret.base64"));
    let public_path = directory.join(format!("{name}.public.base64"));
    fs::write(&secret_path, format!("{}\n", STANDARD.encode(seed))).expect("write test secret");
    fs::write(
        &public_path,
        format!("{}\n", STANDARD.encode(signing.verifying_key().to_bytes())),
    )
    .expect("write test public key");
    (secret_path, public_path)
}

fn sign(secret_path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_osl-sign-build-proof"))
        .args([
            "--signing-key-file",
            secret_path.to_str().unwrap(),
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
        ])
        .output()
        .expect("run signer command")
}

fn check(proof_path: &Path, public_path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_osl-check-build-proof-signature"))
        .args([
            "--proof-file",
            proof_path.to_str().unwrap(),
            "--trusted-public-key-file",
            public_path.to_str().unwrap(),
        ])
        .output()
        .expect("run signature check command")
}

#[test]
fn direct_sign_and_check_accept_trusted_key_and_refuse_different_key() {
    let temporary = TempDir::new();
    let (trusted_secret, trusted_public) = write_keypair(&temporary.0, "trusted", [7; 32]);
    let (foreign_secret, _) = write_keypair(&temporary.0, "foreign", [9; 32]);

    let trusted_output = sign(&trusted_secret);
    assert!(
        trusted_output.status.success(),
        "{}",
        String::from_utf8_lossy(&trusted_output.stderr)
    );
    let signed: Value = serde_json::from_slice(&trusted_output.stdout).expect("signed proof JSON");
    assert_eq!(signed["schemaVersion"], 1);
    assert_eq!(signed["proof"]["buildFingerprint"], FINGERPRINT);
    assert_eq!(signed["proof"]["deviceId"], DEVICE);
    assert_eq!(signed["proof"]["personId"], PERSON);
    assert_eq!(
        signed["proof"]["madeAtUnixSeconds"],
        MADE_AT.parse::<u64>().unwrap()
    );
    assert_eq!(
        signed["proof"]["stopsCountingAtUnixSeconds"],
        STOPS_AT.parse::<u64>().unwrap()
    );
    let signature = STANDARD
        .decode(
            signed["signatureBase64"]
                .as_str()
                .expect("signature string"),
        )
        .expect("signature base64");
    assert_eq!(signature.len(), 64);

    let trusted_proof = temporary.0.join("trusted-proof.json");
    fs::write(&trusted_proof, &trusted_output.stdout).expect("write trusted proof");
    let accepted = check(&trusted_proof, &trusted_public);
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&accepted.stdout).trim(),
        "signature valid"
    );

    let foreign_output = sign(&foreign_secret);
    assert!(foreign_output.status.success());
    let foreign_proof = temporary.0.join("foreign-proof.json");
    fs::write(&foreign_proof, foreign_output.stdout).expect("write foreign proof");
    let refused = check(&foreign_proof, &trusted_public);
    assert_eq!(refused.status.code(), Some(1));
    let refusal = String::from_utf8_lossy(&refused.stderr);
    assert!(refusal.contains("bad-signature"), "{refusal}");

    println!("TASK3097_SIGN proof_count=1 bound_value_count=5 signature_bytes=64");
    println!("TASK3097_CHECK accepted=1 output=signature_valid");
    println!("TASK3097_WRONG_KEY refused=1 exit=1 error=bad-signature");
}

#[test]
fn signature_covers_each_of_the_five_values() {
    let temporary = TempDir::new();
    let (trusted_secret, trusted_public) = write_keypair(&temporary.0, "trusted", [11; 32]);
    let output = sign(&trusted_secret);
    assert!(output.status.success());
    let original: Value = serde_json::from_slice(&output.stdout).expect("signed proof JSON");
    let changes = [
        ("buildFingerprint", Value::String("a".repeat(64))),
        ("deviceId", Value::String("device:other".to_owned())),
        ("personId", Value::String("person:other".to_owned())),
        ("madeAtUnixSeconds", Value::from(1_786_399_231_u64)),
        ("stopsCountingAtUnixSeconds", Value::from(1_789_077_631_u64)),
    ];

    let mut refused = 0;
    for (field, replacement) in changes {
        let mut changed = original.clone();
        changed["proof"][field] = replacement;
        let path = temporary.0.join(format!("changed-{field}.json"));
        fs::write(&path, serde_json::to_vec_pretty(&changed).unwrap())
            .expect("write changed proof");
        let output = check(&path, &trusted_public);
        assert_eq!(output.status.code(), Some(1), "changed field {field}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("bad-signature"),
            "changed field {field}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        refused += 1;
    }
    assert_eq!(refused, 5);
    println!("TASK3097_BINDING changed_values_refused=5 error=bad-signature");
}
