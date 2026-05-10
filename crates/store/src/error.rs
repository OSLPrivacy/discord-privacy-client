//! Typed errors returned by [`crate::MessageStore`] operations.

use thiserror::Error;

/// Errors returned by [`crate::MessageStore`] operations.
///
/// Variants:
///
/// - `Io` — filesystem-level failure during `open` (parent
///   directory creation, file permissions). All other I/O on
///   the SQLite handle surfaces as `Sqlite`.
/// - `Sqlite` — `rusqlite` returned an error from any DB
///   operation. Carries the underlying error verbatim for
///   diagnostics; never panics.
/// - `Sealer` — key-derivation failure OR canary-validation
///   failure. At `open()` time this is the diagnostic for
///   "wrong identity_secret": the on-disk canary cannot be
///   unsealed under the caller-supplied key.
/// - `Schema` — schema-version mismatch we can't migrate, or
///   a structural corruption (a `_meta` row of unexpected
///   length). The migration path is conservative — newer-than-
///   supported on-disk versions refuse to open rather than
///   risk silent forward-incompat.
/// - `NotFound` — a lookup that explicitly distinguishes
///   absence-as-error from absence-as-`None`. `get` returns
///   `Ok(None)` for absent rows; `mark_burned` returns
///   `NotFound` so callers can tell apart "I burned it" from
///   "there was nothing to burn."
/// - `Corrupted` — AEAD tag check failed at runtime
///   (`get` / `list_by_channel` / `search`). Either the row
///   was tampered with on disk OR the data key drifted from
///   the one used at write time. The canary check at `open()`
///   should rule out the latter for normal operation; surfacing
///   `Corrupted` at runtime points at on-disk tampering.
#[derive(Debug, Error)]
pub enum StoreError {
    /// Filesystem error during `open` (parent directory
    /// creation, file permissions, etc.).
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// `rusqlite` returned an error.
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Key derivation or canary validation failed. At `open()`
    /// this means the caller-supplied `identity_secret` does
    /// not match the secret that originally initialised the
    /// store on disk.
    #[error("sealer: {0}")]
    Sealer(String),

    /// On-disk schema is structurally wrong or newer than this
    /// binary supports.
    #[error("schema: {0}")]
    Schema(String),

    /// The requested `discord_message_id` does not exist in
    /// the store.
    #[error("not found: {0}")]
    NotFound(String),

    /// AEAD tag check failed reading a row's ciphertext —
    /// on-disk tampering or per-row key drift.
    #[error("corrupted: {0}")]
    Corrupted(String),
}
