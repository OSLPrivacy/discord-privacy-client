//! TASK 4657 — the records OSL Chats stores for a **post**, a **story** and an
//! **archive item**, and the repository that persists them.
//!
//! ## What one record is
//!
//! One record is one authenticated statement by an author. Every field a
//! reader would act on — the author identity and the profile scope it was
//! published under, the version of owner authority that was live when it was
//! signed, the canonical body and media bytes, their independent digest, the
//! created time, the delete state, the visibility stable ID and the
//! storage/archive choice — sits inside a single canonical byte string that
//! the author signs. A record whose body, media, digest, visibility, storage
//! choice, created time or delete state has moved by one byte no longer
//! verifies, so "the record is present" and "the record says what its author
//! said" are the same statement.
//!
//! ## Where the policy vocabulary comes from
//!
//! - **Visibility** — the four frozen stable IDs of TASK 4651, taken from the
//!   shipping [`content_defaults::VisibilityOption`] rather than re-declared
//!   here, so there is one vocabulary in the product and not two that can
//!   drift apart.
//! - **Story lifetime ceiling** — TASK 4650's 168 h, written here as its own
//!   number ([`STORY_MAX_LIFETIME_SECONDS`]). 4650 says explicitly that a
//!   story implementation deriving its lifetime from a relay constant fails
//!   that decision *even where the number agrees today*, so this ceiling is
//!   deliberately not read from the relay TTL allowlist.
//! - **Archive** — TASK 4652's local saved copy.
//!   [`ARCHIVE_KIND_LOCAL_SAVED_COPY`] paid for by [`ARCHIVE_PAYER_POSTER_DISK`]
//!   is the only admitted pair. [`ARCHIVE_KIND_RELAY_POINTER`] and
//!   [`ARCHIVE_PAYER_OSL_DATA_ALLOWANCE`] exist in the vocabulary only so they
//!   can be named and refused — a vocabulary that cannot express the rejected
//!   choice cannot prove it is rejected.
//! - **Storage** — a post and a story go to [`STORAGE_RELAY`] (4650: a story is
//!   an ordinary relay object and gets no story-specific path); an archive
//!   item goes to [`STORAGE_LOCAL_ARCHIVE`], because 4652's archive was never
//!   uploaded.
//!
//! ## Details the task left open, decided here
//!
//! - A story's `expires_unix_ms` must equal `created_unix_ms +
//!   lifetime_seconds * 1000` exactly, so expiry and lifetime can never
//!   disagree about when the story dies.
//! - The owner-authority directory is keyed on author id **and** profile
//!   scope, so substituting the scope leaves the record with no live grant
//!   rather than silently reusing another scope's key.
//! - Records live in their own `social.sqlite` rather than as an eleventh
//!   migration of `messages.sqlite`: a new record class with its own envelope
//!   does not justify rewriting every existing message row.
//!
//! ## What this module is not
//!
//! This is the record foundation. It does not decide **who may press publish
//! or delete** — that is TASK 4659-4661. [`SocialFields::deleted`] is a bound
//! field, not a permission, and [`AuthorityDirectory`] records who is
//! currently authorized to sign without deciding what they may do.

use crate::cipher;
use crate::StoreError;
use content_defaults::VisibilityOption;
use crypto::{aead, ed25519, hkdf};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Record kinds, one per production constructor.
pub const KIND_POST: &str = "social.post";
/// See [`KIND_POST`].
pub const KIND_STORY: &str = "social.story";
/// See [`KIND_POST`].
pub const KIND_ARCHIVE: &str = "social.archive";

/// Storage choices. A post and a story are ordinary relay objects (4650); an
/// archive item is the poster's own local saved copy (4652).
pub const STORAGE_RELAY: &str = "store.relay";
/// See [`STORAGE_RELAY`].
pub const STORAGE_LOCAL_ARCHIVE: &str = "store.local_archive";

/// The only archive kind TASK 4652 admits.
pub const ARCHIVE_KIND_LOCAL_SAVED_COPY: &str = "archive.local_saved_copy";
/// Named so it can be refused: 4652 rejects a pointer, which dies with its
/// source, and a 168 h story always expires.
pub const ARCHIVE_KIND_RELAY_POINTER: &str = "archive.relay_pointer";
/// The only payer TASK 4652 admits — a local copy consumes the poster's disk.
pub const ARCHIVE_PAYER_POSTER_DISK: &str = "payer.poster_disk";
/// Named so it can be refused: 4652 rules that nobody pays OSL for an archive.
pub const ARCHIVE_PAYER_OSL_DATA_ALLOWANCE: &str = "payer.osl_data_allowance";

