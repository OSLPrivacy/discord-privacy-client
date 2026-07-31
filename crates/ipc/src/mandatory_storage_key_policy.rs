//! `MandatoryStorageKeyPolicy` — deciding *when* a file must be encrypted
//! at rest.
//!
//! This unit exists because of a recurring defect (checklist A6, "no
//! protected plaintext at rest"): `peer_map::write_peer_map`,
//! `membership::write_scope_membership`, and
//! `main_password::maybe_encrypt` all share the same shape — when no
//! `file_storage_key` is installed, the write silently **degrades to
//! plaintext** instead of refusing. This module is the policy type that
//! makes "must this be encrypted?" an explicit, auditable decision instead
//! of an implicit fallback buried in each writer.
//!
//! ## Deny-by-default
//!
//! [`MandatoryStorageKeyPolicy::classify`] treats an unrecognized file
//! identity as [`StorageClass::MandatoryEncrypt`]. Only file identities the
//! caller has explicitly enrolled as plaintext-safe come back as
//! [`StorageClass::PlaintextAllowed`]. There is no third "unknown, assume
//! plaintext is fine" outcome — that is precisely the bug this policy
//! replaces. A freshly constructed [`MandatoryStorageKeyPolicy::new`] has an
//! empty allowlist, so by default *every* file identity is mandatory-encrypt.
//!
//! ## No degrade-to-plaintext branch
//!
//! [`MandatoryStorageKeyPolicy::authorize_write`] is the only entry point
//! that combines a classification with key availability, and its signature
//! makes the bug structurally impossible to reintroduce: when the class is
//! `MandatoryEncrypt` and no key is available, the function returns
//! `Err(StorageKeyRefusal)`. There is no `Ok` variant that carries "write
//! plaintext anyway" — the caller gets a refusal or an authorization to
//! encrypt, never a silent downgrade.
//!
//! This module does no I/O and does not touch the process-global
//! `file_storage_key` slot in [`crate::main_password`] — it is a pure
//! classification/decision type so it can be unit-tested without any
//! filesystem or key-material fixtures. Wiring it into
//! `write_peer_map` / `write_scope_membership` / `maybe_encrypt` is left to
//! later units (per the unit-a41 brief, this unit is type-and-error only).
//!
//! ## Seam with `a45`'s `SecureLocalStore`
//!
//! A sibling unit (`a45`) is defining `SecureLocalStore` in `crates/ipc/`
//! as the UI-side storage *mechanism* (how a value actually gets read from
//! and written to disk/keyring). `MandatoryStorageKeyPolicy` is the
//! *policy* consulted before that mechanism is allowed to persist
//! anything: a store implementation should call
//! [`MandatoryStorageKeyPolicy::authorize_write`] (or `classify`, if it
//! only needs the classification and manages key availability itself)
//! before it writes bytes for a given file identity, and must not define
//! its own notion of "this one's fine as plaintext." This module
//! deliberately does not define a store, a trait for one, or any I/O — that
//! is `a45`'s contract, not this one's.

use std::collections::HashSet;
use std::fmt;

/// The outcome of classifying a logical storage-file identity.
///
/// There are exactly two variants. There is intentionally no
/// "unclassified" or "unknown" variant — every file identity resolves to
/// one of these two via [`MandatoryStorageKeyPolicy::classify`], and the
/// deny-by-default rule means anything not explicitly allowlisted lands on
/// `MandatoryEncrypt`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum StorageClass {
    /// The file must be encrypted at rest. A write may only proceed if a
    /// storage key is available; absence of a key is a refusal, never a
    /// plaintext write.
    MandatoryEncrypt,
    /// The file has been explicitly enrolled as safe to store in
    /// plaintext. This is opt-in only — see
    /// [`MandatoryStorageKeyPolicy::with_plaintext_allowlist`].
    PlaintextAllowed,
}

impl StorageClass {
    /// True for [`StorageClass::MandatoryEncrypt`].
    pub fn requires_key(self) -> bool {
        matches!(self, StorageClass::MandatoryEncrypt)
    }
}

/// Deny-by-default policy: classifies logical storage-file identities
/// (e.g. `"peer_map.json"`, `"whitelist_state.json"`) as either
/// [`StorageClass::MandatoryEncrypt`] or [`StorageClass::PlaintextAllowed`],
/// and authorizes writes against a caller-supplied key-availability flag
/// with no silent plaintext downgrade.
///
/// File identities are matched as opaque strings — this type has no
/// knowledge of the filesystem, `Path`, or any particular directory
/// layout. Callers pass whatever stable identity they use to key a given
/// logical file (a filename is the natural choice for the existing
/// `peer_map.json` / `whitelist_state.json` / `burned_scopes.json` trio).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MandatoryStorageKeyPolicy {
    plaintext_allowlist: HashSet<String>,
}

