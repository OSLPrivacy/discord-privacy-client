//! TASK 4658 — enforce the chosen fetch rule without server-side reading.
//!
//! TASK 4651 froze the rule: **the sender decides the audience, and the
//! audience is expressed as WHO GETS KEYS, not as a flag a server enforces.**
//! TASK 4656 put the four frozen options into Settings. TASK 4657 built the
//! signed post/story/archive record. This module is the part that carries a
//! 4657 record from the sender to the people the Settings choice named — and
//! only to them — across a store that never reads a byte of it.
//!
//! ## The shape of the enforcement
//!
//! ```text
//!   Settings (content-defaults)                 the sender's device
//!        │  post_visibility / story_visibility        │
//!        ▼                                            │
//!   resolve_post_audience / resolve_story_audience     │  4651's rule, shipped once
//!        │  Audience { keyed, unkeyed }                │
//!        ▼                                            ▼
//!   distribute_post / distribute_story  ──►  one random content key CK
//!        │                                     content = AEAD(CK, canonical||sig)
//!        │                                     one wrap of CK per keyed account
//!        │                                     + decoy wraps to a fixed cohort size
//!        ▼
//!   ContentRelay  ── opaque bytes in, the same opaque bytes out, to anyone ──►
//!        │                                            │
//!        ▼                                            ▼
//!   allowed viewer: one wrap opens, CK opens    refused viewer: no wrap opens,
//!   the content, 4657 re-admits the record      0 keys, 0 openable bytes, and
//!   byte-identically                            the frozen refusal copy
//! ```
//!
//! ## Why the server cannot read, structurally and not by promise
//!
//! [`ContentRelay`]'s entire typed surface is `&[u8]` in and `Vec<u8>` out. It
//! is never handed a [`SocialRecord`], a visibility id, an audience, or a
//! plaintext byte; it has no type through which to see one. It answers **every**
//! fetch from **every** requester with the identical bytes, because it has
//! nothing to discriminate on. The one counter that would move if it ever did
//! look — [`ContentRelay::content_checks`] — is incremented only by
//! [`ContentRelay::note_content_inspection`], which nothing in this module
//! calls.
//!
//! ## What the envelope does not say
//!
//! - **No recipient labels.** A wrap carries a nonce and a ciphertext and
//!   nothing else. A viewer finds its own wrap by trying all of them; the store
//!   cannot tell one recipient's slot from another's, or from a decoy.
//! - **No audience size.** Wraps are padded with indistinguishable decoys to a
//!   fixed cohort size ([`cohort_slots`]), so a one-person audience and an
//!   eight-person audience produce byte-identical shapes.
//! - **No slot order.** Slots are sorted by their own ciphertext bytes, so
//!   position carries no information about who was keyed.
//! - **No record id.** The relay is keyed on [`fetch_handle`], a
//!   domain-separated hash of the record id.
//! - **No kind.** Whether the object is a post or a story lives inside the
//!   sealed canonical bytes, which is why the refusal copy cannot name it.
//!
//! ## There is no key store to seed
//!
//! A [`ViewerAccount`] has no content-key field. The only content key it ever
//! holds is one it unwrapped inside a single [`ViewerAccount::open_fetched`]
//! call, and that key is dropped when the call returns. An account cannot be
//! handed a content key out of band because there is nowhere to put one.
//!
//! ## Scope
//!
//! This module distributes and opens. It does not decide whether a person may
//! *create* or *delete* anything — that is TASK 4659-4661 — and it adds no
//! server-side policy of any kind, which is the whole point.

use crate::social::{
    decode_canonical, AuthorityDirectory, SocialFields, SocialRecord, SocialRecordStore, KIND_POST,
    KIND_STORY, STORAGE_RELAY,
};
use crate::StoreError;
use content_defaults::{
    resolve_post_audience, resolve_story_audience, Defaults, SignedSendTo, VisibilityOption,
};
use crypto::{aead, hkdf, random, x25519};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use thiserror::Error;

/// An account: one person's device, addressed the same way a person is.
pub type AccountId = String;

/// Domain prefix of the wire envelope.
pub const FETCH_MAGIC: &[u8] = b"osl-social-fetch/v1";

/// HKDF info for the relay lookup handle. The record id itself never reaches
/// the relay.
pub const HANDLE_INFO: &[u8] = b"osl-social-fetch/handle/v1";

/// HKDF info for one per-recipient key-wrapping key.
pub const WRAP_INFO: &[u8] = b"osl-social-fetch/wrap/v1";

/// AEAD associated data domain for a key wrap. Binds the wrap to its handle, so
/// a wrap lifted off one object cannot be replayed onto another.
pub const WRAP_AAD: &[u8] = b"osl-social-fetch/wrap-aad/v1";

/// AEAD associated data domain for the sealed content.
pub const CONTENT_AAD: &[u8] = b"osl-social-fetch/content/v1";

/// HKDF info for an account's long-lived X25519 fetch key, derived from the
/// account's identity secret so it survives a restart without a key file.
pub const ACCOUNT_FETCH_INFO: &[u8] = b"osl-social-fetch/account-key/v1";

/// The relay's SQLite file. Its own file: the relay is not the client's store.
pub const RELAY_DB_FILENAME: &str = "social-relay.sqlite";

/// Bytes of one sealed content key: 32 key bytes plus the 16-byte AEAD tag.
pub const WRAP_CIPHERTEXT_LEN: usize = 32 + aead::TAG_SIZE;