/// TASK 4650's story ceiling: 168 hours, on both tiers, stated as its own
/// number and not derived from any relay limit.
pub const STORY_MAX_LIFETIME_SECONDS: u64 = 168 * 60 * 60;

/// Domain prefix of the canonical byte string an author signs.
pub const CANONICAL_MAGIC: &[u8] = b"osl-social-record/v1";

/// Domain prefix of the independent body/media digest preimage.
pub const DIGEST_DOMAIN: &[u8] = b"osl-social-record/digest/v1";

/// HKDF info for the social-record sealing key. Separate from the message
/// store's own info, so the two record classes never share a key.
pub const SOCIAL_HKDF_INFO: &[u8] = b"osl-social-record-store-v1";

/// HKDF info for the social-record blind-index key.
pub const SOCIAL_INDEX_HKDF_INFO: &[u8] = b"osl-social-record-store-index-v1";

/// Blind-index domain for a record id. The id itself never reaches disk.
pub const BI_RECORD_ID: &[u8] = b"osl-social-bi/record_id-v1";

/// AEAD associated-data domain for a sealed social row.
pub const SOCIAL_ROW_AAD: &[u8] = b"osl-social-record/row/v1";

/// The SQLite file, under the caller's app-data directory.
pub const SOCIAL_DB_FILENAME: &str = "social.sqlite";

/// Every way a submitted record can be refused. All of them are refused
/// **before** anything reaches the disk.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SocialRecordError {
    /// A field the record cannot mean anything without was absent or empty.
    #[error("missing field: {0}")]
    MissingField(String),

    /// The visibility stable ID is not one of TASK 4651's four.
    #[error("unknown visibility stable id: field visibility_stable_id, value {0}")]
    UnknownVisibility(String),

    /// The storage choice is neither the relay nor the local archive.
    #[error("unknown storage choice: field storage_choice, value {0}")]
    UnknownStorage(String),

    /// A named field carries a value the 4650/4652 decisions refuse.
    #[error("policy refused: field {field}, {reason}")]
    Policy { field: String, reason: String },

    /// No owner authority is currently live for this author and profile scope
    /// — never granted, or revoked since.
    #[error("no live owner authority: field author_id, value {0}")]
    NoAuthority(String),

    /// The submitted authority version is not the one currently authorized.
    #[error(
        "stale or revoked authority version: field authority_version, \
         currently authorized {authorized}, submitted {submitted}"
    )]
    StaleAuthority { authorized: u32, submitted: u32 },

    /// The submitted digest does not cover the submitted body/media bytes.
    #[error(
        "digest mismatch: field digest, recomputed {recomputed}, \
         submitted {submitted}, first differing byte offset {offset}"
    )]
    DigestMismatch {
        recomputed: String,
        submitted: String,
        offset: usize,
    },

    /// The signature is not by the currently authorized key over exactly
    /// these canonical bytes.
    #[error(
        "signature refused: field signature, no signature by the currently \
         authorized key for author {author_id} scope {profile_scope} over \
         {canonical_len} canonical bytes"
    )]
    SignatureRefused {
        author_id: String,
        profile_scope: String,
        canonical_len: usize,
    },

    /// The canonical byte string on the wire is not a canonical encoding of a
    /// social record.
    #[error("malformed canonical record: field {field}, {reason}")]
    Malformed { field: String, reason: String },
}

/// The authenticated author identity: who signed, and which per-scope profile
/// (TASK 4654) they published under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorBinding {
    pub author_id: String,
    pub profile_scope: String,
}

/// Story-only terms: how long the story lives, and the instant it dies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoryTerms {
    pub lifetime_seconds: u64,
    pub expires_unix_ms: i64,
}

/// Archive-only terms: the chosen kind (never a pointer) and who pays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveTerms {
    pub archive_kind: String,
    pub payer: String,
}

/// Everything one record says. This is the *submission* shape: holding a
/// `SocialFields` proves nothing, because [`SocialRecord`] is the only type
/// that can be persisted and its fields are private.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocialFields {
    /// One of [`KIND_POST`], [`KIND_STORY`], [`KIND_ARCHIVE`]. Set by the
    /// constructor, never by the caller.
    pub kind: String,
    /// The unique stable record ID.
    pub record_id: String,
    pub author: AuthorBinding,
    /// Which grant of owner authority was live at signing time.
    pub authority_version: u32,
    pub created_unix_ms: i64,
    pub deleted: bool,
    /// One of TASK 4651's four frozen stable IDs.
    pub visibility_stable_id: String,
    pub storage_choice: String,
    /// Canonical body bytes, verbatim. Never normalised, trimmed or
    /// re-encoded on any path in or out.
    pub body: Vec<u8>,
    /// Canonical media bytes, verbatim. May be empty for a text-only record.
    pub media: Vec<u8>,
    /// SHA-256 over a domain-separated, length-prefixed preimage of exactly
    /// the body and media bytes. Independent of the signature: the signature
    /// proves who said it, the digest proves the bytes did not move.
    pub digest: Vec<u8>,
    pub story: Option<StoryTerms>,
    pub archive: Option<ArchiveTerms>,
}

