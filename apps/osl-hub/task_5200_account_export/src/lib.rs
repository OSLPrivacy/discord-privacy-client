//! TASK 5200 portable account-export writer and packaged journey model.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chacha20poly1305::{
    aead::{Aead, Payload},
    KeyInit, XChaCha20Poly1305, XNonce,
};
use hkdf::Hkdf;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub const EXPORT_KEY_WARNING: &str =
    "OSL cannot recover your export key. Save and verify it before leaving this screen.";
pub const INDEPENDENT_COPY_WARNING: &str = "Exports are independent copies. Burn, timers, retention, disconnect, and account deletion cannot remove an archive or key you saved outside OSL.";
pub const PAGE_ITEMS: usize = 16;
pub const BLOCK_BYTES: usize = 1024;
const MAGIC: &[u8; 8] = b"OSLAX01\0";
const KDF_INFO: &[u8] = b"OSL-ACCOUNT-EXPORT-v1";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum DataClass {
    IdentityProfile,
    Settings,
    FriendRelationships,
    Messages,
    Attachments,
    AppAccounts,
    WhitelistRules,
    ActivityReceipts,
}

impl DataClass {
    pub const ALL: [Self; 8] = [
        Self::IdentityProfile,
        Self::Settings,
        Self::FriendRelationships,
        Self::Messages,
        Self::Attachments,
        Self::AppAccounts,
        Self::WhitelistRules,
        Self::ActivityReceipts,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::IdentityProfile => "identity_profile",
            Self::Settings => "settings",
            Self::FriendRelationships => "friend_relationships",
            Self::Messages => "messages",
            Self::Attachments => "attachments",
            Self::AppAccounts => "app_accounts",
            Self::WhitelistRules => "whitelist_rules",
            Self::ActivityReceipts => "activity_receipts",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InventoryRow {
    pub class: DataClass,
    pub source: &'static str,
    pub schema: &'static str,
    pub storage: &'static str,
    pub page_items: usize,
    pub chunk_bytes: usize,
}

pub fn production_inventory() -> Vec<InventoryRow> {
    DataClass::ALL
        .into_iter()
        .map(|class| InventoryRow {
            class,
            source: match class {
                DataClass::IdentityProfile => "identity_registry+osl_profile",
                DataClass::Settings => "preferences",
                DataClass::FriendRelationships => "security.people",
                DataClass::Messages => "message_store.messages",
                DataClass::Attachments => "message_store.attachments",
                DataClass::AppAccounts => "service_accounts",
                DataClass::WhitelistRules => "security.whitelist",
                DataClass::ActivityReceipts => "activity_journal",
            },
            schema: class.label(),
            storage: match class {
                DataClass::IdentityProfile => "active-account/identity.json+hub-profile.json",
                DataClass::Settings => "active-account/preferences+policy ledgers",
                DataClass::FriendRelationships => "active-account/hub_people+peer/security ledgers",
                DataClass::Messages => "active-account/store/messages.sqlite",
                DataClass::Attachments => "active-account/store+attachment/blob records",
                DataClass::AppAccounts => "active-account/service-registry.json",
                DataClass::WhitelistRules => "active-account/whitelist+allowed-place ledgers",
                DataClass::ActivityReceipts => "active-account/activity+receipt+journal ledgers",
            },
            page_items: PAGE_ITEMS,
            chunk_bytes: if class == DataClass::Attachments {
                BLOCK_BYTES
            } else {
                0
            },
        })
        .collect()
}

pub fn verify_independent_inventories(
    source: &[DataClass],
    schema: &[DataClass],
    storage: &[DataClass],
) -> Result<(), String> {
    let required = DataClass::ALL.into_iter().collect::<BTreeSet<_>>();
    for (name, inventory) in [("source", source), ("schema", schema), ("storage", storage)] {
        let got = inventory.iter().copied().collect::<BTreeSet<_>>();
        if got != required {
            let missing = required
                .difference(&got)
                .map(|c| c.label())
                .collect::<Vec<_>>();
            return Err(format!("{name} inventory omission: {}", missing.join(",")));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedRecord {
    pub class: DataClass,
    pub id: String,
    pub owner: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct FixedOracle {
    pub owner: String,
    pub records: Vec<OwnedRecord>,
    pub class_counts: BTreeMap<String, usize>,
    pub item_ids: BTreeSet<String>,
    pub byte_hashes: BTreeMap<String, String>,
    pub total_bytes: usize,
}

pub fn fix_oracle(owner: &str, records: Vec<OwnedRecord>) -> Result<FixedOracle, String> {
    let own = records
        .into_iter()
        .filter(|r| r.owner == owner)
        .collect::<Vec<_>>();
    let mut class_counts = BTreeMap::new();
    let mut item_ids = BTreeSet::new();
    let mut byte_hashes = BTreeMap::new();
    let mut total_bytes = 0;
    for r in &own {
        *class_counts.entry(r.class.label().to_owned()).or_insert(0) += 1;
        if !item_ids.insert(r.id.clone()) {
            return Err(format!("duplicate oracle item {}", r.id));
        }
        total_bytes += r.bytes.len();
        byte_hashes.insert(r.id.clone(), hex_hash(&r.bytes));
    }
    for class in DataClass::ALL {
        if class_counts.get(class.label()).copied().unwrap_or(0) == 0 {
            return Err(format!("oracle omission: {}", class.label()));
        }
    }
    Ok(FixedOracle {
        owner: owner.to_owned(),
        records: own,
        class_counts,
        item_ids,
        byte_hashes,
        total_bytes,
    })
}

#[derive(Serialize)]
struct Header<'a> {
    format: &'a str,
    archive_id: &'a str,
    kdf: Kdf<'a>,
    aead: AeadParameters<'a>,
}
#[derive(Serialize)]
struct Kdf<'a> {
    name: &'a str,
    hash: &'a str,
    info: &'a str,
    salt_b64: String,
    input_bytes: usize,
    output_bytes: usize,
}
#[derive(Serialize)]
struct AeadParameters<'a> {
    name: &'a str,
    nonce_bytes: usize,
    tag_bytes: usize,
    nonce_construction: &'a str,
    nonce_prefix_b64: String,
}
#[derive(Clone, Serialize)]
struct ManifestEntry {
    path: String,
    class: String,
    owner: String,
    bytes: usize,
    sha256: String,
}
#[derive(Clone, Serialize)]
struct ManifestBlock {
    index: u64,
    entry_path: String,
    entry_offset: usize,
    plaintext_bytes: usize,
    plaintext_sha256: String,
    final_block: bool,
}
#[derive(Serialize)]
struct Manifest {
    format: &'static str,
    archive_id: String,
    owner: String,
    expected_archive_bytes: usize,
    total_plaintext_bytes: usize,
    entries: Vec<ManifestEntry>,
    blocks: Vec<ManifestBlock>,
    inventory_classes: Vec<String>,
    inventory_item_counts: BTreeMap<String, usize>,
}
#[derive(Serialize, Deserialize)]
struct KeyFile {
    format: String,
    archive_id: String,
    key_material_b64: String,
    expected_archive_bytes: usize,
    archive_sha256: String,
}

#[derive(Clone, Debug)]
pub struct GeneratedExport {
    pub archive: Vec<u8>,
    pub key_file: Vec<u8>,
    pub archive_id: String,
    pub nonce_prefix: [u8; 16],
    pub manifest_blocks: BTreeSet<u64>,
    pub total_plaintext_bytes: usize,
    pub item_ids: BTreeSet<String>,
}

fn hex_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn nonce(prefix: &[u8; 16], index: u64) -> [u8; 24] {
    let mut out = [0u8; 24];
    out[..16].copy_from_slice(prefix);
    out[16..].copy_from_slice(&index.to_le_bytes());
    out
}
fn aad(header_hash: &[u8], index: u64, kind: u8) -> Vec<u8> {
    let mut out = b"OSLAX01-AAD".to_vec();
    out.extend_from_slice(header_hash);
    out.extend_from_slice(&index.to_le_bytes());
    out.push(kind);
    out
}
fn push_frame(out: &mut Vec<u8>, index: u64, kind: u8, ct: &[u8]) {
    out.extend_from_slice(&index.to_le_bytes());
    out.push(kind);
    out.extend_from_slice(&(ct.len() as u32).to_le_bytes());
    out.extend_from_slice(ct);
}

pub fn generate(oracle: &FixedOracle) -> Result<GeneratedExport, String> {
    let mut seed = [0u8; 32];
    let mut salt = [0u8; 16];
    let mut prefix = [0u8; 16];
    let mut archive_random = [0u8; 16];
    OsRng.fill_bytes(&mut seed);
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut prefix);
    OsRng.fill_bytes(&mut archive_random);
    let archive_id = hex_hash(&archive_random)[..32].to_owned();
    let header = Header {
        format: "osl-account-export-v1",
        archive_id: &archive_id,
        kdf: Kdf {
            name: "HKDF",
            hash: "SHA-256",
            info: "OSL-ACCOUNT-EXPORT-v1",
            salt_b64: STANDARD.encode(salt),
            input_bytes: 32,
            output_bytes: 32,
        },
        aead: AeadParameters {
            name: "XChaCha20-Poly1305",
            nonce_bytes: 24,
            tag_bytes: 16,
            nonce_construction: "16-byte random prefix || little-endian u64 block index",
            nonce_prefix_b64: STANDARD.encode(prefix),
        },
    };
    let header_bytes = serde_json::to_vec(&header).map_err(|e| e.to_string())?;
    let header_hash = Sha256::digest(&header_bytes);
    let hk = Hkdf::<Sha256>::new(Some(&salt), &seed);
    let mut derived = [0u8; 32];
    hk.expand(KDF_INFO, &mut derived)
        .map_err(|_| "KDF failure".to_owned())?;
    let cipher = XChaCha20Poly1305::new((&derived).into());
    let mut entries = Vec::new();
    let mut blocks = Vec::new();
    let mut payloads = Vec::new();
    let mut index = 1u64;
    for record in &oracle.records {
        let path = format!("{}/{}.json", record.class.label(), record.id);
        entries.push(ManifestEntry {
            path: path.clone(),
            class: record.class.label().to_owned(),
            owner: record.owner.clone(),
            bytes: record.bytes.len(),
            sha256: hex_hash(&record.bytes),
        });
        for (chunk_number, chunk) in record.bytes.chunks(BLOCK_BYTES).enumerate() {
            payloads.push(chunk.to_vec());
            blocks.push(ManifestBlock {
                index,
                entry_path: path.clone(),
                entry_offset: chunk_number * BLOCK_BYTES,
                plaintext_bytes: chunk.len(),
                plaintext_sha256: hex_hash(chunk),
                final_block: false,
            });
            index += 1;
        }
    }
    if let Some(last) = blocks.last_mut() {
        last.final_block = true;
    } else {
        return Err("no data blocks".to_owned());
    }
    let build = |expected_archive_bytes: usize| -> Result<(Vec<u8>, BTreeSet<u64>), String> {
        let manifest = Manifest {
            format: "osl-account-export-manifest-v1",
            archive_id: archive_id.clone(),
            owner: oracle.owner.clone(),
            expected_archive_bytes,
            total_plaintext_bytes: oracle.total_bytes,
            entries: entries.clone(),
            blocks: blocks.clone(),
            inventory_classes: DataClass::ALL
                .into_iter()
                .map(|c| c.label().to_owned())
                .collect(),
            inventory_item_counts: oracle.class_counts.clone(),
        };
        let manifest_bytes = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(header_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&header_bytes);
        let tag = cipher
            .encrypt(
                XNonce::from_slice(&nonce(&prefix, u64::MAX)),
                Payload {
                    msg: &[],
                    aad: &aad(&header_hash, u64::MAX, 2),
                },
            )
            .map_err(|_| "header encryption".to_owned())?;
        out.extend_from_slice(&(tag.len() as u32).to_le_bytes());
        out.extend_from_slice(&tag);
        let manifest_ct = cipher
            .encrypt(
                XNonce::from_slice(&nonce(&prefix, 0)),
                Payload {
                    msg: &manifest_bytes,
                    aad: &aad(&header_hash, 0, 0),
                },
            )
            .map_err(|_| "manifest encryption".to_owned())?;
        push_frame(&mut out, 0, 0, &manifest_ct);
        let mut set = BTreeSet::from([0]);
        for (block, plain) in blocks.iter().zip(&payloads) {
            let ct = cipher
                .encrypt(
                    XNonce::from_slice(&nonce(&prefix, block.index)),
                    Payload {
                        msg: plain,
                        aad: &aad(&header_hash, block.index, 1),
                    },
                )
                .map_err(|_| format!("block {} encryption", block.index))?;
            push_frame(&mut out, block.index, 1, &ct);
            set.insert(block.index);
        }
        Ok((out, set))
    };
    let mut expected = 0usize;
    let (archive, manifest_blocks) = loop {
        let built = build(expected)?;
        if built.0.len() == expected {
            break built;
        }
        expected = built.0.len();
    };
    let key = KeyFile {
        format: "osl-account-export-key-v1".to_owned(),
        archive_id: archive_id.clone(),
        key_material_b64: STANDARD.encode(seed),
        expected_archive_bytes: archive.len(),
        archive_sha256: hex_hash(&archive),
    };
    let key_file = serde_json::to_vec_pretty(&key).map_err(|e| e.to_string())?;
    Ok(GeneratedExport {
        archive,
        key_file,
        archive_id,
        nonce_prefix: prefix,
        manifest_blocks,
        total_plaintext_bytes: oracle.total_bytes,
        item_ids: oracle.item_ids.clone(),
    })
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MediaFault {
    ArchiveCancel,
    KeyCancel,
    ArchiveWriteFailure,
    KeyWriteFailure,
    DiskFull,
    ShortWrite,
    TornFinalBlock,
    PostWriteCorruption,
    LostKey,
    UnreadableKey,
    UnreadableArchive,
}
impl MediaFault {
    pub const ALL: [Self; 11] = [
        Self::ArchiveCancel,
        Self::KeyCancel,
        Self::ArchiveWriteFailure,
        Self::KeyWriteFailure,
        Self::DiskFull,
        Self::ShortWrite,
        Self::TornFinalBlock,
        Self::PostWriteCorruption,
        Self::LostKey,
        Self::UnreadableKey,
        Self::UnreadableArchive,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::ArchiveCancel => "cancel archive native save",
            Self::KeyCancel => "cancel key native save",
            Self::ArchiveWriteFailure => "archive failed write",
            Self::KeyWriteFailure => "key failed write",
            Self::DiskFull => "disk-full",
            Self::ShortWrite => "short write",
            Self::TornFinalBlock => "torn final block",
            Self::PostWriteCorruption => "post-write corruption",
            Self::LostKey => "lost key",
            Self::UnreadableKey => "unreadable key",
            Self::UnreadableArchive => "unreadable archive",
        }
    }
}

#[derive(Clone, Debug)]
pub struct JourneyRequest {
    pub signed_in_account: String,
    pub reauthorized_account: String,
    pub archive_destination: Option<PathBuf>,
    pub key_destination: Option<PathBuf>,
    pub warning_seen: String,
    pub independent_copy_seen: String,
    pub catalogue_routed: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuccessReceipt {
    pub owner: String,
    pub archive_bytes_read: usize,
    pub key_bytes_read: usize,
    pub authenticated_blocks: BTreeSet<u64>,
}

pub fn save_with_full_readback(
    request: &JourneyRequest,
    generated: &GeneratedExport,
    fault: Option<MediaFault>,
) -> Result<SuccessReceipt, String> {
    if request.signed_in_account != request.reauthorized_account {
        return Err("reauthorization boundary: authenticated account mismatch".to_owned());
    }
    if request.warning_seen != EXPORT_KEY_WARNING || !request.catalogue_routed {
        return Err(
            "missing user boundary: shipping English catalogue export-key warning".to_owned(),
        );
    }
    if request.independent_copy_seen != INDEPENDENT_COPY_WARNING {
        return Err("absent starvation: independent-copy sentence".to_owned());
    }
    let archive_path = request
        .archive_destination
        .as_ref()
        .ok_or_else(|| "missing user boundary: archive native save cancelled".to_owned())?;
    if fault == Some(MediaFault::ArchiveCancel) {
        return Err("missing user boundary: archive native save cancelled".to_owned());
    }
    let key_path = request
        .key_destination
        .as_ref()
        .ok_or_else(|| "missing user boundary: key native save cancelled".to_owned())?;
    if fault == Some(MediaFault::KeyCancel) {
        return Err("missing user boundary: key native save cancelled".to_owned());
    }
    if archive_path == key_path {
        return Err(
            "missing user boundary: archive and key destinations must be separate".to_owned(),
        );
    }
    if fault == Some(MediaFault::ArchiveWriteFailure) {
        return Err(format!(
            "unread block: {} during archive write",
            fault.unwrap().label()
        ));
    }
    let archive_write = if fault == Some(MediaFault::ShortWrite) {
        &generated.archive[..generated.archive.len() / 2]
    } else if fault == Some(MediaFault::TornFinalBlock) {
        &generated.archive[..generated.archive.len() - 1]
    } else {
        &generated.archive
    };
    fs::write(archive_path, archive_write)
        .map_err(|e| format!("unread block: archive failed write: {e}"))?;
    if fault == Some(MediaFault::DiskFull) {
        return Err(
            "unread block: disk-full injected after OS reported archive write success".to_owned(),
        );
    }
    if fault == Some(MediaFault::KeyWriteFailure) {
        return Err("unread block: key failed write".to_owned());
    }
    fs::write(key_path, &generated.key_file)
        .map_err(|e| format!("unread block: key failed write: {e}"))?;
    if fault == Some(MediaFault::PostWriteCorruption) {
        let mut bytes = fs::read(archive_path).map_err(|e| e.to_string())?;
        let at = bytes.len() - 17;
        bytes[at] ^= 1;
        fs::write(archive_path, bytes).map_err(|e| e.to_string())?;
    }
    if fault == Some(MediaFault::LostKey) {
        fs::remove_file(key_path).map_err(|e| e.to_string())?;
    }
    if fault == Some(MediaFault::UnreadableKey) {
        fs::write(key_path, b"not a key").map_err(|e| e.to_string())?;
    }
    if fault == Some(MediaFault::UnreadableArchive) {
        fs::write(archive_path, b"not an archive").map_err(|e| e.to_string())?;
    }
    let archive_read =
        fs::read(archive_path).map_err(|e| format!("unread block: archive full readback: {e}"))?;
    let key_read =
        fs::read(key_path).map_err(|e| format!("unread block: key full readback: {e}"))?;
    let verified = task_5200_reader_bridge(&archive_read, &key_read)?;
    if archive_read.len() != generated.archive.len()
        || key_read.len() != generated.key_file.len()
        || verified.1 != generated.manifest_blocks
    {
        return Err(
            "false success: full-readback byte count or authenticated-block set mismatch"
                .to_owned(),
        );
    }
    Ok(SuccessReceipt {
        owner: request.signed_in_account.clone(),
        archive_bytes_read: archive_read.len(),
        key_bytes_read: key_read.len(),
        authenticated_blocks: verified.1,
    })
}

// Production uses this same complete validator contract; tests inject the
// separately implemented clean-reader crate through `install_reader`.
type ReaderFn = fn(&[u8], &[u8]) -> Result<(usize, BTreeSet<u64>), String>;
static READER: std::sync::OnceLock<std::sync::RwLock<Option<ReaderFn>>> =
    std::sync::OnceLock::new();
pub fn install_reader(reader: ReaderFn) {
    *READER
        .get_or_init(|| std::sync::RwLock::new(None))
        .write()
        .unwrap() = Some(reader);
}
fn task_5200_reader_bridge(a: &[u8], k: &[u8]) -> Result<(usize, BTreeSet<u64>), String> {
    if let Some(reader) = READER
        .get_or_init(|| std::sync::RwLock::new(None))
        .read()
        .unwrap()
        .as_ref()
        .copied()
    {
        return reader(a, k);
    }
    // The separately implemented clean reader is a production dependency, not
    // a test-only callback. The packaged path therefore cannot report success
    // because a verifier happened to be installed by a test process.
    let verified = task_5200_clean_reader::read_complete(a, k)?;
    Ok((verified.archive_bytes, verified.authenticated_blocks))
}

pub fn paths_are_gone(paths: &[&Path]) -> bool {
    paths.iter().all(|p| !p.exists())
}