/// The refusal a person sees when the sender did not give them a key.
///
/// Frozen copy. It names no author, no audience member, no record and not even
/// whether the object was a post or a story — the refused client genuinely does
/// not know, because the kind is inside the sealed bytes. Saying more would
/// leak exactly what the fetch rule exists to withhold.
pub const REFUSED_NO_KEY_COPY: &str = "You can't open this. Whoever shared it chose who gets the \
     key and you weren't given one. No OSL server holds a key either, so there is nothing here to \
     unlock.";

/// The refusal a person sees when a key opened but the sealed content did not:
/// something altered the bytes between the sender and here.
pub const REFUSED_DAMAGED_COPY: &str = "You can't open this. The copy that came back did not \
     survive the trip intact, so OSL will not show you a guess at what it said.";

/// The refusal a person sees when the content opened but the record inside is
/// not signed by the currently authorized author it claims.
pub const REFUSED_NOT_AUTHENTIC_COPY: &str = "You can't open this. It opened, but it is not \
     signed by the person it claims to come from, so OSL will not show it to you.";

/// Every way distribution can be refused **before** anything reaches the relay.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DistributionError {
    /// A post entry point was handed a story record, or the reverse.
    #[error("wrong kind: field kind, expected {expected}, record says {found}")]
    WrongKind { expected: String, found: String },

    /// The signed record's visibility is not the option Settings resolved. The
    /// sender's intent and the sender's signature have to be the same thing.
    #[error(
        "settings disagree: field visibility_stable_id, record says {record_says}, \
         settings resolved {settings_say}"
    )]
    SettingsDisagree {
        record_says: String,
        settings_say: String,
    },

    /// A record bound for the relay must say so.
    #[error("wrong storage: field storage_choice, expected {expected}, record says {found}")]
    WrongStorage { expected: String, found: String },

    /// Somebody the audience names has published no fetch key, so there is no
    /// way to give them a key. Refused loudly rather than silently skipped: a
    /// silently shrunk audience is the exact failure this task exists to catch.
    #[error("no published fetch key: field audience, account {0}")]
    NoFetchKey(AccountId),

    /// The resolved audience is empty. 4651 requires every option to produce at
    /// least one keyed person; an object nobody can ever open is not a post.
    #[error("empty audience: field visibility_stable_id, option {0} keyed nobody")]
    EmptyAudience(String),

    /// A per-story SEND TO override was presented and did not verify.
    #[error("story override refused: field send_to, {0}")]
    StoryOverride(String),

    /// A primitive failed.
    #[error("crypto: field {field}, {reason}")]
    Crypto { field: String, reason: String },

    /// The relay would not take the bytes.
    #[error("relay: field envelope, {0}")]
    Relay(String),
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The relay lookup handle for one record id.
///
/// Deterministic and domain-separated: any client that knows the record id can
/// compute where the bytes live, which is deliberate. The fetch rule does not
/// depend on hiding the address — it depends on who holds a key.
pub fn fetch_handle(record_id: &str) -> [u8; 32] {
    hkdf::derive_32(&[], record_id.as_bytes(), HANDLE_INFO)
        .expect("HKDF-SHA256 over a 32-byte output never fails")
}

/// Derive an account's long-lived X25519 fetch key from its identity secret.
///
/// Public on purpose, exactly as [`crate::social::derive_social_key`] is: an
/// independent auditor can re-derive an account's key from the secret it was
/// created with, without the account handing over anything at run time.
pub fn derive_fetch_secret(identity_secret: &[u8; 32]) -> x25519::SecretKey {
    let bytes = hkdf::derive_32(&[], identity_secret, ACCOUNT_FETCH_INFO)
        .expect("HKDF-SHA256 over a 32-byte output never fails");
    x25519::SecretKey::from_bytes(bytes)
}

/// How many wrap slots an envelope carries for a keyed audience of `n`.
///
/// A ladder, not `n`: the number of slots is what the relay can see, so it must
/// not be the number of people. Everything from a one-person audience to an
/// eight-person one produces eight slots.
pub fn cohort_slots(n: usize) -> usize {
    for rung in [8usize, 32, 128, 512, 2048] {
        if n <= rung {
            return rung;
        }
    }
    n.div_ceil(2048) * 2048
}

/// The world at post time, as the sender's device knows it.
///
/// `universe` is every other person the sender knows of; `friends_at_time` is
/// the friend list at that instant — 4651 is explicit that a person who becomes
/// a friend later is not re-keyed; `author_devices` is the author's own devices,
/// which only `vis.onlyme` keys.
#[derive(Debug, Clone, Default)]
pub struct WorldSnapshot {
    pub universe: BTreeSet<AccountId>,
    pub friends_at_time: BTreeSet<AccountId>,
    pub author_devices: BTreeSet<AccountId>,
}

/// The published fetch keys the sender can address. A person with no entry
/// here cannot be keyed, and distribution refuses rather than dropping them.
#[derive(Debug, Clone, Default)]
pub struct RecipientDirectory {
    keys: BTreeMap<AccountId, x25519::PublicKey>,
}

impl RecipientDirectory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn publish(&mut self, account: &str, public: x25519::PublicKey) {
        self.keys.insert(account.to_owned(), public);
    }

    pub fn get(&self, account: &str) -> Option<&x25519::PublicKey> {
        self.keys.get(account)
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// One wrap slot: a nonce and a ciphertext, and nothing that says whose it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySlot {
    pub nonce: [u8; aead::NONCE_SIZE],
    pub ciphertext: Vec<u8>,
}

impl KeySlot {
    fn sort_bytes(&self) -> Vec<u8> {
        let mut out = self.nonce.to_vec();
        out.extend_from_slice(&self.ciphertext);
        out
    }
}