/// The independent digest of one body/media pair.
///
/// Length-prefixed and domain-separated so no two different `(body, media)`
/// pairs share a preimage.
pub fn body_media_digest(body: &[u8], media: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(DIGEST_DOMAIN);
    hasher.update((body.len() as u64).to_be_bytes());
    hasher.update(body);
    hasher.update((media.len() as u64).to_be_bytes());
    hasher.update(media);
    hasher.finalize().into()
}

fn put_str(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u32).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn put_bytes(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u32).to_be_bytes());
    out.extend_from_slice(value);
}

/// The canonical byte string an author signs, and the only shape that reaches
/// the disk. Every field is length-prefixed, so no two different records share
/// an encoding.
pub fn canonical_bytes(fields: &SocialFields) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(CANONICAL_MAGIC);
    put_str(&mut out, &fields.kind);
    put_str(&mut out, &fields.record_id);
    put_str(&mut out, &fields.author.author_id);
    put_str(&mut out, &fields.author.profile_scope);
    out.extend_from_slice(&fields.authority_version.to_be_bytes());
    out.extend_from_slice(&fields.created_unix_ms.to_be_bytes());
    out.push(u8::from(fields.deleted));
    put_str(&mut out, &fields.visibility_stable_id);
    put_str(&mut out, &fields.storage_choice);
    put_bytes(&mut out, &fields.body);
    put_bytes(&mut out, &fields.media);
    put_bytes(&mut out, &fields.digest);
    match &fields.story {
        Some(story) => {
            out.push(1);
            out.extend_from_slice(&story.lifetime_seconds.to_be_bytes());
            out.extend_from_slice(&story.expires_unix_ms.to_be_bytes());
        }
        None => out.push(0),
    }
    match &fields.archive {
        Some(archive) => {
            out.push(1);
            put_str(&mut out, &archive.archive_kind);
            put_str(&mut out, &archive.payer);
        }
        None => out.push(0),
    }
    out
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize, field: &str) -> Result<&'a [u8], SocialRecordError> {
        if self.at + n > self.bytes.len() {
            return Err(SocialRecordError::Malformed {
                field: field.to_owned(),
                reason: format!(
                    "wanted {n} bytes at offset {}, only {} remain",
                    self.at,
                    self.bytes.len() - self.at
                ),
            });
        }
        let slice = &self.bytes[self.at..self.at + n];
        self.at += n;
        Ok(slice)
    }

    fn len_prefixed(&mut self, field: &str) -> Result<&'a [u8], SocialRecordError> {
        let raw = self.take(4, field)?;
        let n = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
        self.take(n, field)
    }

    fn text(&mut self, field: &str) -> Result<String, SocialRecordError> {
        let raw = self.len_prefixed(field)?;
        String::from_utf8(raw.to_vec()).map_err(|error| SocialRecordError::Malformed {
            field: field.to_owned(),
            reason: format!("not UTF-8: {error}"),
        })
    }

    fn u8(&mut self, field: &str) -> Result<u8, SocialRecordError> {
        Ok(self.take(1, field)?[0])
    }

    fn u32(&mut self, field: &str) -> Result<u32, SocialRecordError> {
        let raw = self.take(4, field)?;
        Ok(u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]))
    }

    fn u64(&mut self, field: &str) -> Result<u64, SocialRecordError> {
        let raw = self.take(8, field)?;
        let mut buf = [0u8; 8];
        buf.copy_from_slice(raw);
        Ok(u64::from_be_bytes(buf))
    }

    fn i64(&mut self, field: &str) -> Result<i64, SocialRecordError> {
        Ok(self.u64(field)? as i64)
    }
}

