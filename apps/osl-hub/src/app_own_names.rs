use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::models::ServiceKind;

const STORE_VERSION: u8 = 1;
const MAX_STORE_BYTES: u64 = 128 * 1024;
const MAX_OWNER_BYTES: usize = 128;
const MAX_ACCOUNT_ID_BYTES: usize = 128;
const MAX_NAMES_PER_APP: usize = 12;
const MAX_NAME_BYTES: usize = 80;

#[derive(Debug, Clone, Copy, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowWhoWroteIt {
    Yours,
    Theirs,
    NotPublishedByApp,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppOwnNameConfirmation {
    pub service_id: ServiceKind,
    pub account_id: String,
    pub names: Vec<String>,
    pub shown_to_person: bool,
    pub needs_confirmation: bool,
    pub warning: &'static str,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NamePublishedRow {
    pub row_id: String,
    pub published_name: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RowOwnershipMark {
    pub row_id: String,
    pub answer: RowWhoWroteIt,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NamePublishedReadAnswer {
    pub marks: Vec<RowOwnershipMark>,
    pub refused: usize,
    pub refusal: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoreDocument {
    version: u8,
    entries: Vec<NameEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NameEntry {
    owner_osl_user_id: String,
    service_id: ServiceKind,
    account_id: String,
    names: Vec<String>,
    confirmed: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct EntryKey {
    owner_osl_user_id: String,
    service_id: ServiceKind,
    account_id: String,
}

#[derive(Default)]
struct StoreCache {
    loaded: bool,
    entries: BTreeMap<EntryKey, NameEntry>,
}

pub struct AppOwnNameState {
    path: PathBuf,
    cache: Mutex<StoreCache>,
}

impl AppOwnNameState {
    pub fn load(path: PathBuf) -> Self {
        Self {
            path,
            cache: Mutex::new(StoreCache::default()),
        }
    }

    pub fn record_connected_account_names(
        &self,
        owner_osl_user_id: &str,
        service_id: ServiceKind,
        account_id: &str,
        observed_names: Vec<String>,
    ) -> Result<AppOwnNameConfirmation, String> {
        validate_owner_osl_user_id(owner_osl_user_id)?;
        validate_account_id(account_id)?;
        let names = normalize_name_list(observed_names)?;
        let key = EntryKey::new(owner_osl_user_id, service_id, account_id);
        let mut cache = self.locked_cache()?;
        let confirmed = cache
            .entries
            .get(&key)
            .is_some_and(|entry| entry.confirmed && entry.names == names);
        cache.entries.insert(
            key.clone(),
            NameEntry {
                owner_osl_user_id: key.owner_osl_user_id.clone(),
                service_id,
                account_id: key.account_id.clone(),
                names,
                confirmed,
            },
        );
        write_store(&self.path, cache.entries.values())?;
        Ok(confirmation_dto(
            cache.entries.get(&key).expect("entry was just inserted"),
        ))
    }

    pub fn correction_for_person(
        &self,
        owner_osl_user_id: &str,
        service_id: ServiceKind,
        account_id: &str,
        corrected_names: Vec<String>,
    ) -> Result<AppOwnNameConfirmation, String> {
        validate_owner_osl_user_id(owner_osl_user_id)?;
        validate_account_id(account_id)?;
        let names = normalize_name_list(corrected_names)?;
        let key = EntryKey::new(owner_osl_user_id, service_id, account_id);
        let mut cache = self.locked_cache()?;
        cache.entries.insert(
            key.clone(),
            NameEntry {
                owner_osl_user_id: key.owner_osl_user_id.clone(),
                service_id,
                account_id: key.account_id.clone(),
                names,
                confirmed: true,
            },
        );
        write_store(&self.path, cache.entries.values())?;
        Ok(confirmation_dto(
            cache.entries.get(&key).expect("entry was just inserted"),
        ))
    }

    pub fn confirmation_for_person(
        &self,
        owner_osl_user_id: &str,
        service_id: ServiceKind,
        account_id: &str,
    ) -> Result<AppOwnNameConfirmation, String> {
        validate_owner_osl_user_id(owner_osl_user_id)?;
        validate_account_id(account_id)?;
        let key = EntryKey::new(owner_osl_user_id, service_id, account_id);
        let cache = self.locked_cache()?;
        let entry = cache.entries.get(&key).cloned().unwrap_or(NameEntry {
            owner_osl_user_id: owner_osl_user_id.to_owned(),
            service_id,
            account_id: account_id.to_owned(),
            names: Vec::new(),
            confirmed: false,
        });
        Ok(confirmation_dto(&entry))
    }

    pub fn read_name_published_rows(
        &self,
        owner_osl_user_id: &str,
        service_id: ServiceKind,
        account_id: &str,
        rows: &[NamePublishedRow],
    ) -> Result<NamePublishedReadAnswer, String> {
        let confirmation =
            self.confirmation_for_person(owner_osl_user_id, service_id, account_id)?;
        Ok(mark_name_published_rows(&confirmation.names, rows))
    }

    fn locked_cache(&self) -> Result<std::sync::MutexGuard<'_, StoreCache>, String> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "app own-name store is unavailable".to_owned())?;
        if !cache.loaded {
            cache.entries = load_store(&self.path)?;
            cache.loaded = true;
        }
        Ok(cache)
    }
}

impl EntryKey {
    fn new(owner_osl_user_id: &str, service_id: ServiceKind, account_id: &str) -> Self {
        Self {
            owner_osl_user_id: owner_osl_user_id.to_owned(),
            service_id,
            account_id: account_id.to_owned(),
        }
    }
}

impl Ord for EntryKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.owner_osl_user_id
            .cmp(&other.owner_osl_user_id)
            .then_with(|| service_rank(self.service_id).cmp(&service_rank(other.service_id)))
            .then_with(|| self.account_id.cmp(&other.account_id))
    }
}

impl PartialOrd for EntryKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

const fn service_rank(service_id: ServiceKind) -> u8 {
    match service_id {
        ServiceKind::Discord => 1,
        ServiceKind::Telegram => 2,
        ServiceKind::Instagram => 3,
        ServiceKind::WhatsApp => 4,
        ServiceKind::Email => 5,
        ServiceKind::Signal => 6,
        ServiceKind::X => 7,
        ServiceKind::Messenger => 8,
    }
}

pub fn mark_name_published_rows(
    own_names: &[String],
    rows: &[NamePublishedRow],
) -> NamePublishedReadAnswer {
    let own_names = own_names
        .iter()
        .filter_map(|name| normalized_name(name).ok())
        .collect::<BTreeSet<_>>();
    if own_names.is_empty() {
        return NamePublishedReadAnswer {
            marks: rows
                .iter()
                .map(|row| RowOwnershipMark {
                    row_id: row.row_id.clone(),
                    answer: RowWhoWroteIt::NotPublishedByApp,
                })
                .collect(),
            refused: rows.len(),
            refusal: Some(format!(
                "OSL: own-name list is empty; refused {} name-published rows",
                rows.len()
            )),
        };
    }

    let marks = rows
        .iter()
        .map(|row| {
            let answer = match row
                .published_name
                .as_deref()
                .and_then(|name| normalized_name(name).ok())
            {
                Some(name) if own_names.contains(&name) => RowWhoWroteIt::Yours,
                Some(_) => RowWhoWroteIt::Theirs,
                None => RowWhoWroteIt::NotPublishedByApp,
            };
            RowOwnershipMark {
                row_id: row.row_id.clone(),
                answer,
            }
        })
        .collect::<Vec<_>>();
    let refused = marks
        .iter()
        .filter(|mark| mark.answer == RowWhoWroteIt::NotPublishedByApp)
        .count();
    NamePublishedReadAnswer {
        marks,
        refused,
        refusal: (refused > 0)
            .then(|| format!("OSL: refused {refused} rows because the app did not publish a name")),
    }
}

fn confirmation_dto(entry: &NameEntry) -> AppOwnNameConfirmation {
    AppOwnNameConfirmation {
        service_id: entry.service_id,
        account_id: entry.account_id.clone(),
        names: entry.names.clone(),
        shown_to_person: true,
        needs_confirmation: !entry.confirmed,
        warning: "Names narrow row ownership but never prove it on their own.",
    }
}

fn normalize_name_list(names: Vec<String>) -> Result<Vec<String>, String> {
    let mut normalized = BTreeSet::new();
    for name in names {
        normalized.insert(normalized_name(&name)?);
        if normalized.len() > MAX_NAMES_PER_APP {
            return Err("own-name list is too long".to_owned());
        }
    }
    Ok(normalized.into_iter().collect())
}

fn normalized_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_NAME_BYTES
        || trimmed.chars().any(|character| character.is_control())
    {
        return Err("own name must be 1-80 printable characters".to_owned());
    }
    Ok(trimmed.to_owned())
}

fn validate_owner_osl_user_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > MAX_OWNER_BYTES
        || value.trim() != value
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err("active OSL identity is invalid".to_owned());
    }
    Ok(())
}