/// What the relay holds and what every requester gets back, byte for byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchEnvelope {
    pub handle: [u8; 32],
    pub ephemeral_public: [u8; 32],
    pub slots: Vec<KeySlot>,
    pub content_nonce: [u8; aead::NONCE_SIZE],
    pub content_ciphertext: Vec<u8>,
}

fn put_u32(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(&(n as u32).to_be_bytes());
}

impl FetchEnvelope {
    /// The canonical wire encoding. Length-prefixed throughout, so a truncated
    /// or padded envelope is a decode refusal rather than a short read.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(FETCH_MAGIC);
        put_u32(&mut out, self.handle.len());
        out.extend_from_slice(&self.handle);
        out.extend_from_slice(&self.ephemeral_public);
        put_u32(&mut out, self.slots.len());
        for slot in &self.slots {
            out.extend_from_slice(&slot.nonce);
            put_u32(&mut out, slot.ciphertext.len());
            out.extend_from_slice(&slot.ciphertext);
        }
        out.extend_from_slice(&self.content_nonce);
        put_u32(&mut out, self.content_ciphertext.len());
        out.extend_from_slice(&self.content_ciphertext);
        out
    }

    /// Strict decode. Any short field, bad length prefix or trailing byte is a
    /// refusal naming the field and the byte offset.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut at = 0usize;
        let take = |n: usize, field: &str, at: &mut usize| -> Result<Vec<u8>, String> {
            if *at + n > bytes.len() {
                return Err(format!(
                    "field {field}, wanted {n} bytes at offset {}, only {} remain",
                    *at,
                    bytes.len().saturating_sub(*at)
                ));
            }
            let slice = bytes[*at..*at + n].to_vec();
            *at += n;
            Ok(slice)
        };
        let magic = take(FETCH_MAGIC.len(), "magic", &mut at)?;
        if magic != FETCH_MAGIC {
            return Err("field magic, envelope does not begin with osl-social-fetch/v1".to_owned());
        }
        let raw = take(4, "handle_len", &mut at)?;
        let handle_len = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
        if handle_len != 32 {
            return Err(format!(
                "field handle_len, a handle is 32 bytes, envelope says {handle_len}"
            ));
        }
        let handle: [u8; 32] = take(32, "handle", &mut at)?
            .try_into()
            .expect("32 bytes taken");
        let ephemeral_public: [u8; 32] = take(32, "ephemeral_public", &mut at)?
            .try_into()
            .expect("32 bytes taken");
        let raw = take(4, "slot_count", &mut at)?;
        let slot_count = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
        if slot_count > 1_000_000 {
            return Err(format!("field slot_count, implausible slot count {slot_count}"));
        }
        let mut slots = Vec::with_capacity(slot_count);
        for index in 0..slot_count {
            let nonce: [u8; aead::NONCE_SIZE] =
                take(aead::NONCE_SIZE, &format!("slot[{index}].nonce"), &mut at)?
                    .try_into()
                    .expect("nonce-sized slice");
            let raw = take(4, &format!("slot[{index}].len"), &mut at)?;
            let n = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
            let ciphertext = take(n, &format!("slot[{index}].ciphertext"), &mut at)?;
            slots.push(KeySlot { nonce, ciphertext });
        }
        let content_nonce: [u8; aead::NONCE_SIZE] =
            take(aead::NONCE_SIZE, "content_nonce", &mut at)?
                .try_into()
                .expect("nonce-sized slice");
        let raw = take(4, "content_len", &mut at)?;
        let n = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
        let content_ciphertext = take(n, "content_ciphertext", &mut at)?;
        if at != bytes.len() {
            return Err(format!(
                "field trailing, {} trailing byte(s) after a complete envelope at offset {at}",
                bytes.len() - at
            ));
        }
        Ok(Self {
            handle,
            ephemeral_public,
            slots,
            content_nonce,
            content_ciphertext,
        })
    }
}

/// What the sender learns about a distribution it just performed. The keyed set
/// is reported so a caller can be held to it; nothing here is sent anywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistributionReceipt {
    pub handle: [u8; 32],
    pub handle_hex: String,
    pub option_stable_id: String,
    pub keyed: BTreeSet<AccountId>,
    pub unkeyed: BTreeSet<AccountId>,
    pub real_wraps: usize,
    pub decoy_wraps: usize,
    pub slot_count: usize,
    pub content_len: usize,
    pub wire_len: usize,
}

/// The sealed content plaintext: the 4657 canonical bytes and the author's
/// signature over them, so what a viewer opens still carries its own proof.
fn content_plaintext(canonical: &[u8], signature: &[u8; 64]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + canonical.len() + 64);
    put_u32(&mut out, canonical.len());
    out.extend_from_slice(canonical);
    out.extend_from_slice(signature);
    out
}

fn split_content(plain: &[u8]) -> Result<(Vec<u8>, [u8; 64]), String> {
    if plain.len() < 4 + 64 {
        return Err(format!(
            "field content, {} bytes is too short to hold a record and a signature",
            plain.len()
        ));
    }
    let n = u32::from_be_bytes([plain[0], plain[1], plain[2], plain[3]]) as usize;
    if plain.len() != 4 + n + 64 {
        return Err(format!(
            "field content, {} bytes, want {} for a {n}-byte record",
            plain.len(),
            4 + n + 64
        ));
    }
    let canonical = plain[4..4 + n].to_vec();
    let mut signature = [0u8; 64];
    signature.copy_from_slice(&plain[4 + n..]);
    Ok((canonical, signature))
}

