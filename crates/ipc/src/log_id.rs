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

/// A filesystem path with the user's home directory replaced by `~`.
///
/// D-191 turned the subscriber on, which made every `path = %p.display()` in
/// the workspace a thing a friend can be asked to paste into a chat window.
/// The OSL profile lives under the user's home, so those paths read
/// `C:\Users\Jane Smith\AppData\Roaming\org.oslprivacy.hub\…` on Windows and
/// `/home/jsmith/.local/share/…` on Linux: the account name of the person
/// running a privacy product, and very often their real name. The **path
/// shape** is what makes a path-carrying diagnostic useful — which file, in
/// which profile, under which account directory — and that survives redaction
/// intact.
///
/// Two passes, because the first cannot cover every case:
///
/// 1. strip the process's own home directory (`HOME`, else `USERPROFILE`);
/// 2. failing that, replace the component after a `Users` or `home` component
///    with `<user>` — which catches a path built for a *different* profile
///    root, e.g. a copied or QA-remapped directory.
///
/// This is a helper for call sites, not a filter over the event stream: a
/// subscriber-level scrubber is a promise the next `tracing::warn!` has no way
/// to keep.
pub fn redact_path(path: &std::path::Path) -> String {
    let text = path.display().to_string();

    for var in ["HOME", "USERPROFILE"] {
        if let Ok(home) = std::env::var(var) {
            if !home.is_empty() && text.starts_with(&home) {
                let rest = &text[home.len()..];
                return format!("~{rest}");
            }
        }
    }

    // Split on both separators rather than on `Path::components`: a diagnostic
    // is read on a machine that is not necessarily the one that produced it,
    // and `Path::components` on Linux sees `C:\Users\Jane\…` as a single
    // component, which would leave the name in place.
    let mut out = String::with_capacity(text.len());
    let mut redact_next = false;
    let mut rest = text.as_str();
    loop {
        let split = rest.find(['/', '\\']);
        let (part, sep) = match split {
            Some(index) => (&rest[..index], Some(&rest[index..index + 1])),
            None => (rest, None),
        };
        if redact_next && !part.is_empty() {
            out.push_str("<user>");
            redact_next = false;
        } else {
            redact_next = matches!(part, "Users" | "users" | "home");
            out.push_str(part);
        }
        match sep {
            Some(sep) => {
                out.push_str(sep);
                rest = &rest[part.len() + 1..];
            }
            None => break,
        }
    }
    out
}

/// A server-supplied string, bounded and escaped, for a `tracing` field.
///
/// Response bodies are attacker-influenced and unbounded: pasted verbatim with
/// `%body` a newline in one forges a log line, and a large one eats the
/// diagnostic's rotation budget in a single event. `Debug`-formatting the
/// result escapes control characters, so callers should use `?` on it.
pub fn bounded_detail(value: &str) -> String {
    const MAX: usize = 256;
    if value.len() <= MAX {
        return value.to_owned();
    }
    let mut end = MAX;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… ({} bytes total)", &value[..end], value.len())
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

    /// D-191: the account name must not survive into a diagnostic a friend is
    /// asked to send, from either redaction pass.
    #[test]
    fn a_redacted_path_does_not_contain_the_account_name() {
        let windows = std::path::Path::new(
            r"C:\Users\Jane Smith\AppData\Roaming\org.oslprivacy.hub\osl-core\identity.json",
        );
        let redacted = redact_path(windows);
        assert!(
            !redacted.contains("Jane Smith"),
            "account name survived: {redacted}"
        );
        assert!(
            redacted.contains("identity.json") && redacted.contains("osl-core"),
            "the useful shape was destroyed: {redacted}"
        );

        let linux = std::path::Path::new("/home/jsmith/.local/share/osl/store");
        let redacted = redact_path(linux);
        assert!(!redacted.contains("jsmith"), "survived: {redacted}");
        assert!(redacted.contains("store"), "shape destroyed: {redacted}");
    }

    /// The `HOME`-prefix pass is the common case and must also lose the name.
    #[test]
    fn the_home_prefix_pass_keeps_only_the_tail() {
        let home = std::env::var("HOME").unwrap_or_default();
        if home.is_empty() {
            return;
        }
        let path = std::path::Path::new(&home).join("osl-core").join("peer_map.json");
        let redacted = redact_path(&path);
        assert!(redacted.starts_with('~'), "{redacted}");
        assert!(redacted.ends_with("peer_map.json"), "{redacted}");
        assert!(!redacted.contains(&home), "{redacted}");
    }

    /// A server body must not be able to forge a line or eat the log budget.
    #[test]
    fn a_server_detail_is_bounded_and_escapes_when_debug_formatted() {
        let long = "x".repeat(4096);
        let bounded = bounded_detail(&long);
        assert!(bounded.len() < 400, "not bounded: {} bytes", bounded.len());
        assert!(bounded.contains("4096 bytes total"));

        let forged = bounded_detail("ok\nERROR forged line");
        assert!(!format!("{forged:?}").contains('\n'));
    }
}
