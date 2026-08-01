//! Bounded, cover-only context retained for optional cover selection.
//!
//! This module deliberately has no constructor that accepts `String` or
//! `&str` as context. Callers can retain only [`CoverText`], an opaque value
//! obtained from a completed canonical carrier render. Scope bindings are
//! immediately hashed and are used as AEAD associated data; contact names and
//! identifiers are not retained.

use std::collections::HashMap;

/// Maximum number of independently scoped cover conversations retained.
pub const MAX_SCOPES: usize = 32;
/// Maximum rendered-cover turns retained for one scope.
pub const MAX_TURNS: usize = 12;
/// Covers are short carrier text, never arbitrary document-sized context.
pub const MAX_COVER_BYTES: usize = 1_024;
/// Context is useful only for a short, visible conversation window.
pub const MAX_RETENTION_SECONDS: u64 = 24 * 60 * 60;

const HISTORY_AAD_DOMAIN: &[u8] = b"osl-cover-ai-history-v1";

/// A canonical cover already rendered by the carrier codec.
///
/// The inner text is intentionally private. A plaintext message cannot be
/// substituted at the `CoverHistory::record` boundary by accident.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoverText(String);

impl CoverText {
    /// Accept carrier output after the renderer has completed its exact-cover
    /// verification. The adapter which owns that verification is the sole
    /// caller of this constructor.
    pub(crate) fn from_verified_render(cover: String) -> Result<Self, HistoryError> {
        validate_cover(&cover)?;
        Ok(Self(cover))
    }

    /// Expose text only to the local selection/model invocation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A salted/hashed conversation binding. Raw account, contact, and channel
/// identifiers must never enter the history map.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CoverScope([u8; 32]);

impl CoverScope {
    pub fn from_hash(scope_hash: [u8; 32]) -> Self {
        Self(scope_hash)
    }
}

/// Ciphertext plus the nonce produced by the application's AEAD implementation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SealedHistory {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

/// The store requires authenticated encryption, but does not choose a cipher
/// or hold a key. The shipping adapter uses the application's AES-GCM key;
/// keeping the primitive outside this crate prevents a second key hierarchy.
pub trait CoverHistoryAead {
    type Error;

    fn seal(&self, aad: &[u8], plaintext: &[u8]) -> Result<SealedHistory, Self::Error>;
    fn open(&self, aad: &[u8], sealed: &SealedHistory) -> Result<Vec<u8>, Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EncryptedTranscript {
    sealed: SealedHistory,
    expires_at: u64,
}

/// In-memory index of AEAD-sealed cover-only transcripts.
pub struct CoverHistory<A> {
    aead: A,
    records: HashMap<CoverScope, EncryptedTranscript>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HistoryError {
    InvalidCover,
    InvalidTranscript,
    Aead,
}

impl<A: CoverHistoryAead> CoverHistory<A> {
    pub fn new(aead: A) -> Self {
        Self {
            aead,
            records: HashMap::new(),
        }
    }

    /// Add one verified carrier render, retaining at most twelve recent turns.
    /// `now_seconds` is supplied by the caller to make expiry deterministic and
    /// avoid this privacy boundary reading system state itself.
    pub fn record(
        &mut self,
        scope: CoverScope,
        cover: CoverText,
        ttl_seconds: u32,
        now_seconds: u64,
    ) -> Result<(), HistoryError> {
        self.purge_expired(now_seconds);
        let mut transcript = self.history(scope, now_seconds)?;
        transcript.push(cover);
        if transcript.len() > MAX_TURNS {
            transcript.drain(..transcript.len() - MAX_TURNS);
        }

        let plaintext = encode_transcript(&transcript)?;
        let sealed = self
            .aead
            .seal(&history_aad(scope), &plaintext)
            .map_err(|_| HistoryError::Aead)?;
        let expires_at =
            now_seconds.saturating_add(u64::from(ttl_seconds.max(1)).min(MAX_RETENTION_SECONDS));

        if !self.records.contains_key(&scope) && self.records.len() >= MAX_SCOPES {
            if let Some(oldest_scope) = self
                .records
                .iter()
                .min_by_key(|(_, record)| record.expires_at)
                .map(|(scope, _)| *scope)
            {
                self.records.remove(&oldest_scope);
            }
        }
        self.records
            .insert(scope, EncryptedTranscript { sealed, expires_at });
        Ok(())
    }

    /// Returns only previously stored cover text, never a platform transcript.
    pub fn history(
        &mut self,
        scope: CoverScope,
        now_seconds: u64,
    ) -> Result<Vec<CoverText>, HistoryError> {
        self.purge_expired(now_seconds);
        let Some(record) = self.records.get(&scope) else {
            return Ok(Vec::new());
        };
        let plaintext = self
            .aead
            .open(&history_aad(scope), &record.sealed)
            .map_err(|_| HistoryError::Aead)?;
        decode_transcript(&plaintext)
    }

    pub fn burn_scope(&mut self, scope: CoverScope) {
        self.records.remove(&scope);
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }

    fn purge_expired(&mut self, now_seconds: u64) {
        self.records
            .retain(|_, record| record.expires_at > now_seconds);
    }

    #[cfg(test)]
    pub(crate) fn sealed_for_test(&self, scope: CoverScope) -> Option<&SealedHistory> {
        self.records.get(&scope).map(|record| &record.sealed)
    }
}

fn validate_cover(cover: &str) -> Result<(), HistoryError> {
    if cover.is_empty() || cover.len() > MAX_COVER_BYTES || cover.chars().any(char::is_control) {
        Err(HistoryError::InvalidCover)
    } else {
        Ok(())
    }
}

fn encode_transcript(transcript: &[CoverText]) -> Result<Vec<u8>, HistoryError> {
    if transcript.len() > MAX_TURNS {
        return Err(HistoryError::InvalidTranscript);
    }
    let mut encoded = Vec::new();
    for cover in transcript {
        validate_cover(cover.as_str())?;
        let length = u16::try_from(cover.0.len()).map_err(|_| HistoryError::InvalidTranscript)?;
        encoded.extend_from_slice(&length.to_le_bytes());
        encoded.extend_from_slice(cover.0.as_bytes());
    }
    Ok(encoded)
}

fn decode_transcript(encoded: &[u8]) -> Result<Vec<CoverText>, HistoryError> {
    let mut transcript = Vec::new();
    let mut cursor = 0;
    while cursor < encoded.len() {
        if encoded.len() - cursor < 2 || transcript.len() == MAX_TURNS {
            return Err(HistoryError::InvalidTranscript);
        }
        let length = usize::from(u16::from_le_bytes([encoded[cursor], encoded[cursor + 1]]));
        cursor += 2;
        let end = cursor
            .checked_add(length)
            .filter(|end| *end <= encoded.len())
            .ok_or(HistoryError::InvalidTranscript)?;
        let cover = std::str::from_utf8(&encoded[cursor..end])
            .map_err(|_| HistoryError::InvalidTranscript)?;
        transcript.push(CoverText::from_verified_render(cover.to_owned())?);
        cursor = end;
    }
    Ok(transcript)
}

fn history_aad(scope: CoverScope) -> Vec<u8> {
    [HISTORY_AAD_DOMAIN, &scope.0].concat()
}