fn wrap_key_for(
    handle: &[u8; 32],
    shared: &[u8; 32],
    field: &str,
) -> Result<aead::Key, DistributionError> {
    let bytes = hkdf::derive_32(handle, shared, WRAP_INFO).map_err(|e| DistributionError::Crypto {
        field: field.to_owned(),
        reason: format!("HKDF wrap-key derive: {e}"),
    })?;
    Ok(aead::Key::from_bytes(bytes))
}

fn wrap_aad(handle: &[u8; 32]) -> Vec<u8> {
    let mut aad = WRAP_AAD.to_vec();
    aad.extend_from_slice(handle);
    aad
}

fn content_aad(handle: &[u8; 32]) -> Vec<u8> {
    let mut aad = CONTENT_AAD.to_vec();
    aad.extend_from_slice(handle);
    aad
}

/// Production sender path 1 of 2 — a post.
///
/// A post has no per-post visibility control (TASK 4656): the audience comes
/// from the Settings default and nowhere else, which is why this function takes
/// no visibility argument.
pub fn distribute_post(
    record: &SocialRecord,
    defaults: &Defaults,
    world: &WorldSnapshot,
    directory: &RecipientDirectory,
    relay: &ContentRelay,
) -> Result<DistributionReceipt, DistributionError> {
    require_kind(record, KIND_POST)?;
    let option = defaults.post_visibility.option;
    let audience = resolve_post_audience(
        defaults,
        &world.universe,
        &world.friends_at_time,
        &world.author_devices,
    );
    distribute(record, option, audience, directory, relay)
}

/// Production sender path 2 of 2 — a story.
///
/// A story may carry a signed per-story SEND TO override (TASK 4656). When it
/// does, the override is the audience and the Settings default is not consulted;
/// when it does not, a story behaves exactly as a post does.
pub fn distribute_story(
    record: &SocialRecord,
    defaults: &Defaults,
    world: &WorldSnapshot,
    directory: &RecipientDirectory,
    relay: &ContentRelay,
    override_send_to: Option<&SignedSendTo>,
) -> Result<DistributionReceipt, DistributionError> {
    require_kind(record, KIND_STORY)?;
    let option = match override_send_to {
        Some(send_to) => send_to.choice.option,
        None => defaults.story_visibility.option,
    };
    let audience = resolve_story_audience(
        defaults,
        &world.universe,
        &world.friends_at_time,
        &world.author_devices,
        override_send_to,
    )
    .map_err(DistributionError::StoryOverride)?;
    distribute(record, option, audience, directory, relay)
}

fn require_kind(record: &SocialRecord, expected: &str) -> Result<(), DistributionError> {
    if record.fields().kind != expected {
        return Err(DistributionError::WrongKind {
            expected: expected.to_owned(),
            found: record.fields().kind.clone(),
        });
    }
    Ok(())
}

/// The single distribution gate. Both production sender paths reach the relay
/// only through here, so starving one entry point starves that content type
/// rather than falling through to a generic one.
fn distribute(
    record: &SocialRecord,
    option: VisibilityOption,
    audience: content_defaults::Audience,
    directory: &RecipientDirectory,
    relay: &ContentRelay,
) -> Result<DistributionReceipt, DistributionError> {
    // The signature and the intent have to be the same statement. A record
    // signed `vis.onlyme` cannot be distributed under a `vis.everyone` setting,
    // in either direction.
    if record.fields().visibility_stable_id != option.stable_id() {
        return Err(DistributionError::SettingsDisagree {
            record_says: record.fields().visibility_stable_id.clone(),
            settings_say: option.stable_id().to_owned(),
        });
    }
    if record.fields().storage_choice != STORAGE_RELAY {
        return Err(DistributionError::WrongStorage {
            expected: STORAGE_RELAY.to_owned(),
            found: record.fields().storage_choice.clone(),
        });
    }
    if audience.keyed.is_empty() {
        return Err(DistributionError::EmptyAudience(
            option.stable_id().to_owned(),
        ));
    }

    let handle = fetch_handle(&record.fields().record_id);
    let canonical = record.canonical_bytes();
    let plain = content_plaintext(&canonical, record.signature());

    let content_key_bytes: [u8; 32] = random::random_bytes(32)
        .try_into()
        .expect("random_bytes(32) is 32 bytes");
    let content_key = aead::Key::from_bytes(content_key_bytes);
    let content_nonce = random::random_nonce();
    let content_ciphertext = aead::seal(&content_key, &content_nonce, &content_aad(&handle), &plain)
        .map_err(|e| DistributionError::Crypto {
            field: "content".to_owned(),
            reason: format!("AEAD seal: {e}"),
        })?;

    let (ephemeral_secret, ephemeral_public) = x25519::generate_keypair();
    let mut slots: Vec<KeySlot> = Vec::new();
    for account in &audience.keyed {
        let peer = directory
            .get(account)
            .ok_or_else(|| DistributionError::NoFetchKey(account.clone()))?;
        let shared = x25519::diffie_hellman(&ephemeral_secret, peer).map_err(|e| {
            DistributionError::Crypto {
                field: format!("wrap[{account}]"),
                reason: format!("X25519: {e}"),
            }
        })?;
        let key = wrap_key_for(&handle, shared.as_bytes(), &format!("wrap[{account}]"))?;
        let nonce = random::random_nonce();
        let ciphertext = aead::seal(&key, &nonce, &wrap_aad(&handle), &content_key_bytes).map_err(
            |e| DistributionError::Crypto {
                field: format!("wrap[{account}]"),
                reason: format!("AEAD seal: {e}"),
            },
        )?;
        slots.push(KeySlot {
            nonce: *nonce.as_bytes(),
            ciphertext,
        });
    }
    let real_wraps = slots.len();

    // Pad to the cohort size with wraps nobody holds a key for. A decoy is the
    // same length, sealed under a fresh random key with the same AAD, so it is
    // indistinguishable from a real one to anybody who cannot open it — which
    // includes the relay and every refused viewer.
    let slot_count = cohort_slots(real_wraps);
    while slots.len() < slot_count {
        let decoy_key_bytes: [u8; 32] = random::random_bytes(32)
            .try_into()
            .expect("random_bytes(32) is 32 bytes");
        let decoy_key = aead::Key::from_bytes(decoy_key_bytes);
        let nonce = random::random_nonce();
        let filler: [u8; 32] = random::random_bytes(32)
            .try_into()
            .expect("random_bytes(32) is 32 bytes");
        let ciphertext =
            aead::seal(&decoy_key, &nonce, &wrap_aad(&handle), &filler).map_err(|e| {
                DistributionError::Crypto {
                    field: "decoy".to_owned(),
                    reason: format!("AEAD seal: {e}"),
                }
            })?;
        slots.push(KeySlot {
            nonce: *nonce.as_bytes(),
            ciphertext,
        });
    }
    let decoy_wraps = slots.len() - real_wraps;

    // Position must not encode identity: sort by the slot's own bytes.
    slots.sort_by(|left, right| left.sort_bytes().cmp(&right.sort_bytes()));

    let envelope = FetchEnvelope {
        handle,
        ephemeral_public: *ephemeral_public.as_bytes(),
        slots,
        content_nonce: *content_nonce.as_bytes(),
        content_ciphertext,
    };
    let wire = envelope.encode();
    relay
        .publish(&handle, &wire)
        .map_err(|e| DistributionError::Relay(e.to_string()))?;

    Ok(DistributionReceipt {
        handle,
        handle_hex: hex(&handle),
        option_stable_id: option.stable_id().to_owned(),
        keyed: audience.keyed.clone(),
        unkeyed: audience.unkeyed.clone(),
        real_wraps,
        decoy_wraps,
        slot_count,
        content_len: envelope.content_ciphertext.len(),
        wire_len: wire.len(),
    })
}

