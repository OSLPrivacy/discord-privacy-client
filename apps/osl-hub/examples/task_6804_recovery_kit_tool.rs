//! TASK 6804 — the independent exporter, and the two real recovery journeys.
//!
//! The check for "load a recovery kit file on both recovery pages" must not
//! write the kit it then loads. A checker that mints its own file proves only
//! that its writer and its reader agree with each other, which is exactly the
//! failure mode a hand-rolled fixture has. So the kit comes from here: a
//! separate program, in a different language from the loader, sharing no state
//! with it, using the product's own primitives —
//!
//!   * `ipc::main_password::set_main_password` mints the **password** recovery
//!     phrase, by writing a real Argon2id password marker for a real account
//!     directory. It is the same call the product makes when somebody sets
//!     their password for the first time.
//!   * `bip39::Mnemonic::from_entropy_in` over 16 bytes of OS randomness mints
//!     the **identity** phrase, and
//!     `password_lifecycle::identity_user_id_for_recovery_phrase` turns it into
//!     the `osl_` id — the same function `import_native_identity_phrase` uses,
//!     so "the exact source identity" is the identity the importer would
//!     actually install.
//!
//! The two journeys are here for the same reason. `derive-identity` is the
//! Restore Account journey's authority, and `reset-password` runs the real
//! `verify_recovery_phrase` → `set_main_password_after_recovery` pair against
//! real account bytes, so "restores the exact source identity through its
//! proper journey" is measured against the product rather than against a stub.
//!
//! `digest` exists so a caller can state, in one number, that a refusal left
//! the account bytes alone.
//!
//! ```text
//! export          --out DIR [--user-id OSL_ID] [--kit-version N] [--name FILE]
//! derive-identity --phrase-file FILE
//! reset-password  --account DIR --phrase-file FILE --new-password PASSWORD
//! digest          --dir DIR
//! ```
//!
//! Every subcommand prints one line of JSON on success and exits 0, or prints
//! one line of JSON with an `error` field and exits 1. No subcommand ever
//! prints a recovery word: `export` writes the words into the kit file and into
//! nothing else, and the journeys take their phrase from a file rather than
//! from a command line, so the words never reach a process table or a shell
//! history either.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use bip39::{Language, Mnemonic};
use sha2::{Digest, Sha256};

/// Mirrors `RECOVERY_KIT_FILE_TYPE` in `recovery-kit-file-6804.ts`.
const KIT_TYPE: &str = "OSL-RECOVERY-KIT";
/// Mirrors `RECOVERY_KIT_FILE_VERSION`.
const KIT_VERSION: u32 = 1;
/// The password the exported account is created with. It is not a secret of the
/// kit: the kit carries phrases, and this is only what the source account's
/// marker was built from so a journey has something to reset away from.
const SOURCE_PASSWORD: &str = "osl-6804-source-password";