/// Decode a canonical byte string back into fields. Strict: a trailing byte,
/// a short field or a bad length prefix is a refusal, never a truncation.
pub fn decode_canonical(bytes: &[u8]) -> Result<SocialFields, SocialRecordError> {
    if !bytes.starts_with(CANONICAL_MAGIC) {
        return Err(SocialRecordError::Malformed {
            field: "magic".to_owned(),
            reason: "canonical bytes do not begin with osl-social-record/v1".to_owned(),
        });
    }
    let mut cursor = Cursor {
        bytes,
        at: CANONICAL_MAGIC.len(),
    };
    let kind = cursor.text("kind")?;
    let record_id = cursor.text("record_id")?;
    let author_id = cursor.text("author_id")?;
    let profile_scope = cursor.text("profile_scope")?;
    let authority_version = cursor.u32("authority_version")?;
    let created_unix_ms = cursor.i64("created_unix_ms")?;
    let deleted = cursor.u8("deleted")? != 0;
    let visibility_stable_id = cursor.text("visibility_stable_id")?;
    let storage_choice = cursor.text("storage_choice")?;
    let body = cursor.len_prefixed("body")?.to_vec();
    let media = cursor.len_prefixed("media")?.to_vec();
    let digest = cursor.len_prefixed("digest")?.to_vec();
    let story = match cursor.u8("story")? {
        0 => None,
        _ => Some(StoryTerms {
            lifetime_seconds: cursor.u64("story.lifetime_seconds")?,
            expires_unix_ms: cursor.i64("story.expires_unix_ms")?,
        }),
    };
    let archive = match cursor.u8("archive")? {
        0 => None,
        _ => Some(ArchiveTerms {
            archive_kind: cursor.text("archive.archive_kind")?,
            payer: cursor.text("archive.payer")?,
        }),
    };
    if cursor.at != bytes.len() {
        return Err(SocialRecordError::Malformed {
            field: "trailing".to_owned(),
            reason: format!(
                "{} trailing byte(s) after a complete record",
                bytes.len() - cursor.at
            ),
        });
    }
    Ok(SocialFields {
        kind,
        record_id,
        author: AuthorBinding {
            author_id,
            profile_scope,
        },
        authority_version,
        created_unix_ms,
        deleted,
        visibility_stable_id,
        storage_choice,
        body,
        media,
        digest,
        story,
        archive,
    })
}

/// One live grant of owner authority: the key currently authorized to sign as
/// an author within one profile scope, and the version of that grant.
#[derive(Debug, Clone)]
pub struct AuthorityGrant {
    pub public_key: ed25519::PublicKey,
    pub authority_version: u32,
}

/// Who is currently authorized to sign, keyed on author id **and** profile
/// scope. Records who holds authority; says nothing about what they may do —
/// create/delete authorization is TASK 4659-4661.
#[derive(Debug, Clone, Default)]
pub struct AuthorityDirectory {
    grants: BTreeMap<(String, String), AuthorityGrant>,
}

impl AuthorityDirectory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Authorize `public_key` at `authority_version`, replacing any earlier
    /// grant for the same author and scope. A rotation is a `grant` with a new
    /// key; a version bump is a `grant` with a higher version.
    pub fn grant(
        &mut self,
        author_id: &str,
        profile_scope: &str,
        public_key: ed25519::PublicKey,
        authority_version: u32,
    ) {
        self.grants.insert(
            (author_id.to_owned(), profile_scope.to_owned()),
            AuthorityGrant {
                public_key,
                authority_version,
            },
        );
    }

    /// Withdraw authority. Everything signed under it stops being admissible
    /// immediately, on write and on read alike.
    pub fn revoke(&mut self, author_id: &str, profile_scope: &str) {
        self.grants
            .remove(&(author_id.to_owned(), profile_scope.to_owned()));
    }

    pub fn live(&self, author_id: &str, profile_scope: &str) -> Option<&AuthorityGrant> {
        self.grants
            .get(&(author_id.to_owned(), profile_scope.to_owned()))
    }
}

fn hex32(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn first_difference(left: &[u8], right: &[u8]) -> usize {
    left.iter()
        .zip(right.iter())
        .position(|(l, r)| l != r)
        .unwrap_or_else(|| left.len().min(right.len()))
}

/// An admitted record. `fields` and `signature` are private, so the only ways
/// to hold one are the three production constructors and
/// [`SocialRecordStore::get`], which re-admits on the way out. All four route
/// through [`admit`], so there is no unchecked path to a `SocialRecord`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocialRecord {
    fields: SocialFields,
    signature: [u8; 64],
}

impl SocialRecord {
    /// Production constructor 1 of 3 — a post.
    pub fn new_post(
        fields: SocialFields,
        signature: [u8; 64],
        directory: &AuthorityDirectory,
    ) -> Result<Self, SocialRecordError> {
        admit(KIND_POST, fields, signature, directory)
    }

    /// Production constructor 2 of 3 — a story.
    pub fn new_story(
        fields: SocialFields,
        signature: [u8; 64],
        directory: &AuthorityDirectory,
    ) -> Result<Self, SocialRecordError> {
        admit(KIND_STORY, fields, signature, directory)
    }

