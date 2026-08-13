//! Clean-profile reference reader for OSL-EXPORT-1.
//!
//! This package has no dependency on `osl-hub`, `ipc`, `store`, `crypto`, or
//! any production exporter module. It implements the published document using
//! only general-purpose cryptographic crates.

use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use hkdf::Hkdf;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const MAGIC: &[u8; 8] = b"OSLXPORT";
const INFO: &str = "org.openstandardlibraries.account-export.archive-key.v1";
const BLOCK_BYTES: usize = 65_536;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Kdf {
    algorithm: String,
    salt_base64: String,
    info: String,
    output_bytes: u16,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Header {
    format: String,
    version: u16,
    archive_id: String,
    account_id_hash_sha256: String,
    kdf: Kdf,
    aead: String,
    nonce_prefix_base64: String,
    nonce_construction: String,
    block_plaintext_bytes: u32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct KeyFile {
    format: String,
    version: u16,
    archive_id: String,
    secret_base64: String,
    secret_sha256: String,
    warning: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    format: String,
    version: u16,
    account_id: String,
    classes: BTreeMap<String, u64>,
    objects: Vec<Object>,
    blocks: Vec<Entry>,
    total_plaintext_bytes: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Object {
    class: String,
    id: String,
    byte_count: u64,
    sha256: String,
    first_block: u64,
    block_count: u64,
    metadata: BTreeMap<String, String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    index: u64,
    class: String,
    object_id: String,
    object_offset: u64,
    plaintext_bytes: u32,
    sha256: String,
    final_block_for_object: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OracleObject {
    byte_count: u64,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Oracle {
    account_id: String,
    objects: BTreeMap<String, OracleObject>,
}

fn main() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|arg| arg == "--compare") {
        if args.len() != 5 {
            eprintln!(
                "usage: osl-export-clean-reader --compare ARCHIVE KEY OTHER_ARCHIVE OTHER_KEY"
            );
            std::process::exit(2);
        }
        match compare(
            Path::new(&args[1]),
            Path::new(&args[2]),
            Path::new(&args[3]),
            Path::new(&args[4]),
        ) {
            Ok(()) => println!("exports use distinct key material and nonce prefixes"),
            Err(error) => {
                eprintln!("integrity failure: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if args.len() != 2 && args.len() != 3 {
        eprintln!("usage: osl-export-clean-reader ARCHIVE KEY [ORACLE]");
        std::process::exit(2);
    }
    let oracle = if args.len() == 3 {
        match std::fs::read(&args[2])
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Oracle>(&bytes).ok())
        {
            Some(value) => Some(value),
            None => {
                eprintln!("integrity failure: oracle unavailable or undocumented field");
                std::process::exit(1);
            }
        }
    } else {
        None
    };
    match verify(Path::new(&args[0]), Path::new(&args[1]), oracle.as_ref()) {
        Ok(report) => println!("{}", serde_json::to_string(&report).unwrap()),
        Err(error) => {
            eprintln!("integrity failure: {error}");
            std::process::exit(1);
        }
    }
}

fn verify(
    archive_path: &Path,
    key_path: &Path,
    oracle: Option<&Oracle>,
) -> Result<serde_json::Value, String> {
    let archive = std::fs::read(archive_path).map_err(|_| "archive unreadable")?;
    let key_bytes = std::fs::read(key_path).map_err(|_| "unavailable key")?;
    let key_file: KeyFile = serde_json::from_slice(&key_bytes)
        .map_err(|_| "undocumented or missing required key field")?;
    let mut cursor = Cursor::new(&archive);
    if cursor.take(8)? != MAGIC {
        return Err("archive magic".into());
    }
    if cursor.u16()? != 1 {
        return Err("archive version".into());
    }
    let header_len = cursor.u32()? as usize;
    if !(1..=16_384).contains(&header_len) {
        return Err("header length".into());
    }
    let header_bytes = cursor.take(header_len)?.to_vec();
    let header: Header = serde_json::from_slice(&header_bytes)
        .map_err(|_| "undocumented or missing required header field")?;
    validate_parameters(&header, &key_file)?;
    let salt = decode::<32>(&header.kdf.salt_base64, "salt")?;
    let prefix = decode::<16>(&header.nonce_prefix_base64, "nonce prefix")?;
    let secret = decode::<32>(&key_file.secret_base64, "secret")?;
    if sha(&secret) != key_file.secret_sha256 {
        return Err("key checksum".into());
    }
    let hk = Hkdf::<Sha256>::new(Some(&salt), &secret);
    let mut derived = [0u8; 32];
    hk.expand(INFO.as_bytes(), &mut derived)
        .map_err(|_| "KDF")?;
    let cipher = XChaCha20Poly1305::new((&derived).into());
    let frame_count = cursor.u64()?;
    if frame_count == 0 || frame_count > 20_000_000 {
        return Err("frame count".into());
    }
    let header_hash = Sha256::digest(&header_bytes);
    let mut verified = BTreeMap::<u64, (u8, Vec<u8>)>::new();
    for expected in 0..frame_count {
        let index = cursor.u64()?;
        let kind = cursor.u8()?;
        let plain_len = cursor.u32()?;
        let cipher_len = cursor.u32()? as usize;
        if index != expected {
            return Err("reordered authenticated block".into());
        }
        if kind > 1 {
            return Err("block kind".into());
        }
        let maximum = if kind == 0 {
            512 * 1024 * 1024
        } else {
            BLOCK_BYTES
        };
        if plain_len as usize > maximum || cipher_len != plain_len as usize + 16 {
            return Err("block length".into());
        }
        let ciphertext = cursor.take(cipher_len)?;
        let mut nonce = [0u8; 24];
        nonce[..16].copy_from_slice(&prefix);
        nonce[16..].copy_from_slice(&index.to_be_bytes());
        let mut aad = Vec::new();
        aad.extend_from_slice(&header_hash);
        aad.extend_from_slice(&index.to_be_bytes());
        aad.push(kind);
        aad.extend_from_slice(&plain_len.to_be_bytes());
        let plaintext = cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| "block authentication")?;
        verified.insert(index, (kind, plaintext));
    }
    if !cursor.finished() {
        return Err("trailing bytes".into());
    }
    let manifest: Manifest = serde_json::from_slice(&verified.get(&0).ok_or("missing manifest")?.1)
        .map_err(|_| "undocumented or missing required manifest field")?;
    let mut report = validate_manifest(&header, manifest, &verified, frame_count, oracle)?;
    report["archiveBytes"] = serde_json::json!(archive.len());
    // This is the first point at which any plaintext-derived information is
    // returned to the caller or printed by main.
    Ok(report)
}

fn compare(a: &Path, ak: &Path, b: &Path, bk: &Path) -> Result<(), String> {
    // Fully validate both before examining their non-secret archive parameters.
    verify(a, ak, None)?;
    verify(b, bk, None)?;
    let key_a: KeyFile = serde_json::from_slice(&std::fs::read(ak).map_err(|_| "unavailable key")?)
        .map_err(|_| "key schema")?;
    let key_b: KeyFile = serde_json::from_slice(&std::fs::read(bk).map_err(|_| "unavailable key")?)
        .map_err(|_| "key schema")?;
    let head = |path: &Path| -> Result<Header, String> {
        let bytes = std::fs::read(path).map_err(|_| "archive unreadable")?;
        let mut cursor = Cursor::new(&bytes);
        cursor.take(10)?;
        let length = cursor.u32()? as usize;
        serde_json::from_slice(cursor.take(length)?).map_err(|_| "header schema".into())
    };
    let header_a = head(a)?;
    let header_b = head(b)?;
    if key_a.secret_base64 == key_b.secret_base64
        && header_a.kdf.salt_base64 == header_b.kdf.salt_base64
        && header_a.nonce_prefix_base64 == header_b.nonce_prefix_base64
    {
        return Err("nonce reuse".into());
    }
    Ok(())
}

fn validate_parameters(header: &Header, key: &KeyFile) -> Result<(), String> {
    if header.format != "OSL-EXPORT-1"
        || header.version != 1
        || header.kdf.algorithm != "HKDF-SHA256"
        || header.kdf.info != INFO
        || header.kdf.output_bytes != 32
        || header.aead != "XChaCha20-Poly1305-IETF"
        || header.nonce_construction != "16-byte random prefix || uint64-big-endian block index"
        || header.block_plaintext_bytes != BLOCK_BYTES as u32
        || key.format != "OSL-EXPORT-KEY-1"
        || key.version != 1
        || key.archive_id != header.archive_id
        || key.warning
            != "OSL cannot recover your export key. Save and verify it before leaving this screen."
    {
        return Err("cryptographic parameters".into());
    }
    Ok(())
}

fn validate_manifest(
    header: &Header,
    manifest: Manifest,
    verified: &BTreeMap<u64, (u8, Vec<u8>)>,
    count: u64,
    oracle: Option<&Oracle>,
) -> Result<serde_json::Value, String> {
    if manifest.format != "OSL-EXPORT-1"
        || manifest.version != 1
        || sha(manifest.account_id.as_bytes()) != header.account_id_hash_sha256
        || manifest.blocks.len() as u64 + 1 != count
    {
        return Err("manifest completeness".into());
    }
    let required = [
        "identity_profile",
        "settings",
        "friend_relationships",
        "messages",
        "attachments",
    ];
    if manifest
        .classes
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != required.into_iter().collect()
    {
        return Err("class inventory".into());
    }
    let mut total = 0u64;
    for (position, entry) in manifest.blocks.iter().enumerate() {
        if entry.index != position as u64 + 1 {
            return Err("manifest block order".into());
        }
        let (kind, bytes) = verified.get(&entry.index).ok_or("unread block")?;
        if *kind != 1 || bytes.len() != entry.plaintext_bytes as usize || sha(bytes) != entry.sha256
        {
            return Err("block digest".into());
        }
        total += bytes.len() as u64;
    }
    if total != manifest.total_plaintext_bytes {
        return Err("manifest byte count".into());
    }
    let mut object_hashes = BTreeMap::new();
    let mut object_details = BTreeMap::new();
    let mut message_ids = BTreeSet::new();
    let mut attachment_ids = BTreeSet::new();
    for object in &manifest.objects {
        let entries = manifest
            .blocks
            .iter()
            .filter(|entry| entry.class == object.class && entry.object_id == object.id)
            .collect::<Vec<_>>();
        if entries.len() as u64 != object.block_count
            || entries.first().map(|entry| entry.index) != Some(object.first_block)
            || entries
                .last()
                .is_none_or(|entry| !entry.final_block_for_object)
            || entries[..entries.len().saturating_sub(1)]
                .iter()
                .any(|entry| entry.final_block_for_object)
        {
            return Err("object block set".into());
        }
        let mut bytes = Vec::new();
        for entry in entries {
            if entry.object_offset != bytes.len() as u64 {
                return Err("object offset".into());
            }
            bytes.extend_from_slice(&verified[&entry.index].1);
        }
        if bytes.len() as u64 != object.byte_count || sha(&bytes) != object.sha256 {
            return Err("object digest".into());
        }
        if object.class == "attachments" {
            if !["ownerId", "messageId", "filename", "mimeType"]
                .iter()
                .all(|key| object.metadata.contains_key(*key))
            {
                return Err("undocumented or missing required attachment field".into());
            }
            if object.metadata.get("ownerId") != Some(&manifest.account_id) {
                return Err("foreign owner attachment".into());
            }
            attachment_ids.insert(object.id.clone());
        } else {
            let document: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|_| "document JSON")?;
            if document.get("ownerId").and_then(serde_json::Value::as_str)
                != Some(manifest.account_id.as_str())
            {
                return Err("foreign owner".into());
            }
            if object.class == "messages" {
                message_ids.insert(object.id.clone());
            }
        }
        let key = format!("{}:{}", object.class, object.id);
        object_hashes.insert(key.clone(), object.sha256.clone());
        object_details.insert(
            key,
            serde_json::json!({
                "byteCount": object.byte_count,
                "sha256": object.sha256,
            }),
        );
    }
    for object in &manifest.objects {
        if object.class == "attachments"
            && !message_ids.contains(
                object
                    .metadata
                    .get("messageId")
                    .ok_or("attachment metadata")?,
            )
        {
            return Err(format!(
                "missing item messages:{} referenced by attachment {}",
                object.metadata["messageId"], object.id
            ));
        }
        if object.class != "attachments" {
            let entries = manifest
                .blocks
                .iter()
                .filter(|entry| entry.class == object.class && entry.object_id == object.id);
            let mut bytes = Vec::new();
            for entry in entries {
                bytes.extend_from_slice(&verified[&entry.index].1);
            }
            let value: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|_| "document JSON")?;
            if let Some(ids) = value
                .get("attachmentIds")
                .and_then(serde_json::Value::as_array)
            {
                for id in ids {
                    let id = id.as_str().ok_or("attachment reference")?;
                    if !attachment_ids.contains(id) {
                        return Err(format!("missing item attachments:{id}"));
                    }
                }
            }
        }
    }
    let mut object_counts = required
        .into_iter()
        .map(|name| (name.to_owned(), 0u64))
        .collect::<BTreeMap<_, _>>();
    for object in &manifest.objects {
        *object_counts.entry(object.class.clone()).or_default() += 1;
    }
    if object_counts != manifest.classes {
        return Err("manifest object counts".into());
    }
    if let Some(oracle) = oracle {
        if oracle.account_id != manifest.account_id {
            return Err("oracle foreign owner".into());
        }
        let actual = manifest
            .objects
            .iter()
            .map(|object| (format!("{}:{}", object.class, object.id), object))
            .collect::<BTreeMap<_, _>>();
        let expected_class_counts =
            oracle
                .objects
                .keys()
                .fold(BTreeMap::<&str, u64>::new(), |mut counts, key| {
                    *counts
                        .entry(key.split(':').next().unwrap_or(""))
                        .or_default() += 1;
                    counts
                });
        for (class, expected) in expected_class_counts {
            let found = manifest.classes.get(class).copied().unwrap_or(0);
            if expected > 0 && found == 0 {
                return Err(format!(
                    "oracle missing class {class}: expected {expected}, found {found}"
                ));
            }
        }
        for (key, expected) in &oracle.objects {
            let got = actual
                .get(key)
                .ok_or_else(|| format!("oracle page boundary: missing item {key}"))?;
            if got.byte_count != expected.byte_count {
                return Err(format!(
                    "oracle hash/byte-count mismatch {key}: expected {}, found {}",
                    expected.byte_count, got.byte_count
                ));
            }
            if got.sha256 != expected.sha256 {
                return Err(format!("oracle hash mismatch {key}"));
            }
        }
        for key in actual.keys() {
            if !oracle.objects.contains_key(key) {
                return Err(format!("oracle foreign or undocumented item {key}"));
            }
        }
    }
    Ok(serde_json::json!({
        "format": "OSL-EXPORT-1", "archiveBytes": 0, "authenticatedBlocks": count,
        "plaintextBytes": total, "classes": manifest.classes, "objectHashes": object_hashes,
        "objects": object_details,
    }))
}

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn decode<const N: usize>(value: &str, label: &str) -> Result<[u8; N], String> {
    STANDARD_NO_PAD
        .decode(value)
        .map_err(|_| label.to_owned())?
        .try_into()
        .map_err(|_| label.to_owned())
}
struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    fn take(&mut self, length: usize) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or("truncated archive")?;
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn finished(&self) -> bool {
        self.offset == self.bytes.len()
    }
}