/// What the relay said when asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayResponse {
    /// The stored bytes, verbatim. This is the only answer shipping code
    /// produces when the object exists.
    Delivered(Vec<u8>),
    /// No object under that handle. Not a policy refusal — an empty shelf.
    NoSuchObject,
    /// A policy refusal. Shipping code never produces one; the variant exists
    /// so a starved build *can*, and so the check can prove it did not happen.
    Refused(String),
}

/// One request as the relay saw it: who asked, for what, and what it answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayRequest {
    pub requester: String,
    pub handle_hex: String,
    pub delivered: bool,
}

/// The blind object store the sender publishes to and every client fetches
/// from.
///
/// Opaque bytes in, the same opaque bytes out, to whoever asks. It is told who
/// is asking — a real relay sees a connection — and answers all of them
/// identically anyway, because there is nothing in its typed surface it could
/// discriminate on.
pub struct ContentRelay {
    conn: Connection,
    path: PathBuf,
    answered: AtomicU64,
    delivered: AtomicU64,
    missing: AtomicU64,
    refused: AtomicU64,
    content_checks: AtomicU64,
    content_check_reasons: Mutex<Vec<String>>,
    request_log: Mutex<Vec<RelayRequest>>,
}

impl ContentRelay {
    /// Open (creating if absent) `<dir>/social-relay.sqlite`.
    pub fn open(dir: &Path) -> Result<Self, StoreError> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(RELAY_DB_FILENAME);
        let conn = Connection::open(&path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS envelopes (
                 handle BLOB PRIMARY KEY NOT NULL,
                 wire   BLOB NOT NULL
             );",
        )?;
        Ok(Self {
            conn,
            path,
            answered: AtomicU64::new(0),
            delivered: AtomicU64::new(0),
            missing: AtomicU64::new(0),
            refused: AtomicU64::new(0),
            content_checks: AtomicU64::new(0),
            content_check_reasons: Mutex::new(Vec::new()),
            request_log: Mutex::new(Vec::new()),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Take bytes. The relay is handed a handle and a blob and is told nothing
    /// else — not the kind, not the author, not the audience.
    pub fn publish(&self, handle: &[u8], wire: &[u8]) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO envelopes (handle, wire) VALUES (?1, ?2)",
            params![handle, wire],
        )?;
        Ok(())
    }