    /// Production constructor 3 of 3 — an archive item.
    pub fn new_archive_item(
        fields: SocialFields,
        signature: [u8; 64],
        directory: &AuthorityDirectory,
    ) -> Result<Self, SocialRecordError> {
        admit(KIND_ARCHIVE, fields, signature, directory)
    }

    pub fn fields(&self) -> &SocialFields {
        &self.fields
    }

    pub fn signature(&self) -> &[u8; 64] {
        &self.signature
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        canonical_bytes(&self.fields)
    }
}

fn require_text(value: &str, field: &str) -> Result<(), SocialRecordError> {
    if value.is_empty() {
        return Err(SocialRecordError::MissingField(field.to_owned()));
    }
    Ok(())
}

/// The single admission gate. Every constructor and every read goes through
/// it, in this order:
///
/// 1. presence — a field the record cannot mean anything without,
/// 2. vocabulary — visibility and storage drawn from the frozen lists,
/// 3. policy — the 4650/4652 decisions for this kind,
/// 4. authority — a live grant at exactly the submitted version,
/// 5. digest — the submitted digest covers the submitted bytes,
/// 6. signature — by the currently authorized key over these canonical bytes.
///
/// Nothing is written by this function; a caller persists only what it
/// returns.
fn admit(
    kind: &str,
    mut fields: SocialFields,
    signature: [u8; 64],
    directory: &AuthorityDirectory,
) -> Result<SocialRecord, SocialRecordError> {
    fields.kind = kind.to_owned();

    // 1. presence
    require_text(&fields.record_id, "record_id")?;
    require_text(&fields.author.author_id, "author_id")?;
    require_text(&fields.author.profile_scope, "profile_scope")?;
    require_text(&fields.visibility_stable_id, "visibility_stable_id")?;
    require_text(&fields.storage_choice, "storage_choice")?;
    if fields.body.is_empty() {
        return Err(SocialRecordError::MissingField("body".to_owned()));
    }
    if fields.digest.is_empty() {
        return Err(SocialRecordError::MissingField("digest".to_owned()));
    }
    if fields.created_unix_ms <= 0 {
        return Err(SocialRecordError::MissingField("created_unix_ms".to_owned()));
    }
    if fields.authority_version == 0 {
        return Err(SocialRecordError::MissingField(
            "authority_version".to_owned(),
        ));
    }
    if fields.digest.len() != 32 {
        return Err(SocialRecordError::Policy {
            field: "digest".to_owned(),
            reason: format!(
                "a body/media digest is 32 bytes, submitted {}",
                fields.digest.len()
            ),
        });
    }

    // 2. vocabulary
    if VisibilityOption::from_stable_id(&fields.visibility_stable_id).is_none() {
        return Err(SocialRecordError::UnknownVisibility(
            fields.visibility_stable_id.clone(),
        ));
    }
    if fields.storage_choice != STORAGE_RELAY && fields.storage_choice != STORAGE_LOCAL_ARCHIVE {
        return Err(SocialRecordError::UnknownStorage(
            fields.storage_choice.clone(),
        ));
    }

    // 3. policy, per kind
    match kind {
        KIND_POST => {
            if fields.story.is_some() {
                return Err(SocialRecordError::Policy {
                    field: "story".to_owned(),
                    reason: "a post has no story lifetime".to_owned(),
                });
            }
            if fields.archive.is_some() {
                return Err(SocialRecordError::Policy {
                    field: "archive".to_owned(),
                    reason: "a post has no archive terms".to_owned(),
                });
            }
            if fields.storage_choice != STORAGE_RELAY {
                return Err(SocialRecordError::Policy {
                    field: "storage_choice".to_owned(),
                    reason: format!("a post is stored on {STORAGE_RELAY}"),
                });
            }
        }
        KIND_STORY => {
            if fields.archive.is_some() {
                return Err(SocialRecordError::Policy {
                    field: "archive".to_owned(),
                    reason: "a story has no archive terms".to_owned(),
                });
            }
            if fields.storage_choice != STORAGE_RELAY {
                return Err(SocialRecordError::Policy {
                    field: "storage_choice".to_owned(),
                    reason: format!(
                        "TASK 4650 gives a story no storage path of its own; it is stored on \
                         {STORAGE_RELAY}"
                    ),
                });
            }
            let Some(story) = fields.story else {
                return Err(SocialRecordError::MissingField("story".to_owned()));
            };
            if story.lifetime_seconds == 0 {
                return Err(SocialRecordError::MissingField(
                    "story.lifetime_seconds".to_owned(),
                ));
            }
            if story.expires_unix_ms <= 0 {
                return Err(SocialRecordError::MissingField(
                    "story.expires_unix_ms".to_owned(),
                ));
            }
            if story.lifetime_seconds > STORY_MAX_LIFETIME_SECONDS {
                return Err(SocialRecordError::Policy {
                    field: "story.lifetime_seconds".to_owned(),
                    reason: format!(
                        "TASK 4650 caps a story at {STORY_MAX_LIFETIME_SECONDS} s, submitted {}",
                        story.lifetime_seconds
                    ),
                });
            }
            let expected = fields
                .created_unix_ms
                .saturating_add((story.lifetime_seconds as i64).saturating_mul(1000));
            if story.expires_unix_ms != expected {
                return Err(SocialRecordError::Policy {
                    field: "story.expires_unix_ms".to_owned(),
                    reason: format!(
                        "expiry {} does not equal created {} plus lifetime {} s",
                        story.expires_unix_ms, fields.created_unix_ms, story.lifetime_seconds
                    ),
                });
            }
        }
        KIND_ARCHIVE => {
            if fields.story.is_some() {
                return Err(SocialRecordError::Policy {
                    field: "story".to_owned(),
                    reason: "an archive item does not expire with the story it saved".to_owned(),
                });
            }
            if fields.storage_choice != STORAGE_LOCAL_ARCHIVE {
                return Err(SocialRecordError::Policy {
                    field: "storage_choice".to_owned(),
                    reason: format!(
                        "TASK 4652 makes the archive a local saved copy, stored on \
                         {STORAGE_LOCAL_ARCHIVE}"
                    ),
                });
            }
            let Some(archive) = fields.archive.as_ref() else {
                return Err(SocialRecordError::MissingField("archive".to_owned()));
            };
            if archive.archive_kind.is_empty() {
                return Err(SocialRecordError::MissingField(
                    "archive.archive_kind".to_owned(),
                ));
            }
            if archive.payer.is_empty() {
                return Err(SocialRecordError::MissingField("archive.payer".to_owned()));
            }
            if archive.archive_kind != ARCHIVE_KIND_LOCAL_SAVED_COPY {
                return Err(SocialRecordError::Policy {
                    field: "archive.archive_kind".to_owned(),
                    reason: format!(
                        "TASK 4652 admits only {ARCHIVE_KIND_LOCAL_SAVED_COPY}, a pointer dies \
                         with its source; submitted {}",
                        archive.archive_kind
                    ),
                });
            }
            if archive.payer != ARCHIVE_PAYER_POSTER_DISK {
                return Err(SocialRecordError::Policy {
                    field: "archive.payer".to_owned(),
                    reason: format!(
                        "TASK 4652 rules nobody pays OSL for an archive; admits only \
                         {ARCHIVE_PAYER_POSTER_DISK}, submitted {}",
                        archive.payer
                    ),
                });
            }
        }
        other => {
            return Err(SocialRecordError::Policy {
                field: "kind".to_owned(),
                reason: format!("no production constructor produces kind {other}"),
            })
        }
    }

    // 4. authority
    let Some(grant) = directory.live(&fields.author.author_id, &fields.author.profile_scope) else {
        return Err(SocialRecordError::NoAuthority(
            fields.author.author_id.clone(),
        ));
    };
    if grant.authority_version != fields.authority_version {
        return Err(SocialRecordError::StaleAuthority {
            authorized: grant.authority_version,
            submitted: fields.authority_version,
        });
    }

    // 5. digest
    let recomputed = body_media_digest(&fields.body, &fields.media);
    if recomputed.as_slice() != fields.digest.as_slice() {
        return Err(SocialRecordError::DigestMismatch {
            recomputed: hex32(&recomputed),
            submitted: hex32(&fields.digest),
            offset: first_difference(&recomputed, &fields.digest),
        });
    }

    // 6. signature
    let canonical = canonical_bytes(&fields);
    let verified = ed25519::verify(
        &grant.public_key,
        &canonical,
        &ed25519::Signature::from_bytes(signature),
    )
    .unwrap_or(false);
    if !verified {
        return Err(SocialRecordError::SignatureRefused {
            author_id: fields.author.author_id.clone(),
            profile_scope: fields.author.profile_scope.clone(),
            canonical_len: canonical.len(),
        });
    }

    Ok(SocialRecord { fields, signature })
}

