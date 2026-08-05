//! Local-only encrypted cover history.
//!
//! This component never accepts private message text, Discord history, or
//! account metadata. Its bounded, per-scope transcript contains only cover
//! text that the platform already holds and remains AEAD-encrypted in memory.
//! It deliberately does not generate cover: every tier uses the same carrier
//! path.
//!
//! # What may be retained, for how long, and under which scope (D-265)
//!
//! * **What** — only a [`RenderedCover`]: the wordbank flagtext a completed
//!   carrier render produced, which is exactly the public text OSL is about to
//!   type into the platform in the clear. Its constructor is `pub(crate)` and
//!   the hub **binary** is a separate crate from this library, so `main.rs`
//!   cannot mint one from a `&str`. A draft, a Discord row, or any other
//!   plaintext therefore has no route to [`LocalCoverState::record_cover`] at
//!   all — the same type-level barrier `cover_ai::cover_history::CoverText`
//!   gives the sibling store.
//! * **How long** — the local copy expires with the protected message it points
//!   at (`PreparedNativeOverlayText::expires_at`, which is the peer scope's own
//!   configured TTL), hard-capped at [`MAX_RETENTION_SECONDS`]. There is no
//!   caller-supplied TTL parameter, so no caller can widen retention.
//! * **Which scope** — keyed by `SHA-256("osl-local-cover-scope-v1" ‖ binding)`
//!   where the binding is `native_discord_scope_binding`, the identical value
//!   `burn_native_discord_overlay_chat` hands to [`LocalCoverState::burn_scope`].
//!   Everything recorded is therefore reachable by the existing burn.

use crate::broker::PreparedNativeOverlayCarrier;
use crypto::aes_gcm::{self, Key, Nonce};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_SCOPES: usize = 32;
const MAX_TURNS: usize = 12;
const MAX_COVER_BYTES: usize = 1_024;
const MAX_TRANSCRIPT_BYTES: usize = MAX_TURNS * (MAX_COVER_BYTES + 2);
/// A local cover copy never outlives one day, whatever the message's own TTL.
pub const MAX_RETENTION_SECONDS: u64 = 24 * 60 * 60;
const COVER_AAD_DOMAIN: &[u8] = b"osl-native-discord-local-cover-v1";

/// The source of "now" for expiry.
///
/// D-266: while `record_cover` and `cover_history` read `SystemTime::now()`
/// themselves, no test could advance the clock, so the per-record expiry and
/// the 24h retention cap were graded by nothing. The owner of the process
/// supplies the clock instead, exactly as `cover_ai::cover_history` requires its
/// caller to supply `now_seconds`.
pub trait CoverClock: Send + Sync {
    fn unix_seconds(&self) -> Result<u64, String>;
}

/// The shipping clock. [`LocalCoverState::default`] — the constructor the hub
/// binary registers as Tauri state — uses this and nothing else.
pub struct SystemCoverClock;

impl CoverClock for SystemCoverClock {
    fn unix_seconds(&self) -> Result<u64, String> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .map_err(|_| "The system clock is unavailable".to_owned())
    }
}

/// A cover the shared carrier has already rendered, carried together with the
/// expiry of the protected message it points at.
///
/// Both fields are private and [`RenderedCover::from_carrier_render`] is
/// `pub(crate)`, so the text cannot be substituted and the retention cannot be
/// widened from outside this library. See the module docs.
#[derive(Clone)]
pub struct RenderedCover {
    cover: String,
    expires_at: u64,
}

impl RenderedCover {
    /// Accept carrier output once the broker has completed the send it belongs
    /// to. The broker is the only producer of a `PreparedNativeOverlayCarrier`,
    /// so [`recordable_cover`] below is the only route into this constructor.
    pub(crate) fn from_carrier_render(cover: String, expires_at: u64) -> Result<Self, String> {
        if cover.is_empty() || cover.len() > MAX_COVER_BYTES || cover.chars().any(char::is_control)
        {
            return Err("The local cover text is invalid".to_owned());
        }
        Ok(Self { cover, expires_at })
    }
}

/// The one cover a completed native-Discord send rendered, in the only form
/// [`LocalCoverState::record_cover`] accepts.
///
/// `None` when the message needed more than one cipher-store pointer and so has
/// no single cover, or when the carrier text is outside this store's bounds.
/// Recording nothing is the fail-closed outcome: nothing is retained.
pub fn recordable_cover(carrier: &PreparedNativeOverlayCarrier) -> Option<RenderedCover> {
    let cover = carrier.flagtext.as_ref()?;
    let expires_at = u64::try_from(carrier.prepared.expires_at).ok()?;
    RenderedCover::from_carrier_render(cover.clone(), expires_at).ok()
}

#[derive(Clone)]
struct EncryptedTranscript {
    nonce: Nonce,
    ciphertext: Vec<u8>,
    expires_at: u64,
}