    /// Answer a fetch. Every requester, every time, the same bytes.
    ///
    /// `requester` is logged and then ignored: the relay records who asked
    /// because a real one cannot help seeing, and it does not branch on it.
    pub fn fetch(&self, requester: &str, handle: &[u8]) -> RelayResponse {
        self.answered.fetch_add(1, Ordering::SeqCst);
        let row: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT wire FROM envelopes WHERE handle = ?1",
                params![handle],
                |row| row.get(0),
            )
            .optional()
            .unwrap_or(None);
        let response = match row {
            Some(wire) => {
                self.delivered.fetch_add(1, Ordering::SeqCst);
                RelayResponse::Delivered(wire)
            }
            None => {
                self.missing.fetch_add(1, Ordering::SeqCst);
                RelayResponse::NoSuchObject
            }
        };
        if let Ok(mut log) = self.request_log.lock() {
            log.push(RelayRequest {
                requester: requester.to_owned(),
                handle_hex: hex(handle),
                delivered: matches!(response, RelayResponse::Delivered(_)),
            });
        }
        response
    }

    /// Record that the relay looked at what it is holding, for any reason at
    /// all. Nothing in this module calls it, which is the claim; a caller that
    /// adds server-side reading has to move this counter to do it.
    pub fn note_content_inspection(&self, reason: &str) {
        self.content_checks.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut reasons) = self.content_check_reasons.lock() {
            reasons.push(reason.to_owned());
        }
    }

    /// Record that the relay turned a requester away on policy grounds.
    /// Shipping code never calls it either.
    pub fn note_policy_refusal(&self, reason: &str) {
        self.refused.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut reasons) = self.content_check_reasons.lock() {
            reasons.push(format!("refusal: {reason}"));
        }
    }

    pub fn answered(&self) -> u64 {
        self.answered.load(Ordering::SeqCst)
    }

    pub fn delivered(&self) -> u64 {
        self.delivered.load(Ordering::SeqCst)
    }

    pub fn missing(&self) -> u64 {
        self.missing.load(Ordering::SeqCst)
    }

    pub fn refused(&self) -> u64 {
        self.refused.load(Ordering::SeqCst)
    }

    pub fn content_checks(&self) -> u64 {
        self.content_checks.load(Ordering::SeqCst)
    }

    pub fn content_check_reasons(&self) -> Vec<String> {
        self.content_check_reasons
            .lock()
            .map(|reasons| reasons.clone())
            .unwrap_or_default()
    }

    pub fn request_log(&self) -> Vec<RelayRequest> {
        self.request_log
            .lock()
            .map(|log| log.clone())
            .unwrap_or_default()
    }

    pub fn row_count(&self) -> Result<i64, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM envelopes", [], |row| row.get(0))?)
    }
}

/// Why a viewer could not open an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalReason {
    /// No wrap in the envelope opened under this account's key.
    NoKey,
    /// A wrap opened but the sealed content did not, or would not decode.
    Damaged,
    /// The content opened and the record inside is not admissible under the
    /// currently authorized author.
    NotAuthentic,
}

impl RefusalReason {
    pub fn copy(self) -> &'static str {
        match self {
            RefusalReason::NoKey => REFUSED_NO_KEY_COPY,
            RefusalReason::Damaged => REFUSED_DAMAGED_COPY,
            RefusalReason::NotAuthentic => REFUSED_NOT_AUTHENTIC_COPY,
        }
    }
}

/// A refusal, with the two numbers the fetch rule is measured by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchRefusal {
    pub reason: RefusalReason,
    /// The frozen copy shown to the person. Always [`RefusalReason::copy`].
    pub copy: &'static str,
    /// Content keys this account recovered from the envelope.
    pub keys_recovered: usize,
    /// Bytes of content this account can render. Always 0 — a refusal that
    /// leaked a partial body would be a leak.
    pub openable_bytes: usize,
    /// How many wrap slots were tried. Every slot is always tried, so this is
    /// the cohort size and says nothing about the audience.
    pub wraps_tried: usize,
    /// Detail for the log, never for the person.
    pub detail: String,
}

/// What an allowed viewer got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedContent {
    pub record_id: String,
    pub kind: String,
    pub visibility_stable_id: String,
    pub body: Vec<u8>,
    pub media: Vec<u8>,
    pub digest: Vec<u8>,
    pub canonical: Vec<u8>,
    pub signature: [u8; 64],
    pub keys_recovered: usize,
    pub openable_bytes: usize,
    pub wraps_tried: usize,
}

/// The outcome of one account trying to open one envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenOutcome {
    Opened(Box<OpenedContent>),
    Refused(FetchRefusal),
}

impl OpenOutcome {
    pub fn keys_recovered(&self) -> usize {
        match self {
            OpenOutcome::Opened(opened) => opened.keys_recovered,
            OpenOutcome::Refused(refusal) => refusal.keys_recovered,
        }
    }

    pub fn openable_bytes(&self) -> usize {
        match self {
            OpenOutcome::Opened(opened) => opened.openable_bytes,
            OpenOutcome::Refused(refusal) => refusal.openable_bytes,
        }
    }

    pub fn wraps_tried(&self) -> usize {
        match self {
            OpenOutcome::Opened(opened) => opened.wraps_tried,
            OpenOutcome::Refused(refusal) => refusal.wraps_tried,
        }
    }

    pub fn is_opened(&self) -> bool {
        matches!(self, OpenOutcome::Opened(_))
    }
}

/// The viewer's re-admission gate: run the received record back through TASK
/// 4657's production constructors under the currently authorized author.
///
/// Deliberately its own function with one call site, so a build that stops
/// re-admitting is a one-line, visible change rather than a subtle one.
fn admit_for_view(
    fields: &SocialFields,
    signature: &[u8; 64],
    authority: &AuthorityDirectory,
) -> Result<(), String> {
    let admitted = match fields.kind.as_str() {
        KIND_POST => SocialRecord::new_post(fields.clone(), *signature, authority),
        KIND_STORY => SocialRecord::new_story(fields.clone(), *signature, authority),
        other => {
            return Err(format!(
                "field kind, {other} is not distributed over the relay by this module"
            ))
        }
    };
    admitted.map(|_| ()).map_err(|error| error.to_string())
}

