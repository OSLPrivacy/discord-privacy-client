//! Authenticated, encrypted, identity-scoped storage for the local Notes app.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroize;

const FILE_NAME: &str = "osl_notes.bin";
const VERSION: u8 = 1;
const MAX_NOTES: usize = 5_000;
const MAX_TITLE_BYTES: usize = 720;
const MAX_BODY_BYTES: usize = 256 * 1024;
const MAX_PLAINTEXT_BYTES: usize = 64 * 1024 * 1024;
const MAX_SEALED_BYTES: u64 = (MAX_PLAINTEXT_BYTES + 8 * 1024) as u64;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OslNote {
    pub id: String,
    pub title: String,
    pub body: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OslNoteInput {
    pub id: Option<String>,
    pub title: String,
    pub body: String,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NotesDocument {
    version: u8,
    notes: Vec<OslNote>,
}

pub fn list() -> Result<Vec<OslNote>, String> {
    let mut key = unlocked_key()?;
    let result = load(&path()?, &key).map(|document| document.notes);
    key.zeroize();
    result
}

pub fn upsert(input: OslNoteInput) -> Result<OslNote, String> {
    validate_text(&input.title, MAX_TITLE_BYTES, "title")?;
    validate_text(&input.body, MAX_BODY_BYTES, "body")?;
    let mut key = unlocked_key()?;
    let file = path()?;
    let mut document = load(&file, &key)?;
    let now = now()?;
    let saved = if let Some(id) = input.id {
        validate_id(&id)?;
        let note = document
            .notes
            .iter_mut()
            .find(|note| note.id == id)
            .ok_or_else(|| "OSL note was not found".to_owned())?;
        note.title = input.title;
        note.body = input.body;
        note.updated_at = now.max(note.created_at);
        note.clone()
    } else {
        if document.notes.len() >= MAX_NOTES {
            key.zeroize();
            return Err("OSL Notes reached its local note limit".to_owned());
        }
        let note = OslNote {
            id: random_id()?,
            title: input.title,
            body: input.body,
            created_at: now,
            updated_at: now,
        };
        document.notes.push(note.clone());
        note
    };
    document
        .notes
        .sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    let result = save(&file, &document, &key).map(|()| saved);
    key.zeroize();
    result
}

pub fn delete(id: &str) -> Result<bool, String> {
    validate_id(id)?;
    let mut key = unlocked_key()?;
    let file = path()?;
    let mut document = load(&file, &key)?;
    let before = document.notes.len();
    document.notes.retain(|note| note.id != id);
    let changed = before != document.notes.len();
    let result = if changed {
        save(&file, &document, &key).map(|()| true)
    } else {
        Ok(false)
    };
    key.zeroize();
    result
}

fn unlocked_key() -> Result<[u8; 32], String> {
    ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "Unlock the OSL main password before opening Notes".to_owned())
}

fn path() -> Result<PathBuf, String> {
    keystore::active_account_dir()
        .map(|directory| directory.join(FILE_NAME))
        .ok_or_else(|| "OSL active identity storage is unavailable".to_owned())
}

fn load(path: &Path, key: &[u8; 32]) -> Result<NotesDocument, String> {
    let Some(sealed) = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_SEALED_BYTES,
        "OSL Notes",
    )? else {
        return Ok(NotesDocument { version: VERSION, notes: Vec::new() });
    };
    if !ipc::main_password::has_enc_magic(&sealed) {
        return Err("OSL Notes storage is not encrypted".to_owned());
    }
    let mut plaintext = ipc::main_password::decrypt_at_rest(&sealed, key)
        .map_err(|_| "OSL Notes could not be decrypted".to_owned())?;
    if plaintext.len() > MAX_PLAINTEXT_BYTES {
        plaintext.zeroize();
        return Err("OSL Notes storage exceeds its limit".to_owned());
    }
    let decoded = serde_json::from_slice::<NotesDocument>(&plaintext);
    plaintext.zeroize();
    let document = decoded.map_err(|_| "OSL Notes storage is malformed".to_owned())?;
    validate_document(document)
}

fn save(path: &Path, document: &NotesDocument, key: &[u8; 32]) -> Result<(), String> {
    let mut plaintext = serde_json::to_vec(document)
        .map_err(|_| "OSL Notes could not be encoded".to_owned())?;
    if plaintext.len() > MAX_PLAINTEXT_BYTES {
        plaintext.zeroize();
        return Err("OSL Notes storage exceeds its limit".to_owned());
    }
    let encrypted = ipc::main_password::encrypt_at_rest(&plaintext, key)
        .map_err(|_| "OSL Notes encryption failed".to_owned());
    plaintext.zeroize();
    let sealed = encrypted?;
    if sealed.len() as u64 > MAX_SEALED_BYTES {
        return Err("OSL Notes encrypted storage exceeds its limit".to_owned());
    }
    crate::atomic_file::write_recoverable(path, &sealed, "OSL Notes")
}

fn validate_document(document: NotesDocument) -> Result<NotesDocument, String> {
    if document.version != VERSION || document.notes.len() > MAX_NOTES {
        return Err("OSL Notes storage has an unsupported version".to_owned());
    }
    let mut ids = std::collections::BTreeSet::new();
    for note in &document.notes {
        validate_id(&note.id)?;
        validate_text(&note.title, MAX_TITLE_BYTES, "title")?;
        validate_text(&note.body, MAX_BODY_BYTES, "body")?;
        if note.created_at == 0 || note.updated_at < note.created_at || !ids.insert(&note.id) {
            return Err("OSL Notes storage is malformed".to_owned());
        }
    }
    Ok(document)
}

fn validate_text(value: &str, maximum: usize, field: &str) -> Result<(), String> {
    if value.as_bytes().len() > maximum || value.contains('\0') {
        return Err(format!("OSL note {field} is invalid or too large"));
    }
    Ok(())
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.len() != 32 || !id.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
        return Err("OSL note identifier is invalid".to_owned());
    }
    Ok(())
}

fn now() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .map_err(|_| "OSL Notes clock is unavailable".to_owned())
}

fn random_id() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| "OSL Notes identifier generation failed".to_owned())?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_inputs_and_documents_fail_closed() {
        assert!(validate_text("private", MAX_BODY_BYTES, "body").is_ok());
        assert!(validate_text("bad\0text", MAX_BODY_BYTES, "body").is_err());
        assert!(validate_id(&"a".repeat(32)).is_ok());
        assert!(validate_id("../identity").is_err());
    }
}