/// The production social-record repository: `social.sqlite` under the app-data
/// directory, one sealed row per record.
///
/// The record id never reaches disk — the row is keyed on a keyed blind index
/// of it, and that index is the AEAD associated data, so a row cannot be
/// transplanted onto another record's id. `kind` is not stored in the clear
/// either; it lives inside the sealed canonical bytes.
pub struct SocialRecordStore {
    conn: Connection,
    key: aead::Key,
    index_key: [u8; 32],
    path: PathBuf,
}

/// Derive the social-record sealing key from the caller's identity secret.
/// Public so an independent reader can unseal a durable row without going
/// through this repository.
pub fn derive_social_key(identity_secret: &[u8; 32]) -> Result<[u8; 32], StoreError> {
    hkdf::derive_32(&[], identity_secret, SOCIAL_HKDF_INFO)
        .map_err(|e| StoreError::Sealer(format!("HKDF social derive: {e}")))
}

/// Derive the social-record blind-index key. See [`derive_social_key`].
pub fn derive_social_index_key(identity_secret: &[u8; 32]) -> Result<[u8; 32], StoreError> {
    hkdf::derive_32(&[], identity_secret, SOCIAL_INDEX_HKDF_INFO)
        .map_err(|e| StoreError::Sealer(format!("HKDF social index derive: {e}")))
}