/// One real client: an account id, the X25519 fetch key it publishes, and its
/// own TASK 4657 record store.
///
/// **There is no content-key field.** The only content key this type ever holds
/// is a local in [`ViewerAccount::open_fetched`], dropped when the call returns.
/// An account cannot be pre-seeded with a key it was not given, because there is
/// nowhere to seed one.
pub struct ViewerAccount {
    account_id: AccountId,
    fetch_secret: x25519::SecretKey,
    fetch_public: x25519::PublicKey,
    store: SocialRecordStore,
    app_data_dir: PathBuf,
}

impl ViewerAccount {
    /// Open an account against its own app-data directory. The fetch key is
    /// derived from the identity secret, so reopening the same directory with
    /// the same secret is genuinely the same account after a restart.
    pub fn open(
        account_id: &str,
        app_data_dir: &Path,
        identity_secret: &[u8; 32],
    ) -> Result<Self, StoreError> {
        let fetch_secret = derive_fetch_secret(identity_secret);
        let fetch_public = x25519::derive_public(&fetch_secret);
        let store = SocialRecordStore::open(app_data_dir, identity_secret)?;
        Ok(Self {
            account_id: account_id.to_owned(),
            fetch_secret,
            fetch_public,
            store,
            app_data_dir: app_data_dir.to_path_buf(),
        })
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn fetch_public(&self) -> x25519::PublicKey {
        self.fetch_public
    }

    pub fn app_data_dir(&self) -> &Path {
        &self.app_data_dir
    }

    pub fn store(&self) -> &SocialRecordStore {
        &self.store
    }

    /// Try to open one envelope. Every wrap slot is tried, always, so the work
    /// this does is the cohort size and not the audience size.
    pub fn open_fetched(&self, wire: &[u8], authority: &AuthorityDirectory) -> OpenOutcome {
        let envelope = match FetchEnvelope::decode(wire) {
            Ok(envelope) => envelope,
            Err(detail) => {
                return OpenOutcome::Refused(FetchRefusal {
                    reason: RefusalReason::Damaged,
                    copy: RefusalReason::Damaged.copy(),
                    keys_recovered: 0,
                    openable_bytes: 0,
                    wraps_tried: 0,
                    detail,
                })
            }
        };
        let wraps_tried = envelope.slots.len();
        let peer = x25519::PublicKey::from_bytes(envelope.ephemeral_public);
        let shared = match x25519::diffie_hellman(&self.fetch_secret, &peer) {
            Ok(shared) => shared,
            Err(error) => {
                return OpenOutcome::Refused(FetchRefusal {
                    reason: RefusalReason::NoKey,
                    copy: RefusalReason::NoKey.copy(),
                    keys_recovered: 0,
                    openable_bytes: 0,
                    wraps_tried,
                    detail: format!("field ephemeral_public, X25519: {error}"),
                })
            }
        };
        let wrap_key = match hkdf::derive_32(&envelope.handle, shared.as_bytes(), WRAP_INFO) {
            Ok(bytes) => aead::Key::from_bytes(bytes),
            Err(error) => {
                return OpenOutcome::Refused(FetchRefusal {
                    reason: RefusalReason::NoKey,
                    copy: RefusalReason::NoKey.copy(),
                    keys_recovered: 0,
                    openable_bytes: 0,
                    wraps_tried,
                    detail: format!("field wrap_key, HKDF: {error}"),
                })
            }
        };
        let aad = wrap_aad(&envelope.handle);
        let mut content_key: Option<[u8; 32]> = None;
        let mut keys_recovered = 0usize;
        for slot in &envelope.slots {
            let nonce = aead::Nonce::from_bytes(slot.nonce);
            if let Ok(plain) = aead::open(&wrap_key, &nonce, &aad, &slot.ciphertext) {
                if let Ok(bytes) = <[u8; 32]>::try_from(plain.as_slice()) {
                    keys_recovered += 1;
                    if content_key.is_none() {
                        content_key = Some(bytes);
                    }
                }
            }
        }
        let Some(content_key_bytes) = content_key else {
            return OpenOutcome::Refused(FetchRefusal {
                reason: RefusalReason::NoKey,
                copy: RefusalReason::NoKey.copy(),
                keys_recovered: 0,
                openable_bytes: 0,
                wraps_tried,
                detail: format!(
                    "field slots, none of the {wraps_tried} wraps opened under account {}",
                    self.account_id
                ),
            });
        };

        let content_key = aead::Key::from_bytes(content_key_bytes);
        let nonce = aead::Nonce::from_bytes(envelope.content_nonce);
        let plain = match aead::open(
            &content_key,
            &nonce,
            &content_aad(&envelope.handle),
            &envelope.content_ciphertext,
        ) {
            Ok(plain) => plain,
            Err(error) => {
                return OpenOutcome::Refused(FetchRefusal {
                    reason: RefusalReason::Damaged,
                    copy: RefusalReason::Damaged.copy(),
                    keys_recovered,
                    openable_bytes: 0,
                    wraps_tried,
                    detail: format!("field content_ciphertext, AEAD: {error}"),
                })
            }
        };
        let (canonical, signature) = match split_content(&plain) {
            Ok(parts) => parts,
            Err(detail) => {
                return OpenOutcome::Refused(FetchRefusal {
                    reason: RefusalReason::Damaged,
                    copy: RefusalReason::Damaged.copy(),
                    keys_recovered,
                    openable_bytes: 0,
                    wraps_tried,
                    detail,
                })
            }
        };
        let fields = match decode_canonical(&canonical) {
            Ok(fields) => fields,
            Err(error) => {
                return OpenOutcome::Refused(FetchRefusal {
                    reason: RefusalReason::Damaged,
                    copy: RefusalReason::Damaged.copy(),
                    keys_recovered,
                    openable_bytes: 0,
                    wraps_tried,
                    detail: format!("field canonical, {error}"),
                })
            }
        };
        // Re-admit through TASK 4657's gate: a viewer trusts the sealed bytes no
        // more than the disk does. Holding a key is not the same as the record
        // being real, so a key that opens content signed by nobody in
        // particular still shows the person a refusal.
        if let Err(error) = admit_for_view(&fields, &signature, authority) {
            return OpenOutcome::Refused(FetchRefusal {
                reason: RefusalReason::NotAuthentic,
                copy: RefusalReason::NotAuthentic.copy(),
                keys_recovered,
                openable_bytes: 0,
                wraps_tried,
                detail: error,
            });
        }
        OpenOutcome::Opened(Box::new(OpenedContent {
            record_id: fields.record_id.clone(),
            kind: fields.kind.clone(),
            visibility_stable_id: fields.visibility_stable_id.clone(),
            openable_bytes: fields.body.len() + fields.media.len(),
            body: fields.body.clone(),
            media: fields.media.clone(),
            digest: fields.digest.clone(),
            canonical,
            signature,
            keys_recovered,
            wraps_tried,
        }))
    }

    /// Persist an opened record into this account's own TASK 4657 store,
    /// through that store's own gate.
    pub fn store_opened(
        &self,
        opened: &OpenedContent,
        authority: &AuthorityDirectory,
    ) -> Result<(), StoreError> {
        self.store.put_canonical(
            &opened.kind,
            &opened.canonical,
            &opened.signature,
            authority,
        )
    }

    /// Read one record back out of this account's own store.
    pub fn stored(
        &self,
        record_id: &str,
        authority: &AuthorityDirectory,
    ) -> Result<Option<SocialRecord>, StoreError> {
        self.store.get(record_id, authority)
    }

    pub fn stored_rows(&self) -> Result<i64, StoreError> {
        self.store.row_count()
    }

    /// The whole production fetch path a client runs: ask the relay, try to
    /// open what came back, and keep it if it opened.
    pub fn fetch_open_and_store(
        &self,
        relay: &ContentRelay,
        handle: &[u8; 32],
        authority: &AuthorityDirectory,
    ) -> FetchAttempt {
        let response = relay.fetch(&self.account_id, handle);
        let wire = match &response {
            RelayResponse::Delivered(wire) => wire.clone(),
            _ => Vec::new(),
        };
        let answered = matches!(response, RelayResponse::Delivered(_));
        if !answered {
            return FetchAttempt {
                requester: self.account_id.clone(),
                handle_hex: hex(handle),
                answered,
                response,
                wire,
                outcome: OpenOutcome::Refused(FetchRefusal {
                    reason: RefusalReason::Damaged,
                    copy: RefusalReason::Damaged.copy(),
                    keys_recovered: 0,
                    openable_bytes: 0,
                    wraps_tried: 0,
                    detail: "field relay, the relay did not answer with bytes".to_owned(),
                }),
                persisted: false,
                store_error: None,
            };
        }
        let outcome = self.open_fetched(&wire, authority);
        let mut persisted = false;
        let mut store_error = None;
        if let OpenOutcome::Opened(opened) = &outcome {
            match self.store_opened(opened, authority) {
                Ok(()) => persisted = true,
                Err(error) => store_error = Some(error.to_string()),
            }
        }
        FetchAttempt {
            requester: self.account_id.clone(),
            handle_hex: hex(handle),
            answered,
            response,
            wire,
            outcome,
            persisted,
            store_error,
        }
    }
}

/// One account's attempt at one object, end to end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchAttempt {
    pub requester: AccountId,
    pub handle_hex: String,
    /// Did the relay hand over bytes? The fetch rule requires this to be true
    /// for allowed and refused accounts alike.
    pub answered: bool,
    pub response: RelayResponse,
    pub wire: Vec<u8>,
    pub outcome: OpenOutcome,
    pub persisted: bool,
    pub store_error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cohort_hides_the_audience_size() {
        assert_eq!(cohort_slots(1), 8);
        assert_eq!(cohort_slots(2), 8);
        assert_eq!(cohort_slots(8), 8);
        assert_eq!(cohort_slots(9), 32);
    }