#[derive(serde::Serialize)]
struct KitPayload {
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "identityWords")]
    identity_words: Vec<String>,
    #[serde(rename = "passwordWords")]
    password_words: Vec<String>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(bytes);
    let digest = hash.finalize();
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest.iter() {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn words(phrase: &str) -> Vec<String> {
    phrase.split_whitespace().map(str::to_owned).collect()
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|value| value == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn required(args: &[String], name: &str) -> Result<String, String> {
    flag(args, name).ok_or_else(|| format!("{name} is required"))
}

/// The kit file, byte for byte: one header line, then the payload, and nothing
/// after it. The digest covers the payload bytes exactly as written, so a
/// single flipped byte anywhere past the newline fails integrity in the reader.
fn kit_file_bytes(payload: &KitPayload, version: u32) -> Result<Vec<u8>, String> {
    let payload =
        serde_json::to_vec(payload).map_err(|_| "recovery kit payload is unserialisable")?;
    let header = format!("{KIT_TYPE} v{version} sha256:{}\n", sha256_hex(&payload));
    let mut bytes = header.into_bytes();
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}

fn export(args: &[String]) -> Result<String, String> {
    let out = PathBuf::from(required(args, "--out")?);
    let account = out.join("account");
    std::fs::create_dir_all(&account).map_err(|error| format!("create account dir: {error}"))?;

    // The real first-password path: real Argon2id marker, real lockout reset,
    // real file-key installation. Its return value is the password recovery
    // phrase, which is the only time the product ever surfaces it either.
    let password_phrase = ipc::main_password::set_main_password(&account, SOURCE_PASSWORD)?;

    // A fresh identity, from the operating system's randomness.
    let entropy: [u8; 16] = {
        let mut bytes = [0u8; 16];
        let random = crypto::random::random_bytes(16);
        bytes.copy_from_slice(&random);
        bytes
    };
    let identity_phrase = Mnemonic::from_entropy_in(Language::English, &entropy)
        .map(|mnemonic| mnemonic.to_string())
        .map_err(|_| "identity phrase could not be minted".to_owned())?;
    let derived_user_id =
        osl_privacy_hub::password_lifecycle::identity_user_id_for_recovery_phrase(
            &identity_phrase,
        )?;

    // `--user-id` writes a kit that *declares* another account while carrying
    // this one's identity phrase. That is the wrong-identity case, and it is
    // deliberately a well-formed kit: its digest is recomputed, so it fails on
    // the identity comparison and on nothing else.
    let declared_user_id = flag(args, "--user-id").unwrap_or_else(|| derived_user_id.clone());
    let version = match flag(args, "--kit-version") {
        Some(value) => value
            .parse::<u32>()
            .map_err(|_| "--kit-version must be a number".to_owned())?,
        None => KIT_VERSION,
    };
    let name = flag(args, "--name").unwrap_or_else(|| "fresh.oslkit".to_owned());

    let payload = KitPayload {
        user_id: declared_user_id.clone(),
        identity_words: words(&identity_phrase),
        password_words: words(&password_phrase),
    };
    let bytes = kit_file_bytes(&payload, version)?;
    let kit_path = out.join(&name);
    std::fs::write(&kit_path, &bytes).map_err(|error| format!("write kit: {error}"))?;

    // What the caller is allowed to learn. The words are in the file and
    // nowhere else; the caller gets the identity to compare against and the
    // account bytes to prove nothing moved.
    Ok(serde_json::json!({
        "kitPath": kit_path.to_string_lossy(),
        "kitBytes": bytes.len(),
        "kitSha256": sha256_hex(&bytes),
        "declaredUserId": declared_user_id,
        "sourceUserId": derived_user_id,
        "accountDir": account.to_string_lossy(),
        "accountDigest": directory_digest(&account)?,
        "sourcePassword": SOURCE_PASSWORD,
    })
    .to_string())
}

/// The Restore Account journey's authority: which account this phrase is.
fn derive_identity(args: &[String]) -> Result<String, String> {
    let phrase = read_phrase(&required(args, "--phrase-file")?)?;
    let user_id =
        osl_privacy_hub::password_lifecycle::identity_user_id_for_recovery_phrase(&phrase)?;
    Ok(serde_json::json!({ "userId": user_id }).to_string())
}

/// The Forgot Password journey, run for real against real account bytes.
fn reset_password(args: &[String]) -> Result<String, String> {
    let account = PathBuf::from(required(args, "--account")?);
    let phrase = read_phrase(&required(args, "--phrase-file")?)?;
    let new_password = required(args, "--new-password")?;
    let state = ipc::AppState::new();
    let before = directory_digest(&account)?;
    let token = match ipc::main_password::verify_recovery_phrase(&state, &account, &phrase) {
        Ok(token) => token,
        Err(_) => {
            // The verifier's own error carries lockout counters, and on some
            // paths the offending text; neither belongs in this tool's output.
            return Ok(serde_json::json!({
                "ok": false,
                "stage": "verify",
                "accountDigestBefore": before,
                "accountDigestAfter": directory_digest(&account)?,
            })
            .to_string());
        }
    };
    match ipc::main_password::set_main_password_after_recovery(
        &state,
        &account,
        &new_password,
        &token,
    ) {
        Ok(()) => Ok(serde_json::json!({
            "ok": true,
            "stage": "reset",
            "accountDigestBefore": before,
            "accountDigestAfter": directory_digest(&account)?,
        })
        .to_string()),
        Err(_) => Ok(serde_json::json!({
            "ok": false,
            "stage": "reset",
            "accountDigestBefore": before,
            "accountDigestAfter": directory_digest(&account)?,
        })
        .to_string()),
    }
}

/// Prove the new password actually opens the account the reset was run on.
fn verify_password(args: &[String]) -> Result<String, String> {
    let account = PathBuf::from(required(args, "--account")?);
    let password = required(args, "--password")?;
    let opened = ipc::main_password::verify_main_password(&account, &password).is_ok();
    Ok(serde_json::json!({ "opened": opened }).to_string())
}

fn digest(args: &[String]) -> Result<String, String> {
    let dir = PathBuf::from(required(args, "--dir")?);
    Ok(serde_json::json!({ "digest": directory_digest(&dir)? }).to_string())
}

fn read_phrase(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map(|value| value.trim().to_owned())
        .map_err(|_| "phrase file could not be read".to_owned())
}

/// One number for "these account bytes did not move".
///
/// Names and contents both, sorted, so a refusal that added a lockout file or
/// rewrote a marker changes the digest even when every existing byte survived.
fn directory_digest(dir: &Path) -> Result<String, String> {
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    collect(dir, dir, &mut files)?;
    let mut hash = Sha256::new();
    hash.update(b"OSL-6804-ACCOUNT-BYTES-v1");
    for (name, bytes) in &files {
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    Ok(STANDARD.encode(hash.finalize()))
}

fn collect(root: &Path, dir: &Path, into: &mut BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, into)?;
        } else if let Ok(bytes) = std::fs::read(&path) {
            let name = path
                .strip_prefix(root)
                .map(|value| value.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| path.to_string_lossy().into_owned());
            into.insert(name, bytes);
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let outcome = match args.get(1).map(String::as_str) {
        Some("export") => export(&args),
        Some("derive-identity") => derive_identity(&args),
        Some("reset-password") => reset_password(&args),
        Some("verify-password") => verify_password(&args),
        Some("digest") => digest(&args),
        _ => Err(
            "usage: task_6804_recovery_kit_tool export|derive-identity|reset-password|verify-password|digest"
                .to_owned(),
        ),
    };
    match outcome {
        Ok(line) => println!("{line}"),
        Err(error) => {
            println!("{}", serde_json::json!({ "error": error }));
            std::process::exit(1);
        }
    }
}
