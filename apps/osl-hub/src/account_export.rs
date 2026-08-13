//! Portable, owner-controlled account exports (`OSL-EXPORT-1`).
//!
//! This module deliberately has no Tauri dependency. The packaged Settings
//! command supplies the two paths chosen by the operating-system dialogs; the
//! format writer and the full readback verifier remain independently testable.

use base64::engine::general_purpose::{STANDARD_NO_PAD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zeroize::Zeroize;

pub const FORMAT_NAME: &str = "OSL-EXPORT-1";
pub const FORMAT_VERSION: u16 = 1;
pub const DEFAULT_BLOCK_PLAINTEXT_BYTES: usize = 64 * 1024;
pub const PAGE_ITEMS: usize = 16;
pub const KEY_WARNING: &str =
    "OSL cannot recover your export key. Save and verify it before leaving this screen.";
pub const INDEPENDENT_COPY_WARNING: &str = "Exports are independent copies. Burn, timers, retention, disconnect, and account deletion cannot remove an archive or key you saved outside OSL.";

/// Three independently maintained inventories. The acceptance check requires
/// exact equality so adding a production class to only one layer fails closed.
pub const SOURCE_OWNERSHIP_INVENTORY: &[&str] = &[
    "identity_profile",
    "settings",
    "friend_relationships",
    "messages",
    "attachments",
];
pub const SCHEMA_OWNERSHIP_INVENTORY: &[&str] = &[
    "identity_profile",
    "settings",
    "friend_relationships",
    "messages",
    "attachments",
];
pub const STORAGE_OWNERSHIP_INVENTORY: &[&str] = &[
    "identity_profile",
    "settings",
    "friend_relationships",
    "messages",
    "attachments",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnedDocument {
    pub class: String,
    pub id: String,
    pub owner_id: String,
    pub fields: Value,
    #[serde(default)]
    pub attachment_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnedAttachment {
    pub id: String,
    pub owner_id: String,
    pub message_id: String,
    pub filename: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountExportSnapshot {
    pub account_id: String,
    pub documents: Vec<OwnedDocument>,
    pub attachments: Vec<OwnedAttachment>,
}

/// A page source must apply the account predicate before returning a row. The
/// collector nevertheless checks every owner again, providing a second-account
/// isolation boundary independent from the storage query.
pub trait AccountExportSource {
    fn signed_in_account_id(&self) -> Result<String, String>;
    fn page_documents(
        &self,
        account_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<OwnedDocument>, String>;
    fn page_attachments(
        &self,
        account_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<OwnedAttachment>, String>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct KdfHeader {
    algorithm: String,
    salt_base64: String,
    info: String,
    output_bytes: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArchiveHeader {
    format: String,
    version: u16,
    archive_id: String,
    account_id_hash_sha256: String,
    kdf: KdfHeader,
    aead: String,
    nonce_prefix_base64: String,
    nonce_construction: String,
    block_plaintext_bytes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportKeyFile {
    pub format: String,
    pub version: u16,
    pub archive_id: String,
    pub secret_base64: String,
    pub secret_sha256: String,
    pub warning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestBlock {
    format: String,
    version: u16,
    account_id: String,
    classes: BTreeMap<String, u64>,
    objects: Vec<ManifestObject>,
    blocks: Vec<ManifestEntry>,
    total_plaintext_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestObject {
    class: String,
    id: String,
    byte_count: u64,
    sha256: String,
    first_block: u64,
    block_count: u64,
    metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestEntry {
    index: u64,
    class: String,
    object_id: String,
    object_offset: u64,
    plaintext_bytes: u32,
    sha256: String,
    final_block_for_object: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportReceipt {
    pub archive_bytes: u64,
    pub key_bytes: u64,
    pub authenticated_blocks: BTreeSet<u64>,
    pub manifest_blocks: BTreeSet<u64>,
    pub class_counts: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostWriteMediaFault {
    None,
    FailedWriteArchive,
    DiskFullKey,
    ShortWriteArchive,
    DeleteKey,
    TruncateArchive,
    CorruptArchive,
    TearFinalBlock,
}

#[derive(Debug, Clone)]
struct PlainBlock {
    index: u64,
    kind: u8,
    bytes: Vec<u8>,
}

const MAGIC: &[u8; 8] = b"OSLXPORT";
const HEADER_LIMIT: usize = 16 * 1024;
const KEY_LIMIT: u64 = 64 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024 * 1024;
const MANIFEST_KIND: u8 = 0;
const DATA_KIND: u8 = 1;
const KDF_INFO: &str = "org.openstandardlibraries.account-export.archive-key.v1";

pub fn verify_inventory_agreement() -> Result<(), String> {
    if SOURCE_OWNERSHIP_INVENTORY != SCHEMA_OWNERSHIP_INVENTORY {
        return Err("source/schema ownership inventory mismatch".to_owned());
    }
    if SOURCE_OWNERSHIP_INVENTORY != STORAGE_OWNERSHIP_INVENTORY {
        return Err("source/storage ownership inventory mismatch".to_owned());
    }
    Ok(())
}

pub fn collect_complete_snapshot<S: AccountExportSource>(
    source: &S,
    reauthorized_account_id: &str,
) -> Result<AccountExportSnapshot, String> {
    verify_inventory_agreement()?;
    let signed_in = source.signed_in_account_id()?;
    if signed_in != reauthorized_account_id {
        return Err("reauthorization did not match the signed-in account".to_owned());
    }
    let mut documents = Vec::new();
    let mut after = None::<String>;
    loop {
        let page = source.page_documents(&signed_in, after.as_deref(), PAGE_ITEMS)?;
        if page.len() > PAGE_ITEMS {
            return Err("document page exceeded the published boundary".to_owned());
        }
        if page.is_empty() {
            break;
        }
        let previous = after.clone();
        for document in page {
            if document.owner_id != signed_in {
                return Err("second-account document ownership leak".to_owned());
            }
            if !SOURCE_OWNERSHIP_INVENTORY.contains(&document.class.as_str())
                || document.class == "attachments"
            {
                return Err(format!("unknown account export class {}", document.class));
            }
            after = Some(document.id.clone());
            documents.push(document);
        }
        if after == previous {
            return Err("document pagination cursor did not advance".to_owned());
        }
    }
    let mut attachments = Vec::new();
    let mut after = None::<String>;
    loop {
        let page = source.page_attachments(&signed_in, after.as_deref(), PAGE_ITEMS)?;
        if page.len() > PAGE_ITEMS {
            return Err("attachment page exceeded the published boundary".to_owned());
        }
        if page.is_empty() {
            break;
        }
        let previous = after.clone();
        for attachment in page {
            if attachment.owner_id != signed_in {
                return Err("second-account attachment ownership leak".to_owned());
            }
            after = Some(attachment.id.clone());
            attachments.push(attachment);
        }
        if after == previous {
            return Err("attachment pagination cursor did not advance".to_owned());
        }
    }
    let attachment_ids = attachments
        .iter()
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();
    let referenced = documents
        .iter()
        .flat_map(|item| item.attachment_ids.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    if referenced != attachment_ids {
        return Err("attachment reference inventory is incomplete".to_owned());
    }
    Ok(AccountExportSnapshot {
        account_id: signed_in,
        documents,
        attachments,
    })
}

/// Write both user-selected files and report success only after a full reopen,
/// read-to-EOF, authentication and manifest comparison of both destinations.
pub fn export_to_user_paths(
    snapshot: &AccountExportSnapshot,
    archive_path: &Path,
    key_path: &Path,
    fault: PostWriteMediaFault,
) -> Result<ExportReceipt, String> {
    export_destinations_are_separated(archive_path, key_path)?;
    validate_snapshot(snapshot)?;
    let (archive, key) = generate_archive(snapshot)?;
    let archive_len = archive.len() as u64;
    let key_bytes = serde_json::to_vec_pretty(&key)
        .map_err(|_| "export key could not be encoded".to_owned())?;
    let key_len = key_bytes.len() as u64;

    write_destination(archive_path, &archive, "archive")?;
    if let Err(error) = write_destination(key_path, &key_bytes, "key") {
        let _ = fs::remove_file(archive_path);
        return Err(error);
    }
    inject_media_fault(archive_path, key_path, fault)?;

    let result = (|| {
        let saved_key_bytes = read_entire_file(key_path, KEY_LIMIT, "saved key")?;
        if saved_key_bytes.len() as u64 != key_len {
            return Err("saved key full-readback byte count mismatch".to_owned());
        }
        let saved_key: ExportKeyFile = serde_json::from_slice(&saved_key_bytes)
            .map_err(|_| "saved key is unreadable".to_owned())?;
        let saved_archive = read_entire_file(archive_path, MAX_ARCHIVE_BYTES, "saved archive")?;
        if saved_archive.len() as u64 != archive_len {
            return Err("saved archive full-readback byte count mismatch".to_owned());
        }
        let verified = decrypt_archive_bytes(&saved_archive, &saved_key)?;
        let mut expected_blocks = verified
            .manifest
            .blocks
            .iter()
            .map(|b| b.index)
            .collect::<BTreeSet<_>>();
        expected_blocks.insert(0);
        if verified.authenticated_blocks != expected_blocks {
            return Err("saved archive authenticated-block set mismatch".to_owned());
        }
        Ok(ExportReceipt {
            archive_bytes: archive_len,
            key_bytes: key_len,
            authenticated_blocks: verified.authenticated_blocks,
            manifest_blocks: expected_blocks,
            class_counts: verified.manifest.classes,
        })
    })();
    if let Err(error) = result {
        // A failed journey must not leave a successful receipt. The files are
        // intentionally retained when readable so the OS/user may recover
        // them; they are never represented as verified. Fault-injection runs
        // retain the precise storage fault in the user-boundary diagnostic.
        return if fault == PostWriteMediaFault::None {
            Err(error)
        } else {
            Err(format!("injected storage fault {fault:?}: {error}"))
        };
    }
    result
}

fn validate_snapshot(snapshot: &AccountExportSnapshot) -> Result<(), String> {
    if snapshot.account_id.is_empty() || snapshot.account_id.len() > 256 {
        return Err("account export owner is invalid".to_owned());
    }
    let mut ids = BTreeSet::new();
    for item in &snapshot.documents {
        if item.owner_id != snapshot.account_id {
            return Err("second-account document ownership leak".to_owned());
        }
        if !ids.insert((item.class.clone(), item.id.clone())) {
            return Err("duplicate account export object".to_owned());
        }
    }
    let mut attachment_ids = BTreeSet::new();
    for item in &snapshot.attachments {
        if item.owner_id != snapshot.account_id {
            return Err("second-account attachment ownership leak".to_owned());
        }
        if !attachment_ids.insert(item.id.clone()) {
            return Err("duplicate account export attachment".to_owned());
        }
    }
    let referenced = snapshot
        .documents
        .iter()
        .flat_map(|item| item.attachment_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    if referenced != attachment_ids {
        return Err("attachment reference inventory is incomplete".to_owned());
    }
    Ok(())
}

fn generate_archive(snapshot: &AccountExportSnapshot) -> Result<(Vec<u8>, ExportKeyFile), String> {
    let archive_id_bytes = crypto::random::random_bytes(16);
    let mut master_secret = crypto::random::random_bytes(32);
    let salt = crypto::random::random_bytes(32);
    let nonce_prefix = crypto::random::random_bytes(16);
    let archive_id = STANDARD_NO_PAD.encode(&archive_id_bytes);
    let header = ArchiveHeader {
        format: FORMAT_NAME.to_owned(),
        version: FORMAT_VERSION,
        archive_id: archive_id.clone(),
        account_id_hash_sha256: hex_sha256(snapshot.account_id.as_bytes()),
        kdf: KdfHeader {
            algorithm: "HKDF-SHA256".to_owned(),
            salt_base64: STANDARD_NO_PAD.encode(&salt),
            info: KDF_INFO.to_owned(),
            output_bytes: 32,
        },
        aead: "XChaCha20-Poly1305-IETF".to_owned(),
        nonce_prefix_base64: STANDARD_NO_PAD.encode(&nonce_prefix),
        nonce_construction: "16-byte random prefix || uint64-big-endian block index".to_owned(),
        block_plaintext_bytes: DEFAULT_BLOCK_PLAINTEXT_BYTES as u32,
    };
    let header_bytes =
        serde_json::to_vec(&header).map_err(|_| "export header could not be encoded".to_owned())?;
    let derived = crypto::hkdf::derive_32(&salt, &master_secret, KDF_INFO.as_bytes())
        .map_err(|_| "export key derivation failed".to_owned())?;
    let key = crypto::aead::Key::from_bytes(derived);

    let (manifest, mut blocks) = build_manifest_and_blocks(snapshot)?;
    let manifest_bytes = serde_json::to_vec(&manifest)
        .map_err(|_| "export manifest could not be encoded".to_owned())?;
    blocks.insert(
        0,
        PlainBlock {
            index: 0,
            kind: MANIFEST_KIND,
            bytes: manifest_bytes,
        },
    );

    let mut archive = Vec::new();
    archive.extend_from_slice(MAGIC);
    archive.extend_from_slice(&FORMAT_VERSION.to_be_bytes());
    archive.extend_from_slice(&(header_bytes.len() as u32).to_be_bytes());
    archive.extend_from_slice(&header_bytes);
    archive.extend_from_slice(&(blocks.len() as u64).to_be_bytes());
    let header_hash = Sha256::digest(&header_bytes);
    for block in blocks {
        let nonce = nonce_for(&nonce_prefix, block.index)?;
        let aad = block_aad(
            &header_hash,
            block.index,
            block.kind,
            block.bytes.len() as u32,
        );
        let ciphertext = crypto::aead::seal(&key, &nonce, &aad, &block.bytes)
            .map_err(|_| "export block encryption failed".to_owned())?;
        archive.extend_from_slice(&block.index.to_be_bytes());
        archive.push(block.kind);
        archive.extend_from_slice(&(block.bytes.len() as u32).to_be_bytes());
        archive.extend_from_slice(&(ciphertext.len() as u32).to_be_bytes());
        archive.extend_from_slice(&ciphertext);
    }
    let secret_sha256 = hex_sha256(&master_secret);
    let key_file = ExportKeyFile {
        format: "OSL-EXPORT-KEY-1".to_owned(),
        version: FORMAT_VERSION,
        archive_id,
        secret_base64: STANDARD_NO_PAD.encode(&master_secret),
        secret_sha256,
        warning: KEY_WARNING.to_owned(),
    };
    master_secret.zeroize();
    Ok((archive, key_file))
}

fn build_manifest_and_blocks(
    snapshot: &AccountExportSnapshot,
) -> Result<(ManifestBlock, Vec<PlainBlock>), String> {
    let mut classes = SOURCE_OWNERSHIP_INVENTORY
        .iter()
        .map(|class| ((*class).to_owned(), 0u64))
        .collect::<BTreeMap<_, _>>();
    let mut objects = Vec::new();
    let mut manifest_entries = Vec::new();
    let mut blocks = Vec::new();
    let mut next_index = 1u64;
    let mut total = 0u64;

    for document in &snapshot.documents {
        let bytes = serde_json::to_vec(document)
            .map_err(|_| "account document could not be encoded".to_owned())?;
        append_object(
            &document.class,
            &document.id,
            &bytes,
            BTreeMap::new(),
            &mut next_index,
            &mut objects,
            &mut manifest_entries,
            &mut blocks,
        )?;
        *classes.entry(document.class.clone()).or_default() += 1;
        total = total
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| "account export byte count overflow".to_owned())?;
    }
    for attachment in &snapshot.attachments {
        let metadata = [
            ("ownerId".to_owned(), attachment.owner_id.clone()),
            ("messageId".to_owned(), attachment.message_id.clone()),
            ("filename".to_owned(), attachment.filename.clone()),
            ("mimeType".to_owned(), attachment.mime_type.clone()),
        ]
        .into_iter()
        .collect();
        append_object(
            "attachments",
            &attachment.id,
            &attachment.bytes,
            metadata,
            &mut next_index,
            &mut objects,
            &mut manifest_entries,
            &mut blocks,
        )?;
        *classes.entry("attachments".to_owned()).or_default() += 1;
        total = total
            .checked_add(attachment.bytes.len() as u64)
            .ok_or_else(|| "account export byte count overflow".to_owned())?;
    }
    Ok((
        ManifestBlock {
            format: FORMAT_NAME.to_owned(),
            version: FORMAT_VERSION,
            account_id: snapshot.account_id.clone(),
            classes,
            objects,
            blocks: manifest_entries,
            total_plaintext_bytes: total,
        },
        blocks,
    ))
}

#[allow(clippy::too_many_arguments)]
fn append_object(
    class: &str,
    object_id: &str,
    bytes: &[u8],
    metadata: BTreeMap<String, String>,
    next_index: &mut u64,
    objects: &mut Vec<ManifestObject>,
    entries: &mut Vec<ManifestEntry>,
    blocks: &mut Vec<PlainBlock>,
) -> Result<(), String> {
    let first_block = *next_index;
    let chunks = if bytes.is_empty() {
        vec![&[][..]]
    } else {
        bytes.chunks(DEFAULT_BLOCK_PLAINTEXT_BYTES).collect()
    };
    for (position, chunk) in chunks.iter().enumerate() {
        let index = *next_index;
        *next_index = next_index
            .checked_add(1)
            .ok_or_else(|| "account export block index overflow".to_owned())?;
        entries.push(ManifestEntry {
            index,
            class: class.to_owned(),
            object_id: object_id.to_owned(),
            object_offset: (position * DEFAULT_BLOCK_PLAINTEXT_BYTES) as u64,
            plaintext_bytes: chunk.len() as u32,
            sha256: hex_sha256(chunk),
            final_block_for_object: position + 1 == chunks.len(),
        });
        blocks.push(PlainBlock {
            index,
            kind: DATA_KIND,
            bytes: chunk.to_vec(),
        });
    }
    objects.push(ManifestObject {
        class: class.to_owned(),
        id: object_id.to_owned(),
        byte_count: bytes.len() as u64,
        sha256: hex_sha256(bytes),
        first_block,
        block_count: chunks.len() as u64,
        metadata,
    });
    Ok(())
}

struct VerifiedArchive {
    manifest: ManifestBlock,
    authenticated_blocks: BTreeSet<u64>,
}

fn decrypt_archive_bytes(
    archive: &[u8],
    key_file: &ExportKeyFile,
) -> Result<VerifiedArchive, String> {
    // Nothing from `plaintexts` is returned until every frame, including the
    // final frame, and the complete manifest have authenticated.
    let result = (|| {
        let mut cursor = Cursor::new(archive);
        if cursor.take(8)? != MAGIC {
            return Err("integrity failure: archive magic".to_owned());
        }
        if cursor.u16()? != FORMAT_VERSION {
            return Err("integrity failure: archive version".to_owned());
        }
        let header_len = cursor.u32()? as usize;
        if header_len == 0 || header_len > HEADER_LIMIT {
            return Err("integrity failure: header length".to_owned());
        }
        let header_bytes = cursor.take(header_len)?.to_vec();
        let header: ArchiveHeader = serde_json::from_slice(&header_bytes)
            .map_err(|_| "integrity failure: header schema".to_owned())?;
        validate_header(&header, key_file)?;
        let salt = decode_exact::<32>(&header.kdf.salt_base64, "KDF salt")?;
        let nonce_prefix = decode_exact::<16>(&header.nonce_prefix_base64, "nonce prefix")?;
        let mut master = decode_exact::<32>(&key_file.secret_base64, "key secret")?;
        if hex_sha256(&master) != key_file.secret_sha256 {
            master.zeroize();
            return Err("integrity failure: key checksum".to_owned());
        }
        let derived = crypto::hkdf::derive_32(&salt, &master, KDF_INFO.as_bytes())
            .map_err(|_| "integrity failure: KDF".to_owned())?;
        master.zeroize();
        let key = crypto::aead::Key::from_bytes(derived);
        let count = cursor.u64()?;
        if count == 0 || count > 20_000_000 {
            return Err("integrity failure: block count".to_owned());
        }
        let header_hash = Sha256::digest(&header_bytes);
        let mut plaintexts = BTreeMap::<u64, (u8, Vec<u8>)>::new();
        let mut authenticated = BTreeSet::new();
        for expected in 0..count {
            let index = cursor.u64()?;
            let kind = cursor.u8()?;
            let plain_len = cursor.u32()?;
            let cipher_len = cursor.u32()? as usize;
            if index != expected || (kind != MANIFEST_KIND && kind != DATA_KIND) {
                return Err("integrity failure: reordered authenticated block".to_owned());
            }
            let maximum = if kind == MANIFEST_KIND {
                512 * 1024 * 1024
            } else {
                DEFAULT_BLOCK_PLAINTEXT_BYTES
            };
            if plain_len as usize > maximum
                || cipher_len != plain_len as usize + crypto::aead::TAG_SIZE
            {
                return Err("integrity failure: block length".to_owned());
            }
            let ciphertext = cursor.take(cipher_len)?;
            let nonce = nonce_for(&nonce_prefix, index)?;
            let aad = block_aad(&header_hash, index, kind, plain_len);
            let plaintext = crypto::aead::open(&key, &nonce, &aad, ciphertext)
                .map_err(|_| "integrity failure: block authentication".to_owned())?;
            if plaintext.len() != plain_len as usize {
                return Err("integrity failure: plaintext length".to_owned());
            }
            authenticated.insert(index);
            plaintexts.insert(index, (kind, plaintext));
        }
        if !cursor.finished() {
            return Err("integrity failure: trailing archive bytes".to_owned());
        }
        let (kind, manifest_bytes) = plaintexts
            .get(&0)
            .ok_or_else(|| "integrity failure: missing manifest".to_owned())?;
        if *kind != MANIFEST_KIND {
            return Err("integrity failure: manifest block kind".to_owned());
        }
        let manifest: ManifestBlock = serde_json::from_slice(manifest_bytes)
            .map_err(|_| "integrity failure: manifest schema".to_owned())?;
        verify_manifest(&manifest, &header, &plaintexts, count)?;
        Ok(VerifiedArchive {
            manifest,
            authenticated_blocks: authenticated,
        })
    })();
    result.map_err(|error| {
        if error.contains("integrity failure") {
            error
        } else {
            format!("integrity failure: {error}")
        }
    })
}

fn validate_header(header: &ArchiveHeader, key_file: &ExportKeyFile) -> Result<(), String> {
    if header.format != FORMAT_NAME
        || header.version != FORMAT_VERSION
        || header.kdf.algorithm != "HKDF-SHA256"
        || header.kdf.info != KDF_INFO
        || header.kdf.output_bytes != 32
        || header.aead != "XChaCha20-Poly1305-IETF"
        || header.nonce_construction != "16-byte random prefix || uint64-big-endian block index"
        || header.block_plaintext_bytes != DEFAULT_BLOCK_PLAINTEXT_BYTES as u32
        || key_file.format != "OSL-EXPORT-KEY-1"
        || key_file.version != FORMAT_VERSION
        || header.archive_id != key_file.archive_id
        || key_file.warning != KEY_WARNING
    {
        return Err("integrity failure: cryptographic parameters".to_owned());
    }
    Ok(())
}

fn verify_manifest(
    manifest: &ManifestBlock,
    header: &ArchiveHeader,
    plaintexts: &BTreeMap<u64, (u8, Vec<u8>)>,
    count: u64,
) -> Result<(), String> {
    if manifest.format != FORMAT_NAME
        || manifest.version != FORMAT_VERSION
        || hex_sha256(manifest.account_id.as_bytes()) != header.account_id_hash_sha256
        || manifest.blocks.len() as u64 + 1 != count
    {
        return Err("integrity failure: manifest completeness".to_owned());
    }
    let expected_classes = SOURCE_OWNERSHIP_INVENTORY
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<BTreeSet<_>>();
    if manifest.classes.keys().cloned().collect::<BTreeSet<_>>() != expected_classes {
        return Err("integrity failure: manifest class inventory".to_owned());
    }
    let mut expected_index = 1u64;
    let mut authenticated_total = 0u64;
    for entry in &manifest.blocks {
        if entry.index != expected_index {
            return Err("integrity failure: manifest block order".to_owned());
        }
        let (kind, bytes) = plaintexts
            .get(&entry.index)
            .ok_or_else(|| "integrity failure: manifest names unread block".to_owned())?;
        if *kind != DATA_KIND
            || bytes.len() != entry.plaintext_bytes as usize
            || hex_sha256(bytes) != entry.sha256
        {
            return Err("integrity failure: manifest block digest".to_owned());
        }
        authenticated_total += bytes.len() as u64;
        expected_index += 1;
    }
    if authenticated_total != manifest.total_plaintext_bytes {
        return Err("integrity failure: manifest byte count".to_owned());
    }
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
            return Err("integrity failure: object block set".to_owned());
        }
        let mut bytes = Vec::new();
        for entry in entries {
            if entry.object_offset != bytes.len() as u64 {
                return Err("integrity failure: object block offset".to_owned());
            }
            bytes.extend_from_slice(&plaintexts[&entry.index].1);
        }
        if bytes.len() as u64 != object.byte_count || hex_sha256(&bytes) != object.sha256 {
            return Err("integrity failure: object digest".to_owned());
        }
    }
    let mut object_counts = SOURCE_OWNERSHIP_INVENTORY
        .iter()
        .map(|class| ((*class).to_owned(), 0u64))
        .collect::<BTreeMap<_, _>>();
    for object in &manifest.objects {
        *object_counts.entry(object.class.clone()).or_default() += 1;
    }
    if object_counts != manifest.classes {
        return Err("integrity failure: manifest object counts".to_owned());
    }
    Ok(())
}

/// TASK 5402 — the exported archive is authenticated ciphertext and the key
/// file holds its unwrapped data key in the clear, so this is the one shipping
/// pair where "never persisted in plaintext beside it" is a placement rule
/// rather than a cryptographic one.
pub const EXPORT_KEY_MUST_NOT_SIT_BESIDE_ARCHIVE: &str =
    "the export key must be saved to a different folder from the archive: a key stored beside \
     the archive it opens gives away everything the archive's encryption was protecting";

pub(crate) fn export_destinations_are_separated(
    archive_path: &Path,
    key_path: &Path,
) -> Result<(), String> {
    if archive_path == key_path {
        return Err("archive and key require separate user destinations".to_owned());
    }
    let archive_parent = archive_path.parent().map(normalized_parent);
    let key_parent = key_path.parent().map(normalized_parent);
    if archive_parent == key_parent {
        return Err(EXPORT_KEY_MUST_NOT_SIT_BESIDE_ARCHIVE.to_owned());
    }
    Ok(())
}

fn normalized_parent(parent: &Path) -> PathBuf {
    fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf())
}

fn write_destination(path: &Path, bytes: &[u8], label: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{label} destination has no parent"))?;
    fs::create_dir_all(parent).map_err(|_| format!("{label} destination is unavailable"))?;
    let temporary = temporary_path(path);
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|_| format!("{label} temporary file could not be created"))?;
        file.write_all(bytes)
            .map_err(|_| format!("{label} write was short or failed"))?;
        file.sync_all()
            .map_err(|_| format!("{label} write could not be synchronized"))?;
        drop(file);
        fs::rename(&temporary, path)
            .map_err(|_| format!("{label} destination could not be committed"))?;
        sync_parent(parent).map_err(|_| format!("{label} directory could not be synchronized"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    } else {
        // TASK 5402: an export archive is a recovery package. The key file is
        // the one shipping artifact that carries an unwrapped data key, which
        // is why `export_destinations_are_separated` refuses to let it land in
        // the archive's own folder.
        keystore::secret_trace::record(
            keystore::secret_trace::SecretOp::Write,
            if label == "key" {
                keystore::secret_trace::SecretClass::AdjacentDataKey
            } else {
                keystore::secret_trace::SecretClass::RecoveryPackage
            },
            if label == "key" {
                keystore::secret_trace::Protection::Plaintext
            } else {
                keystore::secret_trace::Protection::UserDerivedAead
            },
            "osl_privacy_hub::account_export::write_destination",
            path,
            bytes.len(),
        );
    }
    result
}

fn temporary_path(path: &Path) -> PathBuf {
    let suffix = URL_SAFE_NO_PAD.encode(crypto::random::random_bytes(12));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("osl-export");
    path.with_file_name(format!(".{name}.{suffix}.tmp"))
}

fn sync_parent(parent: &Path) -> std::io::Result<()> {
    File::open(parent)?.sync_all()
}

fn read_entire_file(path: &Path, max: u64, label: &str) -> Result<Vec<u8>, String> {
    let mut file = File::open(path).map_err(|_| format!("{label} could not be reopened"))?;
    let metadata_len = file
        .metadata()
        .map_err(|_| format!("{label} metadata is unreadable"))?
        .len();
    if metadata_len > max {
        return Err(format!("{label} exceeds the readback limit"));
    }
    let mut bytes = Vec::with_capacity(metadata_len as usize);
    file.read_to_end(&mut bytes)
        .map_err(|_| format!("{label} full readback failed"))?;
    if bytes.len() as u64 != metadata_len {
        return Err(format!("{label} short full readback"));
    }
    Ok(bytes)
}

fn inject_media_fault(
    archive_path: &Path,
    key_path: &Path,
    fault: PostWriteMediaFault,
) -> Result<(), String> {
    match fault {
        PostWriteMediaFault::None => Ok(()),
        PostWriteMediaFault::FailedWriteArchive => fs::remove_file(archive_path)
            .map_err(|_| "media fault could not remove failed archive write".to_owned()),
        PostWriteMediaFault::DiskFullKey => OpenOptions::new()
            .write(true)
            .open(key_path)
            .and_then(|file| file.set_len(0))
            .map_err(|_| "media fault could not simulate disk-full key write".to_owned()),
        PostWriteMediaFault::ShortWriteArchive => {
            let file = OpenOptions::new()
                .write(true)
                .open(archive_path)
                .map_err(|_| "media fault could not open archive".to_owned())?;
            let len = file
                .metadata()
                .map_err(|_| "media fault could not stat archive".to_owned())?
                .len();
            file.set_len(len / 2)
                .map_err(|_| "media fault could not simulate short write".to_owned())
        }
        PostWriteMediaFault::DeleteKey => {
            fs::remove_file(key_path).map_err(|_| "media fault could not delete key".to_owned())
        }
        PostWriteMediaFault::TruncateArchive | PostWriteMediaFault::TearFinalBlock => {
            let file = OpenOptions::new()
                .write(true)
                .open(archive_path)
                .map_err(|_| "media fault could not open archive".to_owned())?;
            let len = file
                .metadata()
                .map_err(|_| "media fault could not stat archive".to_owned())?
                .len();
            file.set_len(
                len.saturating_sub(if fault == PostWriteMediaFault::TearFinalBlock {
                    8
                } else {
                    1
                }),
            )
            .map_err(|_| "media fault could not truncate archive".to_owned())
        }
        PostWriteMediaFault::CorruptArchive => {
            use std::io::{Seek, SeekFrom};
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(archive_path)
                .map_err(|_| "media fault could not open archive".to_owned())?;
            let len = file
                .metadata()
                .map_err(|_| "media fault could not stat archive".to_owned())?
                .len();
            let offset = len.saturating_sub(9);
            file.seek(SeekFrom::Start(offset))
                .map_err(|_| "media fault seek failed".to_owned())?;
            let mut byte = [0u8; 1];
            file.read_exact(&mut byte)
                .map_err(|_| "media fault read failed".to_owned())?;
            byte[0] ^= 0x80;
            file.seek(SeekFrom::Start(offset))
                .map_err(|_| "media fault seek failed".to_owned())?;
            file.write_all(&byte)
                .map_err(|_| "media fault write failed".to_owned())?;
            file.sync_all()
                .map_err(|_| "media fault sync failed".to_owned())
        }
    }
}

fn nonce_for(prefix: &[u8], index: u64) -> Result<crypto::aead::Nonce, String> {
    if prefix.len() != 16 {
        return Err("integrity failure: nonce prefix".to_owned());
    }
    let mut nonce = [0u8; 24];
    nonce[..16].copy_from_slice(prefix);
    nonce[16..].copy_from_slice(&index.to_be_bytes());
    Ok(crypto::aead::Nonce::from_bytes(nonce))
}

fn block_aad(header_hash: &[u8], index: u64, kind: u8, plaintext_len: u32) -> Vec<u8> {
    let mut aad = Vec::with_capacity(32 + 8 + 1 + 4);
    aad.extend_from_slice(header_hash);
    aad.extend_from_slice(&index.to_be_bytes());
    aad.push(kind);
    aad.extend_from_slice(&plaintext_len.to_be_bytes());
    aad
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn decode_exact<const N: usize>(value: &str, label: &str) -> Result<[u8; N], String> {
    let bytes = STANDARD_NO_PAD
        .decode(value)
        .map_err(|_| format!("integrity failure: malformed {label}"))?;
    bytes
        .try_into()
        .map_err(|_| format!("integrity failure: wrong-length {label}"))
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
            .ok_or_else(|| "integrity failure: truncated archive".to_owned())?;
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