    #[test]
    fn an_envelope_round_trips_and_refuses_a_trailing_byte() {
        let envelope = FetchEnvelope {
            handle: [7u8; 32],
            ephemeral_public: [9u8; 32],
            slots: vec![KeySlot {
                nonce: [1u8; aead::NONCE_SIZE],
                ciphertext: vec![2u8; WRAP_CIPHERTEXT_LEN],
            }],
            content_nonce: [3u8; aead::NONCE_SIZE],
            content_ciphertext: vec![4u8; 100],
        };
        let wire = envelope.encode();
        assert_eq!(FetchEnvelope::decode(&wire).unwrap(), envelope);
        let mut padded = wire.clone();
        padded.push(0);
        assert!(FetchEnvelope::decode(&padded)
            .unwrap_err()
            .contains("trailing"));
    }

    #[test]
    fn every_refusal_reason_has_its_own_copy() {
        let copies = [
            RefusalReason::NoKey.copy(),
            RefusalReason::Damaged.copy(),
            RefusalReason::NotAuthentic.copy(),
        ];
        for copy in copies {
            assert!(!copy.is_empty());
            // A refusal must not name the content type: a refused client
            // genuinely does not know whether it was a post or a story.
            assert!(!copy.contains("post"));
            assert!(!copy.contains("story"));
        }
        assert_eq!(copies.len(), copies.iter().collect::<BTreeSet<_>>().len());
    }
}
