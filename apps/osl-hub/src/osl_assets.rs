//! Chunked authenticated-encrypted storage for large OSL Notes creative assets.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroize;

const VERSION: u8 = 1;
const DIRECTORY: &str = "osl_assets";
const MANIFEST: &str = "manifest.bin";
const CHUNK_BYTES: usize = 256 * 1024;
const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ASSETS: usize = 10_000;
const MAX_ASSET_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_VAULT_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_CHUNKS: usize = (MAX_ASSET_BYTES as usize).div_ceil(CHUNK_BYTES);
const MAX_REFERENCES_PER_ASSET: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslAssetDescriptor {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub chunk_count: usize,
    pub created_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslAssetUpload {
    pub asset: OslAssetDescriptor,
    pub chunk_bytes: usize,
    pub next_chunk: usize,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AssetRecord {
    id: String,
    name: String,
    mime: String,
    size: u64,
    chunk_count: usize,
    created_at: u64,
    received_sizes: Vec<u32>,
    chunk_hashes: Vec<String>,
    complete: bool,
    #[serde(default)]
    linked_note_ids: Vec<String>,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AssetManifest {
    version: u8,
    assets: Vec<AssetRecord>,
}

pub fn list() -> Result<Vec<OslAssetDescriptor>, String> {
    let key = unlocked_key()?;
    let mut assets = load_manifest(&root()?, &key)?
        .assets
        .into_iter()
        .filter(|asset| asset.complete)
        .map(descriptor)
        .collect::<Vec<_>>();
    assets.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(assets)
}

pub fn list_for_note(note_id: &str) -> Result<Vec<OslAssetDescriptor>, String> {
    valid_id(note_id)?;
    let key = unlocked_key()?;
    let mut assets = load_manifest(&root()?, &key)?
        .assets
        .into_iter()
        .filter(|asset| asset.complete && asset.linked_note_ids.iter().any(|id| id == note_id))
        .map(descriptor)
        .collect::<Vec<_>>();
    assets.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(assets)
}

pub fn begin(name: String, mime: String, size: u64) -> Result<OslAssetUpload, String> {
    validate_metadata(&name, &mime, size)?;
    let key = unlocked_key()?;
    let root = root()?;
    let mut manifest = load_manifest(&root, &key)?;
    if let Some(existing) = manifest
        .assets
        .iter()
        .find(|asset| {
            !asset.complete && asset.name == name && asset.mime == mime && asset.size == size
        })
        .cloned()
    {
        return Ok(OslAssetUpload {
            next_chunk: existing.received_sizes.len(),
            asset: descriptor(existing),
            chunk_bytes: CHUNK_BYTES,
        });
    }
    if manifest.assets.len() >= MAX_ASSETS {
        return Err("OSL Notes reached its local asset limit".into());
    }
    if manifest
        .assets
        .iter()
        .map(|asset| asset.size)
        .sum::<u64>()
        .saturating_add(size)
        > MAX_VAULT_BYTES
    {
        return Err("OSL Notes reached its 64 GiB encrypted asset quota".into());
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "The system clock is unavailable".to_owned())?;
    let mut ordinal = manifest.assets.len();
    let id = loop {
        let candidate = asset_id(now.as_nanos(), &name, size, ordinal);
        if !manifest.assets.iter().any(|asset| asset.id == candidate) {
            break candidate;
        }
        ordinal += 1;
    };
    let chunk_count = (size as usize).div_ceil(CHUNK_BYTES);
    let record = AssetRecord {
        id: id.clone(),
        name,
        mime,
        size,
        chunk_count,
        created_at: now.as_secs(),
        received_sizes: vec![],
        chunk_hashes: vec![],
        complete: false,
        linked_note_ids: vec![],
    };
    let result = OslAssetUpload {
        asset: descriptor(record.clone()),
        chunk_bytes: CHUNK_BYTES,
        next_chunk: 0,
    };
    manifest.assets.push(record);
    save_manifest(&root, &manifest, &key)?;
    Ok(result)
}

pub fn append(asset_id: &str, index: usize, mut bytes: Vec<u8>) -> Result<usize, String> {
    valid_id(asset_id)?;
    if bytes.is_empty() || bytes.len() > CHUNK_BYTES || index >= MAX_CHUNKS {
        return Err("The encrypted asset chunk is invalid".into());
    }
    let key = unlocked_key()?;
    let root = root()?;
    let mut manifest = load_manifest(&root, &key)?;
    let asset = manifest
        .assets
        .iter_mut()
        .find(|asset| asset.id == asset_id)
        .ok_or_else(|| "That encrypted asset upload no longer exists".to_owned())?;
    if asset.complete || index != asset.received_sizes.len() || index >= asset.chunk_count {
        return Err("The encrypted asset chunk is out of sequence".into());
    }
    let received = asset
        .received_sizes
        .iter()
        .map(|size| u64::from(*size))
        .sum::<u64>();
    let final_chunk = index + 1 == asset.chunk_count;
    if received + bytes.len() as u64 > asset.size
        || (!final_chunk && bytes.len() != CHUNK_BYTES)
        || (final_chunk && received + bytes.len() as u64 != asset.size)
    {
        return Err("The encrypted asset chunk size does not match its manifest".into());
    }
    let plain_size = bytes.len();
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let sealed = ipc::main_password::encrypt_at_rest(&bytes, &key)
        .map_err(|_| "The asset chunk could not be encrypted".to_owned())?;
    bytes.zeroize();
    let directory = chunk_directory(&root, asset_id)?;
    std::fs::create_dir_all(&directory)
        .map_err(|_| "The encrypted asset directory is unavailable".to_owned())?;
    crate::atomic_file::write_recoverable(
        &directory.join(format!("{index:08}.bin")),
        &sealed,
        "encrypted OSL asset chunk",
    )?;
    asset.received_sizes.push(plain_size as u32);
    asset.chunk_hashes.push(hash);
    save_manifest(&root, &manifest, &key)?;
    Ok(index + 1)
}

pub fn finish(asset_id: &str) -> Result<OslAssetDescriptor, String> {
    valid_id(asset_id)?;
    let key = unlocked_key()?;
    let root = root()?;
    let mut manifest = load_manifest(&root, &key)?;
    let asset = manifest
        .assets
        .iter_mut()
        .find(|asset| asset.id == asset_id)
        .ok_or_else(|| "That encrypted asset upload no longer exists".to_owned())?;
    let received = asset
        .received_sizes
        .iter()
        .map(|size| u64::from(*size))
        .sum::<u64>();
    if asset.received_sizes.len() != asset.chunk_count || received != asset.size {
        return Err("The encrypted asset upload is incomplete".into());
    }
    asset.complete = true;
    let result = descriptor(asset.clone());
    save_manifest(&root, &manifest, &key)?;
    Ok(result)
}

pub fn read_chunk(asset_id: &str, index: usize) -> Result<Vec<u8>, String> {
    valid_id(asset_id)?;
    let key = unlocked_key()?;
    let root = root()?;
    let manifest = load_manifest(&root, &key)?;
    let asset = manifest
        .assets
        .iter()
        .find(|asset| asset.id == asset_id && asset.complete)
        .ok_or_else(|| "That encrypted asset is unavailable".to_owned())?;
    if index >= asset.chunk_count {
        return Err("The encrypted asset chunk is out of range".into());
    }
    read_chunk_at(&root, &key, asset, index)
}

pub fn read_bounded(
    asset_id: &str,
    maximum_bytes: u64,
) -> Result<(OslAssetDescriptor, Vec<u8>), String> {
    valid_id(asset_id)?;
    let key = unlocked_key()?;
    let root = root()?;
    let manifest = load_manifest(&root, &key)?;
    let asset = manifest
        .assets
        .iter()
        .find(|asset| asset.id == asset_id && asset.complete)
        .ok_or_else(|| "That encrypted asset is unavailable".to_owned())?;
    if asset.size > maximum_bytes {
        return Err("That encrypted source is too large for this local decoder".into());
    }
    let capacity = usize::try_from(asset.size)
        .map_err(|_| "That encrypted source is too large for this device".to_owned())?;
    let mut plain = Vec::with_capacity(capacity);
    for index in 0..asset.chunk_count {
        let mut chunk = read_chunk_at(&root, &key, asset, index)?;
        plain.extend_from_slice(&chunk);
        chunk.zeroize();
    }
    if plain.len() != capacity {
        plain.zeroize();
        return Err("The encrypted asset size failed integrity verification".into());
    }
    Ok((descriptor(asset.clone()), plain))
}

fn read_chunk_at(
    root: &Path,
    key: &[u8; 32],
    asset: &AssetRecord,
    index: usize,
) -> Result<Vec<u8>, String> {
    let path = chunk_directory(root, &asset.id)?.join(format!("{index:08}.bin"));
    let sealed = crate::atomic_file::read_recoverable_bounded(
        &path,
        (CHUNK_BYTES * 2) as u64,
        "encrypted OSL asset chunk",
    )?
    .ok_or_else(|| "The encrypted asset chunk is missing".to_owned())?;
    let plain = ipc::main_password::decrypt_at_rest(&sealed, &key)
        .map_err(|_| "The asset chunk could not be authenticated".to_owned())?;
    if plain.len() != asset.received_sizes[index] as usize
        || format!("{:x}", Sha256::digest(&plain)) != asset.chunk_hashes[index]
    {
        return Err("The encrypted asset chunk failed integrity verification".into());
    }
    Ok(plain)
}

pub fn cancel_upload(asset_id: &str) -> Result<bool, String> {
    valid_id(asset_id)?;
    let key = unlocked_key()?;
    let root = root()?;
    cancel_upload_at(&root, &key, asset_id)
}

pub fn link(asset_id: &str, note_id: &str) -> Result<bool, String> {
    valid_id(asset_id)?;
    valid_id(note_id)?;
    let key = unlocked_key()?;
    let root = root()?;
    link_at(&root, &key, asset_id, note_id)
}

fn link_at(root: &Path, key: &[u8; 32], asset_id: &str, note_id: &str) -> Result<bool, String> {
    valid_id(asset_id)?;
    valid_id(note_id)?;
    let mut manifest = load_manifest(&root, &key)?;
    let asset = manifest
        .assets
        .iter_mut()
        .find(|asset| asset.id == asset_id && asset.complete)
        .ok_or_else(|| "That completed encrypted asset is unavailable".to_owned())?;
    if asset.linked_note_ids.iter().any(|id| id == note_id) {
        return Ok(false);
    }
    if asset.linked_note_ids.len() >= MAX_REFERENCES_PER_ASSET {
        return Err("That encrypted asset reached its project-reference limit".into());
    }
    asset.linked_note_ids.push(note_id.to_owned());
    asset.linked_note_ids.sort();
    save_manifest(root, &manifest, key)?;
    Ok(true)
}

pub fn release(asset_id: &str, note_id: &str) -> Result<bool, String> {
    valid_id(asset_id)?;
    valid_id(note_id)?;
    let key = unlocked_key()?;
    let root = root()?;
    release_at(&root, &key, asset_id, note_id)
}

fn release_at(root: &Path, key: &[u8; 32], asset_id: &str, note_id: &str) -> Result<bool, String> {
    valid_id(asset_id)?;
    valid_id(note_id)?;
    let mut manifest = load_manifest(&root, &key)?;
    let asset = manifest
        .assets
        .iter_mut()
        .find(|asset| asset.id == asset_id && asset.complete)
        .ok_or_else(|| "That completed encrypted asset is unavailable".to_owned())?;
    let before = asset.linked_note_ids.len();
    asset.linked_note_ids.retain(|id| id != note_id);
    if before == asset.linked_note_ids.len() {
        return Ok(false);
    }
    if !asset.linked_note_ids.is_empty() {
        save_manifest(root, &manifest, key)?;
        return Ok(false);
    }
    remove_unreferenced_at(root, key, &mut manifest, asset_id)
}

pub fn discard_unreferenced(asset_id: &str) -> Result<bool, String> {
    valid_id(asset_id)?;
    let key = unlocked_key()?;
    let root = root()?;
    let mut manifest = load_manifest(&root, &key)?;
    remove_unreferenced_at(&root, &key, &mut manifest, asset_id)
}

pub fn release_all_for_note(note_id: &str) -> Result<usize, String> {
    valid_id(note_id)?;
    let key = unlocked_key()?;
    let root = root()?;
    let asset_ids = load_manifest(&root, &key)?
        .assets
        .iter()
        .filter(|asset| asset.linked_note_ids.iter().any(|id| id == note_id))
        .map(|asset| asset.id.clone())
        .collect::<Vec<_>>();
    let mut deleted = 0;
    for asset_id in asset_ids {
        if release_at(&root, &key, &asset_id, note_id)? {
            deleted += 1;
        }
    }
    Ok(deleted)
}

fn remove_unreferenced_at(
    root: &Path,
    key: &[u8; 32],
    manifest: &mut AssetManifest,
    asset_id: &str,
) -> Result<bool, String> {
    let Some(position) = manifest.assets.iter().position(|asset| {
        asset.id == asset_id && asset.complete && asset.linked_note_ids.is_empty()
    }) else {
        return Ok(false);
    };
    let chunks = chunk_directory(root, asset_id)?;
    let quarantine = root.join("quarantine").join(format!("released-{asset_id}"));
    if chunks.exists() {
        if let Some(parent) = quarantine.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| "The encrypted asset quarantine is unavailable".to_owned())?;
        }
        std::fs::rename(&chunks, &quarantine)
            .map_err(|_| "The released encrypted asset could not be quarantined".to_owned())?;
    }
    let record = manifest.assets.remove(position);
    if let Err(error) = save_manifest(root, manifest, key) {
        manifest.assets.insert(position, record);
        if quarantine.exists() {
            let _ = std::fs::rename(&quarantine, &chunks);
        }
        return Err(error);
    }
    if quarantine.exists() {
        std::fs::remove_dir_all(&quarantine)
            .map_err(|_| "The released encrypted asset remains quarantined".to_owned())?;
    }
    Ok(true)
}

fn cancel_upload_at(root: &Path, key: &[u8; 32], asset_id: &str) -> Result<bool, String> {
    valid_id(asset_id)?;
    let mut manifest = load_manifest(&root, &key)?;
    let Some(position) = manifest
        .assets
        .iter()
        .position(|asset| asset.id == asset_id && !asset.complete)
    else {
        return Ok(false);
    };
    let chunks = chunk_directory(root, asset_id)?;
    let quarantine = root.join("quarantine").join(asset_id);
    if chunks.exists() {
        if let Some(parent) = quarantine.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| "The encrypted asset quarantine is unavailable".to_owned())?;
        }
        std::fs::rename(&chunks, &quarantine)
            .map_err(|_| "The incomplete encrypted upload could not be quarantined".to_owned())?;
    }
    manifest.assets.remove(position);
    if let Err(error) = save_manifest(root, &manifest, key) {
        if quarantine.exists() {
            let _ = std::fs::rename(&quarantine, &chunks);
        }
        return Err(error);
    }
    if quarantine.exists() {
        std::fs::remove_dir_all(&quarantine)
            .map_err(|_| "The cancelled encrypted upload remains quarantined".to_owned())?;
    }
    Ok(true)
}