fn validate_account_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > MAX_ACCOUNT_ID_BYTES
        || value.trim() != value
        || value.chars().any(|character| character.is_control())
    {
        return Err("service account is invalid".to_owned());
    }
    Ok(())
}

fn load_store(path: &Path) -> Result<BTreeMap<EntryKey, NameEntry>, String> {
    let Some(bytes) =
        crate::atomic_file::read_recoverable_bounded(path, MAX_STORE_BYTES, "app own-name store")?
    else {
        return Ok(BTreeMap::new());
    };
    let document: StoreDocument =
        serde_json::from_slice(&bytes).map_err(|_| "app own-name store is malformed".to_owned())?;
    if document.version != STORE_VERSION {
        return Err("app own-name store version is unsupported".to_owned());
    }
    let mut entries = BTreeMap::new();
    for entry in document.entries {
        if validate_owner_osl_user_id(&entry.owner_osl_user_id).is_err()
            || validate_account_id(&entry.account_id).is_err()
        {
            continue;
        }
        let names = normalize_name_list(entry.names)?;
        entries.insert(
            EntryKey::new(
                &entry.owner_osl_user_id,
                entry.service_id,
                &entry.account_id,
            ),
            NameEntry { names, ..entry },
        );
    }
    Ok(entries)
}

fn write_store<'a>(
    path: &Path,
    entries: impl IntoIterator<Item = &'a NameEntry>,
) -> Result<(), String> {
    let document = StoreDocument {
        version: STORE_VERSION,
        entries: entries.into_iter().cloned().collect(),
    };
    let bytes = serde_json::to_vec(&document)
        .map_err(|_| "app own-name store could not be encoded".to_owned())?;
    if bytes.len() as u64 > MAX_STORE_BYTES {
        return Err("app own-name store exceeds limit".to_owned());
    }
    crate::atomic_file::write_recoverable(path, &bytes, "app own-name store")
}