pub struct LocalCoverState {
    key: Key,
    transcripts: Mutex<HashMap<[u8; 32], EncryptedTranscript>>,
    clock: Arc<dyn CoverClock>,
}

impl Default for LocalCoverState {
    fn default() -> Self {
        Self::with_clock(Arc::new(SystemCoverClock))
    }
}

impl LocalCoverState {
    pub fn with_clock(clock: Arc<dyn CoverClock>) -> Self {
        let mut key = [0u8; aes_gcm::KEY_SIZE];
        key.copy_from_slice(&crypto::random::random_bytes(aes_gcm::KEY_SIZE));
        Self {
            key: Key::from_bytes(key),
            transcripts: Mutex::new(HashMap::new()),
            clock,
        }
    }

    /// Record cover text after it has been rendered by the shared carrier.
    /// Plaintext messages and third-party conversation history are never valid
    /// inputs here — they cannot be named, because [`RenderedCover`] cannot be
    /// constructed outside this library.
    pub fn record_cover(&self, scope_binding: &str, cover: &RenderedCover) -> Result<(), String> {
        validate_scope(scope_binding)?;
        let now = self.clock.unix_seconds()?;
        // Retention is not the caller's to choose: the local copy dies with the
        // protected message it points at, and never later than the 24h cap.
        let expires_at = cover
            .expires_at
            .min(now.saturating_add(MAX_RETENTION_SECONDS));
        if expires_at <= now {
            return Err("The local cover text has already expired".to_owned());
        }
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
        transcript.push(cover.cover.clone());
        if transcript.len() > MAX_TURNS {
            transcript.drain(..transcript.len() - MAX_TURNS);
        }
        let encoded = encode_transcript(&transcript)?;
        debug_assert!(encoded.len() <= MAX_TRANSCRIPT_BYTES);
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
        let now = self.clock.unix_seconds()?;
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
    use crate::broker::{PreparedNativeOverlayCarrier, PreparedNativeOverlayText};
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A clock the test drives. D-266: without this, no assertion anywhere could
    /// reach the expiry branch, because the store read `SystemTime::now()`.
    struct TestClock(AtomicU64);

    impl TestClock {
        fn at(seconds: u64) -> Arc<Self> {
            Arc::new(Self(AtomicU64::new(seconds)))
        }

        fn set(&self, seconds: u64) {
            self.0.store(seconds, Ordering::SeqCst);
        }
    }

    impl CoverClock for TestClock {
        fn unix_seconds(&self) -> Result<u64, String> {
            Ok(self.0.load(Ordering::SeqCst))
        }
    }

    /// The only route into `RenderedCover` outside this module: a completed
    /// carrier render. Building it here proves the barrier is usable by the
    /// shipping producer and by nothing else.
    fn rendered(cover: &str, expires_at: u64) -> RenderedCover {
        recordable_cover(&PreparedNativeOverlayCarrier {
            prepared: PreparedNativeOverlayText {
                message_id: "message".to_owned(),
                expires_at: i64::try_from(expires_at).expect("test expiry fits i64"),
                person_to_person_e2ee: true,
                view_once: false,
                delivered_to_osl_inbox: true,
            },
            flagtext: Some(cover.to_owned()),
        })
        .expect("a single-chunk carrier render is recordable")
    }

    fn system_now() -> u64 {
        SystemCoverClock
            .unix_seconds()
            .expect("the system clock is available")
    }

    #[test]
    fn cover_history_is_encrypted_bounded_and_burnable() {
        let state = LocalCoverState::default();
        let expires_at = system_now() + 3_600;
        for _ in 0..30 {
            state
                .record_cover("scope-a", &rendered("neutral cover text", expires_at))
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
        let expires_at = system_now() + 3_600;
        for index in 0..(MAX_SCOPES + 8) {
            state
                .record_cover(
                    &format!("scope-{index}"),
                    &rendered("neutral cover text", expires_at),
                )
                .unwrap();
        }
        assert_eq!(state.retained_scope_count(), MAX_SCOPES);
    }

    #[test]
    fn malformed_history_is_rejected() {
        assert!(decode_transcript(&[1]).is_err());
        assert!(decode_transcript(&[2, 0, b'a']).is_err());
    }

    /// D-266. The store used to read the clock itself, so nothing could observe
    /// this boundary. The cover is readable up to the last second of the
    /// protected message's own life and is gone on the second it expires.
    #[test]
    fn a_recorded_cover_expires_exactly_when_its_protected_message_does() {
        let clock = TestClock::at(1_000);
        let state = LocalCoverState::with_clock(clock.clone());
        state
            .record_cover("scope-a", &rendered("neutral cover text", 1_000 + 3_600))
            .unwrap();

        clock.set(1_000 + 3_599);
        assert_eq!(
            state.cover_history("scope-a").unwrap(),
            vec!["neutral cover text".to_owned()],
            "the cover must survive until the message it points at expires"
        );

        clock.set(1_000 + 3_600);
        assert!(
            state.cover_history("scope-a").unwrap().is_empty(),
            "the cover must be unreadable once the message it points at expires"
        );
        assert_eq!(
            state.retained_scope_count(),
            0,
            "an expired transcript must be dropped, not merely hidden from the reader"
        );
    }

    /// D-266. The 24h cap is the floor under every per-message TTL: a scope
    /// configured to keep messages for a month must not leave cover text in
    /// memory for a month.
    #[test]
    fn retention_is_capped_at_24h_however_long_the_message_lives() {
        let thirty_days = 30 * 24 * 60 * 60;
        let clock = TestClock::at(1_000);
        let state = LocalCoverState::with_clock(clock.clone());
        state
            .record_cover(
                "scope-a",
                &rendered("neutral cover text", 1_000 + thirty_days),
            )
            .unwrap();

        clock.set(1_000 + MAX_RETENTION_SECONDS - 1);
        assert_eq!(state.cover_history("scope-a").unwrap().len(), 1);

        clock.set(1_000 + MAX_RETENTION_SECONDS);
        assert!(
            state.cover_history("scope-a").unwrap().is_empty(),
            "no local cover copy may outlive the 24h cap"
        );
    }

    /// D-266. Recording an already-dead cover must retain nothing at all, rather
    /// than storing it with a past expiry and relying on the next reader to
    /// notice.
    #[test]
    fn an_already_expired_cover_is_refused_and_retains_nothing() {
        let clock = TestClock::at(5_000);
        let state = LocalCoverState::with_clock(clock);
        assert!(state
            .record_cover("scope-a", &rendered("neutral cover text", 5_000))
            .is_err());
        assert_eq!(state.retained_scope_count(), 0);
    }

    /// A multi-chunk send has no single cover, so there is nothing to retain.
    #[test]
    fn a_send_without_a_single_cover_records_nothing() {
        assert!(recordable_cover(&PreparedNativeOverlayCarrier {
            prepared: PreparedNativeOverlayText {
                message_id: "message".to_owned(),
                expires_at: 9_000,
                person_to_person_e2ee: true,
                view_once: false,
                delivered_to_osl_inbox: true,
            },
            flagtext: None,
        })
        .is_none());
    }

    fn function_body(source: &'static str, signature: &str) -> &'static str {
        let start = source
            .find(signature)
            .unwrap_or_else(|| panic!("{signature} is missing"));
        let body = &source[start..];
        let end = body
            .find("\n}\n")
            .unwrap_or_else(|| panic!("{signature} is unterminated"));
        &body[..end]
    }

    /// D-265. `burn_scope` was wired and mutation-proved while `record_cover` had
    /// zero non-test callers repo-wide, so the shipping burn always destroyed an
    /// empty map. This grades the write half of that pair on the shipping
    /// binary's own source, and it grades the two halves against the *same*
    /// scope binding — a record filed under a binding the burn does not use
    /// would be just as unreachable.
    #[test]
    fn the_shipping_send_records_the_cover_the_shipping_burn_destroys() {
        let source = include_str!("main.rs");

        let prepare = function_body(source, "async fn prepare_native_discord_overlay_text(");
        let binding = prepare
            .find("let scope_binding = native_discord_scope_binding(&app)?;")
            .expect("the shipping prepare resolves the peer scope binding");
        let mint = prepare
            .find("osl_privacy_hub::pro_context_cover::recordable_cover(&carrier)")
            .expect(
                "the cover recorded must be the one this send rendered, minted by the library \
                 that owns the retention rule",
            );
        let record = prepare
            .find(".record_cover(&scope_binding, &cover)")
            .expect(
                "D-265: the shipping send must record the cover it renders, or \
                 burn_native_discord_overlay_chat destroys an empty map",
            );
        assert!(
            binding < mint && mint < record,
            "the binding and the render both precede the record"
        );

        let burn = function_body(source, "async fn burn_native_discord_overlay_chat(");
        assert!(
            burn.contains("let cover_scope = native_discord_scope_binding(&app)?;"),
            "the burn resolves the same binding the record used"
        );
        assert!(
            burn.contains("app.state::<LocalCoverState>().burn_scope(&cover_scope);"),
            "the burn destroys that scope's local cover history"
        );
    }

    // The plaintext barrier itself is NOT asserted here on purpose. The hub
    // binary is a separate crate, so `RenderedCover::from_carrier_render` being
    // `pub(crate)` makes `main.rs` naming it an E0624 and a `RenderedCover { .. }`
    // literal an E0451. The compiler enforces it absolutely; a source-text
    // assertion over `main.rs` would only add a pin that cannot catch anything
    // the build does not already refuse.
}