/// The blind index a record id is stored under. See [`derive_social_index_key`].
pub fn social_record_blind_index(
    index_key: &[u8; 32],
    record_id: &str,
) -> Result<Vec<u8>, StoreError> {
    let out = hkdf::derive_32(index_key, record_id.as_bytes(), BI_RECORD_ID)
        .map_err(|e| StoreError::Sealer(format!("social blind index derive: {e}")))?;
    Ok(out.to_vec())
}

/// The sealed payload: the canonical bytes and the signature over them, so a
/// row that survives a restart still carries its own proof of authorship.
fn sealed_payload(canonical: &[u8], signature: &[u8; 64]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + canonical.len() + 64);
    out.extend_from_slice(&(canonical.len() as u32).to_be_bytes());
    out.extend_from_slice(canonical);
    out.extend_from_slice(signature);
    out
}

fn split_payload(payload: &[u8]) -> Result<(Vec<u8>, [u8; 64]), StoreError> {
    if payload.len() < 4 + 64 {
        return Err(StoreError::Corrupted(format!(
            "social row payload is {} bytes, too short to hold a record and a signature",
            payload.len()
        )));
    }
    let n = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
    if payload.len() != 4 + n + 64 {
        return Err(StoreError::Corrupted(format!(
            "social row payload is {} bytes, want {} for a {n}-byte record",
            payload.len(),
            4 + n + 64
        )));
    }
    let canonical = payload[4..4 + n].to_vec();
    let mut signature = [0u8; 64];
    signature.copy_from_slice(&payload[4 + n..]);
    Ok((canonical, signature))
}

