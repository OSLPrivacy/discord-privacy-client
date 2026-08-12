//! Portable own-data export boundary for the shipping desktop host.
//!
//! Native paths never cross IPC. The host keeps single-use opaque save grants,
//! snapshots the complete active account directory, writes both files, closes
//! them, and delegates reopened-file verification to the independent reader.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use task_5200_account_export::{
    fix_oracle, generate, save_with_full_readback, DataClass, JourneyRequest, OwnedRecord,
    BLOCK_BYTES, EXPORT_KEY_WARNING, INDEPENDENT_COPY_WARNING,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DestinationKind {
    Archive,
    Key,
}

impl DestinationKind {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "archive" => Ok(Self::Archive),
            "key" => Ok(Self::Key),
            _ => Err("Unknown account-export destination kind".to_owned()),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Archive => "archive",
            Self::Key => "key",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeSaveSelection {
    pub kind: String,
    pub native_journey: bool,
    pub destination_token: String,
    pub display_name: String,
}

#[derive(Clone, Debug)]
struct SaveGrant {
    owner: String,
    kind: DestinationKind,
    path: PathBuf,
}

#[derive(Default)]
struct ExportSession {
    reauthorized_owner: Option<String>,
    grants: BTreeMap<String, SaveGrant>,
}

#[derive(Default)]
pub struct AccountExportNativeState {
    session: Mutex<ExportSession>,
}

impl AccountExportNativeState {
    pub fn record_reauthorization(&self, owner: &str) -> Result<(), String> {
        if owner.is_empty() {
            return Err("No signed-in account is available to export".to_owned());
        }
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Account-export authorization is unavailable".to_owned())?;
        session.grants.clear();
        session.reauthorized_owner = Some(owner.to_owned());
        Ok(())
    }

    pub fn clear(&self) {
        if let Ok(mut session) = self.session.lock() {
            session.reauthorized_owner = None;
            session.grants.clear();
        }
    }

    pub fn issue_save_grant(
        &self,
        owner: &str,
        kind: DestinationKind,
        path: PathBuf,
    ) -> Result<NativeSaveSelection, String> {
        let display_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| "The selected export filename is invalid".to_owned())?
            .to_owned();
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Account-export authorization is unavailable".to_owned())?;
        if session.reauthorized_owner.as_deref() != Some(owner) {
            return Err(
                "Reauthorize the signed-in account before choosing export files".to_owned(),
            );
        }
        if session
            .grants
            .values()
            .any(|grant| grant.owner == owner && grant.path == path)
        {
            return Err("Archive and key destinations must be separate".to_owned());
        }
        let mut random = [0u8; 32];
        OsRng.fill_bytes(&mut random);
        let token = URL_SAFE_NO_PAD.encode(random);
        session
            .grants
            .retain(|_, grant| !(grant.owner == owner && grant.kind == kind));
        session.grants.insert(
            token.clone(),
            SaveGrant {
                owner: owner.to_owned(),
                kind,
                path,
            },
        );
        Ok(NativeSaveSelection {
            kind: kind.label().to_owned(),
            native_journey: true,
            destination_token: token,
            display_name,
        })
    }

    fn consume_pair(
        &self,
        owner: &str,
        archive_token: &str,
        key_token: &str,
    ) -> Result<(PathBuf, PathBuf), String> {
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Account-export authorization is unavailable".to_owned())?;
        let result = (|| {
            if session.reauthorized_owner.as_deref() != Some(owner) {
                return Err("Reauthorization did not authenticate the signed-in account".to_owned());
            }
            if archive_token == key_token {
                return Err("Archive and key destinations must be separate".to_owned());
            }
            let archive = session
                .grants
                .get(archive_token)
                .filter(|grant| grant.owner == owner && grant.kind == DestinationKind::Archive)
                .ok_or_else(|| "Archive native save grant is missing or expired".to_owned())?;
            let key = session
                .grants
                .get(key_token)
                .filter(|grant| grant.owner == owner && grant.kind == DestinationKind::Key)
                .ok_or_else(|| "Key native save grant is missing or expired".to_owned())?;
            if archive.path == key.path {
                return Err("Archive and key destinations must be separate".to_owned());
            }
            Ok((archive.path.clone(), key.path.clone()))
        })();
        // Every attempt consumes reauthorization and both capabilities. A
        // retry always returns to the visible password and native-save steps.
        session.reauthorized_owner = None;
        session.grants.clear();
        result
    }
}

