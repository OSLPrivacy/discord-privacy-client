//! Independent recovery reader for TASK 6573 personal archives.
//!
//! This package deliberately has no dependency on `osl-hub` or its exporter.
//! It authenticates each independently sealed class, reports every byte it can
//! recover, and keeps going when one class is absent or damaged.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

const FORMAT: &str = "osl-personal-archive-v1";
const KDF_ALGORITHM: &str = "sha256-iterated-v1";
const KDF_ROUNDS: u32 = 120_000;
const SALT_BYTES: usize = 16;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KdfSpec {
    algorithm: String,
    rounds: u32,
    salt: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SealedPart {
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArchiveEnvelope {
    format: String,
    kdf: KdfSpec,
    metadata: SealedPart,
    classes: BTreeMap<String, SealedPart>,
}

fn main() {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.len() != 2 {
        eprintln!("usage: task-6573-archive-reader ARCHIVE PASSPHRASE");
        std::process::exit(2);
    }
    if let Err(error) = read_archive(Path::new(&arguments[0]), &arguments[1].to_string_lossy()) {
        eprintln!("TASK6573_READER_FAIL reason={error}");
        std::process::exit(1);
    }
}

fn read_archive(path: &Path, passphrase: &str) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|error| format!("archive unreadable: {error}"))?;
    let envelope: ArchiveEnvelope =
        serde_json::from_slice(&bytes).map_err(|error| format!("envelope invalid: {error}"))?;
    if envelope.format != FORMAT
        || envelope.kdf.algorithm != KDF_ALGORITHM
        || envelope.kdf.rounds != KDF_ROUNDS
        || envelope.kdf.salt.len() != SALT_BYTES
    {
        return Err("format or KDF parameters invalid".to_string());
    }
    let key = derive_key(passphrase, &envelope.kdf.salt);
    // Metadata is the authority for exclusions and dependencies; unlike a
    // damaged data class, unreadable metadata cannot be safely skipped.
    let metadata = open_part(&key, &[], "metadata", &envelope.metadata)
        .map_err(|error| format!("metadata authentication failed: {error}"))?;
    let metadata_sha = digest(&metadata);
    println!(
        "TASK6573_READER_METADATA plaintext_bytes={} sha256={metadata_sha}",
        metadata.len()
    );

    let mut recovered_classes = 0usize;
    let mut recovered_bytes = 0usize;
    let mut unreadable = 0usize;
    for (class, part) in &envelope.classes {
        match open_part(&key, &envelope.kdf.salt, class, part) {
            Ok(plaintext) => {
                recovered_classes += 1;
                recovered_bytes += plaintext.len();
                println!(
                    "TASK6573_READER_CLASS class={class} plaintext_bytes={} sha256={}",
                    plaintext.len(),
                    digest(&plaintext)
                );
            }
            Err(error) => {
                unreadable += 1;
                println!("TASK6573_READER_UNREADABLE class={class} reason={error}");
            }
        }
    }
    println!(
        "TASK6573_READER_SUMMARY recovered_classes={recovered_classes} recovered_bytes={recovered_bytes} unreadable_classes={unreadable}"
    );
    Ok(())
}

fn derive_key(passphrase: &str, salt: &[u8]) -> crypto::aead::Key {
    let mut digest_bytes: [u8; 32] = Sha256::new()
        .chain_update(b"osl-personal-archive-passphrase-v1\0")
        .chain_update(salt)
        .chain_update(passphrase.as_bytes())
        .finalize()
        .into();
    for round in 1..KDF_ROUNDS {
        digest_bytes = Sha256::new()
            .chain_update(b"osl-personal-archive-passphrase-v1\0")
            .chain_update(digest_bytes)
            .chain_update(salt)
            .chain_update(round.to_le_bytes())
            .chain_update(passphrase.as_bytes())
            .finalize()
            .into();
    }
    crypto::aead::Key::from_bytes(digest_bytes)
}

fn open_part(
    key: &crypto::aead::Key,
    salt: &[u8],
    name: &str,
    part: &SealedPart,
) -> Result<Vec<u8>, String> {
    let nonce_bytes: [u8; crypto::aead::NONCE_SIZE] = part
        .nonce
        .as_slice()
        .try_into()
        .map_err(|_| "nonce bounds".to_string())?;
    crypto::aead::open(
        key,
        &crypto::aead::Nonce::from_bytes(nonce_bytes),
        &associated_data(salt, name),
        &part.ciphertext,
    )
    .map_err(|_| "authentication failed".to_string())
}

fn associated_data(salt: &[u8], name: &str) -> Vec<u8> {
    let mut ad = Vec::with_capacity(FORMAT.len() + salt.len() + name.len() + 2);
    ad.extend_from_slice(FORMAT.as_bytes());
    ad.push(0);
    ad.extend_from_slice(salt);
    ad.push(0);
    ad.extend_from_slice(name.as_bytes());
    ad
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
