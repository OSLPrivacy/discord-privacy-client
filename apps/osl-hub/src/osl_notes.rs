//! Authenticated, encrypted, identity-scoped storage for OSL Notes.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroize;

const VERSION: u8 = 1;
const FILE_NAME: &str = "osl_notes.bin";
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_NOTES: usize = 5_000;
const MAX_BODY_BYTES: usize = 256 * 1024;
const MAX_REVISIONS: usize = 10_000;
const MAX_REVISIONS_PER_NOTE: usize = 50;
const MAX_SAVED_SEARCHES: usize = 64;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OslDocumentKind {
    #[default]
    Note,
    Document,
    Spreadsheet,
    Drawing,
    Presentation,
    Photo,
    Video,
    Audio,
    Model3d,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OslNote {
    pub id: String,
    #[serde(default)]
    pub kind: OslDocumentKind,
    pub title: String,
    pub body: String,
    pub folder: String,
    pub tags: Vec<String>,
    pub favorite: bool,
    #[serde(default)]
    pub pinned: bool,
    pub created_at: u64,
    pub updated_at: u64,
    pub deleted_at: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OslNoteInput {
    pub id: Option<String>,
    #[serde(default)]
    pub kind: OslDocumentKind,
    pub title: String,
    pub body: String,
    pub folder: String,
    pub tags: Vec<String>,
    pub favorite: bool,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OslNoteRevision {
    pub id: String,
    pub note_id: String,
    pub kind: OslDocumentKind,
    pub title: String,
    pub body: String,
    pub folder: String,
    pub tags: Vec<String>,
    pub favorite: bool,
    #[serde(default)]
    pub pinned: bool,
    pub created_at: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OslSavedSearch {
    pub id: String,
    pub name: String,
    pub query: String,
    pub filter: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OslSavedSearchInput {
    pub name: String,
    pub query: String,
    pub filter: String,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u8,
    notes: Vec<OslNote>,
    #[serde(default)]
    revisions: Vec<OslNoteRevision>,
    #[serde(default)]
    saved_searches: Vec<OslSavedSearch>,
}

pub fn list() -> Result<Vec<OslNote>, String> {
    let key = unlocked_key()?;
    Ok(load(&path()?, &key)?.notes)
}

pub fn list_revisions(note_id: &str) -> Result<Vec<OslNoteRevision>, String> {
    valid_id(note_id)?;
    let key = unlocked_key()?;
    let mut revisions = load(&path()?, &key)?
        .revisions
        .into_iter()
        .filter(|revision| revision.note_id == note_id)
        .collect::<Vec<_>>();
    revisions.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(revisions)
}

pub fn list_saved_searches() -> Result<Vec<OslSavedSearch>, String> {
    let key = unlocked_key()?;
    Ok(load(&path()?, &key)?.saved_searches)
}

pub fn save_search(input: OslSavedSearchInput) -> Result<OslSavedSearch, String> {
    validate_saved_search_input(&input)?;
    let key = unlocked_key()?;
    let path = path()?;
    let mut document = load(&path, &key)?;
    if document.saved_searches.len() >= MAX_SAVED_SEARCHES {
        return Err("OSL Notes reached its saved-search limit".into());
    }
    let created_at = now_ms()?;
    let mut ordinal = document.saved_searches.len();
    let id = loop {
        let candidate = note_id(created_at, &input.name, &input.query, ordinal);
        if !document.saved_searches.iter().any(|search| search.id == candidate) {
            break candidate;
        }
        ordinal += 1;
    };
    let search = OslSavedSearch { id, name: input.name, query: input.query, filter: input.filter, created_at };
    document.saved_searches.push(search.clone());
    save(&path, &document, &key)?;
    Ok(search)
}

pub fn delete_saved_search(id: &str) -> Result<bool, String> {
    valid_id(id)?;
    let key = unlocked_key()?;
    let path = path()?;
    let mut document = load(&path, &key)?;
    let before = document.saved_searches.len();
    document.saved_searches.retain(|search| search.id != id);
    if document.saved_searches.len() == before { return Ok(false); }
    save(&path, &document, &key)?;
    Ok(true)
}

pub fn upsert(input: OslNoteInput) -> Result<OslNote, String> {
    validate_input(&input)?;
    let key = unlocked_key()?;
    let path = path()?;
    let mut document = load(&path, &key)?;
    let now = now_ms()?;
    let (id, created_at, deleted_at, previous) = match input.id {
        Some(id) => {
            valid_id(&id)?;
            let existing = document
                .notes
                .iter()
                .find(|note| note.id == id)
                .ok_or_else(|| "That OSL note no longer exists".to_owned())?;
            (
                id,
                existing.created_at,
                existing.deleted_at,
                Some(existing.clone()),
            )
        }
        None => {
            if document.notes.len() >= MAX_NOTES {
                return Err("OSL Notes reached its local note limit".into());
            }
            let mut ordinal = document.notes.len();
            let id = loop {
                let candidate = note_id(now, &input.title, &input.body, ordinal);
                if !document.notes.iter().any(|note| note.id == candidate) {
                    break candidate;
                }
                ordinal += 1;
            };
            (id, now, None, None)
        }
    };
    let note = OslNote {
        id: id.clone(),
        kind: input.kind,
        title: input.title,
        body: input.body,
        folder: input.folder,
        tags: input.tags,
        favorite: input.favorite,
        pinned: input.pinned,
        created_at,
        updated_at: now,
        deleted_at,
    };
    if let Some(existing) = previous
        .as_ref()
        .filter(|existing| content_changed(existing, &note))
    {
        push_revision(&mut document, existing, now);
    }
    if let Some(existing) = document
        .notes
        .iter_mut()
        .find(|candidate| candidate.id == id)
    {
        *existing = note.clone();
    } else {
        document.notes.push(note.clone());
    }
    sort(&mut document.notes);
    save(&path, &document, &key)?;
    Ok(note)
}

pub fn trash(id: &str) -> Result<OslNote, String> {
    set_deleted(id, Some(now_ms()?))
}
pub fn restore(id: &str) -> Result<OslNote, String> {
    set_deleted(id, None)
}

fn set_deleted(id: &str, deleted_at: Option<u64>) -> Result<OslNote, String> {
    valid_id(id)?;
    let key = unlocked_key()?;
    let path = path()?;
    let mut document = load(&path, &key)?;
    let now = now_ms()?;
    let note = document
        .notes
        .iter_mut()
        .find(|note| note.id == id)
        .ok_or_else(|| "That OSL note no longer exists".to_owned())?;
    note.deleted_at = deleted_at;
    note.updated_at = now;
    let result = note.clone();
    sort(&mut document.notes);
    save(&path, &document, &key)?;
    Ok(result)
}

pub fn permanently_delete(id: &str) -> Result<bool, String> {
    valid_id(id)?;
    let key = unlocked_key()?;
    let path = path()?;
    let mut document = load(&path, &key)?;
    let before = document.notes.len();
    document
        .notes
        .retain(|note| note.id != id || note.deleted_at.is_none());
    if before == document.notes.len() {
        return Ok(false);
    }
    document.revisions.retain(|revision| revision.note_id != id);
    save(&path, &document, &key)?;
    Ok(true)
}

pub fn restore_revision(note_id: &str, revision_id: &str) -> Result<OslNote, String> {
    valid_id(note_id)?;
    valid_id(revision_id)?;
    let key = unlocked_key()?;
    let path = path()?;
    let mut document = load(&path, &key)?;
    let revision = document
        .revisions
        .iter()
        .find(|revision| revision.note_id == note_id && revision.id == revision_id)
        .cloned()
        .ok_or_else(|| "That encrypted version no longer exists".to_owned())?;
    let now = now_ms()?;
    let current = document
        .notes
        .iter()
        .find(|note| note.id == note_id)
        .cloned()
        .ok_or_else(|| "That OSL note no longer exists".to_owned())?;
    push_revision(&mut document, &current, now);
    let note = document
        .notes
        .iter_mut()
        .find(|note| note.id == note_id)
        .expect("note checked above");
    note.kind = revision.kind;
    note.title = revision.title;
    note.body = revision.body;
    note.folder = revision.folder;
    note.tags = revision.tags;
    note.favorite = revision.favorite;
    note.updated_at = now;
    let result = note.clone();
    sort(&mut document.notes);
    save(&path, &document, &key)?;
    Ok(result)
}

fn path() -> Result<PathBuf, String> {
    keystore::osl_config_dir()
        .map(|dir| dir.join(FILE_NAME))
        .map_err(|_| "OSL Notes storage is unavailable".into())
}

fn unlocked_key() -> Result<[u8; 32], String> {
    ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "Unlock your OSL identity before opening Notes".into())
}

fn load(path: &Path, key: &[u8; 32]) -> Result<Document, String> {
    let Some(sealed) =
        crate::atomic_file::read_recoverable_bounded(path, MAX_FILE_BYTES, "encrypted OSL Notes")?
    else {
        return Ok(Document {
            version: VERSION,
            notes: vec![],
            revisions: vec![],
            saved_searches: vec![],
        });
    };
    if !ipc::main_password::has_enc_magic(&sealed) {
        return Err("OSL Notes storage is not encrypted".into());
    }
    let mut plain = ipc::main_password::decrypt_at_rest(&sealed, key)
        .map_err(|_| "OSL Notes could not be authenticated".to_owned())?;
    let parsed = serde_json::from_slice::<Document>(&plain)
        .map_err(|_| "OSL Notes storage is malformed".to_owned());
    plain.zeroize();
    let document = parsed?;
    validate_document(&document)?;
    Ok(document)
}

fn save(path: &Path, document: &Document, key: &[u8; 32]) -> Result<(), String> {
    validate_document(document)?;
    let mut plain =
        serde_json::to_vec(document).map_err(|_| "OSL Notes could not be encoded".to_owned())?;
    let sealed = ipc::main_password::encrypt_at_rest(&plain, key)
        .map_err(|_| "OSL Notes could not be encrypted".to_owned())?;
    plain.zeroize();
    if sealed.len() as u64 > MAX_FILE_BYTES {
        return Err("OSL Notes exceeds its local storage limit".into());
    }
    crate::atomic_file::write_recoverable(path, &sealed, "encrypted OSL Notes")
}

fn validate_input(input: &OslNoteInput) -> Result<(), String> {
    if input.title.chars().count() > 120 || input.title.contains('\0') {
        return Err("A note title must be at most 120 characters".into());
    }
    if input.body.len() > MAX_BODY_BYTES || input.body.contains('\0') {
        return Err("A note body must be at most 256 KiB".into());
    }
    if input.folder.chars().count() > 80 || input.folder.chars().any(char::is_control) {
        return Err("A folder name must be at most 80 characters".into());
    }
    if input.tags.len() > 16 {
        return Err("A note can have at most 16 tags".into());
    }
    let mut unique = BTreeSet::new();
    for tag in &input.tags {
        let folded = tag.to_lowercase();
        if tag.is_empty()
            || tag.chars().count() > 32
            || tag.chars().any(char::is_control)
            || !unique.insert(folded)
        {
            return Err("Note tags must be unique and at most 32 characters".into());
        }
    }
    Ok(())
}

fn validate_saved_search_input(input: &OslSavedSearchInput) -> Result<(), String> {
    if input.name.is_empty() || input.name.chars().count() > 80 || input.name.chars().any(char::is_control)
        || input.query.chars().count() > 240 || input.query.contains('\0')
        || !valid_search_filter(&input.filter)
    {
        return Err("A saved search has an invalid name, query, or filter".into());
    }
    Ok(())
}

fn valid_search_filter(filter: &str) -> bool {
    matches!(filter, "all" | "favorites" | "trash")
        || filter.strip_prefix("type:").is_some_and(|kind| matches!(kind, "note" | "document" | "spreadsheet" | "drawing" | "presentation" | "photo" | "video" | "audio" | "model3d"))
        || filter.strip_prefix("folder:").is_some_and(|folder| !folder.is_empty() && folder.chars().count() <= 80 && !folder.chars().any(char::is_control))
        || filter.strip_prefix("tag:").is_some_and(|tag| !tag.is_empty() && tag.chars().count() <= 32 && !tag.chars().any(char::is_control))
}

fn validate_document(document: &Document) -> Result<(), String> {
    if document.version != VERSION
        || document.notes.len() > MAX_NOTES
        || document.revisions.len() > MAX_REVISIONS
        || document.saved_searches.len() > MAX_SAVED_SEARCHES
    {
        return Err("OSL Notes storage has an unsupported shape".into());
    }
    let mut ids = BTreeSet::new();
    for note in &document.notes {
        valid_id(&note.id)?;
        validate_input(&OslNoteInput {
            id: Some(note.id.clone()),
            kind: note.kind,
            title: note.title.clone(),
            body: note.body.clone(),
            folder: note.folder.clone(),
            tags: note.tags.clone(),
            favorite: note.favorite,
            pinned: note.pinned,
        })?;
        if note.created_at == 0
            || note.updated_at < note.created_at
            || note.deleted_at.is_some_and(|value| value < note.created_at)
            || !ids.insert(&note.id)
        {
            return Err("OSL Notes storage is malformed".into());
        }
    }
    let note_ids = document
        .notes
        .iter()
        .map(|note| note.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut revision_ids = BTreeSet::new();
    let mut revision_counts = std::collections::BTreeMap::<&str, usize>::new();
    for revision in &document.revisions {
        valid_id(&revision.id)?;
        valid_id(&revision.note_id)?;
        validate_input(&OslNoteInput {
            id: Some(revision.note_id.clone()),
            kind: revision.kind,
            title: revision.title.clone(),
            body: revision.body.clone(),
            folder: revision.folder.clone(),
            tags: revision.tags.clone(),
            favorite: revision.favorite,
            pinned: revision.pinned,
        })?;
        let count = revision_counts.entry(&revision.note_id).or_default();
        *count += 1;
        if revision.created_at == 0
            || !note_ids.contains(revision.note_id.as_str())
            || !revision_ids.insert(&revision.id)
            || *count > MAX_REVISIONS_PER_NOTE
        {
            return Err("OSL Notes version history is malformed".into());
        }
    }
    let mut search_ids = BTreeSet::new();
    for search in &document.saved_searches {
        valid_id(&search.id)?;
        validate_saved_search_input(&OslSavedSearchInput { name: search.name.clone(), query: search.query.clone(), filter: search.filter.clone() })?;
        if search.created_at == 0 || !search_ids.insert(&search.id) { return Err("OSL Notes saved searches are malformed".into()); }
    }
    Ok(())
}

fn content_changed(note: &OslNote, next: &OslNote) -> bool {
    note.kind != next.kind
        || note.title != next.title
        || note.body != next.body
        || note.folder != next.folder
        || note.tags != next.tags
        || note.favorite != next.favorite
        || note.pinned != next.pinned
}

fn push_revision(document: &mut Document, note: &OslNote, created_at: u64) {
    let id = revision_id(note, created_at, document.revisions.len());
    document.revisions.push(OslNoteRevision {
        id,
        note_id: note.id.clone(),
        kind: note.kind,
        title: note.title.clone(),
        body: note.body.clone(),
        folder: note.folder.clone(),
        tags: note.tags.clone(),
        favorite: note.favorite,
        pinned: note.pinned,
        created_at,
    });
    let mut matching = document
        .revisions
        .iter()
        .filter(|revision| revision.note_id == note.id)
        .map(|revision| (revision.created_at, revision.id.clone()))
        .collect::<Vec<_>>();
    matching.sort();
    let remove = matching.len().saturating_sub(MAX_REVISIONS_PER_NOTE);
    let expired = matching
        .into_iter()
        .take(remove)
        .map(|(_, id)| id)
        .collect::<BTreeSet<_>>();
    document
        .revisions
        .retain(|revision| !expired.contains(&revision.id));
    if document.revisions.len() > MAX_REVISIONS {
        document
            .revisions
            .sort_by(|left, right| right.created_at.cmp(&left.created_at));
        document.revisions.truncate(MAX_REVISIONS);
    }
}

fn valid_id(id: &str) -> Result<(), String> {
    if id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("OSL note identity is invalid".into())
    }
}

fn note_id(now: u64, title: &str, body: &str, ordinal: usize) -> String {
    let mut digest = Sha256::new();
    digest.update(b"OSL-NOTE-v1");
    digest.update(now.to_le_bytes());
    digest.update(ordinal.to_le_bytes());
    digest.update(title.as_bytes());
    digest.update(body.as_bytes());
    format!("{:x}", digest.finalize())[..32].to_owned()
}

fn revision_id(note: &OslNote, now: u64, ordinal: usize) -> String {
    let mut digest = Sha256::new();
    digest.update(b"OSL-NOTE-REVISION-v1");
    digest.update(note.id.as_bytes());
    digest.update(now.to_le_bytes());
    digest.update(ordinal.to_le_bytes());
    digest.update(note.body.as_bytes());
    format!("{:x}", digest.finalize())[..32].to_owned()
}

fn sort(notes: &mut [OslNote]) {
    notes.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.id.cmp(&right.id))
    });
}
fn now_ms() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .map_err(|_| "The system clock is unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_are_bounded_and_distinct() {
        assert_ne!(note_id(1, "a", "b", 0), note_id(1, "a", "b", 1));
        assert_eq!(note_id(1, "a", "b", 0).len(), 32);
    }
    #[test]
    fn rejects_duplicate_tags_and_oversized_bodies() {
        let duplicate = OslNoteInput {
            id: None,
            kind: OslDocumentKind::Note,
            title: "x".into(),
            body: "".into(),
            folder: "".into(),
            tags: vec!["Idea".into(), "idea".into()],
            favorite: false,
            pinned: false,
        };
        assert!(validate_input(&duplicate).is_err());
        let huge = OslNoteInput {
            body: "x".repeat(MAX_BODY_BYTES + 1),
            tags: vec![],
            ..duplicate
        };
        assert!(validate_input(&huge).is_err());
    }
    #[test]
    fn encrypted_round_trip_has_no_plaintext() {
        let note = OslNote {
            id: note_id(1, "secret", "private", 0),
            kind: OslDocumentKind::Note,
            title: "secret".into(),
            body: "private".into(),
            folder: "".into(),
            tags: vec![],
            favorite: false,
            pinned: false,
            created_at: 1,
            updated_at: 1,
            deleted_at: None,
        };
        let plain = serde_json::to_vec(&Document {
            version: VERSION,
            notes: vec![note.clone()],
            revisions: vec![],
            saved_searches: vec![],
        })
        .unwrap();
        let sealed = ipc::main_password::encrypt_at_rest(&plain, &[7; 32]).unwrap();
        assert!(!String::from_utf8_lossy(&sealed).contains("private"));
        let opened = ipc::main_password::decrypt_at_rest(&sealed, &[7; 32]).unwrap();
        assert_eq!(
            serde_json::from_slice::<Document>(&opened).unwrap().notes,
            vec![note]
        );
        assert!(ipc::main_password::decrypt_at_rest(&sealed, &[8; 32]).is_err());
    }

    #[test]
    fn existing_notes_without_a_kind_migrate_to_note() {
        let legacy = r#"{"id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","title":"Legacy","body":"text","folder":"","tags":[],"favorite":false,"createdAt":1,"updatedAt":1,"deletedAt":null}"#;
        let note: OslNote = serde_json::from_str(legacy).unwrap();
        assert_eq!(note.kind, OslDocumentKind::Note);
        assert!(!note.pinned);
        assert_eq!(
            serde_json::to_string(&OslDocumentKind::Model3d).unwrap(),
            "\"model3d\""
        );
    }

    #[test]
    fn version_history_is_bounded_and_keeps_complete_encrypted_snapshots() {
        let note = OslNote {
            id: note_id(1, "Original", "private", 0),
            kind: OslDocumentKind::Document,
            title: "Original".into(),
            body: "private".into(),
            folder: "Work".into(),
            tags: vec!["draft".into()],
            favorite: true,
            pinned: true,
            created_at: 1,
            updated_at: 1,
            deleted_at: None,
        };
        let mut document = Document {
            version: VERSION,
            notes: vec![note.clone()],
            revisions: vec![],
            saved_searches: vec![],
        };
        for timestamp in 2..=60 {
            push_revision(&mut document, &note, timestamp);
        }
        assert_eq!(document.revisions.len(), MAX_REVISIONS_PER_NOTE);
        assert_eq!(document.revisions.last().unwrap().body, "private");
        assert!(validate_document(&document).is_ok());
        let plain = serde_json::to_vec(&document).unwrap();
        let sealed = ipc::main_password::encrypt_at_rest(&plain, &[7; 32]).unwrap();
        assert!(!String::from_utf8_lossy(&sealed).contains("private"));
    }
}
