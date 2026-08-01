//! Local-only encrypted cover history.
//!
//! This component never accepts private message text, Discord history, or
//! account metadata. Its bounded, per-scope transcript contains only cover
//! text that the platform already holds and remains AEAD-encrypted in memory.
//! It deliberately does not generate cover: every tier uses the same carrier
//! path.

use crypto::aes_gcm::{self, Key, Nonce};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_SCOPES: usize = 32;
const MAX_TURNS: usize = 12;
const MAX_COVER_BYTES: usize = 1_024;
const MAX_TRANSCRIPT_BYTES: usize = MAX_TURNS * (MAX_COVER_BYTES + 2);
const MAX_RETENTION_SECONDS: u64 = 24 * 60 * 60;
const COVER_AAD_DOMAIN: &[u8] = b"osl-native-discord-local-cover-v1";

#[derive(Clone)]
struct EncryptedTranscript {
    nonce: Nonce,
    ciphertext: Vec<u8>,
    expires_at: u64,
}

pub struct LocalCoverState {
    key: Key,
    transcripts: Mutex<HashMap<[u8; 32], EncryptedTranscript>>,
}

impl Default for LocalCoverState {
    fn default() -> Self {
        let mut key = [0u8; aes_gcm::KEY_SIZE];
        key.copy_from_slice(&crypto::random::random_bytes(aes_gcm::KEY_SIZE));
        Self {
            key: Key::from_bytes(key),
            transcripts: Mutex::new(HashMap::new()),
        }
    }
}

impl LocalCoverState {
    /// Record cover text after it has been rendered by the shared carrier.
    /// Plaintext messages and third-party conversation history are never valid
    /// inputs here.
    pub fn record_cover(
        &self,
        scope_binding: &str,
        cover: &str,
        ttl_seconds: u32,
    ) -> Result<(), String> {
        validate_scope(scope_binding)?;
        if cover.is_empty() || cover.len() > MAX_COVER_BYTES || cover.chars().any(char::is_control)
        {
            return Err("The local cover text is invalid".to_owned());
        }
        let now = unix_seconds()?;
        let scope = scope_hash(scope_binding);
        let mut records = self
            .transcripts
            .lock()
            .map_err(|_| "The local cover conversation is unavailable".to_owned())?;
        records.retain(|_, record| record.expires_at > now);
        let mut transcript = records
            .get(&scope)
            .map(|record| self.open(&scope, record))
            .transpose()?
            .unwrap_or_default();
        transcript.push(cover.to_owned());
        if transcript.len() > MAX_TURNS {
            transcript.drain(..transcript.len() - MAX_TURNS);
        }
        let encoded = encode_transcript(&transcript)?;
        debug_assert!(encoded.len() <= MAX_TRANSCRIPT_BYTES);
        let expires_at =
            now.saturating_add(u64::from(ttl_seconds.max(1)).min(MAX_RETENTION_SECONDS));
        let (nonce, ciphertext) = aes_gcm::seal(&self.key, &cover_aad(&scope), &encoded)
            .map_err(|_| "The local cover conversation could not be protected".to_owned())?;
        if !records.contains_key(&scope) && records.len() >= MAX_SCOPES {
            if let Some(oldest) = records
                .iter()
                .min_by_key(|(_, record)| record.expires_at)
                .map(|(scope, _)| *scope)
            {
                records.remove(&oldest);
            }
        }
        records.insert(
            scope,
            EncryptedTranscript {
                nonce,
                ciphertext,
                expires_at,
            },
        );
        Ok(())
    }

    pub fn cover_history(&self, scope_binding: &str) -> Result<Vec<String>, String> {
        validate_scope(scope_binding)?;
        let now = unix_seconds()?;
        let scope = scope_hash(scope_binding);
        let mut records = self
            .transcripts
            .lock()
            .map_err(|_| "The local cover conversation is unavailable".to_owned())?;
        records.retain(|_, record| record.expires_at > now);
        records
            .get(&scope)
            .map(|record| self.open(&scope, record))
            .transpose()?
            .map_or_else(|| Ok(Vec::new()), Ok)
    }

    pub fn burn_scope(&self, scope_binding: &str) {
        if let Ok(mut records) = self.transcripts.lock() {
            records.remove(&scope_hash(scope_binding));
        }
    }

    pub fn clear(&self) {
        if let Ok(mut records) = self.transcripts.lock() {
            records.clear();
        }
    }

    fn open(&self, scope: &[u8; 32], record: &EncryptedTranscript) -> Result<Vec<String>, String> {
        let plaintext = aes_gcm::open(
            &self.key,
            &record.nonce,
            &cover_aad(scope),
            &record.ciphertext,
        )
        .map_err(|_| "The local cover conversation failed authentication".to_owned())?;
        decode_transcript(&plaintext)
    }