fn descriptor(asset: AssetRecord) -> OslAssetDescriptor {
    OslAssetDescriptor {
        id: asset.id,
        name: asset.name,
        mime: asset.mime,
        size: asset.size,
        chunk_count: asset.chunk_count,
        created_at: asset.created_at,
    }
}

fn validate_metadata(name: &str, mime: &str, size: u64) -> Result<(), String> {
    if name.trim().is_empty()
        || name.len() > 240
        || name.chars().any(char::is_control)
        || mime.is_empty()
        || mime.len() > 120
        || !mime
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'+' | b'-' | b'.'))
        || size == 0
        || size > MAX_ASSET_BYTES
    {
        return Err("The local asset metadata is invalid".into());
    }
    Ok(())
}

fn valid_id(id: &str) -> Result<(), String> {
    if id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("The OSL asset identity is invalid".into())
    }
}

fn asset_id(now: u128, name: &str, size: u64, ordinal: usize) -> String {
    let mut digest = Sha256::new();
    digest.update(b"OSL-ASSET-v1");
    digest.update(now.to_le_bytes());
    digest.update(size.to_le_bytes());
    digest.update(ordinal.to_le_bytes());
    digest.update(name.as_bytes());
    format!("{:x}", digest.finalize())[..32].to_owned()
}

