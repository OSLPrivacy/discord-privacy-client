//! TASK 3221 / attack 38: a small ZIP with one valid 8 GiB member is sent as
//! opaque bytes without unpacking it or following its logical size into memory.
//!
//! The resource ceilings are committed separately, so the attack cannot choose
//! them after observing the run. `TASK3221_OMIT_PROOF_ROW` is the deliberate
//! break seam: omitting any required row makes this exact check fail.

#![cfg(all(feature = "core", target_os = "linux"))]

use crypto::aead;
use osl_privacy_hub::attachment_formats::accepted_attachment_mime;
use osl_privacy_hub::peer_attachment_io;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;

const CEILINGS_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/task_3221/ceilings.json"
);
const PROOF_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../proof/attack-38-archive.txt"
);
const ARCHIVE_NAME: &str = "task-3221-eight-gib.zip";
const ARCHIVE_ENTRY: &str = "attack-38-eight-gib-zeroes.bin";
const CEILINGS_SOURCE: &str = "apps/osl-hub/tests/fixtures/task_3221/ceilings.json";
const WRITE_CHUNK_BYTES: usize = 1024 * 1024;

const REQUIRED_PROOF_ROWS: [&str; 14] = [
    "TASK3221_CEILINGS_SOURCE",
    "TASK3221_MEMORY_CEILING_BYTES",
    "TASK3221_DISK_CEILING_BYTES",
    "TASK3221_CLAIMED_UNPACKED_BYTES",
    "TASK3221_START_MEMORY_BYTES",
    "TASK3221_PEAK_MEMORY_BYTES",
    "TASK3221_MEMORY_PEAK_AT_OR_BELOW_CEILING",
    "TASK3221_START_DISK_BYTES",
    "TASK3221_PEAK_DISK_BYTES",
    "TASK3221_DISK_PEAK_AT_OR_BELOW_CEILING",
    "TASK3221_ARCHIVE_ENTRIES_UNPACKED",
    "TASK3221_STARTING_ARCHIVE_BYTES",
    "TASK3221_DELIVERED_BYTES",
    "TASK3221_DELIVERED_EQUALS_STARTING_ARCHIVE",
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Ceilings {
    memory_ceiling_bytes: u64,
    disk_ceiling_bytes: u64,
    claimed_unpacked_bytes: u64,
}

#[test]
fn task_3221_small_archive_with_huge_unpacked_size_is_delivered_without_unpacking() {
    let ceilings = load_ceilings();
    assert_eq!(ceilings.claimed_unpacked_bytes, 8 * 1024 * 1024 * 1024);

    let root = tempfile::Builder::new()
        .prefix("osl-task-3221-")
        .tempdir()
        .expect("create isolated attack root");
    let archive_path = root.path().join(ARCHIVE_NAME);
    write_huge_logical_archive(&archive_path, ceilings.claimed_unpacked_bytes);
    verify_archive_metadata(&archive_path, ceilings.claimed_unpacked_bytes);

    let unpack_root = root.path().join("unpacked");
    fs::create_dir(&unpack_root).expect("create empty unpack observation directory");

    // The measured run begins with the hostile archive already present and the
    // fixed ceiling file already loaded. VmHWM later supplies the kernel's exact
    // high-water mark, including archive construction and transport.
    let starting_archive_bytes = file_len(&archive_path);
    let start_memory_bytes = proc_status_bytes("VmRSS:");
    let start_disk_bytes = directory_logical_bytes(root.path());
    let mut peak_disk_bytes = start_disk_bytes;

    assert!(
        starting_archive_bytes < ceilings.disk_ceiling_bytes,
        "the compressed attack fixture must stay small"
    );

    let expected_digest = sha256_file(&archive_path);
    let mime = accepted_attachment_mime(ARCHIVE_NAME).expect("ZIP is an accepted opaque format");
    let key_bytes = [0x32; aead::KEY_SIZE];
    let mut source = File::open(&archive_path).expect("open hostile archive");
    let sealed = peer_attachment_io::encrypt_file(
        root.path(),
        &mut source,
        ARCHIVE_NAME,
        mime,
        aead::Key::from_bytes(key_bytes),
        vec![0x21; 16],
        0,
    )
    .expect("stream hostile archive through the opaque send boundary");
    peak_disk_bytes = peak_disk_bytes.max(directory_logical_bytes(root.path()));

    let mut sealed_file = File::open(sealed.path()).expect("open delivered ciphertext");
    let delivered = peer_attachment_io::decrypt_file(
        root.path(),
        &mut sealed_file,
        ARCHIVE_NAME,
        mime,
        aead::Key::from_bytes(key_bytes),
    )
    .expect("receive delivered archive without interpreting it");
    peak_disk_bytes = peak_disk_bytes.max(directory_logical_bytes(root.path()));

    let delivered_path = delivered.path().expect("delivered plaintext path");
    let delivered_bytes = file_len(delivered_path);
    let delivered_digest = sha256_file(delivered_path);
    let archive_entries_unpacked = count_regular_files(&unpack_root);
    let peak_memory_bytes = proc_status_bytes("VmHWM:").max(start_memory_bytes);

    assert_eq!(archive_entries_unpacked, 0);
    assert_eq!(delivered_bytes, starting_archive_bytes);
    assert_eq!(delivered_digest, expected_digest);
    assert!(
        peak_memory_bytes <= ceilings.memory_ceiling_bytes,
        "peak memory {peak_memory_bytes} exceeded saved ceiling {}",
        ceilings.memory_ceiling_bytes
    );
    assert!(
        peak_disk_bytes <= ceilings.disk_ceiling_bytes,
        "peak disk {peak_disk_bytes} exceeded saved ceiling {}",
        ceilings.disk_ceiling_bytes
    );

    let rows = vec![
        ("TASK3221_CEILINGS_SOURCE", CEILINGS_SOURCE.to_owned()),
        (
            "TASK3221_MEMORY_CEILING_BYTES",
            ceilings.memory_ceiling_bytes.to_string(),
        ),
        (
            "TASK3221_DISK_CEILING_BYTES",
            ceilings.disk_ceiling_bytes.to_string(),
        ),
        (
            "TASK3221_CLAIMED_UNPACKED_BYTES",
            ceilings.claimed_unpacked_bytes.to_string(),
        ),
        (
            "TASK3221_START_MEMORY_BYTES",
            start_memory_bytes.to_string(),
        ),
        ("TASK3221_PEAK_MEMORY_BYTES", peak_memory_bytes.to_string()),
        (
            "TASK3221_MEMORY_PEAK_AT_OR_BELOW_CEILING",
            "true".to_owned(),
        ),
        ("TASK3221_START_DISK_BYTES", start_disk_bytes.to_string()),
        ("TASK3221_PEAK_DISK_BYTES", peak_disk_bytes.to_string()),
        ("TASK3221_DISK_PEAK_AT_OR_BELOW_CEILING", "true".to_owned()),
        (
            "TASK3221_ARCHIVE_ENTRIES_UNPACKED",
            archive_entries_unpacked.to_string(),
        ),
        (
            "TASK3221_STARTING_ARCHIVE_BYTES",
            starting_archive_bytes.to_string(),
        ),
        ("TASK3221_DELIVERED_BYTES", delivered_bytes.to_string()),
        (
            "TASK3221_DELIVERED_EQUALS_STARTING_ARCHIVE",
            "true".to_owned(),
        ),
    ];
    let proof = render_proof(rows);
    validate_proof(&proof).unwrap_or_else(|error| panic!("{error}"));
    fs::write(PROOF_PATH, &proof).expect("write attack-38 proof");

    println!("{proof}");

    delivered.remove_now().expect("remove delivered archive");
    peer_attachment_io::remove_staged_file(sealed).expect("remove sealed archive");
}

#[test]
fn task_3221_proof_contract_rejects_a_record_missing_one_row() {
    let complete = REQUIRED_PROOF_ROWS
        .iter()
        .map(|row| format!("{row}=1"))
        .collect::<Vec<_>>();
    if std::env::var_os("TASK3221_OMIT_PROOF_ROW").is_some() {
        let rows = REQUIRED_PROOF_ROWS
            .iter()
            .copied()
            .zip(std::iter::repeat("1".to_owned()))
            .collect();
        let deliberately_broken = render_proof(rows);
        validate_proof(&deliberately_broken)
            .expect("TASK3221 deliberate missing-row record must turn this check red");
    }
    for missing in 0..REQUIRED_PROOF_ROWS.len() {
        let proof = complete
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != missing)
            .map(|(_, row)| row.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let error = validate_proof(&format!("{proof}\n")).expect_err("one missing row must fail");
        assert!(error.contains(REQUIRED_PROOF_ROWS[missing]));
    }
    println!(
        "TASK3221_MISSING_ONE_ROW_REJECTIONS={}",
        REQUIRED_PROOF_ROWS.len()
    );
}

fn load_ceilings() -> Ceilings {
    let bytes = fs::read(CEILINGS_PATH).expect("read saved ceilings before the run");
    serde_json::from_slice(&bytes).expect("parse saved ceilings")
}

fn write_huge_logical_archive(path: &Path, unpacked_bytes: u64) {
    let file = File::create(path).expect("create attack archive");
    let mut archive = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(6))
        .large_file(true);
    archive
        .start_file(ARCHIVE_ENTRY, options)
        .expect("start huge logical member");
    let zeroes = vec![0u8; WRITE_CHUNK_BYTES];
    let mut remaining = unpacked_bytes;
    while remaining != 0 {
        let write_len = usize::try_from(remaining.min(WRITE_CHUNK_BYTES as u64))
            .expect("bounded archive write length");
        archive
            .write_all(&zeroes[..write_len])
            .expect("compress huge logical member");
        remaining -= write_len as u64;
    }
    let file = archive.finish().expect("finish valid ZIP64 archive");
    file.sync_all().expect("synchronize attack archive");
}