    #[cfg(test)]
    fn retained_scope_count(&self) -> usize {
        self.transcripts
            .lock()
            .map(|value| value.len())
            .unwrap_or(0)
    }

    #[cfg(test)]
    fn encrypted_transcript_for_test(&self, scope_binding: &str) -> Vec<u8> {
        let scope = scope_hash(scope_binding);
        self.transcripts
            .lock()
            .ok()
            .and_then(|records| records.get(&scope).cloned())
            .map(|record| record.ciphertext)
            .unwrap_or_default()
    }
}

fn validate_scope(scope_binding: &str) -> Result<(), String> {
    if scope_binding.is_empty()
        || scope_binding.len() > 512
        || scope_binding.chars().any(char::is_control)
    {
        return Err("The local cover scope is invalid".to_owned());
    }
    Ok(())
}

fn encode_transcript(transcript: &[String]) -> Result<Vec<u8>, String> {
    if transcript.len() > MAX_TURNS {
        return Err("The local cover conversation is invalid".to_owned());
    }
    let mut encoded = Vec::new();
    for cover in transcript {
        let bytes = cover.as_bytes();
        if bytes.is_empty() || bytes.len() > MAX_COVER_BYTES || cover.chars().any(char::is_control)
        {
            return Err("The local cover conversation is invalid".to_owned());
        }
        let length = u16::try_from(bytes.len())
            .map_err(|_| "The local cover conversation is invalid".to_owned())?;
        encoded.extend_from_slice(&length.to_le_bytes());
        encoded.extend_from_slice(bytes);
    }
    Ok(encoded)
}

fn decode_transcript(encoded: &[u8]) -> Result<Vec<String>, String> {
    if encoded.len() > MAX_TRANSCRIPT_BYTES {
        return Err("The local cover conversation is invalid".to_owned());
    }
    let mut transcript = Vec::new();
    let mut cursor = 0;
    while cursor < encoded.len() {
        if encoded.len() - cursor < 2 || transcript.len() == MAX_TURNS {
            return Err("The local cover conversation is invalid".to_owned());
        }
        let length = usize::from(u16::from_le_bytes([encoded[cursor], encoded[cursor + 1]]));
        cursor += 2;
        let end = cursor
            .checked_add(length)
            .filter(|end| *end <= encoded.len())
            .ok_or_else(|| "The local cover conversation is invalid".to_owned())?;
        let cover = std::str::from_utf8(&encoded[cursor..end])
            .map_err(|_| "The local cover conversation is invalid".to_owned())?;
        if cover.is_empty() || cover.len() > MAX_COVER_BYTES || cover.chars().any(char::is_control)
        {
            return Err("The local cover conversation is invalid".to_owned());
        }
        transcript.push(cover.to_owned());
        cursor = end;
    }
    Ok(transcript)
}

fn unix_seconds() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| "The system clock is unavailable".to_owned())
}

fn scope_hash(scope_binding: &str) -> [u8; 32] {
    Sha256::digest(
        [
            b"osl-local-cover-scope-v1".as_slice(),
            scope_binding.as_bytes(),
        ]
        .concat(),
    )
    .into()
}

fn cover_aad(scope: &[u8; 32]) -> Vec<u8> {
    [COVER_AAD_DOMAIN, scope.as_slice()].concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_history_is_encrypted_bounded_and_burnable() {
        let state = LocalCoverState::default();
        for _ in 0..30 {
            state
                .record_cover("scope-a", "neutral cover text", 3_600)
                .unwrap();
        }
        assert_eq!(state.retained_scope_count(), 1);
        assert_eq!(state.cover_history("scope-a").unwrap().len(), MAX_TURNS);
        assert!(!state
            .encrypted_transcript_for_test("scope-a")
            .windows(b"neutral cover text".len())
            .any(|window| window == b"neutral cover text"));
        state.burn_scope("scope-a");
        assert_eq!(state.retained_scope_count(), 0);
        assert!(state.cover_history("scope-a").unwrap().is_empty());
    }

    #[test]
    fn scope_count_is_hard_bounded() {
        let state = LocalCoverState::default();
        for index in 0..(MAX_SCOPES + 8) {
            state
                .record_cover(&format!("scope-{index}"), "neutral cover text", 3_600)
                .unwrap();
        }
        assert_eq!(state.retained_scope_count(), MAX_SCOPES);
    }

    #[test]
    fn malformed_history_is_rejected() {
        assert!(decode_transcript(&[1]).is_err());
        assert!(decode_transcript(&[2, 0, b'a']).is_err());
    }
}