impl MandatoryStorageKeyPolicy {
    /// A policy with an empty allowlist: every file identity classifies as
    /// `MandatoryEncrypt`. This is the safe starting point — enrolling a
    /// file as plaintext-safe is always an explicit, separate step.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a policy that treats exactly the given identities as
    /// [`StorageClass::PlaintextAllowed`]; every other identity — known or
    /// unknown to the caller — remains `MandatoryEncrypt`.
    pub fn with_plaintext_allowlist<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            plaintext_allowlist: names.into_iter().map(Into::into).collect(),
        }
    }

    /// Classify a file identity. Deny-by-default: anything not in the
    /// plaintext allowlist is `MandatoryEncrypt`, including identities this
    /// policy instance has never seen before.
    pub fn classify(&self, file_id: &str) -> StorageClass {
        if self.plaintext_allowlist.contains(file_id) {
            StorageClass::PlaintextAllowed
        } else {
            StorageClass::MandatoryEncrypt
        }
    }

    /// Decide whether a write of `file_id` may proceed given whether a
    /// storage key is currently available.
    ///
    /// - `MandatoryEncrypt` + key available -> `Ok(MandatoryEncrypt)`, caller
    ///   must encrypt.
    /// - `MandatoryEncrypt` + no key -> `Err(StorageKeyRefusal)`. This is
    ///   the only outcome for this combination; there is no `Ok` path that
    ///   authorizes a plaintext write.
    /// - `PlaintextAllowed` (key available or not) -> `Ok(PlaintextAllowed)`.
    pub fn authorize_write(
        &self,
        file_id: &str,
        key_available: bool,
    ) -> Result<StorageClass, StorageKeyRefusal> {
        let class = self.classify(file_id);
        if class.requires_key() && !key_available {
            return Err(StorageKeyRefusal {
                file_id: file_id.to_string(),
            });
        }
        Ok(class)
    }
}

/// Refusal returned when a `MandatoryEncrypt` file has no storage key
/// available. This type carries only the file identity (a filename-shaped
/// string, not secret material) — never a key, key fragment, or any other
/// sensitive value, so its derived `Debug` is safe to log.
#[derive(Clone, Eq, PartialEq)]
pub struct StorageKeyRefusal {
    file_id: String,
}

impl StorageKeyRefusal {
    /// The file identity that was refused.
    pub fn file_id(&self) -> &str {
        &self.file_id
    }
}

impl fmt::Display for StorageKeyRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "OSL: refusing to write {} as plaintext — mandatory-encrypt \
             storage class with no key in slot (password not yet entered?)",
            self.file_id
        )
    }
}

impl fmt::Debug for StorageKeyRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StorageKeyRefusal")
            .field("file_id", &self.file_id)
            .finish()
    }
}

impl std::error::Error for StorageKeyRefusal {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unclassified_path_is_mandatory_encrypt() {
        let policy = MandatoryStorageKeyPolicy::new();
        assert_eq!(
            policy.classify("some_file_never_registered_anywhere.json"),
            StorageClass::MandatoryEncrypt
        );
    }

    #[test]
    fn known_real_world_files_default_to_mandatory_encrypt() {
        // The three confirmed-leaking files from A6: absent an explicit
        // allowlist entry, all of them classify as mandatory-encrypt.
        let policy = MandatoryStorageKeyPolicy::new();
        for name in [
            "peer_map.json",
            "whitelist_state.json",
            "burned_scopes.json",
        ] {
            assert_eq!(policy.classify(name), StorageClass::MandatoryEncrypt);
        }
    }

    #[test]
    fn explicit_allowlist_entry_is_plaintext_allowed() {
        let policy = MandatoryStorageKeyPolicy::with_plaintext_allowlist(["public_readme.txt"]);
        assert_eq!(
            policy.classify("public_readme.txt"),
            StorageClass::PlaintextAllowed
        );
    }

    #[test]
    fn allowlisting_one_file_does_not_relax_others() {
        // Deny-by-default per-identity: enrolling one file plaintext-safe
        // must not accidentally widen the default for everything else.
        let policy = MandatoryStorageKeyPolicy::with_plaintext_allowlist(["public_readme.txt"]);
        assert_eq!(
            policy.classify("peer_map.json"),
            StorageClass::MandatoryEncrypt
        );
    }

    #[test]
    fn mandatory_encrypt_without_key_is_refused_not_downgraded() {
        let policy = MandatoryStorageKeyPolicy::new();
        let result = policy.authorize_write("peer_map.json", false);
        let err = result.expect_err("no key must refuse, not authorize plaintext");
        assert_eq!(err.file_id(), "peer_map.json");
    }

    #[test]
    fn mandatory_encrypt_with_key_is_authorized() {
        let policy = MandatoryStorageKeyPolicy::new();
        let class = policy
            .authorize_write("peer_map.json", true)
            .expect("key available: write must be authorized");
        assert_eq!(class, StorageClass::MandatoryEncrypt);
    }

    #[test]
    fn plaintext_allowed_does_not_require_a_key() {
        let policy = MandatoryStorageKeyPolicy::with_plaintext_allowlist(["public_readme.txt"]);
        let class = policy
            .authorize_write("public_readme.txt", false)
            .expect("plaintext-allowed files never need a key");
        assert_eq!(class, StorageClass::PlaintextAllowed);
    }

    #[test]
    fn refusal_display_names_the_file_and_reason() {
        let policy = MandatoryStorageKeyPolicy::new();
        let err = policy
            .authorize_write("whitelist_state.json", false)
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("whitelist_state.json"));
        assert!(msg.to_lowercase().contains("plaintext"));
    }

    #[test]
    fn refusal_debug_contains_no_secret_material() {
        // StorageKeyRefusal only ever carries a file identity string, never
        // key bytes. Assert the Debug output is exactly the file_id field —
        // there is no hidden key/nonce/material field to leak.
        let policy = MandatoryStorageKeyPolicy::new();
        let err = policy.authorize_write("peer_map.json", false).unwrap_err();
        let dbg = format!("{err:?}");
        assert_eq!(dbg, "StorageKeyRefusal { file_id: \"peer_map.json\" }");
    }

    #[test]
    fn requires_key_reflects_class() {
        assert!(StorageClass::MandatoryEncrypt.requires_key());
        assert!(!StorageClass::PlaintextAllowed.requires_key());
    }
}