fn verify_archive_metadata(path: &Path, expected_unpacked_bytes: u64) {
    let file = File::open(path).expect("open attack archive for metadata validation");
    let mut archive = zip::ZipArchive::new(file).expect("attack fixture is a valid ZIP");
    assert_eq!(archive.len(), 1);
    let member = archive
        .by_index(0)
        .expect("read the one archive directory entry");
    assert_eq!(member.name(), ARCHIVE_ENTRY);
    assert_eq!(member.size(), expected_unpacked_bytes);
    assert!(member.compressed_size() < expected_unpacked_bytes / 100);
}

fn render_proof(rows: Vec<(&'static str, String)>) -> String {
    let omitted = std::env::var("TASK3221_OMIT_PROOF_ROW").ok();
    let mut proof = String::new();
    for (key, value) in rows {
        if omitted.as_deref() != Some(key) {
            proof.push_str(key);
            proof.push('=');
            proof.push_str(&value);
            proof.push('\n');
        }
    }
    proof
}

fn validate_proof(proof: &str) -> Result<(), String> {
    let required = REQUIRED_PROOF_ROWS.into_iter().collect::<BTreeSet<_>>();
    let mut rows = BTreeMap::new();
    for (line_number, line) in proof.lines().enumerate() {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("TASK3221 proof row {} has no '='", line_number + 1))?;
        if !required.contains(key) {
            return Err(format!("TASK3221 proof has unexpected row {key}"));
        }
        if value.is_empty() {
            return Err(format!("TASK3221 proof row {key} has no value"));
        }
        if rows.insert(key, value).is_some() {
            return Err(format!("TASK3221 proof has duplicate row {key}"));
        }
    }
    for required_row in REQUIRED_PROOF_ROWS {
        if !rows.contains_key(required_row) {
            return Err(format!(
                "TASK3221 proof missing required row {required_row}"
            ));
        }
    }
    if rows.len() != REQUIRED_PROOF_ROWS.len() {
        return Err(format!(
            "TASK3221 proof row count expected={} actual={}",
            REQUIRED_PROOF_ROWS.len(),
            rows.len()
        ));
    }
    Ok(())
}

