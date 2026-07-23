//! Local-only contextual cover conversation generator.
//!
//! This component never accepts private message text, Discord history, account
//! metadata, or renderer-provided cover. Its entire input is an opaque OSL
//! scope binding and an expiry. The bounded per-scope transcript contains only
//! phrase-table indices and remains AEAD-encrypted while retained in memory.

use crypto::aes_gcm::{self, Key, Nonce};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

const FREE_COVER: &str = "🔒 OSL private message";
const MAX_SCOPES: usize = 32;
const MAX_TURNS: usize = 12;
const MAX_TRANSCRIPT_BYTES: usize = MAX_TURNS;
const MAX_RETENTION_SECONDS: u64 = 24 * 60 * 60;
const COVER_AAD_DOMAIN: &[u8] = b"osl-native-discord-local-cover-v1";

// Short, neutral turns intentionally disclose nothing and make no factual
// claim about either participant. Groups form a tiny coherent state machine:
// opener -> acknowledgement -> continuation -> close -> opener.
const PHRASES: [&str; 16] = [
    "Hey, hope your day is going well.",
    "Hi, good to hear from you.",
    "Hey, how are things?",
    "Hope everything is going well.",
    "Sounds good to me.",
    "That works for me.",
    "Got it, thanks.",
    "Makes sense.",
    "I can take a look.",
    "Let me check on that.",
    "I’ll keep you posted.",
    "We can pick this up soon.",
    "Talk soon.",
    "Have a good one.",
    "Thanks, catch you later.",
    "All set on my side.",
];

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
    pub fn free_cover() -> &'static str {
        FREE_COVER
    }

    pub fn next_pro_cover(&self, scope_binding: &str, ttl_seconds: u32) -> Result<String, String> {
        if scope_binding.is_empty()
            || scope_binding.len() > 512
            || scope_binding.chars().any(char::is_control)
        {
            return Err("The local cover scope is invalid".to_owned());
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
        let random = crypto::random::random_bytes(8);
        let choice = u64::from_le_bytes(random.try_into().map_err(|_| "OS random failed")?);
        let phrase_index = choose_next(transcript.last().copied(), choice);
        transcript.push(phrase_index);
        if transcript.len() > MAX_TURNS {
            transcript.drain(..transcript.len() - MAX_TURNS);
        }
        debug_assert!(transcript.len() <= MAX_TRANSCRIPT_BYTES);
        let expires_at =
            now.saturating_add(u64::from(ttl_seconds.max(1)).min(MAX_RETENTION_SECONDS));
        let (nonce, ciphertext) = aes_gcm::seal(&self.key, &cover_aad(&scope), &transcript)
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
        Ok(PHRASES[usize::from(phrase_index)].to_owned())
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

    fn open(&self, scope: &[u8; 32], record: &EncryptedTranscript) -> Result<Vec<u8>, String> {
        let plaintext = aes_gcm::open(
            &self.key,
            &record.nonce,
            &cover_aad(scope),
            &record.ciphertext,
        )
        .map_err(|_| "The local cover conversation failed authentication".to_owned())?;
        if plaintext.len() > MAX_TRANSCRIPT_BYTES
            || plaintext
                .iter()
                .any(|index| usize::from(*index) >= PHRASES.len())
        {
            return Err("The local cover conversation is invalid".to_owned());
        }
        Ok(plaintext)
    }

    #[cfg(test)]
    fn retained_scope_count(&self) -> usize {
        self.transcripts
            .lock()
            .map(|value| value.len())
            .unwrap_or(0)
    }

    #[cfg(test)]
    fn transcript_for_test(&self, scope_binding: &str) -> Vec<u8> {
        let scope = scope_hash(scope_binding);
        self.transcripts
            .lock()
            .ok()
            .and_then(|records| records.get(&scope).cloned())
            .and_then(|record| self.open(&scope, &record).ok())
            .unwrap_or_default()
    }
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

fn choose_next(previous: Option<u8>, random: u64) -> u8 {
    let group = previous.map(|value| usize::from(value) / 4);
    let next_group = match group {
        None | Some(3) => 0,
        Some(0) => 1,
        Some(1) => 2,
        Some(2) => 3,
        _ => 0,
    };
    (next_group * 4 + (random as usize % 4)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transitions_are_coherent_and_bounded() {
        for random in 0..32 {
            let opener = choose_next(None, random);
            let acknowledgement = choose_next(Some(opener), random);
            let continuation = choose_next(Some(acknowledgement), random);
            let close = choose_next(Some(continuation), random);
            assert!(opener < 4);
            assert!((4..8).contains(&acknowledgement));
            assert!((8..12).contains(&continuation));
            assert!((12..16).contains(&close));
        }
    }

    #[test]
    fn transcript_is_encrypted_bounded_and_burnable() {
        let state = LocalCoverState::default();
        for _ in 0..30 {
            let cover = state.next_pro_cover("scope-a", 3_600).unwrap();
            assert!(PHRASES.contains(&cover.as_str()));
        }
        assert_eq!(state.retained_scope_count(), 1);
        assert_eq!(state.transcript_for_test("scope-a").len(), MAX_TURNS);
        state.burn_scope("scope-a");
        assert_eq!(state.retained_scope_count(), 0);
    }

    #[test]
    fn scope_count_is_hard_bounded() {
        let state = LocalCoverState::default();
        for index in 0..(MAX_SCOPES + 8) {
            state
                .next_pro_cover(&format!("scope-{index}"), 3_600)
                .unwrap();
        }
        assert_eq!(state.retained_scope_count(), MAX_SCOPES);
    }

    #[test]
    fn fixed_free_cover_is_not_contextual() {
        assert_eq!(LocalCoverState::free_cover(), "🔒 OSL private message");
    }
}