fn unlocked_key() -> Result<[u8; 32], String> {
    ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "Unlock your OSL identity before accessing creative assets".into())
}

fn root() -> Result<PathBuf, String> {
    keystore::osl_config_dir()
        .map(|path| path.join(DIRECTORY))
        .map_err(|_| "OSL creative asset storage is unavailable".into())
}

fn chunk_directory(root: &Path, asset_id: &str) -> Result<PathBuf, String> {
    valid_id(asset_id)?;
    Ok(root.join("chunks").join(asset_id))
}

fn load_manifest(root: &Path, key: &[u8; 32]) -> Result<AssetManifest, String> {
    let Some(sealed) = crate::atomic_file::read_recoverable_bounded(
        &root.join(MANIFEST),
        MAX_MANIFEST_BYTES,
        "encrypted OSL asset manifest",
    )?
    else {
        return Ok(AssetManifest {
            version: VERSION,
            assets: vec![],
        });
    };
    if !ipc::main_password::has_enc_magic(&sealed) {
        return Err("OSL asset storage is not encrypted".into());
    }
    let mut plain = ipc::main_password::decrypt_at_rest(&sealed, key)
        .map_err(|_| "OSL asset storage could not be authenticated".to_owned())?;
    let parsed = serde_json::from_slice::<AssetManifest>(&plain)
        .map_err(|_| "OSL asset storage is malformed".to_owned());
    plain.zeroize();
    let manifest = parsed?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn save_manifest(root: &Path, manifest: &AssetManifest, key: &[u8; 32]) -> Result<(), String> {
    validate_manifest(manifest)?;
    std::fs::create_dir_all(root).map_err(|_| "OSL asset storage is unavailable".to_owned())?;
    let mut plain = serde_json::to_vec(manifest)
        .map_err(|_| "OSL asset storage could not be encoded".to_owned())?;
    let sealed = ipc::main_password::encrypt_at_rest(&plain, key)
        .map_err(|_| "OSL asset storage could not be encrypted".to_owned())?;
    plain.zeroize();
    if sealed.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("OSL asset metadata exceeds its local limit".into());
    }
    crate::atomic_file::write_recoverable(
        &root.join(MANIFEST),
        &sealed,
        "encrypted OSL asset manifest",
    )
}