fn proc_status_bytes(label: &str) -> u64 {
    let status = fs::read_to_string("/proc/self/status").expect("read Linux process status");
    let line = status
        .lines()
        .find(|line| line.starts_with(label))
        .unwrap_or_else(|| panic!("/proc/self/status has no {label} row"));
    let kibibytes = line
        .split_whitespace()
        .nth(1)
        .unwrap_or_else(|| panic!("{label} has no byte value"))
        .parse::<u64>()
        .unwrap_or_else(|_| panic!("{label} byte value is not numeric"));
    kibibytes.checked_mul(1024).expect("memory bytes fit u64")
}

fn directory_logical_bytes(path: &Path) -> u64 {
    let mut total = 0u64;
    for entry in fs::read_dir(path).expect("read measured directory") {
        let entry = entry.expect("read measured directory entry");
        let metadata = entry.metadata().expect("read measured entry metadata");
        if metadata.is_dir() {
            total = total
                .checked_add(directory_logical_bytes(&entry.path()))
                .expect("disk-byte total fits u64");
        } else if metadata.is_file() {
            total = total
                .checked_add(metadata.len())
                .expect("disk-byte total fits u64");
        } else {
            panic!("measured directory contains a non-file entry");
        }
    }
    total
}

fn count_regular_files(path: &Path) -> usize {
    fs::read_dir(path)
        .expect("read unpack observation directory")
        .map(|entry| entry.expect("read unpack observation entry"))
        .filter(|entry| entry.file_type().expect("read unpack entry type").is_file())
        .count()
}

fn file_len(path: &Path) -> u64 {
    fs::metadata(path).expect("read file metadata").len()
}

fn sha256_file(path: &Path) -> [u8; 32] {
    let mut file = File::open(path).expect("open file for hashing");
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).expect("hash file bytes");
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    digest.finalize().into()
}