impl SocialRecordStore {
    /// Open (creating if absent) `<app_data_dir>/social.sqlite`.
    pub fn open(app_data_dir: &Path, identity_secret: &[u8; 32]) -> Result<Self, StoreError> {
        std::fs::create_dir_all(app_data_dir)?;
        let path = app_data_dir.join(SOCIAL_DB_FILENAME);
        let conn = Connection::open(&path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS social_records (
                 record_bi BLOB PRIMARY KEY NOT NULL,
                 nonce     BLOB NOT NULL,
                 sealed    BLOB NOT NULL
             );",
        )?;
        Ok(Self {
            key: aead::Key::from_bytes(derive_social_key(identity_secret)?),
            index_key: derive_social_index_key(identity_secret)?,
            conn,
            path,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Persist an already-admitted record. Re-encodes and delegates, so there
    /// is exactly one gate in front of the disk regardless of entry point.
    pub fn put(
        &self,
        record: &SocialRecord,
        directory: &AuthorityDirectory,
    ) -> Result<(), StoreError> {
        self.put_canonical(
            &record.fields.kind.clone(),
            &record.canonical_bytes(),
            record.signature(),
            directory,
        )
    }

    /// Persist a record submitted as canonical bytes plus a signature — the
    /// shape a record arrives in over a wire or off another device.
    ///
    /// Dispatches by the kind declared *inside* the canonical bytes through
    /// the three production constructors, so starving a constructor starves
    /// the write path for that kind rather than falling back to a generic one.
    pub fn put_canonical(
        &self,
        declared_kind: &str,
        canonical: &[u8],
        signature: &[u8; 64],
        directory: &AuthorityDirectory,
    ) -> Result<(), StoreError> {
        let fields = decode_canonical(canonical)?;
        if fields.kind != declared_kind {
            return Err(StoreError::SocialRecord(SocialRecordError::Policy {
                field: "kind".to_owned(),
                reason: format!(
                    "declared {declared_kind}, canonical bytes say {}",
                    fields.kind
                ),
            }));
        }
        let record = match fields.kind.as_str() {
            KIND_POST => SocialRecord::new_post(fields, *signature, directory),
            KIND_STORY => SocialRecord::new_story(fields, *signature, directory),
            KIND_ARCHIVE => SocialRecord::new_archive_item(fields, *signature, directory),
            other => Err(SocialRecordError::Policy {
                field: "kind".to_owned(),
                reason: format!("no production constructor produces kind {other}"),
            }),
        }?;

        let record_bi = social_record_blind_index(&self.index_key, &record.fields.record_id)?;
        let mut aad = SOCIAL_ROW_AAD.to_vec();
        aad.extend_from_slice(&record_bi);
        let payload = sealed_payload(&record.canonical_bytes(), record.signature());
        let (nonce, sealed) = cipher::seal(&self.key, &aad, &payload)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO social_records (record_bi, nonce, sealed)
             VALUES (?1, ?2, ?3)",
            params![record_bi, nonce, sealed],
        )?;
        Ok(())
    }

    /// Read one record back, re-admitting it through the same gate. A row
    /// whose author lost authority since it was written no longer reads.
    pub fn get(
        &self,
        record_id: &str,
        directory: &AuthorityDirectory,
    ) -> Result<Option<SocialRecord>, StoreError> {
        let record_bi = social_record_blind_index(&self.index_key, record_id)?;
        let row: Option<(Vec<u8>, Vec<u8>)> = self
            .conn
            .query_row(
                "SELECT nonce, sealed FROM social_records WHERE record_bi = ?1",
                params![record_bi],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((nonce, sealed)) = row else {
            return Ok(None);
        };
        let mut aad = SOCIAL_ROW_AAD.to_vec();
        aad.extend_from_slice(&record_bi);
        let payload = cipher::unseal(&self.key, &aad, &nonce, &sealed)?;
        let (canonical, signature) = split_payload(&payload)?;
        let fields = decode_canonical(&canonical)?;
        let record = match fields.kind.as_str() {
            KIND_POST => SocialRecord::new_post(fields, signature, directory),
            KIND_STORY => SocialRecord::new_story(fields, signature, directory),
            KIND_ARCHIVE => SocialRecord::new_archive_item(fields, signature, directory),
            other => Err(SocialRecordError::Policy {
                field: "kind".to_owned(),
                reason: format!("no production constructor produces kind {other}"),
            }),
        }?;
        Ok(Some(record))
    }

    /// How many records are durably stored.
    pub fn row_count(&self) -> Result<i64, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM social_records", [], |row| row.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed(
        kind: &str,
        fields: SocialFields,
        secret: &ed25519::SecretKey,
    ) -> (SocialFields, [u8; 64]) {
        let mut fields = fields;
        fields.kind = kind.to_owned();
        fields.digest = body_media_digest(&fields.body, &fields.media).to_vec();
        let signature = ed25519::sign(secret, &canonical_bytes(&fields));
        (fields, *signature.as_bytes())
    }

    fn base(kind: &str) -> SocialFields {
        SocialFields {
            kind: kind.to_owned(),
            record_id: "rec.unit".to_owned(),
            author: AuthorBinding {
                author_id: "osl:author:unit".to_owned(),
                profile_scope: "osl_chats".to_owned(),
            },
            authority_version: 3,
            created_unix_ms: 1_700_000_000_000,
            deleted: false,
            visibility_stable_id: "vis.chosen".to_owned(),
            storage_choice: STORAGE_RELAY.to_owned(),
            body: b"unit body".to_vec(),
            media: vec![0, 1, 2, 3],
            digest: Vec::new(),
            story: None,
            archive: None,
        }
    }

    #[test]
    fn canonical_bytes_round_trip_exactly() {
        let fields = {
            let mut f = base(KIND_POST);
            f.digest = body_media_digest(&f.body, &f.media).to_vec();
            f
        };
        let encoded = canonical_bytes(&fields);
        assert_eq!(decode_canonical(&encoded).unwrap(), fields);
    }

    #[test]
    fn a_post_admits_and_a_moved_byte_does_not() {
        let (secret, public) = ed25519::generate_keypair();
        let mut directory = AuthorityDirectory::new();
        directory.grant("osl:author:unit", "osl_chats", public, 3);
        let (fields, signature) = signed(KIND_POST, base(KIND_POST), &secret);
        SocialRecord::new_post(fields.clone(), signature, &directory).unwrap();

        let mut moved = fields;
        moved.body[0] ^= 0x01;
        moved.digest = body_media_digest(&moved.body, &moved.media).to_vec();
        let error = SocialRecord::new_post(moved, signature, &directory).unwrap_err();
        assert!(matches!(error, SocialRecordError::SignatureRefused { .. }));
    }
}
