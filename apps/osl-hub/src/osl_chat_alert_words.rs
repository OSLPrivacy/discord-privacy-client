//! Device-local, per-chat alert-word settings.
//!
//! This module deliberately has no transport parameter, relay client, or wire
//! representation. Callers hand [`AlertWordStore::matches_decrypted_text`] text
//! only after decrypting it on this device. The sole serialized representation
//! is the bounded local settings document written to the path supplied at open.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use zeroize::Zeroize as _;

/// Copy shown beside the per-chat setting.
pub const PRIVACY_STATEMENT: &str =
    "Matching runs on this device against already-decrypted text. The list never leaves the machine.";

const DOCUMENT_VERSION: u8 = 1;
const MAX_STORE_BYTES: u64 = 64 * 1024;
const MAX_CHAT_ID_BYTES: usize = 512;
const MAX_CHATS: usize = 512;
const MAX_WORDS_PER_CHAT: usize = 32;
const MAX_WORD_BYTES: usize = 128;
const STORE_LABEL: &str = "OSL Chat alert-word settings";

#[derive(Debug, Eq, PartialEq)]
pub enum AlertWordError {
    InvalidChatId,
    InvalidWordList,
    Storage,
}

impl fmt::Display for AlertWordError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidChatId => "the chat identifier is invalid",
            Self::InvalidWordList => "the alert-word list is invalid",
            Self::Storage => "the alert-word settings could not be stored",
        })
    }
}

impl std::error::Error for AlertWordError {}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AlertWordDocument {
    version: u8,
    /// Chat identifiers remain JSON map keys. They are never joined to a path.
    chats: BTreeMap<String, Vec<String>>,
}

/// Durable local alert-word settings for every chat in one profile.
///
/// The type exposes no serialized document and accepts no networking object,
/// which keeps the list out of relay request construction by construction.
pub struct AlertWordStore {
    path: PathBuf,
    document: AlertWordDocument,
}

impl AlertWordStore {
    /// Opens a fixed local settings file, or starts with an empty document when
    /// it does not exist. Existing content is size-bounded before it is read.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AlertWordError> {
        let path = path.as_ref().to_path_buf();
        let document =
            match crate::atomic_file::read_recoverable_bounded(&path, MAX_STORE_BYTES, STORE_LABEL)
                .map_err(|_| AlertWordError::Storage)?
            {
                Some(bytes) => {
                    let mut plain = ipc::main_password::maybe_decrypt_file(&path, &bytes)
                        .map_err(|_| AlertWordError::Storage)?;
                    let parsed =
                        serde_json::from_slice(&plain).map_err(|_| AlertWordError::Storage);
                    plain.zeroize();
                    let document: AlertWordDocument = parsed?;
                    validate_document(&document)?;
                    document
                }
                None => AlertWordDocument {
                    version: DOCUMENT_VERSION,
                    chats: BTreeMap::new(),
                },
            };

        Ok(Self { path, document })
    }

    /// Atomically replaces one chat's list. Input spelling and case are kept
    /// exactly; invalid whitespace, duplicate, or over-limit words are rejected
    /// instead of silently normalizing user intent.
    pub fn save_words(&mut self, chat_id: &str, words: Vec<String>) -> Result<(), AlertWordError> {
        validate_chat_id(chat_id)?;
        validate_words(&words)?;

        let previous = if words.is_empty() {
            self.document.chats.remove(chat_id)
        } else {
            self.document.chats.insert(chat_id.to_owned(), words)
        };

        if let Err(error) = self.persist() {
            match previous {
                Some(previous) => {
                    self.document.chats.insert(chat_id.to_owned(), previous);
                }
                None => {
                    self.document.chats.remove(chat_id);
                }
            }
            return Err(error);
        }
        Ok(())
    }

    /// Returns the exact saved strings for this chat in their saved order.
    pub fn words_for_chat(&self, chat_id: &str) -> Result<&[String], AlertWordError> {
        validate_chat_id(chat_id)?;
        Ok(self
            .document
            .chats
            .get(chat_id)
            .map(Vec::as_slice)
            .unwrap_or_default())
    }

    /// Matches complete Unicode alphanumeric/underscore words, ignoring case.
    ///
    /// `already_decrypted_text` is the only message input. A listed word found
    /// merely inside a larger word is not an alert.
    pub fn matches_decrypted_text(
        &self,
        chat_id: &str,
        already_decrypted_text: &str,
    ) -> Result<bool, AlertWordError> {
        let words = self.words_for_chat(chat_id)?;
        if words.is_empty() {
            return Ok(false);
        }

        let wanted: BTreeSet<String> = words.iter().map(|word| word.to_lowercase()).collect();
        Ok(already_decrypted_text
            .split(|character: char| !is_word_character(character))
            .filter(|candidate| !candidate.is_empty())
            .any(|candidate| wanted.contains(&candidate.to_lowercase())))
    }

    fn persist(&self) -> Result<(), AlertWordError> {
        if self.document.chats.len() > MAX_CHATS {
            return Err(AlertWordError::Storage);
        }
        let mut plain = serde_json::to_vec(&self.document).map_err(|_| AlertWordError::Storage)?;
        let sealed = ipc::main_password::maybe_encrypt(&plain).map_err(|_| AlertWordError::Storage);
        plain.zeroize();
        let sealed = sealed?;
        if sealed.len() as u64 > MAX_STORE_BYTES {
            return Err(AlertWordError::Storage);
        }
        crate::atomic_file::write_recoverable(&self.path, &sealed, STORE_LABEL)
            .map_err(|_| AlertWordError::Storage)
    }
}

fn validate_document(document: &AlertWordDocument) -> Result<(), AlertWordError> {
    if document.version != DOCUMENT_VERSION || document.chats.len() > MAX_CHATS {
        return Err(AlertWordError::Storage);
    }
    for (chat_id, words) in &document.chats {
        validate_chat_id(chat_id).map_err(|_| AlertWordError::Storage)?;
        validate_words(words).map_err(|_| AlertWordError::Storage)?;
        if words.is_empty() {
            return Err(AlertWordError::Storage);
        }
    }
    Ok(())
}

fn validate_chat_id(chat_id: &str) -> Result<(), AlertWordError> {
    if chat_id.is_empty()
        || chat_id.len() > MAX_CHAT_ID_BYTES
        || chat_id.chars().any(char::is_control)
    {
        return Err(AlertWordError::InvalidChatId);
    }
    Ok(())
}

fn validate_words(words: &[String]) -> Result<(), AlertWordError> {
    if words.len() > MAX_WORDS_PER_CHAT {
        return Err(AlertWordError::InvalidWordList);
    }

    let mut normalized = BTreeSet::new();
    for word in words {
        if word.is_empty()
            || word.len() > MAX_WORD_BYTES
            || !word.chars().all(is_word_character)
            || !normalized.insert(word.to_lowercase())
        {
            return Err(AlertWordError::InvalidWordList);
        }
    }
    Ok(())
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}