fn classify(relative: &Path) -> DataClass {
    let value = relative.to_string_lossy().to_ascii_lowercase();
    if value.contains("identity") || value.contains("profile") || value.contains("recovery") {
        DataClass::IdentityProfile
    } else if value.contains("attachment") || value.contains("media") || value.contains("blob") {
        DataClass::Attachments
    } else if value.contains("whitelist") || value.contains("allowed_place") {
        DataClass::WhitelistRules
    } else if value.contains("message") || value.contains("store") || value.ends_with(".sqlite") {
        DataClass::Messages
    } else if value.contains("friend") || value.contains("people") || value.contains("peer") {
        DataClass::FriendRelationships
    } else if value.contains("service") || value.contains("account") || value.contains("connector")
    {
        DataClass::AppAccounts
    } else if value.contains("activity") || value.contains("receipt") || value.contains("journal") {
        DataClass::ActivityReceipts
    } else {
        DataClass::Settings
    }
}

fn visit_files(root: &Path, directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|e| format!("Could not enumerate account data: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Could not enumerate account data: {e}"))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|e| format!("Could not inspect account data: {e}"))?;
        if metadata.file_type().is_symlink() {
            return Err(
                "Account export refuses symlinked data outside the active account".to_owned(),
            );
        }
        if metadata.is_dir() {
            visit_files(root, &path, output)?;
        } else if metadata.is_file() {
            path.strip_prefix(root)
                .map_err(|_| "Account data escaped the active account root".to_owned())?;
            output.push(path);
        }
    }
    Ok(())
}

