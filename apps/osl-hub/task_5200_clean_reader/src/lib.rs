//! Independent, clean-profile reader for the documented OSL account-export v1
//! format. This crate deliberately has no dependency on the production
//! exporter and stages every plaintext byte until the complete archive is
//! authenticated.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chacha20poly1305::{
    aead::{Aead, Payload},
    KeyInit, XChaCha20Poly1305, XNonce,
};
use hkdf::Hkdf;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const MAGIC: &[u8; 8] = b"OSLAX01\0";
const KDF_INFO: &[u8] = b"OSL-ACCOUNT-EXPORT-v1";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    format: String,
    archive_id: String,
    kdf: Kdf,
    aead: AeadParameters,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Kdf {
    name: String,
    hash: String,
    info: String,
    salt_b64: String,
    input_bytes: usize,
    output_bytes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AeadParameters {
    name: String,
    nonce_bytes: usize,
    tag_bytes: usize,
    nonce_construction: String,
    nonce_prefix_b64: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ManifestEntry {
    pub path: String,
    pub class: String,
    pub owner: String,
    pub bytes: usize,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ManifestBlock {
    pub index: u64,
    pub entry_path: String,
    pub entry_offset: usize,
    pub plaintext_bytes: usize,
    pub plaintext_sha256: String,
    pub final_block: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub archive_id: String,
    pub owner: String,
    pub expected_archive_bytes: usize,
    pub total_plaintext_bytes: usize,
    pub entries: Vec<ManifestEntry>,
    pub blocks: Vec<ManifestBlock>,
    pub inventory_classes: Vec<String>,
    pub inventory_item_counts: std::collections::BTreeMap<String, usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyFile {
    format: String,
    archive_id: String,
    key_material_b64: String,
    expected_archive_bytes: usize,
    archive_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedEntry {
    pub manifest: ManifestEntry,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedExport {
    pub archive_id: String,
    pub archive_bytes: usize,
    pub authenticated_blocks: BTreeSet<u64>,
    pub manifest: Manifest,
    pub entries: Vec<VerifiedEntry>,
}

/// Read only the public envelope metadata needed by an ownership oracle to
/// reject repeated nonce material before it asks this reader for plaintext.
/// This does not authenticate the archive and must never be treated as a
/// successful read.
pub fn public_nonce_material(archive: &[u8]) -> Result<(String, String), String> {
    let mut cursor = 0usize;
    if take(archive, &mut cursor, MAGIC.len(), "magic")? != MAGIC {
        return Err(fail("unsupported archive magic"));
    }
    let header_len = u32_at(archive, &mut cursor, "header length")? as usize;
    let header: Header = serde_json::from_slice(take(archive, &mut cursor, header_len, "header")?)
        .map_err(|_| fail("invalid header"))?;
    Ok((header.archive_id, header.aead.nonce_prefix_b64))
}

fn fail(detail: impl AsRef<str>) -> String {
    format!(
        "integrity failure: {} (0 plaintext released)",
        detail.as_ref()
    )
}

fn take<'a>(
    input: &'a [u8],
    cursor: &mut usize,
    n: usize,
    label: &str,
) -> Result<&'a [u8], String> {
    let end = cursor.checked_add(n).ok_or_else(|| fail(label))?;
    let value = input
        .get(*cursor..end)
        .ok_or_else(|| fail(format!("truncated {label}")))?;
    *cursor = end;
    Ok(value)
}

fn u32_at(input: &[u8], cursor: &mut usize, label: &str) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        take(input, cursor, 4, label)?.try_into().unwrap(),
    ))
}

fn nonce(prefix: &[u8], index: u64) -> Result<[u8; 24], String> {
    if prefix.len() != 16 {
        return Err(fail("invalid nonce prefix"));
    }
    let mut out = [0u8; 24];
    out[..16].copy_from_slice(prefix);
    out[16..].copy_from_slice(&index.to_le_bytes());
    Ok(out)
}

fn aad(header_hash: &[u8], index: u64, kind: u8) -> Vec<u8> {
    let mut out = b"OSLAX01-AAD".to_vec();
    out.extend_from_slice(header_hash);
    out.extend_from_slice(&index.to_le_bytes());
    out.push(kind);
    out
}

/// Authenticate and decrypt a complete archive. No entry bytes are returned
/// unless header, manifest, ordering, every data block, hashes, final block and
/// both full-file byte counts agree.
pub fn read_complete(archive: &[u8], key_file: &[u8]) -> Result<VerifiedExport, String> {
    let key_doc: KeyFile =
        serde_json::from_slice(key_file).map_err(|_| fail("unreadable key file"))?;
    if key_doc.format != "osl-account-export-key-v1" {
        return Err(fail("unsupported key format"));
    }
    if archive.len() != key_doc.expected_archive_bytes {
        return Err(fail("archive byte-count mismatch"));
    }
    if format!("{:x}", Sha256::digest(archive)) != key_doc.archive_sha256 {
        return Err(fail("archive hash mismatch"));
    }
    let input_key = STANDARD
        .decode(&key_doc.key_material_b64)
        .map_err(|_| fail("invalid key encoding"))?;
    if input_key.len() != 32 {
        return Err(fail("invalid key length"));
    }

    let mut cursor = 0usize;
    if take(archive, &mut cursor, MAGIC.len(), "magic")? != MAGIC {
        return Err(fail("unsupported archive magic"));
    }
    let header_len = u32_at(archive, &mut cursor, "header length")? as usize;
    let header_bytes = take(archive, &mut cursor, header_len, "header")?;
    let header: Header =
        serde_json::from_slice(header_bytes).map_err(|_| fail("invalid header"))?;
    if header.format != "osl-account-export-v1"
        || header.kdf.name != "HKDF"
        || header.kdf.hash != "SHA-256"
        || header.kdf.info != "OSL-ACCOUNT-EXPORT-v1"
        || header.kdf.input_bytes != 32
        || header.kdf.output_bytes != 32
        || header.aead.name != "XChaCha20-Poly1305"
        || header.aead.nonce_bytes != 24
        || header.aead.tag_bytes != 16
        || header.aead.nonce_construction
            != "16-byte random prefix || little-endian u64 block index"
    {
        return Err(fail(
            "unsupported cryptographic parameters before plaintext",
        ));
    }
    if header.archive_id != key_doc.archive_id {
        return Err(fail("key/archive id mismatch"));
    }
    let salt = STANDARD
        .decode(&header.kdf.salt_b64)
        .map_err(|_| fail("invalid KDF salt"))?;
    let prefix = STANDARD
        .decode(&header.aead.nonce_prefix_b64)
        .map_err(|_| fail("invalid nonce prefix"))?;
    if salt.len() != 16 || prefix.len() != 16 {
        return Err(fail("invalid cryptographic parameter length"));
    }
    let hk = Hkdf::<Sha256>::new(Some(&salt), &input_key);
    let mut derived = [0u8; 32];
    hk.expand(KDF_INFO, &mut derived)
        .map_err(|_| fail("key derivation"))?;
    let cipher = XChaCha20Poly1305::new((&derived).into());
    let header_hash = Sha256::digest(header_bytes);
    let tag_len = u32_at(archive, &mut cursor, "header authenticator length")? as usize;
    let header_tag = take(archive, &mut cursor, tag_len, "header authenticator")?;
    cipher
        .decrypt(
            XNonce::from_slice(&nonce(&prefix, u64::MAX)?),
            Payload {
                msg: header_tag,
                aad: &aad(&header_hash, u64::MAX, 2),
            },
        )
        .map_err(|_| fail("header authentication"))?;

    let mut staged_frames: Vec<(u64, u8, Vec<u8>)> = Vec::new();
    let mut expected_index = 0u64;
    while cursor < archive.len() {
        let index = u64::from_le_bytes(
            take(archive, &mut cursor, 8, "block index")?
                .try_into()
                .unwrap(),
        );
        let kind = take(archive, &mut cursor, 1, "block kind")?[0];
        let ct_len = u32_at(archive, &mut cursor, "block length")? as usize;
        let ciphertext = take(archive, &mut cursor, ct_len, "ciphertext block")?;
        if index != expected_index {
            return Err(fail(format!(
                "reordered authenticated block {index}; expected {expected_index}"
            )));
        }
        if (index == 0 && kind != 0) || (index > 0 && kind != 1) {
            return Err(fail(format!("invalid block kind at {index}")));
        }
        let plain = cipher
            .decrypt(
                XNonce::from_slice(&nonce(&prefix, index)?),
                Payload {
                    msg: ciphertext,
                    aad: &aad(&header_hash, index, kind),
                },
            )
            .map_err(|_| fail(format!("authenticated block {index}")))?;
        staged_frames.push((index, kind, plain));
        expected_index += 1;
    }
    if staged_frames.is_empty() {
        return Err(fail("missing manifest block"));
    }
    let manifest: Manifest =
        serde_json::from_slice(&staged_frames[0].2).map_err(|_| fail("manifest parse"))?;
    if manifest.format != "osl-account-export-manifest-v1"
        || manifest.archive_id != header.archive_id
    {
        return Err(fail("manifest/header mismatch"));
    }
    if manifest.expected_archive_bytes != archive.len() {
        return Err(fail("manifest full-read byte count"));
    }
    const REQUIRED_CLASSES: [&str; 8] = [
        "identity_profile",
        "settings",
        "friend_relationships",
        "messages",
        "attachments",
        "app_accounts",
        "whitelist_rules",
        "activity_receipts",
    ];
    let declared_classes = manifest
        .inventory_classes
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if declared_classes != REQUIRED_CLASSES.into_iter().collect::<BTreeSet<_>>()
        || manifest.inventory_classes.len() != REQUIRED_CLASSES.len()
    {
        return Err(fail("production class inventory"));
    }
    if manifest.blocks.len() + 1 != staged_frames.len() {
        return Err(fail("authenticated-block set mismatch"));
    }
    let observed: BTreeSet<u64> = staged_frames.iter().map(|f| f.0).collect();
    let expected: BTreeSet<u64> = std::iter::once(0)
        .chain(manifest.blocks.iter().map(|b| b.index))
        .collect();
    if observed != expected {
        return Err(fail("authenticated-block set mismatch"));
    }
    if manifest.blocks.last().map(|b| b.final_block) != Some(true)
        || manifest.blocks[..manifest.blocks.len().saturating_sub(1)]
            .iter()
            .any(|b| b.final_block)
    {
        return Err(fail("final block marker"));
    }

    let mut assembled = std::collections::BTreeMap::<String, Vec<u8>>::new();
    for (position, block) in manifest.blocks.iter().enumerate() {
        let (index, kind, bytes) = &staged_frames[position + 1];
        if *index != block.index
            || *kind != 1
            || bytes.len() != block.plaintext_bytes
            || format!("{:x}", Sha256::digest(bytes)) != block.plaintext_sha256
        {
            return Err(fail(format!(
                "manifest mismatch for authenticated block {}",
                block.index
            )));
        }
        let target = assembled.entry(block.entry_path.clone()).or_default();
        if target.len() != block.entry_offset {
            return Err(fail(format!("block boundary {}", block.index)));
        }
        target.extend_from_slice(bytes);
    }
    let mut entries = Vec::new();
    let mut entry_paths = BTreeSet::new();
    let mut observed_class_counts = std::collections::BTreeMap::<String, usize>::new();
    for entry in &manifest.entries {
        if entry.owner != manifest.owner {
            return Err(fail(format!("ownership leak in {}", entry.path)));
        }
        if !declared_classes.contains(entry.class.as_str()) {
            return Err(fail(format!("unknown class in {}", entry.path)));
        }
        if !entry_paths.insert(entry.path.clone()) {
            return Err(fail(format!("duplicate entry {}", entry.path)));
        }
        *observed_class_counts
            .entry(entry.class.clone())
            .or_insert(0) += 1;
        let bytes = assembled
            .remove(&entry.path)
            .ok_or_else(|| fail(format!("missing entry {}", entry.path)))?;
        if bytes.len() != entry.bytes || format!("{:x}", Sha256::digest(&bytes)) != entry.sha256 {
            return Err(fail(format!("entry bytes {}", entry.path)));
        }
        entries.push(VerifiedEntry {
            manifest: entry.clone(),
            bytes,
        });
    }
    if !assembled.is_empty()
        || entries.iter().map(|e| e.bytes.len()).sum::<usize>() != manifest.total_plaintext_bytes
    {
        return Err(fail("whole-oracle byte total"));
    }
    if observed_class_counts != manifest.inventory_item_counts
        || REQUIRED_CLASSES
            .into_iter()
            .any(|class| observed_class_counts.get(class).copied().unwrap_or(0) == 0)
    {
        return Err(fail("inventory item counts"));
    }
    Ok(VerifiedExport {
        archive_id: header.archive_id,
        archive_bytes: archive.len(),
        authenticated_blocks: observed,
        manifest,
        entries,
    })
}