fn validate_manifest(manifest: &AssetManifest) -> Result<(), String> {
    if manifest.version != VERSION || manifest.assets.len() > MAX_ASSETS {
        return Err("OSL asset storage has an unsupported shape".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    for asset in &manifest.assets {
        valid_id(&asset.id)?;
        validate_metadata(&asset.name, &asset.mime, asset.size)?;
        if asset.chunk_count == 0
            || asset.chunk_count > MAX_CHUNKS
            || asset.chunk_count != (asset.size as usize).div_ceil(CHUNK_BYTES)
            || asset.received_sizes.len() != asset.chunk_hashes.len()
            || asset.received_sizes.len() > asset.chunk_count
            || asset
                .received_sizes
                .iter()
                .map(|size| u64::from(*size))
                .sum::<u64>()
                > asset.size
            || asset
                .chunk_hashes
                .iter()
                .any(|hash| hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
            || asset.complete
                && (asset.received_sizes.len() != asset.chunk_count
                    || asset
                        .received_sizes
                        .iter()
                        .map(|size| u64::from(*size))
                        .sum::<u64>()
                        != asset.size)
            || asset.linked_note_ids.len() > MAX_REFERENCES_PER_ASSET
            || asset.linked_note_ids.iter().any(|id| valid_id(id).is_err())
            || asset
                .linked_note_ids
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || !ids.insert(&asset.id)
        {
            return Err("OSL asset storage is malformed".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_and_chunk_counts_are_bounded() {
        assert!(validate_metadata("photo.png", "image/png", 1).is_ok());
        assert!(validate_metadata("", "image/png", 1).is_err());
        assert!(validate_metadata("photo", "image/png;remote=x", 1).is_err());
        assert_eq!((CHUNK_BYTES as u64 + 1).div_ceil(CHUNK_BYTES as u64), 2);
    }
    #[test]
    fn encrypted_chunks_do_not_expose_plaintext() {
        let plain = b"private photo bytes";
        let sealed = ipc::main_password::encrypt_at_rest(plain, &[9; 32]).unwrap();
        assert!(!String::from_utf8_lossy(&sealed).contains("private photo"));
        assert_eq!(
            ipc::main_password::decrypt_at_rest(&sealed, &[9; 32]).unwrap(),
            plain
        );
    }
    #[test]
    fn cancellation_quarantines_exact_incomplete_upload_before_removal() {
        let id = "a".repeat(32);
        let root = std::env::temp_dir().join(format!("osl-assets-test-{}", asset_id(1, "x", 1, 0)));
        let chunks = chunk_directory(&root, &id).unwrap();
        std::fs::create_dir_all(&chunks).unwrap();
        std::fs::write(chunks.join("00000000.bin"), b"sealed-placeholder").unwrap();
        let manifest = AssetManifest {
            version: VERSION,
            assets: vec![AssetRecord {
                id: id.clone(),
                name: "draft.bin".into(),
                mime: "application/octet-stream".into(),
                size: 1,
                chunk_count: 1,
                created_at: 1,
                received_sizes: vec![],
                chunk_hashes: vec![],
                complete: false,
                linked_note_ids: vec![],
            }],
        };
        save_manifest(&root, &manifest, &[7; 32]).unwrap();
        assert!(cancel_upload_at(&root, &[7; 32], &id).unwrap());
        assert!(load_manifest(&root, &[7; 32]).unwrap().assets.is_empty());
        assert!(!chunks.exists());
        std::fs::remove_dir_all(&root).unwrap();
    }
    #[test]
    fn final_unreferenced_asset_is_quarantined_before_manifest_removal() {
        let id = "b".repeat(32);
        let root = std::env::temp_dir().join(format!("osl-assets-test-{}", asset_id(2, "y", 1, 0)));
        let chunks = chunk_directory(&root, &id).unwrap();
        std::fs::create_dir_all(&chunks).unwrap();
        std::fs::write(chunks.join("00000000.bin"), b"sealed-placeholder").unwrap();
        let mut manifest = AssetManifest {
            version: VERSION,
            assets: vec![AssetRecord {
                id: id.clone(),
                name: "photo.png".into(),
                mime: "image/png".into(),
                size: 1,
                chunk_count: 1,
                created_at: 1,
                received_sizes: vec![1],
                chunk_hashes: vec!["0".repeat(64)],
                complete: true,
                linked_note_ids: vec![],
            }],
        };
        save_manifest(&root, &manifest, &[8; 32]).unwrap();
        assert!(remove_unreferenced_at(&root, &[8; 32], &mut manifest, &id).unwrap());
        assert!(load_manifest(&root, &[8; 32]).unwrap().assets.is_empty());
        assert!(!chunks.exists());
        std::fs::remove_dir_all(&root).unwrap();
    }
    #[test]
    fn shared_asset_survives_until_its_final_project_reference_is_released() {
        let id = "c".repeat(32);
        let first = "d".repeat(32);
        let second = "e".repeat(32);
        let root = std::env::temp_dir().join(format!("osl-assets-test-{}", asset_id(3, "z", 1, 0)));
        let chunks = chunk_directory(&root, &id).unwrap();
        std::fs::create_dir_all(&chunks).unwrap();
        std::fs::write(chunks.join("00000000.bin"), b"sealed-placeholder").unwrap();
        let manifest = AssetManifest {
            version: VERSION,
            assets: vec![AssetRecord {
                id: id.clone(),
                name: "shared.png".into(),
                mime: "image/png".into(),
                size: 1,
                chunk_count: 1,
                created_at: 1,
                received_sizes: vec![1],
                chunk_hashes: vec!["0".repeat(64)],
                complete: true,
                linked_note_ids: vec![],
            }],
        };
        save_manifest(&root, &manifest, &[6; 32]).unwrap();
        assert!(link_at(&root, &[6; 32], &id, &second).unwrap());
        assert!(link_at(&root, &[6; 32], &id, &first).unwrap());
        assert!(!release_at(&root, &[6; 32], &id, &first).unwrap());
        assert!(chunks.exists());
        assert_eq!(
            load_manifest(&root, &[6; 32]).unwrap().assets[0].linked_note_ids,
            vec![second.clone()]
        );
        assert!(release_at(&root, &[6; 32], &id, &second).unwrap());
        assert!(!chunks.exists());
        assert!(load_manifest(&root, &[6; 32]).unwrap().assets.is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