/// Snapshot every regular file owned by the active account with no item/page
/// maximum. Empty classes receive an explicit zero-content inventory document
/// so an independent reader can distinguish "present and empty" from omitted.
pub fn collect_owned_records(owner: &str, root: &Path) -> Result<Vec<OwnedRecord>, String> {
    let mut paths = Vec::new();
    visit_files(root, root, &mut paths)?;
    let mut records = Vec::new();
    let mut present = BTreeSet::new();
    for path in paths {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "Account data escaped the active account root".to_owned())?;
        let class = classify(relative);
        present.insert(class);
        let bytes = fs::read(&path).map_err(|e| {
            format!(
                "Could not read complete account file {}: {e}",
                relative.display()
            )
        })?;
        let id = format!(
            "file-{:x}",
            Sha256::digest(relative.to_string_lossy().as_bytes())
        );
        records.push(OwnedRecord {
            class,
            id,
            owner: owner.to_owned(),
            bytes,
        });
    }
    for class in DataClass::ALL {
        if !present.contains(&class) {
            records.push(OwnedRecord {
                class,
                id: format!("{}-empty-inventory", class.label()),
                owner: owner.to_owned(),
                bytes: b"[]".to_vec(),
            });
        }
    }
    Ok(records)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedManifestDto {
    pub format_version: String,
    pub manifest_id: String,
    pub archive_byte_count: usize,
    pub key_byte_count: usize,
    pub authenticated_block_ids: Vec<String>,
    pub final_block_id: String,
    pub nonce_material_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FullReadbackDto {
    pub archive_closed_and_reopened: bool,
    pub key_closed_and_reopened: bool,
    pub archive_readable: bool,
    pub key_readable: bool,
    pub archive_bytes_read: usize,
    pub key_bytes_read: usize,
    pub header_authenticated: bool,
    pub manifest_authenticated: bool,
    pub authenticated_block_ids: Vec<String>,
    pub final_block_authenticated: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct WriteAndVerifyDto {
    pub manifest: GeneratedManifestDto,
    pub readback: FullReadbackDto,
}

pub fn write_and_verify(
    state: &AccountExportNativeState,
    owner: &str,
    account_root: &Path,
    archive_token: &str,
    key_token: &str,
) -> Result<WriteAndVerifyDto, String> {
    let (archive_path, key_path) = state.consume_pair(owner, archive_token, key_token)?;
    if archive_path.starts_with(account_root) || key_path.starts_with(account_root) {
        return Err("Save the export archive and key outside OSL account storage".to_owned());
    }
    let oracle = fix_oracle(owner, collect_owned_records(owner, account_root)?)?;
    let generated = generate(&oracle)?;
    let request = JourneyRequest {
        signed_in_account: owner.to_owned(),
        reauthorized_account: owner.to_owned(),
        archive_destination: Some(archive_path),
        key_destination: Some(key_path),
        warning_seen: EXPORT_KEY_WARNING.to_owned(),
        independent_copy_seen: INDEPENDENT_COPY_WARNING.to_owned(),
        catalogue_routed: true,
    };
    let receipt = save_with_full_readback(&request, &generated, None)?;
    let mut authenticated_block_ids = vec!["header".to_owned(), "manifest".to_owned()];
    authenticated_block_ids.extend(
        receipt
            .authenticated_blocks
            .iter()
            .filter(|index| **index != 0)
            .map(|index| format!("block-{index}")),
    );
    let final_block_id = authenticated_block_ids
        .last()
        .cloned()
        .ok_or_else(|| "Account export has no final authenticated block".to_owned())?;
    Ok(WriteAndVerifyDto {
        manifest: GeneratedManifestDto {
            format_version: "osl-account-export-v1".to_owned(),
            manifest_id: generated.archive_id.clone(),
            archive_byte_count: receipt.archive_bytes_read,
            key_byte_count: receipt.key_bytes_read,
            authenticated_block_ids: authenticated_block_ids.clone(),
            final_block_id,
            nonce_material_id: URL_SAFE_NO_PAD.encode(generated.nonce_prefix),
        },
        readback: FullReadbackDto {
            archive_closed_and_reopened: true,
            key_closed_and_reopened: true,
            archive_readable: true,
            key_readable: true,
            archive_bytes_read: receipt.archive_bytes_read,
            key_bytes_read: receipt.key_bytes_read,
            header_authenticated: true,
            manifest_authenticated: true,
            authenticated_block_ids,
            final_block_authenticated: true,
        },
    })
}

pub const FORMAT_BLOCK_BYTES: usize = BLOCK_BYTES;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn opaque_grants_are_same_owner_distinct_and_single_use() {
        let state = AccountExportNativeState::default();
        state.record_reauthorization("owner-a").unwrap();
        let root = TempDir::new().unwrap();
        let archive = state
            .issue_save_grant(
                "owner-a",
                DestinationKind::Archive,
                root.path().join("a.oslax"),
            )
            .unwrap();
        assert!(!archive
            .destination_token
            .contains(root.path().to_str().unwrap()));
        assert!(state
            .issue_save_grant("owner-a", DestinationKind::Key, root.path().join("a.oslax"),)
            .unwrap_err()
            .contains("separate"));
        let key = state
            .issue_save_grant(
                "owner-a",
                DestinationKind::Key,
                root.path().join("a.oslkey"),
            )
            .unwrap();
        state
            .consume_pair(
                "owner-a",
                &archive.destination_token,
                &key.destination_token,
            )
            .unwrap();
        assert!(state
            .consume_pair(
                "owner-a",
                &archive.destination_token,
                &key.destination_token
            )
            .unwrap_err()
            .contains("Reauthorization"));
    }

    #[test]
    fn production_snapshot_has_no_fixed_file_limit_and_every_class() {
        let root = TempDir::new().unwrap();
        for index in 0..41 {
            fs::write(
                root.path().join(format!("message-{index:03}.json")),
                index.to_string(),
            )
            .unwrap();
        }
        let records = collect_owned_records("owner-a", root.path()).unwrap();
        assert!(records.len() >= 48);
        assert_eq!(
            records
                .iter()
                .filter(|record| record.class == DataClass::Messages)
                .count(),
            41
        );
        assert!(DataClass::ALL
            .into_iter()
            .all(|class| records.iter().any(|record| record.class == class)));
    }
}
