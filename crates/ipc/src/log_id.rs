//! Opaque, non-reversible log tokens for identifiers.
//!
//! The standing invariant is that no OSL path writes a message, scope, peer or
//! account identifier into a log. It was not held: `commands.rs` alone emitted
//! exact `discord_message_id`, `scope`, `scope_storage_key`, `peer`, `sender`,
//! `requester` and `user_id` values across `debug!`, `info!` and `warn!`.
//!
//! Those values are the linking keys between a person and their conversations.
//! A Discord snowflake identifies a real account; a scope storage key names one
//! conversation; a message id names one message inside it. A log holding them is
//! a social graph, and unlike a plaintext it is written by paths that are on by
//! default and survive in files nobody treats as secret.
//!
//! # Why a salted hash and not a plain hash
//!
//! Snowflakes and message ids are low-entropy and enumerable — they are 64-bit
//! integers with a time prefix, so a bare `sha256(id)` is reversible by anyone
//! willing to hash a candidate range. The salt below is random per process and
//! never leaves memory, so a token cannot be matched back to a guessed
//! identifier even by someone holding both the log and the candidate list.
//!
//! # What is deliberately kept
//!
//! Two emissions of the same identifier inside one process produce the same
//! token, so a log is still followable: "this is the message the earlier line
//! was about" survives. Across restarts the salt changes and correlation is
//! lost, which is the intended trade — a log is a debugging aid for one run,
//! not a durable index of who talked to whom.

use std::sync::OnceLock;

use sha2::{Digest, Sha256};

/// Random per-process salt. Generated once, never persisted, never logged.
fn salt() -> &'static [u8; 32] {
    static SALT: OnceLock<[u8; 32]> = OnceLock::new();
    SALT.get_or_init(|| {
        let mut salt = [0u8; 32];
        salt.copy_from_slice(&crypto::random::random_bytes(32));
        salt
    })
}

/// Length of the hex token. 12 hex characters is 48 bits: far more than enough
/// to keep two identifiers in one log apart, and short enough to stay readable.
const TOKEN_HEX_LEN: usize = 12;

/// An opaque, stable-within-this-process token for one identifier.
///
/// Use this anywhere an identifier would otherwise be interpolated into a
/// tracing event. An empty input answers a fixed label rather than a token, so
/// "absent" and "present but opaque" stay distinguishable in a log.
///
/// ```ignore
/// tracing::warn!(message = %log_id(&discord_message_id), "store put failed");
/// ```
pub fn log_id(value: &str) -> String {
    if value.is_empty() {
        return "<absent>".to_owned();
    }
    let mut hasher = Sha256::new();
    hasher.update(salt());
    // Length-prefixed so two different identifiers cannot be made to collide by
    // choosing where one ends and the next begins.
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(TOKEN_HEX_LEN);
    for byte in digest.iter().take(TOKEN_HEX_LEN / 2) {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// The same token for an optional identifier.
pub fn log_id_opt(value: Option<&str>) -> String {
    match value {
        Some(value) => log_id(value),
        None => "<absent>".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point: the identifier must not be recoverable from, or even
    /// present in, what gets logged.
    #[test]
    fn a_token_never_contains_the_identifier_it_stands_for() {
        let snowflake = "1234567890123456789";
        let token = log_id(snowflake);
        assert!(!token.contains(snowflake));
        assert!(!token.contains("123456"));
        assert_eq!(token.len(), TOKEN_HEX_LEN);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// A log stays followable within one run.
    #[test]
    fn the_same_identifier_is_the_same_token_within_a_process() {
        assert_eq!(log_id("scope:dm:abc"), log_id("scope:dm:abc"));
        assert_ne!(log_id("scope:dm:abc"), log_id("scope:dm:abd"));
    }

    /// A bare hash of an enumerable id is reversible; this one must be salted,
    /// which is observable as "not the unsalted digest of the same input".
    #[test]
    fn the_token_is_salted_not_a_bare_digest() {
        let value = "1234567890123456789";
        let mut bare = Sha256::new();
        bare.update(value.as_bytes());
        let bare = bare.finalize();
        let bare_hex: String = bare
            .iter()
            .take(TOKEN_HEX_LEN / 2)
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_ne!(log_id(value), bare_hex);
    }

    /// Absent and opaque must not look alike.
    #[test]
    fn an_absent_identifier_is_labelled_not_tokenised() {
        assert_eq!(log_id(""), "<absent>");
        assert_eq!(log_id_opt(None), "<absent>");
        assert_eq!(log_id_opt(Some("peer-a")), log_id("peer-a"));
    }
}
