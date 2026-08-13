//! TASK 4658 — the check for the chosen fetch rule, enforced without any
//! server-side reading.
//!
//! `harness = false`: a starved production path has to exit **1**, and a
//! libtest binary exits 101 with cargo wrapping it. This binary owns
//! `fn main()` and its own exit code, and it never panics — every fallible
//! step is reported as a named `TASK4658 FAIL` instead.
//!
//! The check is deliberately independent of the module it checks:
//!
//! - TASK 4651's four frozen visibility stable IDs, and the per-option rule
//!   each of them means, are written out here as this file's own literals and
//!   its own set algebra — so a production build that renamed, merged or
//!   dropped an option cannot drag the control along with it;
//! - the record's canonical byte string is re-encoded here by this file's own
//!   encoder and compared against the shipping one, so "byte-identical" is
//!   measured against something the shipping module did not produce;
//! - the wire envelope is parsed here by this file's own parser, and every
//!   wrap slot is unwrapped here with this file's own X25519 / HKDF / AEAD
//!   calls against the published domain constants — so "0 keys" for a refused
//!   account is a second, independent measurement and not the shipping
//!   viewer's own opinion of itself;
//! - the bytes the server holds are read straight out of `social-relay.sqlite`
//!   with this file's own SQLite handle;
//! - the refusal copy every refused person sees is this file's own frozen
//!   literal.

use content_defaults::{
    resolve_story_lifetime_seconds, Defaults, SettingsStore, SignedSendTo, StoryLifetime,
    VisibilityChoice, VisibilityOption,
};
use crypto::{aead, ed25519, hkdf, x25519};
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use store::social::{
    canonical_bytes, AuthorBinding, AuthorityDirectory, SocialFields, SocialRecord, StoryTerms,
};
use store::social_distribution::{
    derive_fetch_secret, distribute_post, distribute_story, fetch_handle, ContentRelay,
    DistributionError, OpenOutcome, RecipientDirectory, RefusalReason, RelayResponse,
    ViewerAccount, WorldSnapshot,
};

// ---------------------------------------------------------------------------
// The independent authority: what TASK 4651 froze, as this file's literals.
// ---------------------------------------------------------------------------

/// TASK 4651's four frozen visibility stable IDs, in the order the note lists
/// them. Not a superset, not a subset.
const OPTIONS_4651: [&str; 4] = ["vis.everyone", "vis.chosen", "vis.except", "vis.onlyme"];

/// Names shown, frozen alongside the IDs by the same note.
const LABELS_4651: [&str; 4] = ["Everyone", "Chosen people", "Everyone except", "Only me"];

/// IDs that are NOT in 4651's list. Settings must not admit any of them.
const OPTIONS_NOT_4651: [&str; 6] = [
    "vis.public",
    "vis.followers",
    "vis.friends",
    "vis.none",
    "vis.everyone_except",
    "",
];

/// The two content types this task distributes.
const KIND_POST: &str = "social.post";
const KIND_STORY: &str = "social.story";
const KINDS: [&str; 2] = [KIND_POST, KIND_STORY];

const STORAGE_RELAY: &str = "store.relay";

// The check's own copies of the wire domain constants. If the shipping module
// changes one, the check's independent unwrap stops agreeing and says so.
const OWN_FETCH_MAGIC: &[u8] = b"osl-social-fetch/v1";
const OWN_WRAP_INFO: &[u8] = b"osl-social-fetch/wrap/v1";
const OWN_WRAP_AAD: &[u8] = b"osl-social-fetch/wrap-aad/v1";
const OWN_CONTENT_AAD: &[u8] = b"osl-social-fetch/content/v1";
const OWN_RELAY_DB: &str = "social-relay.sqlite";

// The check's own copy of the 4657 canonical encoding.
const OWN_CANONICAL_MAGIC: &[u8] = b"osl-social-record/v1";
const OWN_DIGEST_DOMAIN: &[u8] = b"osl-social-record/digest/v1";

/// The frozen refusal copy a person sees when the sender did not give them a
/// key. This file's own literal: a shipping build that softened, widened or
/// personalised the copy stops matching.
const OWN_REFUSED_NO_KEY_COPY: &str = "You can't open this. Whoever shared it chose who gets the \
     key and you weren't given one. No OSL server holds a key either, so there is nothing here to \
     unlock.";

const OWN_REFUSED_DAMAGED_COPY: &str = "You can't open this. The copy that came back did not \
     survive the trip intact, so OSL will not show you a guess at what it said.";

const OWN_REFUSED_NOT_AUTHENTIC_COPY: &str = "You can't open this. It opened, but it is not \
     signed by the person it claims to come from, so OSL will not show it to you.";

const AUTHOR_ID: &str = "osl:author:liam";
const PROFILE_SCOPE: &str = "osl_chats";
const AUTHORITY_VERSION: u32 = 7;
const CREATED_UNIX_MS: i64 = 1_770_000_000_000;

/// Six real clients per matrix cell. Five of them are viewers whose state the
/// decision names; the sixth is the sender's own primary device, which is a
/// real client too and fetches like everybody else.
const ROLE_COUNT: usize = 6;

// ---------------------------------------------------------------------------
// Reporting. Nothing here panics: a starved build must exit 1, not 101.
// ---------------------------------------------------------------------------

struct Report {
    passed: usize,
    failed: usize,
}

impl Report {
    fn new() -> Self {
        Self {
            passed: 0,
            failed: 0,
        }
    }

    fn ok(&mut self, id: &str, name: &str, detail: String) {
        println!("{id} {name} :: {detail}");
        self.passed += 1;
    }

    fn fail(&mut self, id: &str, name: &str, detail: String) {
        println!("TASK4658 FAIL {id} {name} :: {detail}");
        self.failed += 1;
    }

    fn expect(&mut self, condition: bool, id: &str, name: &str, detail: String) -> bool {
        if condition {
            self.ok(id, name, detail);
        } else {
            self.fail(id, name, detail);
        }
        condition
    }
}

// ---------------------------------------------------------------------------
// Small independent helpers.
// ---------------------------------------------------------------------------

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn short(bytes: &[u8]) -> String {
    let head = hex(&bytes[..bytes.len().min(8)]);
    format!("{head}…")
}

fn diff_at(left: &[u8], right: &[u8]) -> Option<usize> {
    if left == right {
        return None;
    }
    let n = left.len().min(right.len());
    for i in 0..n {
        if left[i] != right[i] {
            return Some(i);
        }
    }
    Some(n)
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn sha256_32(input: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(input);
    hasher.finalize().into()
}

fn set_of<I: IntoIterator<Item = S>, S: Into<String>>(items: I) -> BTreeSet<String> {
    items.into_iter().map(Into::into).collect()
}

fn joined(set: &BTreeSet<String>) -> String {
    let items: Vec<&str> = set.iter().map(String::as_str).collect();
    format!("[{}]", items.join(", "))
}

/// The check's own cohort ladder. The shipping one is compared against it.
fn own_cohort_slots(n: usize) -> usize {
    for rung in [8usize, 32, 128, 512, 2048] {
        if n <= rung {
            return rung;
        }
    }
    n.div_ceil(2048) * 2048
}

// ---------------------------------------------------------------------------
// The check's own 4657 canonical encoder and digest.
// ---------------------------------------------------------------------------

fn own_put_str(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u32).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn own_put_bytes(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u32).to_be_bytes());
    out.extend_from_slice(value);
}

fn own_canonical(fields: &SocialFields) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(OWN_CANONICAL_MAGIC);
    own_put_str(&mut out, &fields.kind);
    own_put_str(&mut out, &fields.record_id);
    own_put_str(&mut out, &fields.author.author_id);
    own_put_str(&mut out, &fields.author.profile_scope);
    out.extend_from_slice(&fields.authority_version.to_be_bytes());
    out.extend_from_slice(&fields.created_unix_ms.to_be_bytes());
    out.push(u8::from(fields.deleted));
    own_put_str(&mut out, &fields.visibility_stable_id);
    own_put_str(&mut out, &fields.storage_choice);
    own_put_bytes(&mut out, &fields.body);
    own_put_bytes(&mut out, &fields.media);
    own_put_bytes(&mut out, &fields.digest);
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
            own_put_str(&mut out, &archive.archive_kind);
            own_put_str(&mut out, &archive.payer);
        }
        None => out.push(0),
    }
    out
}

fn own_digest(body: &[u8], media: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(OWN_DIGEST_DOMAIN);
    hasher.update((body.len() as u64).to_be_bytes());
    hasher.update(body);
    hasher.update((media.len() as u64).to_be_bytes());
    hasher.update(media);
    hasher.finalize().to_vec()
}

// ---------------------------------------------------------------------------
// The check's own wire parser, unwrapper and builder.
// ---------------------------------------------------------------------------

struct OwnEnvelope {
    handle: [u8; 32],
    ephemeral_public: [u8; 32],
    slots: Vec<(Vec<u8>, Vec<u8>)>,
    content_nonce: Vec<u8>,
    content_ciphertext: Vec<u8>,
}

fn own_parse(bytes: &[u8]) -> Result<OwnEnvelope, String> {
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
    let magic = take(OWN_FETCH_MAGIC.len(), "magic", &mut at)?;
    if magic != OWN_FETCH_MAGIC {
        return Err(format!(
            "field magic, envelope does not begin with {}",
            String::from_utf8_lossy(OWN_FETCH_MAGIC)
        ));
    }
    let raw = take(4, "handle_len", &mut at)?;
    let handle_len = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
    if handle_len != 32 {
        return Err(format!("field handle_len, want 32, envelope says {handle_len}"));
    }
    let handle: [u8; 32] = take(32, "handle", &mut at)?
        .try_into()
        .map_err(|_| "field handle, not 32 bytes".to_owned())?;
    let ephemeral_public: [u8; 32] = take(32, "ephemeral_public", &mut at)?
        .try_into()
        .map_err(|_| "field ephemeral_public, not 32 bytes".to_owned())?;
    let raw = take(4, "slot_count", &mut at)?;
    let slot_count = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
    if slot_count > 100_000 {
        return Err(format!("field slot_count, implausible {slot_count}"));
    }
    let mut slots = Vec::with_capacity(slot_count);
    for index in 0..slot_count {
        let nonce = take(aead::NONCE_SIZE, &format!("slot[{index}].nonce"), &mut at)?;
        let raw = take(4, &format!("slot[{index}].len"), &mut at)?;
        let n = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
        let ciphertext = take(n, &format!("slot[{index}].ciphertext"), &mut at)?;
        slots.push((nonce, ciphertext));
    }
    let content_nonce = take(aead::NONCE_SIZE, "content_nonce", &mut at)?;
    let raw = take(4, "content_len", &mut at)?;
    let n = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
    let content_ciphertext = take(n, "content_ciphertext", &mut at)?;
    if at != bytes.len() {
        return Err(format!(
            "field trailing, {} trailing byte(s) at offset {at}",
            bytes.len() - at
        ));
    }
    Ok(OwnEnvelope {
        handle,
        ephemeral_public,
        slots,
        content_nonce,
        content_ciphertext,
    })
}

fn own_encode(envelope: &OwnEnvelope) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(OWN_FETCH_MAGIC);
    out.extend_from_slice(&(envelope.handle.len() as u32).to_be_bytes());
    out.extend_from_slice(&envelope.handle);
    out.extend_from_slice(&envelope.ephemeral_public);
    out.extend_from_slice(&(envelope.slots.len() as u32).to_be_bytes());
    for (nonce, ciphertext) in &envelope.slots {
        out.extend_from_slice(nonce);
        out.extend_from_slice(&(ciphertext.len() as u32).to_be_bytes());
        out.extend_from_slice(ciphertext);
    }
    out.extend_from_slice(&envelope.content_nonce);
    out.extend_from_slice(&(envelope.content_ciphertext.len() as u32).to_be_bytes());
    out.extend_from_slice(&envelope.content_ciphertext);
    out
}

fn own_wrap_aad(handle: &[u8; 32]) -> Vec<u8> {
    let mut aad = OWN_WRAP_AAD.to_vec();
    aad.extend_from_slice(handle);
    aad
}

fn own_content_aad(handle: &[u8; 32]) -> Vec<u8> {
    let mut aad = OWN_CONTENT_AAD.to_vec();
    aad.extend_from_slice(handle);
    aad
}

/// What this file recovers from an envelope on its own, for one account.
struct OwnOpen {
    keys_recovered: usize,
    canonical: Option<Vec<u8>>,
    signature: Option<[u8; 64]>,
    detail: String,
}

fn own_open(envelope: &OwnEnvelope, secret: &x25519::SecretKey) -> OwnOpen {
    let peer = x25519::PublicKey::from_bytes(envelope.ephemeral_public);
    let Ok(shared) = x25519::diffie_hellman(secret, &peer) else {
        return OwnOpen {
            keys_recovered: 0,
            canonical: None,
            signature: None,
            detail: "X25519 refused".to_owned(),
        };
    };
    let Ok(wrap_bytes) = hkdf::derive_32(&envelope.handle, shared.as_bytes(), OWN_WRAP_INFO) else {
        return OwnOpen {
            keys_recovered: 0,
            canonical: None,
            signature: None,
            detail: "HKDF refused".to_owned(),
        };
    };
    let wrap_key = aead::Key::from_bytes(wrap_bytes);
    let aad = own_wrap_aad(&envelope.handle);
    let mut keys_recovered = 0usize;
    let mut content_key: Option<[u8; 32]> = None;
    for (nonce, ciphertext) in &envelope.slots {
        let Ok(nonce_bytes) = <[u8; aead::NONCE_SIZE]>::try_from(nonce.as_slice()) else {
            continue;
        };
        let nonce = aead::Nonce::from_bytes(nonce_bytes);
        if let Ok(plain) = aead::open(&wrap_key, &nonce, &aad, ciphertext) {
            if let Ok(bytes) = <[u8; 32]>::try_from(plain.as_slice()) {
                keys_recovered += 1;
                if content_key.is_none() {
                    content_key = Some(bytes);
                }
            }
        }
    }
    let Some(content_key_bytes) = content_key else {
        return OwnOpen {
            keys_recovered,
            canonical: None,
            signature: None,
            detail: format!("no wrap of the {} opened", envelope.slots.len()),
        };
    };
    let Ok(nonce_bytes) = <[u8; aead::NONCE_SIZE]>::try_from(envelope.content_nonce.as_slice())
    else {
        return OwnOpen {
            keys_recovered,
            canonical: None,
            signature: None,
            detail: "content nonce is not nonce-sized".to_owned(),
        };
    };
    let plain = match aead::open(
        &aead::Key::from_bytes(content_key_bytes),
        &aead::Nonce::from_bytes(nonce_bytes),
        &own_content_aad(&envelope.handle),
        &envelope.content_ciphertext,
    ) {
        Ok(plain) => plain,
        Err(error) => {
            return OwnOpen {
                keys_recovered,
                canonical: None,
                signature: None,
                detail: format!("content AEAD refused: {error}"),
            }
        }
    };
    if plain.len() < 68 {
        return OwnOpen {
            keys_recovered,
            canonical: None,
            signature: None,
            detail: format!("content plaintext is {} bytes", plain.len()),
        };
    }
    let n = u32::from_be_bytes([plain[0], plain[1], plain[2], plain[3]]) as usize;
    if plain.len() != 4 + n + 64 {
        return OwnOpen {
            keys_recovered,
            canonical: None,
            signature: None,
            detail: format!("content plaintext {} bytes, want {}", plain.len(), 4 + n + 64),
        };
    }
    let canonical = plain[4..4 + n].to_vec();
    let mut signature = [0u8; 64];
    signature.copy_from_slice(&plain[4 + n..]);
    OwnOpen {
        keys_recovered,
        canonical: Some(canonical),
        signature: Some(signature),
        detail: "opened".to_owned(),
    }
}

/// Build an envelope from scratch with this file's own primitives, so the
/// negative cases are not asking the shipping sender to sabotage itself.
fn own_build(
    handle: [u8; 32],
    recipients: &[x25519::PublicKey],
    canonical: &[u8],
    signature: &[u8; 64],
    slot_target: usize,
) -> Result<OwnEnvelope, String> {
    let mut plain = Vec::new();
    plain.extend_from_slice(&(canonical.len() as u32).to_be_bytes());
    plain.extend_from_slice(canonical);
    plain.extend_from_slice(signature);

    let content_key_bytes: [u8; 32] = sha256_32(&[handle.as_slice(), b"own-content-key"].concat());
    let content_nonce_bytes: [u8; aead::NONCE_SIZE] =
        sha256_32(&[handle.as_slice(), b"own-content-nonce"].concat())[..aead::NONCE_SIZE]
            .try_into()
            .map_err(|_| "nonce slice".to_owned())?;
    let content_ciphertext = aead::seal(
        &aead::Key::from_bytes(content_key_bytes),
        &aead::Nonce::from_bytes(content_nonce_bytes),
        &own_content_aad(&handle),
        &plain,
    )
    .map_err(|e| format!("content seal: {e}"))?;

    let ephemeral_secret = x25519::SecretKey::from_bytes(sha256_32(
        &[handle.as_slice(), b"own-ephemeral"].concat(),
    ));
    let ephemeral_public = x25519::derive_public(&ephemeral_secret);
    let aad = own_wrap_aad(&handle);
    let mut slots: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    for (index, peer) in recipients.iter().enumerate() {
        let shared = x25519::diffie_hellman(&ephemeral_secret, peer)
            .map_err(|e| format!("X25519 for recipient {index}: {e}"))?;
        let wrap_bytes = hkdf::derive_32(&handle, shared.as_bytes(), OWN_WRAP_INFO)
            .map_err(|e| format!("HKDF for recipient {index}: {e}"))?;
        let nonce_bytes: [u8; aead::NONCE_SIZE] = sha256_32(
            &[handle.as_slice(), b"own-wrap-nonce", &[index as u8][..]].concat(),
        )[..aead::NONCE_SIZE]
            .try_into()
            .map_err(|_| "nonce slice".to_owned())?;
        let ciphertext = aead::seal(
            &aead::Key::from_bytes(wrap_bytes),
            &aead::Nonce::from_bytes(nonce_bytes),
            &aad,
            &content_key_bytes,
        )
        .map_err(|e| format!("wrap seal for recipient {index}: {e}"))?;
        slots.push((nonce_bytes.to_vec(), ciphertext));
    }
    let mut filler = 0u8;
    while slots.len() < slot_target {
        let decoy_key = sha256_32(&[handle.as_slice(), b"own-decoy", &[filler][..]].concat());
        let nonce_bytes: [u8; aead::NONCE_SIZE] =
            sha256_32(&[handle.as_slice(), b"own-decoy-nonce", &[filler][..]].concat())
                [..aead::NONCE_SIZE]
                .try_into()
                .map_err(|_| "nonce slice".to_owned())?;
        let ciphertext = aead::seal(
            &aead::Key::from_bytes(decoy_key),
            &aead::Nonce::from_bytes(nonce_bytes),
            &aad,
            &[7u8; 32],
        )
        .map_err(|e| format!("decoy seal: {e}"))?;
        slots.push((nonce_bytes.to_vec(), ciphertext));
        filler = filler.wrapping_add(1);
    }
    Ok(OwnEnvelope {
        handle,
        ephemeral_public: *ephemeral_public.as_bytes(),
        slots,
        content_nonce: content_nonce_bytes.to_vec(),
        content_ciphertext,
    })
}

// ---------------------------------------------------------------------------
// The check's own statement of TASK 4651's per-option rule.
// ---------------------------------------------------------------------------

/// 4651, in this file's own set algebra, keyed on the stable ID string.
///
/// - `vis.everyone` — keys to every friend keyed at post time.
/// - `vis.chosen` — keys to exactly the chosen set.
/// - `vis.except` — keys to friends at post time minus the exclusion set.
/// - `vis.onlyme` — keys to the author's own devices.
fn own_expected_keyed(
    option_id: &str,
    friends_at_time: &BTreeSet<String>,
    author_devices: &BTreeSet<String>,
    chosen: &BTreeSet<String>,
    excluded: &BTreeSet<String>,
) -> Option<BTreeSet<String>> {
    match option_id {
        "vis.everyone" => Some(friends_at_time.clone()),
        "vis.chosen" => Some(chosen.clone()),
        "vis.except" => Some(friends_at_time.difference(excluded).cloned().collect()),
        "vis.onlyme" => Some(author_devices.clone()),
        _ => None,
    }
}

/// The viewer states 4651 names in words, per option, and which role account
/// stands in each. This table is derived from the prose; `own_expected_keyed`
/// is derived from the set algebra. The check requires the two to agree.
fn roles_for(option_id: &str) -> Vec<(&'static str, &'static str, bool)> {
    match option_id {
        "vis.everyone" => vec![
            ("a-friend", "allowed:friend-at-post-time", true),
            ("b-friend", "allowed:friend-at-post-time", true),
            ("c-stranger", "refused:never-a-friend", false),
            ("d-later-friend", "refused:friend-only-after-the-post", false),
            ("s-author", "refused:author-device-keyed-only-by-onlyme", false),
            ("e-device2", "refused:author-device-keyed-only-by-onlyme", false),
        ],
        "vis.chosen" => vec![
            ("a-friend", "allowed:chosen", true),
            ("b-friend", "refused:friend-not-chosen", false),
            ("c-stranger", "refused:not-a-friend-and-not-chosen", false),
            ("d-later-friend", "refused:not-chosen", false),
            ("s-author", "refused:author-device-keyed-only-by-onlyme", false),
            ("e-device2", "refused:author-device-keyed-only-by-onlyme", false),
        ],
        "vis.except" => vec![
            ("b-friend", "allowed:friend-not-excluded", true),
            ("a-friend", "refused:excluded-friend", false),
            ("c-stranger", "refused:not-a-friend", false),
            ("d-later-friend", "refused:not-a-friend-at-post-time", false),
            ("s-author", "refused:author-device-keyed-only-by-onlyme", false),
            ("e-device2", "refused:author-device-keyed-only-by-onlyme", false),
        ],
        "vis.onlyme" => vec![
            ("s-author", "allowed:authors-own-device", true),
            ("e-device2", "allowed:authors-own-second-device", true),
            ("a-friend", "refused:friend", false),
            ("b-friend", "refused:friend", false),
            ("c-stranger", "refused:not-a-friend", false),
            ("d-later-friend", "refused:not-a-friend", false),
        ],
        _ => Vec::new(),
    }
}

/// The states the decision names for an option. Every one has to be exercised
/// by an account in that option's cell, for both content types.
fn required_states(option_id: &str) -> Vec<&'static str> {
    match option_id {
        "vis.everyone" => vec![
            "allowed:friend-at-post-time",
            "refused:never-a-friend",
            "refused:friend-only-after-the-post",
        ],
        "vis.chosen" => vec![
            "allowed:chosen",
            "refused:friend-not-chosen",
            "refused:not-a-friend-and-not-chosen",
        ],
        "vis.except" => vec![
            "allowed:friend-not-excluded",
            "refused:excluded-friend",
            "refused:not-a-friend",
        ],
        "vis.onlyme" => vec![
            "allowed:authors-own-device",
            "allowed:authors-own-second-device",
            "refused:friend",
            "refused:not-a-friend",
        ],
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Plans and outcomes.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct RolePlan {
    role: &'static str,
    account_id: String,
    dir: PathBuf,
    secret: [u8; 32],
    state: &'static str,
    allowed: bool,
}

#[derive(Clone)]
struct CellPlan {
    idx: usize,
    option_id: &'static str,
    kind: &'static str,
    record_id: String,
    body: Vec<u8>,
    media: Vec<u8>,
    dir: PathBuf,
    settings_path: PathBuf,
    roles: Vec<RolePlan>,
    universe: BTreeSet<String>,
    friends: BTreeSet<String>,
    devices: BTreeSet<String>,
    chosen: BTreeSet<String>,
    excluded: BTreeSet<String>,
    expected_keyed: BTreeSet<String>,
}

impl CellPlan {
    fn label(&self) -> String {
        format!("cell {} {} {}", self.idx, self.option_id, self.kind)
    }

}

struct CellDistributed {
    plan: CellPlan,
    canonical: Vec<u8>,
    signature: [u8; 64],
    handle: [u8; 32],
    keyed: BTreeSet<String>,
    option_stable_id: String,
    real_wraps: usize,
    decoy_wraps: usize,
    slot_count: usize,
    wire: Vec<u8>,
    fetch_publics: BTreeMap<String, x25519::PublicKey>,
}

fn cell_body(idx: usize, option_id: &str, kind: &str) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(format!("cell {idx} {option_id} {kind}\r\n").as_bytes());
    body.extend_from_slice("\tleading tab, trailing space \t\n".as_bytes());
    // NFC vs NFD: byte-different, visually identical — catches a normaliser.
    body.extend_from_slice("e\u{0301} beside \u{00e9}".as_bytes());
    // Emoji ZWJ sequence and an RTL override.
    body.extend_from_slice(" \u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467} \u{202E}rtl\u{202C}".as_bytes());
    // Embedded NUL and high bytes.
    body.extend_from_slice(&[0u8, 1, 2, 0x7f, 0x80, 0xfe, 0xff]);
    body.push(idx as u8);
    body
}

fn cell_media(idx: usize) -> Vec<u8> {
    // Every one of the 256 byte values, then a per-cell pseudorandom tail.
    let mut media: Vec<u8> = (0u8..=255).collect();
    for i in 0..1024usize {
        media.push(((i * 31 + idx * 7 + 13) % 251) as u8);
    }
    media
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    let mut report = Report::new();
    match tempfile::TempDir::new() {
        Ok(root) => {
            run(&mut report, root.path());
        }
        Err(error) => report.fail("0.0", "workspace", format!("no temp dir: {error}")),
    }
    if report.passed == 0 {
        report.fail(
            "0.1",
            "the-check-ran",
            "no check ran at all, which cannot be a pass".to_owned(),
        );
    }
    println!(
        "TASK4658 SUMMARY checks_passed={} checks_failed={}",
        report.passed, report.failed
    );
    if report.failed == 0 {
        println!("TASK4658 RESULT ok");
        std::process::exit(0);
    }
    println!("TASK4658 RESULT failed");
    std::process::exit(1);
}

fn run(report: &mut Report, root: &Path) {
    let author_secret = ed25519::SecretKey::from_bytes(sha256_32(b"osl:author:liam/4658"));
    let author_public = ed25519::derive_public(&author_secret);
    let mut authority = AuthorityDirectory::new();
    authority.grant(AUTHOR_ID, PROFILE_SCOPE, author_public, AUTHORITY_VERSION);

    section_1_settings(report, root);

    let plans = build_plans(report, root);
    if plans.len() != OPTIONS_4651.len() * KINDS.len() {
        report.fail(
            "2.0",
            "matrix-is-complete",
            format!(
                "planned {} cells, the matrix is {} options x {} content types = {}",
                plans.len(),
                OPTIONS_4651.len(),
                KINDS.len(),
                OPTIONS_4651.len() * KINDS.len()
            ),
        );
        return;
    }

    let relay_dir = root.join("relay");
    let distributed = section_2_distribute(report, &plans, &relay_dir, &author_secret, &authority);
    let fetched = section_3_to_6(report, &distributed, &relay_dir, &authority);
    section_7_no_seeding(report, &distributed, root, &authority);
    section_8_envelope_negatives(report, &distributed, &authority, root);
    section_9_sender_refusals(report, root, &author_secret, &authority);
    section_10_census(report);

    let opened_total: usize = fetched.iter().map(|cell| cell.opened.len()).sum();
    let expected_total: usize = distributed
        .iter()
        .map(|cell| cell.plan.expected_keyed.len())
        .sum();
    report.expect(
        opened_total == expected_total
            && fetched.len() == distributed.len()
            && expected_total > 0,
        "11.1",
        "the-whole-matrix-opened-for-exactly-the-people-4651-names",
        format!(
            "cells fetched={} of {} distributed; accounts that opened={opened_total}; \
             accounts 4651 allows={expected_total}",
            fetched.len(),
            distributed.len()
        ),
    );
}

// ---------------------------------------------------------------------------
// 1 — the frozen option set is non-empty and agrees exactly with Settings.
// ---------------------------------------------------------------------------

fn section_1_settings(report: &mut Report, root: &Path) {
    let shipped: Vec<&str> = VisibilityOption::ALL
        .iter()
        .map(|option| option.stable_id())
        .collect();
    let shipped_labels: Vec<&str> = VisibilityOption::ALL
        .iter()
        .map(|option| option.label())
        .collect();

    report.expect(
        !shipped.is_empty() && !OPTIONS_4651.is_empty(),
        "1.1",
        "option-set-is-non-empty",
        format!(
            "settings options={} 4651 options={} (both required > 0)",
            shipped.len(),
            OPTIONS_4651.len()
        ),
    );

    let shipped_set: BTreeSet<&str> = shipped.iter().copied().collect();
    let frozen_set: BTreeSet<&str> = OPTIONS_4651.iter().copied().collect();
    let missing: Vec<&str> = frozen_set.difference(&shipped_set).copied().collect();
    let extra: Vec<&str> = shipped_set.difference(&frozen_set).copied().collect();
    report.expect(
        shipped == OPTIONS_4651.to_vec()
            && shipped.len() == OPTIONS_4651.len()
            && shipped_set.len() == OPTIONS_4651.len()
            && missing.is_empty()
            && extra.is_empty(),
        "1.2",
        "settings-agrees-exactly-with-4651",
        format!(
            "settings={shipped:?} 4651={:?} missing={missing:?} extra={extra:?} \
             distinct_settings_ids={}",
            OPTIONS_4651,
            shipped_set.len()
        ),
    );

    report.expect(
        shipped_labels == LABELS_4651.to_vec(),
        "1.3",
        "settings-names-agree-with-4651",
        format!("settings={shipped_labels:?} 4651={LABELS_4651:?}"),
    );

    let mut resolvable = 0usize;
    for (index, id) in OPTIONS_4651.iter().enumerate() {
        match VisibilityOption::from_stable_id(id) {
            Some(option) if option.stable_id() == *id && option == VisibilityOption::ALL[index] => {
                resolvable += 1;
            }
            _ => {}
        }
    }
    report.expect(
        resolvable == OPTIONS_4651.len(),
        "1.4",
        "settings-resolves-every-4651-id",
        format!(
            "resolved {resolvable} of {} (required {})",
            OPTIONS_4651.len(),
            OPTIONS_4651.len()
        ),
    );

    let admitted_outsiders: Vec<&str> = OPTIONS_NOT_4651
        .iter()
        .copied()
        .filter(|id| VisibilityOption::from_stable_id(id).is_some())
        .collect();
    report.expect(
        admitted_outsiders.is_empty(),
        "1.5",
        "settings-admits-nothing-outside-4651",
        format!(
            "tried {:?}, admitted {admitted_outsiders:?} (required none)",
            OPTIONS_NOT_4651
        ),
    );

    // Every option has to survive a Settings write + restart, for a post and
    // for a story alike, or Settings cannot really name it.
    let settings_dir = root.join("settings-roundtrip");
    if let Err(error) = std::fs::create_dir_all(&settings_dir) {
        report.fail(
            "1.6",
            "settings-round-trip",
            format!("could not create {settings_dir:?}: {error}"),
        );
        return;
    }
    let mut round_tripped = 0usize;
    for id in OPTIONS_4651 {
        let Some(option) = VisibilityOption::from_stable_id(id) else {
            report.fail(
                "1.6",
                "settings-round-trip",
                format!("settings could not name {id} at all"),
            );
            continue;
        };
        let path = settings_dir.join(format!("{id}.json"));
        let choice = VisibilityChoice {
            option,
            set: set_of(["someone"]),
        };
        let written = Defaults {
            post_visibility: choice.clone(),
            story_visibility: choice.clone(),
            story_lifetime: StoryLifetime::TwentyFourHours,
        };
        let mut store = match SettingsStore::open(&path) {
            Ok(store) => store,
            Err(error) => {
                report.fail("1.6", "settings-round-trip", format!("{id}: open: {error}"));
                continue;
            }
        };
        if let Err(error) = store.set_defaults(written.clone()) {
            report.fail("1.6", "settings-round-trip", format!("{id}: write: {error}"));
            continue;
        }
        drop(store);
        let reopened = match SettingsStore::open(&path) {
            Ok(store) => store,
            Err(error) => {
                report.fail(
                    "1.6",
                    "settings-round-trip",
                    format!("{id}: reopen: {error}"),
                );
                continue;
            }
        };
        let read = reopened.defaults().clone();
        if read == written
            && read.post_visibility.option.stable_id() == id
            && read.story_visibility.option.stable_id() == id
        {
            round_tripped += 1;
        } else {
            report.fail(
                "1.6",
                "settings-round-trip",
                format!(
                    "{id}: wrote post={} story={}, read back post={} story={}",
                    id,
                    id,
                    read.post_visibility.option.stable_id(),
                    read.story_visibility.option.stable_id()
                ),
            );
        }
    }
    report.expect(
        round_tripped == OPTIONS_4651.len(),
        "1.6",
        "every-option-survives-a-settings-restart",
        format!(
            "post and story defaults round-tripped for {round_tripped} of {} options (required {})",
            OPTIONS_4651.len(),
            OPTIONS_4651.len()
        ),
    );
}

// ---------------------------------------------------------------------------
// The plan: 4 options x 2 content types, six real clients each.
// ---------------------------------------------------------------------------

fn build_plans(report: &mut Report, root: &Path) -> Vec<CellPlan> {
    let mut plans = Vec::new();
    let mut idx = 0usize;
    for kind in KINDS {
        for option_id in OPTIONS_4651 {
            let dir = root.join(format!("cell-{idx}"));
            let role_table = roles_for(option_id);
            if role_table.len() != ROLE_COUNT {
                report.fail(
                    "2.0",
                    "cell-has-its-cast",
                    format!("{option_id}: role table has {} entries", role_table.len()),
                );
                idx += 1;
                continue;
            }
            let roles: Vec<RolePlan> = role_table
                .iter()
                .map(|(role, state, allowed)| {
                    let account_id = format!("cell{idx}.{role}");
                    RolePlan {
                        role,
                        secret: sha256_32(format!("identity/{account_id}").as_bytes()),
                        dir: dir.join(format!("account-{role}")),
                        account_id,
                        state,
                        allowed: *allowed,
                    }
                })
                .collect();
            let id_of = |role: &str| -> String {
                roles
                    .iter()
                    .find(|r| r.role == role)
                    .map(|r| r.account_id.clone())
                    .unwrap_or_default()
            };
            let universe = set_of([
                id_of("a-friend"),
                id_of("b-friend"),
                id_of("c-stranger"),
                id_of("d-later-friend"),
            ]);
            let friends = set_of([id_of("a-friend"), id_of("b-friend")]);
            let devices = set_of([id_of("s-author"), id_of("e-device2")]);
            let chosen = set_of([id_of("a-friend")]);
            let excluded = set_of([id_of("a-friend")]);
            let Some(expected_keyed) =
                own_expected_keyed(option_id, &friends, &devices, &chosen, &excluded)
            else {
                report.fail(
                    "2.0",
                    "4651-rule-covers-the-option",
                    format!("{option_id} has no rule in 4651"),
                );
                idx += 1;
                continue;
            };
            plans.push(CellPlan {
                idx,
                option_id,
                kind,
                record_id: format!("rec.4658.{idx}.{option_id}.{kind}"),
                body: cell_body(idx, option_id, kind),
                media: cell_media(idx),
                settings_path: dir.join("settings.json"),
                dir,
                roles,
                universe,
                friends,
                devices,
                chosen,
                excluded,
                expected_keyed,
            });
            idx += 1;
        }
    }
    plans
}

// ---------------------------------------------------------------------------
// 2 — distribution: real Settings, real sender, real relay.
// ---------------------------------------------------------------------------

fn section_2_distribute(
    report: &mut Report,
    plans: &[CellPlan],
    relay_dir: &Path,
    author_secret: &ed25519::SecretKey,
    authority: &AuthorityDirectory,
) -> Vec<CellDistributed> {
    let relay = match ContentRelay::open(relay_dir) {
        Ok(relay) => relay,
        Err(error) => {
            report.fail(
                "2.0",
                "relay-opens",
                format!("could not open the relay at {relay_dir:?}: {error}"),
            );
            return Vec::new();
        }
    };
    report.ok(
        "2.0",
        "relay-opens",
        format!("path={}", relay.path().display()),
    );

    let mut out: Vec<CellDistributed> = Vec::new();
    for plan in plans {
        let label = plan.label();

        // --- the role table and the set algebra have to agree ---
        let table_allowed: BTreeSet<String> = plan
            .roles
            .iter()
            .filter(|r| r.allowed)
            .map(|r| r.account_id.clone())
            .collect();
        report.expect(
            table_allowed == plan.expected_keyed,
            "2.1",
            "named-states-agree-with-the-4651-rule",
            format!(
                "{label} :: states say allowed={} rule says keyed={}",
                joined(&table_allowed),
                joined(&plan.expected_keyed)
            ),
        );

        // --- every state the decision names is exercised ---
        let covered: BTreeSet<&str> = plan.roles.iter().map(|r| r.state).collect();
        let required = required_states(plan.option_id);
        let missing: Vec<&str> = required
            .iter()
            .copied()
            .filter(|state| !covered.contains(state))
            .collect();
        report.expect(
            missing.is_empty() && !required.is_empty(),
            "2.2",
            "every-named-viewer-state-is-exercised",
            format!(
                "{label} :: required={required:?} covered={:?} missing={missing:?}",
                covered.iter().collect::<Vec<_>>()
            ),
        );

        // --- Settings, written and then restarted before it is used ---
        if let Err(error) = std::fs::create_dir_all(&plan.dir) {
            report.fail("2.3", "cell-settings", format!("{label}: {error}"));
            continue;
        }
        let Some(option) = VisibilityOption::from_stable_id(plan.option_id) else {
            report.fail(
                "2.3",
                "cell-settings",
                format!("{label}: settings cannot name {}", plan.option_id),
            );
            continue;
        };
        let set = match plan.option_id {
            "vis.chosen" => plan.chosen.clone(),
            "vis.except" => plan.excluded.clone(),
            _ => BTreeSet::new(),
        };
        let choice = VisibilityChoice { option, set };
        let defaults = Defaults {
            post_visibility: choice.clone(),
            story_visibility: choice.clone(),
            story_lifetime: StoryLifetime::TwentyFourHours,
        };
        let mut settings = match SettingsStore::open(&plan.settings_path) {
            Ok(store) => store,
            Err(error) => {
                report.fail("2.3", "cell-settings", format!("{label}: open: {error}"));
                continue;
            }
        };
        if let Err(error) = settings.set_defaults(defaults) {
            report.fail("2.3", "cell-settings", format!("{label}: write: {error}"));
            continue;
        }
        drop(settings);
        let settings = match SettingsStore::open(&plan.settings_path) {
            Ok(store) => store,
            Err(error) => {
                report.fail("2.3", "cell-settings", format!("{label}: reopen: {error}"));
                continue;
            }
        };
        let live = settings.defaults();
        let live_id = match plan.kind {
            KIND_POST => live.post_visibility.option.stable_id(),
            _ => live.story_visibility.option.stable_id(),
        };
        if !report.expect(
            live_id == plan.option_id,
            "2.3",
            "cell-settings-survived-a-restart",
            format!(
                "{label} :: settings file {:?} reopened, {} default = {live_id} (wanted {})",
                plan.settings_path.file_name().unwrap_or_default(),
                plan.kind,
                plan.option_id
            ),
        ) {
            continue;
        }

        // --- six real clients, each with its own app-data directory ---
        let mut accounts: Vec<ViewerAccount> = Vec::new();
        let mut broke = false;
        for role in &plan.roles {
            match ViewerAccount::open(&role.account_id, &role.dir, &role.secret) {
                Ok(account) => accounts.push(account),
                Err(error) => {
                    report.fail(
                        "2.4",
                        "cell-clients-open",
                        format!("{label}: {}: {error}", role.account_id),
                    );
                    broke = true;
                    break;
                }
            }
        }
        if broke {
            continue;
        }
        let distinct_ids: BTreeSet<&str> = accounts.iter().map(|a| a.account_id()).collect();
        let distinct_dirs: BTreeSet<&Path> = accounts.iter().map(|a| a.app_data_dir()).collect();
        let distinct_keys: BTreeSet<[u8; 32]> = accounts
            .iter()
            .map(|a| *a.fetch_public().as_bytes())
            .collect();
        report.expect(
            distinct_ids.len() == ROLE_COUNT
                && distinct_dirs.len() == ROLE_COUNT
                && distinct_keys.len() == ROLE_COUNT
                && accounts.len() == ROLE_COUNT,
            "2.4",
            "cell-runs-distinct-real-clients",
            format!(
                "{label} :: accounts={} distinct_ids={} distinct_app_data_dirs={} \
                 distinct_fetch_keys={} (required {ROLE_COUNT} and at least 5)",
                accounts.len(),
                distinct_ids.len(),
                distinct_dirs.len(),
                distinct_keys.len()
            ),
        );

        // --- nothing is seeded into any private store ---
        let mut seeded = Vec::new();
        for account in &accounts {
            match account.stored_rows() {
                Ok(0) => {}
                Ok(rows) => seeded.push(format!("{}={rows}", account.account_id())),
                Err(error) => seeded.push(format!("{}=unreadable({error})", account.account_id())),
            }
        }
        report.expect(
            seeded.is_empty(),
            "2.5",
            "no-private-store-is-pre-seeded",
            format!(
                "{label} :: rows in all {ROLE_COUNT} private stores before any fetch = 0 \
                 {seeded:?}"
            ),
        );

        // --- the sender's directory of published fetch keys ---
        let mut directory = RecipientDirectory::new();
        let mut fetch_publics: BTreeMap<String, x25519::PublicKey> = BTreeMap::new();
        for account in &accounts {
            directory.publish(account.account_id(), account.fetch_public());
            fetch_publics.insert(account.account_id().to_owned(), account.fetch_public());
        }

        // --- the signed record ---
        let digest = own_digest(&plan.body, &plan.media);
        let story = if plan.kind == KIND_STORY {
            let lifetime = resolve_story_lifetime_seconds(live);
            Some(StoryTerms {
                lifetime_seconds: lifetime,
                expires_unix_ms: CREATED_UNIX_MS + (lifetime as i64) * 1000,
            })
        } else {
            None
        };
        let fields = SocialFields {
            kind: plan.kind.to_owned(),
            record_id: plan.record_id.clone(),
            author: AuthorBinding {
                author_id: AUTHOR_ID.to_owned(),
                profile_scope: PROFILE_SCOPE.to_owned(),
            },
            authority_version: AUTHORITY_VERSION,
            created_unix_ms: CREATED_UNIX_MS,
            deleted: false,
            visibility_stable_id: plan.option_id.to_owned(),
            storage_choice: STORAGE_RELAY.to_owned(),
            body: plan.body.clone(),
            media: plan.media.clone(),
            digest: digest.clone(),
            story,
            archive: None,
        };
        let mine = own_canonical(&fields);
        let theirs = canonical_bytes(&fields);
        if !report.expect(
            diff_at(&mine, &theirs).is_none(),
            "2.6",
            "canonical-encoding-agrees-with-an-independent-encoder",
            match diff_at(&mine, &theirs) {
                None => format!("{label} :: {} bytes, identical", mine.len()),
                Some(offset) => format!(
                    "{label} :: check {} bytes vs shipping {} bytes, first difference at byte {offset}",
                    mine.len(),
                    theirs.len()
                ),
            },
        ) {
            continue;
        }
        let signature: [u8; 64] = *ed25519::sign(author_secret, &mine).as_bytes();
        let record = match plan.kind {
            KIND_POST => SocialRecord::new_post(fields.clone(), signature, authority),
            _ => SocialRecord::new_story(fields.clone(), signature, authority),
        };
        let record = match record {
            Ok(record) => record,
            Err(error) => {
                report.fail(
                    "2.7",
                    "sender-record-admits",
                    format!("{label}: {error}"),
                );
                continue;
            }
        };

        // --- production distribution ---
        let world = WorldSnapshot {
            universe: plan.universe.clone(),
            friends_at_time: plan.friends.clone(),
            author_devices: plan.devices.clone(),
        };
        let receipt = match plan.kind {
            KIND_POST => distribute_post(&record, live, &world, &directory, &relay),
            _ => distribute_story(&record, live, &world, &directory, &relay, None),
        };
        let receipt = match receipt {
            Ok(receipt) => receipt,
            Err(error) => {
                report.fail(
                    "2.7",
                    "production-distribution-runs",
                    format!("{label}: {error}"),
                );
                continue;
            }
        };
        report.ok(
            "2.7",
            "production-distribution-runs",
            format!(
                "{label} :: record_id={} handle={} option={} keyed={} real_wraps={} \
                 decoy_wraps={} slots={} wire={}B",
                plan.record_id,
                short(&receipt.handle),
                receipt.option_stable_id,
                joined(&receipt.keyed),
                receipt.real_wraps,
                receipt.decoy_wraps,
                receipt.slot_count,
                receipt.wire_len
            ),
        );

        report.expect(
            receipt.option_stable_id == plan.option_id
                && receipt.keyed == plan.expected_keyed
                && receipt.real_wraps == plan.expected_keyed.len()
                && !plan.expected_keyed.is_empty(),
            "2.8",
            "sender-keyed-exactly-the-4651-audience",
            format!(
                "{label} :: settings option={} sender keyed={} 4651 rule keys={} \
                 real_wraps={} (required {} and > 0)",
                receipt.option_stable_id,
                joined(&receipt.keyed),
                joined(&plan.expected_keyed),
                receipt.real_wraps,
                plan.expected_keyed.len()
            ),
        );

        // --- what the server actually holds, read by the check's own handle ---
        let wire = match read_relay_blob(relay_dir, &receipt.handle) {
            Ok(Some(wire)) => wire,
            Ok(None) => {
                report.fail(
                    "2.9",
                    "server-holds-the-envelope",
                    format!("{label}: no row under handle {}", short(&receipt.handle)),
                );
                continue;
            }
            Err(error) => {
                report.fail("2.9", "server-holds-the-envelope", format!("{label}: {error}"));
                continue;
            }
        };
        report.expect(
            wire.len() == receipt.wire_len,
            "2.9",
            "server-holds-the-envelope",
            format!(
                "{label} :: {} bytes read straight out of {OWN_RELAY_DB} (sender published {})",
                wire.len(),
                receipt.wire_len
            ),
        );

        out.push(CellDistributed {
            plan: plan.clone(),
            canonical: mine,
            signature,
            handle: receipt.handle,
            keyed: receipt.keyed.clone(),
            option_stable_id: receipt.option_stable_id.clone(),
            real_wraps: receipt.real_wraps,
            decoy_wraps: receipt.decoy_wraps,
            slot_count: receipt.slot_count,
            wire,
            fetch_publics,
        });
    }

    let expected_cells = OPTIONS_4651.len() * KINDS.len();
    report.expect(
        out.len() == expected_cells,
        "2.10",
        "every-option-x-content-type-cell-distributed",
        format!(
            "cells distributed = {} (required {expected_cells} = {} options x {} content types)",
            out.len(),
            OPTIONS_4651.len(),
            KINDS.len()
        ),
    );

    let options_seen: BTreeSet<&str> = out.iter().map(|c| c.plan.option_id).collect();
    let kinds_seen: BTreeSet<&str> = out.iter().map(|c| c.plan.kind).collect();
    report.expect(
        options_seen.len() == OPTIONS_4651.len() && kinds_seen.len() == KINDS.len(),
        "2.11",
        "no-option-stood-in-for-a-sibling",
        format!(
            "distinct options distributed={:?} distinct content types={:?} (required {} and {})",
            options_seen,
            kinds_seen,
            OPTIONS_4651.len(),
            KINDS.len()
        ),
    );

    let record_ids: BTreeSet<&str> = out.iter().map(|c| c.plan.record_id.as_str()).collect();
    let handles: BTreeSet<[u8; 32]> = out.iter().map(|c| c.handle).collect();
    let canonicals: BTreeSet<&[u8]> = out.iter().map(|c| c.canonical.as_slice()).collect();
    let signatures: BTreeSet<[u8; 64]> = out.iter().map(|c| c.signature).collect();
    report.expect(
        record_ids.len() == out.len()
            && handles.len() == out.len()
            && canonicals.len() == out.len()
            && signatures.len() == out.len(),
        "2.12",
        "every-cell-has-its-own-unique-signed-record",
        format!(
            "cells={} distinct record ids={} distinct handles={} distinct canonical bytes={} \
             distinct signatures={}",
            out.len(),
            record_ids.len(),
            handles.len(),
            canonicals.len(),
            signatures.len()
        ),
    );

    for kind in KINDS {
        let keyed_sets: BTreeSet<String> = out
            .iter()
            .filter(|c| c.plan.kind == kind)
            .map(|c| joined(&c.keyed))
            .collect();
        report.expect(
            keyed_sets.len() == OPTIONS_4651.len(),
            "2.13",
            "each-option-keys-a-different-audience",
            format!(
                "{kind} :: distinct keyed sets across the four options = {} {:?} (required {})",
                keyed_sets.len(),
                keyed_sets,
                OPTIONS_4651.len()
            ),
        );
    }

    // The relay must not be able to see the audience size, or anything else.
    for cell in &out {
        let expected_slots = own_cohort_slots(cell.real_wraps);
        report.expect(
            cell.slot_count == expected_slots
                && cell.slot_count == cell.real_wraps + cell.decoy_wraps,
            "2.14",
            "audience-size-is-padded-off-the-wire",
            format!(
                "{} :: real_wraps={} decoy_wraps={} slots={} (the check's own cohort ladder says \
                 {expected_slots})",
                cell.plan.label(),
                cell.real_wraps,
                cell.decoy_wraps,
                cell.slot_count
            ),
        );
    }
    let slot_counts: BTreeSet<usize> = out.iter().map(|c| c.slot_count).collect();
    let keyed_sizes: BTreeSet<usize> = out.iter().map(|c| c.keyed.len()).collect();
    report.expect(
        slot_counts.len() == 1 && keyed_sizes.len() > 1,
        "2.15",
        "one-shape-for-every-audience-size",
        format!(
            "distinct slot counts on the wire={slot_counts:?} distinct audience sizes={keyed_sizes:?} \
             (required 1 shape and more than 1 size)"
        ),
    );

    for cell in &out {
        let mut leaked: Vec<String> = Vec::new();
        for role in &cell.plan.roles {
            if contains_bytes(&cell.wire, role.account_id.as_bytes()) {
                leaked.push(format!("account:{}", role.account_id));
            }
        }
        for needle in [
            cell.plan.record_id.as_str(),
            cell.plan.option_id,
            cell.plan.kind,
            AUTHOR_ID,
            PROFILE_SCOPE,
            STORAGE_RELAY,
        ] {
            if contains_bytes(&cell.wire, needle.as_bytes()) {
                leaked.push(format!("text:{needle}"));
            }
        }
        if contains_bytes(&cell.wire, &cell.plan.body) {
            leaked.push("body".to_owned());
        }
        if contains_bytes(&cell.wire, &cell.plan.media) {
            leaked.push("media".to_owned());
        }
        if contains_bytes(&cell.wire, &cell.canonical) {
            leaked.push("canonical".to_owned());
        }
        report.expect(
            leaked.is_empty(),
            "2.16",
            "the-envelope-names-nobody-and-says-nothing",
            format!(
                "{} :: {} wire bytes searched for 6 account ids, the record id, the visibility id, \
                 the content type, the author, the storage choice, the body, the media and the \
                 canonical record; found {leaked:?}",
                cell.plan.label(),
                cell.wire.len()
            ),
        );
    }

    out
}

fn read_relay_blob(relay_dir: &Path, handle: &[u8; 32]) -> Result<Option<Vec<u8>>, String> {
    let path = relay_dir.join(OWN_RELAY_DB);
    let conn = Connection::open(&path).map_err(|e| format!("open {path:?}: {e}"))?;
    let mut statement = conn
        .prepare("SELECT wire FROM envelopes WHERE handle = ?1")
        .map_err(|e| format!("prepare: {e}"))?;
    let mut rows = statement
        .query(params![&handle[..]])
        .map_err(|e| format!("query: {e}"))?;
    match rows.next().map_err(|e| format!("step: {e}"))? {
        Some(row) => {
            let wire: Vec<u8> = row.get(0).map_err(|e| format!("column: {e}"))?;
            Ok(Some(wire))
        }
        None => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// 3-6 — restart, fetch, open, refuse, and read back after a second restart.
// ---------------------------------------------------------------------------

struct CellFetched {
    opened: BTreeSet<String>,
}

fn section_3_to_6(
    report: &mut Report,
    cells: &[CellDistributed],
    relay_dir: &Path,
    authority: &AuthorityDirectory,
) -> Vec<CellFetched> {
    // ---- RESTART: every client handle and the server handle are dropped and
    // reopened from the same directories before a single fetch happens. ----
    let relay = match ContentRelay::open(relay_dir) {
        Ok(relay) => relay,
        Err(error) => {
            report.fail(
                "3.1",
                "server-reopens-after-restart",
                format!("{relay_dir:?}: {error}"),
            );
            return Vec::new();
        }
    };
    let rows = relay.row_count().unwrap_or(-1);
    report.expect(
        rows == cells.len() as i64 && rows > 0,
        "3.1",
        "server-reopens-after-restart",
        format!(
            "reopened {OWN_RELAY_DB}, envelopes on the shelf = {rows} (required {})",
            cells.len()
        ),
    );

    let mut fetched: Vec<CellFetched> = Vec::new();
    let mut allowed_opens = 0usize;
    let mut allowed_expected = 0usize;
    let mut refused_zero = 0usize;
    let mut refused_expected = 0usize;
    let mut identical_bytes = 0usize;
    let mut exact_copy = 0usize;
    let mut independent_zero = 0usize;
    let mut delivered_identical = 0usize;
    let mut total_fetches = 0usize;
    let mut readback_allowed = 0usize;
    let mut readback_refused_empty = 0usize;

    for cell in cells {
        let label = cell.plan.label();
        let mut accounts: Vec<(RolePlan, ViewerAccount)> = Vec::new();
        let mut broke = false;
        for role in &cell.plan.roles {
            match ViewerAccount::open(&role.account_id, &role.dir, &role.secret) {
                Ok(account) => accounts.push((role.clone(), account)),
                Err(error) => {
                    report.fail(
                        "3.2",
                        "clients-reopen-after-restart",
                        format!("{label}: {}: {error}", role.account_id),
                    );
                    broke = true;
                    break;
                }
            }
        }
        if broke {
            continue;
        }
        let stable_keys = accounts.iter().all(|(role, account)| {
            cell.fetch_publics
                .get(&role.account_id)
                .map(|before| before.as_bytes() == account.fetch_public().as_bytes())
                .unwrap_or(false)
        });
        report.expect(
            accounts.len() == ROLE_COUNT && stable_keys,
            "3.2",
            "clients-reopen-after-restart",
            format!(
                "{label} :: {} clients reopened from their own app-data dirs, fetch keys \
                 unchanged={stable_keys} (required {ROLE_COUNT} and true)",
                accounts.len()
            ),
        );

        let answered_before = relay.answered();
        let mut opened_set: BTreeSet<String> = BTreeSet::new();
        let mut attempts = Vec::new();
        for (role, account) in &accounts {
            let attempt = account.fetch_open_and_store(&relay, &cell.handle, authority);
            total_fetches += 1;
            attempts.push((role.clone(), attempt));
        }
        let answered_delta = relay.answered() - answered_before;

        // ---- 3.3 the server answered every fetch, from allowed and refused
        // clients alike, with the identical bytes ----
        let all_answered = attempts.iter().all(|(_, a)| {
            a.answered && matches!(a.response, RelayResponse::Delivered(_)) && !a.wire.is_empty()
        });
        let same_bytes = attempts
            .iter()
            .filter(|(_, a)| diff_at(&a.wire, &cell.wire).is_none())
            .count();
        delivered_identical += same_bytes;
        report.expect(
            all_answered && answered_delta == ROLE_COUNT as u64 && same_bytes == ROLE_COUNT,
            "3.3",
            "the-server-answered-every-fetch-with-the-same-bytes",
            format!(
                "{label} :: fetches={ROLE_COUNT} answered={answered_delta} delivered_bytes_identical\
                ={same_bytes} of {ROLE_COUNT} ({} bytes each, compared against the blob read \
                 straight out of {OWN_RELAY_DB})",
                cell.wire.len()
            ),
        );

        // ---- 4/5 per account ----
        let Ok(own_envelope) = own_parse(&cell.wire) else {
            report.fail(
                "4.0",
                "the-checks-own-parser-reads-the-envelope",
                format!("{label}: the check could not parse the published wire"),
            );
            continue;
        };

        for (role, attempt) in &attempts {
            let own = own_open(&own_envelope, &derive_fetch_secret(&role.secret));
            // Recorded for every role, allowed or refused, so 4.2 catches a
            // superset as loudly as it catches a subset.
            if attempt.outcome.is_opened() {
                opened_set.insert(role.account_id.clone());
            }
            if role.allowed {
                allowed_expected += 1;
                match &attempt.outcome {
                    OpenOutcome::Opened(opened) => {
                        allowed_opens += 1;
                        let body_diff = diff_at(&opened.body, &cell.plan.body);
                        let media_diff = diff_at(&opened.media, &cell.plan.media);
                        let canonical_diff = diff_at(&opened.canonical, &cell.canonical);
                        let digest_diff = diff_at(&opened.digest, &own_digest(&cell.plan.body, &cell.plan.media));
                        let signature_ok = opened.signature == cell.signature;
                        let verifies = ed25519::verify(
                            &ed25519::derive_public(&ed25519::SecretKey::from_bytes(sha256_32(
                                b"osl:author:liam/4658",
                            ))),
                            &opened.canonical,
                            &ed25519::Signature::from_bytes(opened.signature),
                        )
                        .unwrap_or(false);
                        let own_agrees = own.keys_recovered == 1
                            && own.canonical.as_deref() == Some(cell.canonical.as_slice())
                            && own.signature == Some(cell.signature);
                        let vis_ok = opened.visibility_stable_id == cell.plan.option_id;
                        let all_identical = body_diff.is_none()
                            && media_diff.is_none()
                            && canonical_diff.is_none()
                            && digest_diff.is_none()
                            && signature_ok
                            && verifies
                            && own_agrees
                            && vis_ok
                            && attempt.persisted;
                        if all_identical {
                            identical_bytes += 1;
                        }
                        report.expect(
                            all_identical,
                            "4.1",
                            "allowed-account-opens-byte-identical-content",
                            format!(
                                "{label} :: {} [{}] body={}B media={}B canonical={}B \
                                 body_diff={body_diff:?} media_diff={media_diff:?} \
                                 canonical_diff={canonical_diff:?} digest_diff={digest_diff:?} \
                                 signature_identical={signature_ok} signature_verifies={verifies} \
                                 independent_unwrap_keys={} independent_unwrap_agrees={own_agrees} \
                                 visibility={} persisted_to_its_own_store={}",
                                role.account_id,
                                role.state,
                                opened.body.len(),
                                opened.media.len(),
                                opened.canonical.len(),
                                own.keys_recovered,
                                opened.visibility_stable_id,
                                attempt.persisted
                            ),
                        );
                    }
                    OpenOutcome::Refused(refusal) => {
                        report.fail(
                            "4.1",
                            "allowed-account-opens-byte-identical-content",
                            format!(
                                "{label} :: {} [{}] was refused: reason={:?} detail={} \
                                 (independent unwrap recovered {} keys)",
                                role.account_id,
                                role.state,
                                refusal.reason,
                                refusal.detail,
                                own.keys_recovered
                            ),
                        );
                    }
                }
            } else {
                refused_expected += 1;
                match &attempt.outcome {
                    OpenOutcome::Refused(refusal) => {
                        let zero = refusal.keys_recovered == 0
                            && refusal.openable_bytes == 0
                            && refusal.reason == RefusalReason::NoKey
                            && refusal.wraps_tried == cell.slot_count;
                        let copy_ok = refusal.copy == OWN_REFUSED_NO_KEY_COPY;
                        let own_zero = own.keys_recovered == 0 && own.canonical.is_none();
                        if zero {
                            refused_zero += 1;
                        }
                        if copy_ok {
                            exact_copy += 1;
                        }
                        if own_zero {
                            independent_zero += 1;
                        }
                        report.expect(
                            zero && copy_ok && own_zero,
                            "5.1",
                            "refused-account-opens-zero-with-the-exact-copy",
                            format!(
                                "{label} :: {} [{}] keys_recovered={} openable_bytes={} \
                                 wraps_tried={} reason={:?} copy_is_the_frozen_one={copy_ok} \
                                 independent_unwrap_keys={} independent_unwrap_opened={} \
                                 independent_detail={:?} \
                                 was_delivered_the_full_{}_byte_envelope={}",
                                role.account_id,
                                role.state,
                                refusal.keys_recovered,
                                refusal.openable_bytes,
                                refusal.wraps_tried,
                                refusal.reason,
                                own.keys_recovered,
                                own.canonical.is_some(),
                                own.detail,
                                attempt.wire.len(),
                                attempt.answered
                            ),
                        );
                    }
                    OpenOutcome::Opened(opened) => {
                        report.fail(
                            "5.1",
                            "refused-account-opens-zero-with-the-exact-copy",
                            format!(
                                "{label} :: {} [{}] OPENED {} bytes it was never keyed for \
                                 (record_id={} keys_recovered={})",
                                role.account_id,
                                role.state,
                                opened.openable_bytes,
                                opened.record_id,
                                opened.keys_recovered
                            ),
                        );
                    }
                }
            }
        }

        // ---- 4.2 exactly the allowed accounts, no more and no fewer ----
        report.expect(
            opened_set == cell.plan.expected_keyed && !opened_set.is_empty(),
            "4.2",
            "exactly-the-allowed-accounts-opened",
            format!(
                "{label} :: option on the receipt={} opened={} 4651 rule allows={} \
                 (not a superset, not a subset)",
                cell.option_stable_id,
                joined(&opened_set),
                joined(&cell.plan.expected_keyed)
            ),
        );

        // ---- RESTART 2: drop every client, reopen, read back from the
        // client's own private store. ----
        drop(accounts);
        for role in &cell.plan.roles {
            let account = match ViewerAccount::open(&role.account_id, &role.dir, &role.secret) {
                Ok(account) => account,
                Err(error) => {
                    report.fail(
                        "6.1",
                        "stored-record-survives-a-second-restart",
                        format!("{label}: {}: {error}", role.account_id),
                    );
                    continue;
                }
            };
            let stored = account.stored(&cell.plan.record_id, authority);
            let rows = account.stored_rows().unwrap_or(-1);
            if role.allowed {
                match stored {
                    Ok(Some(record)) => {
                        let canonical = record.canonical_bytes();
                        let identical = diff_at(&canonical, &cell.canonical).is_none()
                            && record.signature() == &cell.signature
                            && record.fields().body == cell.plan.body
                            && record.fields().media == cell.plan.media
                            && record.fields().visibility_stable_id == cell.plan.option_id
                            && rows == 1;
                        if identical {
                            readback_allowed += 1;
                        }
                        report.expect(
                            identical,
                            "6.1",
                            "stored-record-survives-a-second-restart",
                            format!(
                                "{label} :: {} [{}] rows={rows} canonical={}B diff={:?} \
                                 signature_identical={} body={}B media={}B visibility={}",
                                role.account_id,
                                role.state,
                                canonical.len(),
                                diff_at(&canonical, &cell.canonical),
                                record.signature() == &cell.signature,
                                record.fields().body.len(),
                                record.fields().media.len(),
                                record.fields().visibility_stable_id
                            ),
                        );
                    }
                    Ok(None) => report.fail(
                        "6.1",
                        "stored-record-survives-a-second-restart",
                        format!(
                            "{label} :: {} opened the record but its own store has nothing under \
                             {} after restart (rows={rows})",
                            role.account_id, cell.plan.record_id
                        ),
                    ),
                    Err(error) => report.fail(
                        "6.1",
                        "stored-record-survives-a-second-restart",
                        format!("{label} :: {}: {error}", role.account_id),
                    ),
                }
            } else {
                let empty = matches!(stored, Ok(None)) && rows == 0;
                if empty {
                    readback_refused_empty += 1;
                }
                report.expect(
                    empty,
                    "6.2",
                    "refused-account-stores-nothing",
                    format!(
                        "{label} :: {} [{}] rows={rows} get({})={} (required 0 rows and None)",
                        role.account_id,
                        role.state,
                        cell.plan.record_id,
                        match &stored {
                            Ok(Some(_)) => "a record".to_owned(),
                            Ok(None) => "None".to_owned(),
                            Err(error) => format!("error: {error}"),
                        }
                    ),
                );
            }
        }

        fetched.push(CellFetched { opened: opened_set });
    }

    // ---- 3.4 the server never read a byte and never refused anybody ----
    report.expect(
        relay.content_checks() == 0 && relay.content_check_reasons().is_empty(),
        "3.4",
        "the-server-made-zero-content-checks",
        format!(
            "content_checks={} reasons={:?} (required 0 and [])",
            relay.content_checks(),
            relay.content_check_reasons()
        ),
    );
    report.expect(
        relay.refused() == 0 && relay.missing() == 0,
        "3.5",
        "the-server-refused-nobody-and-answered-everybody",
        format!(
            "answered={} delivered={} missing={} policy_refusals={} (required missing=0, \
             refusals=0, answered=delivered)",
            relay.answered(),
            relay.delivered(),
            relay.missing(),
            relay.refused()
        ),
    );
    let log = relay.request_log();
    let requesters: BTreeSet<&str> = log.iter().map(|r| r.requester.as_str()).collect();
    report.expect(
        log.len() == total_fetches
            && log.iter().all(|r| r.delivered)
            && requesters.len() == total_fetches,
        "3.6",
        "every-distinct-client-was-answered",
        format!(
            "requests logged={} distinct requesters={} all delivered={} (required {total_fetches})",
            log.len(),
            requesters.len(),
            log.iter().all(|r| r.delivered)
        ),
    );

    let expected_fetches = cells.len() * ROLE_COUNT;
    report.expect(
        total_fetches == expected_fetches && delivered_identical == expected_fetches,
        "3.7",
        "every-matrix-cell-fetched-after-restart",
        format!(
            "fetches after restart={total_fetches} byte-identical deliveries={delivered_identical} \
             (required {expected_fetches} = {} cells x {ROLE_COUNT} clients)",
            cells.len()
        ),
    );

    report.expect(
        allowed_opens == allowed_expected && allowed_expected > 0,
        "4.3",
        "every-allowed-account-opened",
        format!("opened {allowed_opens} of {allowed_expected} allowed accounts (required all, > 0)"),
    );
    report.expect(
        identical_bytes == allowed_expected,
        "4.4",
        "every-allowed-open-was-byte-identical",
        format!("byte-identical {identical_bytes} of {allowed_expected}"),
    );
    report.expect(
        refused_zero == refused_expected && refused_expected > 0,
        "5.2",
        "every-refused-account-opened-zero",
        format!(
            "0 keys and 0 openable bytes for {refused_zero} of {refused_expected} refused \
             accounts (required all, > 0)"
        ),
    );
    report.expect(
        exact_copy == refused_expected,
        "5.3",
        "every-refused-account-got-the-exact-copy",
        format!(
            "exact frozen copy for {exact_copy} of {refused_expected}; the copy is {:?}",
            OWN_REFUSED_NO_KEY_COPY
        ),
    );
    report.expect(
        independent_zero == refused_expected,
        "5.4",
        "an-independent-unwrap-also-recovers-zero",
        format!(
            "the check's own X25519/HKDF/AEAD unwrap over every slot recovered 0 keys for \
             {independent_zero} of {refused_expected} refused accounts"
        ),
    );
    report.expect(
        readback_allowed == allowed_expected && readback_refused_empty == refused_expected,
        "6.3",
        "read-back-after-the-second-restart",
        format!(
            "allowed accounts holding the record={readback_allowed} of {allowed_expected}; \
             refused accounts holding nothing={readback_refused_empty} of {refused_expected}"
        ),
    );

    fetched
}

// ---------------------------------------------------------------------------
// 7 — seeded keys, seeded accounts and a private store cannot pass.
// ---------------------------------------------------------------------------

fn section_7_no_seeding(
    report: &mut Report,
    cells: &[CellDistributed],
    root: &Path,
    authority: &AuthorityDirectory,
) {
    let mut cold_ok = 0usize;
    let mut impostor_ok = 0usize;
    for cell in cells {
        let label = cell.plan.label();
        let Some(allowed) = cell
            .plan
            .roles
            .iter()
            .find(|r| r.allowed)
            .cloned()
        else {
            report.fail(
                "7.1",
                "cell-has-an-allowed-account",
                format!("{label}: none"),
            );
            continue;
        };

        // A brand-new empty app-data directory, the same identity secret: the
        // only inputs are the secret and the envelope, so nothing cached
        // locally can be what made the open work.
        let cold_dir = root.join(format!("cold-{}", cell.plan.idx));
        match ViewerAccount::open(&allowed.account_id, &cold_dir, &allowed.secret) {
            Ok(cold) => {
                let rows_before = cold.stored_rows().unwrap_or(-1);
                let outcome = cold.open_fetched(&cell.wire, authority);
                match &outcome {
                    OpenOutcome::Opened(opened) => {
                        let identical = diff_at(&opened.canonical, &cell.canonical).is_none()
                            && opened.signature == cell.signature
                            && opened.body == cell.plan.body
                            && opened.media == cell.plan.media;
                        if identical && rows_before == 0 {
                            cold_ok += 1;
                        }
                        report.expect(
                            identical && rows_before == 0,
                            "7.1",
                            "a-cold-client-opens-with-only-its-key-and-the-envelope",
                            format!(
                                "{label} :: {} reopened in an empty directory (rows before={rows_before}), \
                                 opened {} body bytes byte-identical={identical}",
                                allowed.account_id,
                                opened.body.len()
                            ),
                        );
                    }
                    OpenOutcome::Refused(refusal) => report.fail(
                        "7.1",
                        "a-cold-client-opens-with-only-its-key-and-the-envelope",
                        format!(
                            "{label} :: {} refused from a cold directory: {:?} {}",
                            allowed.account_id, refusal.reason, refusal.detail
                        ),
                    ),
                }
            }
            Err(error) => report.fail(
                "7.1",
                "a-cold-client-opens-with-only-its-key-and-the-envelope",
                format!("{label}: {error}"),
            ),
        }

        // The same private store directory, a different identity secret:
        // holding somebody's app-data folder is not holding their key.
        let impostor_secret = sha256_32(format!("impostor/{}", allowed.account_id).as_bytes());
        match ViewerAccount::open("impostor", &allowed.dir, &impostor_secret) {
            Ok(impostor) => {
                let outcome = impostor.open_fetched(&cell.wire, authority);
                let zero = outcome.keys_recovered() == 0 && outcome.openable_bytes() == 0;
                let store_says = match impostor.stored(&cell.plan.record_id, authority) {
                    Ok(Some(_)) => "a record".to_owned(),
                    Ok(None) => "None".to_owned(),
                    Err(error) => format!("refused: {error}"),
                };
                let store_blind = store_says != "a record";
                if zero && store_blind {
                    impostor_ok += 1;
                }
                report.expect(
                    zero && store_blind,
                    "7.2",
                    "the-private-store-alone-opens-nothing",
                    format!(
                        "{label} :: an impostor pointed at {}'s own app-data directory with a \
                         different identity secret recovered keys={} openable_bytes={} and its \
                         read of that private store returned {store_says}",
                        allowed.account_id,
                        outcome.keys_recovered(),
                        outcome.openable_bytes()
                    ),
                );
            }
            Err(error) => report.fail(
                "7.2",
                "the-private-store-alone-opens-nothing",
                format!("{label}: {error}"),
            ),
        }
    }
    report.expect(
        cold_ok == cells.len() && impostor_ok == cells.len() && !cells.is_empty(),
        "7.3",
        "no-cell-passes-on-seeded-keys-accounts-or-a-private-store",
        format!(
            "cold-client opens={cold_ok} of {} impostor shut out={impostor_ok} of {}",
            cells.len(),
            cells.len()
        ),
    );
    report.ok(
        "7.4",
        "there-is-nowhere-to-seed-a-content-key",
        "ViewerAccount carries an account id, an X25519 fetch secret/public and its own \
         SocialRecordStore; it has no content-key field, so the only content key it ever holds is \
         the local unwrapped inside one open_fetched call"
            .to_owned(),
    );
}

// ---------------------------------------------------------------------------
// 8 — the signature and fidelity comparison, and the wrap boundary.
// ---------------------------------------------------------------------------

fn section_8_envelope_negatives(
    report: &mut Report,
    cells: &[CellDistributed],
    authority: &AuthorityDirectory,
    root: &Path,
) {
    let Some(cell) = cells.first() else {
        report.fail("8.0", "envelope-negatives", "no distributed cell".to_owned());
        return;
    };
    let Some(allowed) = cell.plan.roles.iter().find(|r| r.allowed).cloned() else {
        report.fail("8.0", "envelope-negatives", "no allowed account".to_owned());
        return;
    };
    let account = match ViewerAccount::open(
        &allowed.account_id,
        &root.join("negatives-account"),
        &allowed.secret,
    ) {
        Ok(account) => account,
        Err(error) => {
            report.fail("8.0", "envelope-negatives", format!("{error}"));
            return;
        }
    };
    let peer = x25519::derive_public(&derive_fetch_secret(&allowed.secret));
    let handle = fetch_handle(&cell.plan.record_id);

    // 8.1 the control: this file's own envelope, built from scratch, opens.
    let control = match own_build(
        handle,
        &[peer],
        &cell.canonical,
        &cell.signature,
        own_cohort_slots(1),
    ) {
        Ok(envelope) => envelope,
        Err(error) => {
            report.fail("8.1", "independent-control-envelope", error);
            return;
        }
    };
    let control_wire = own_encode(&control);
    let control_outcome = account.open_fetched(&control_wire, authority);
    report.expect(
        control_outcome.is_opened()
            && control_outcome.openable_bytes() == cell.plan.body.len() + cell.plan.media.len(),
        "8.1",
        "an-independently-built-envelope-opens",
        format!(
            "{} bytes built by the check's own encoder opened={} openable_bytes={} \
             (so the refusals below are not a client that refuses everything)",
            control_wire.len(),
            control_outcome.is_opened(),
            control_outcome.openable_bytes()
        ),
    );

    // 8.2 a forged signature inside a wrap the account really holds.
    let mut forged_signature = cell.signature;
    forged_signature[0] ^= 0x01;
    match own_build(
        handle,
        &[peer],
        &cell.canonical,
        &forged_signature,
        own_cohort_slots(1),
    ) {
        Ok(envelope) => {
            let outcome = account.open_fetched(&own_encode(&envelope), authority);
            let refused = match &outcome {
                OpenOutcome::Refused(refusal) => {
                    refusal.reason == RefusalReason::NotAuthentic
                        && refusal.openable_bytes == 0
                        && refusal.keys_recovered == 1
                        && refusal.copy == OWN_REFUSED_NOT_AUTHENTIC_COPY
                }
                OpenOutcome::Opened(_) => false,
            };
            report.expect(
                refused,
                "8.2",
                "a-forged-signature-inside-a-real-wrap-is-refused",
                format!(
                    "signature byte 0 flipped: keys_recovered={} openable_bytes={} outcome={}",
                    outcome.keys_recovered(),
                    outcome.openable_bytes(),
                    describe(&outcome)
                ),
            );
        }
        Err(error) => report.fail(
            "8.2",
            "a-forged-signature-inside-a-real-wrap-is-refused",
            error,
        ),
    }

    // 8.3 one altered content byte.
    let mut damaged = control_wire.clone();
    if let Some(byte) = damaged.last_mut() {
        *byte ^= 0x01;
    }
    let outcome = account.open_fetched(&damaged, authority);
    let refused = match &outcome {
        OpenOutcome::Refused(refusal) => {
            refusal.reason == RefusalReason::Damaged
                && refusal.openable_bytes == 0
                && refusal.copy == OWN_REFUSED_DAMAGED_COPY
        }
        OpenOutcome::Opened(_) => false,
    };
    report.expect(
        refused,
        "8.3",
        "one-altered-content-byte-is-refused",
        format!(
            "last content byte flipped at offset {}: openable_bytes={} outcome={}",
            damaged.len() - 1,
            outcome.openable_bytes(),
            describe(&outcome)
        ),
    );

    // 8.4 a truncated envelope.
    let truncated = control_wire[..control_wire.len() - 9].to_vec();
    let outcome = account.open_fetched(&truncated, authority);
    let refused = matches!(&outcome, OpenOutcome::Refused(r)
        if r.reason == RefusalReason::Damaged && r.openable_bytes == 0);
    let detail = match &outcome {
        OpenOutcome::Refused(refusal) => refusal.detail.clone(),
        OpenOutcome::Opened(_) => "opened".to_owned(),
    };
    report.expect(
        refused && detail.contains("offset"),
        "8.4",
        "a-truncated-envelope-is-refused-by-field-and-offset",
        format!(
            "{} of {} bytes offered: openable_bytes={} detail={detail}",
            truncated.len(),
            control_wire.len(),
            outcome.openable_bytes()
        ),
    );

    // 8.5 a wrap lifted off one object cannot be replayed onto another.
    let other_handle = fetch_handle("rec.4658.some-other-object");
    match (
        own_build(handle, &[peer], &cell.canonical, &cell.signature, 8),
        own_build(
            other_handle,
            &[peer],
            &cell.canonical,
            &cell.signature,
            8,
        ),
    ) {
        (Ok(first), Ok(mut second)) => {
            // Splice the wrap the account really holds from object one into
            // object two, and strip object two's own wrap for the account.
            second.slots[0] = first.slots[0].clone();
            for index in 1..second.slots.len() {
                let mut ciphertext = second.slots[index].1.clone();
                if let Some(byte) = ciphertext.first_mut() {
                    *byte ^= 0xff;
                }
                second.slots[index].1 = ciphertext;
            }
            let outcome = account.open_fetched(&own_encode(&second), authority);
            let zero = outcome.keys_recovered() == 0 && outcome.openable_bytes() == 0;
            report.expect(
                zero,
                "8.5",
                "a-wrap-cannot-be-lifted-onto-another-object",
                format!(
                    "the account's own wrap from handle {} spliced into handle {}: \
                     keys_recovered={} openable_bytes={} outcome={}",
                    short(&handle),
                    short(&other_handle),
                    outcome.keys_recovered(),
                    outcome.openable_bytes(),
                    describe(&outcome)
                ),
            );
        }
        _ => report.fail(
            "8.5",
            "a-wrap-cannot-be-lifted-onto-another-object",
            "could not build the two envelopes".to_owned(),
        ),
    }

    // 8.6 an account nobody wrapped for gets nothing from a full envelope.
    let outsider_secret = sha256_32(b"outsider/4658");
    match ViewerAccount::open("outsider", &root.join("outsider"), &outsider_secret) {
        Ok(outsider) => {
            let outcome = outsider.open_fetched(&control_wire, authority);
            let zero = outcome.keys_recovered() == 0
                && outcome.openable_bytes() == 0
                && outcome.wraps_tried() == control.slots.len();
            report.expect(
                zero,
                "8.6",
                "an-unwrapped-account-gets-nothing-from-a-whole-envelope",
                format!(
                    "wraps_tried={} keys_recovered={} openable_bytes={} outcome={}",
                    outcome.wraps_tried(),
                    outcome.keys_recovered(),
                    outcome.openable_bytes(),
                    describe(&outcome)
                ),
            );
        }
        Err(error) => report.fail(
            "8.6",
            "an-unwrapped-account-gets-nothing-from-a-whole-envelope",
            format!("{error}"),
        ),
    }
}

fn describe(outcome: &OpenOutcome) -> String {
    match outcome {
        OpenOutcome::Opened(opened) => format!("OPENED {}", opened.record_id),
        OpenOutcome::Refused(refusal) => format!("refused {:?} ({})", refusal.reason, refusal.detail),
    }
}

// ---------------------------------------------------------------------------
// 9 — the sender refuses before anything reaches the server.
// ---------------------------------------------------------------------------

fn section_9_sender_refusals(
    report: &mut Report,
    root: &Path,
    author_secret: &ed25519::SecretKey,
    authority: &AuthorityDirectory,
) {
    let dir = root.join("sender-refusals");
    let relay = match ContentRelay::open(&dir) {
        Ok(relay) => relay,
        Err(error) => {
            report.fail("9.0", "sender-refusals", format!("{error}"));
            return;
        }
    };
    let people = ["r.alice", "r.bob", "r.carol", "r.dave"];
    let mut directory = RecipientDirectory::new();
    for person in people {
        let secret = derive_fetch_secret(&sha256_32(format!("identity/{person}").as_bytes()));
        directory.publish(person, x25519::derive_public(&secret));
    }
    let world = WorldSnapshot {
        universe: set_of(["r.alice", "r.bob", "r.carol", "r.dave"]),
        friends_at_time: set_of(["r.alice", "r.bob"]),
        author_devices: set_of(["r.device"]),
    };

    let make = |option_id: &str, kind: &str, record_id: &str| -> Option<SocialRecord> {
        let body = b"sender refusal body".to_vec();
        let media = b"m".to_vec();
        let story = if kind == KIND_STORY {
            Some(StoryTerms {
                lifetime_seconds: 86_400,
                expires_unix_ms: CREATED_UNIX_MS + 86_400_000,
            })
        } else {
            None
        };
        let fields = SocialFields {
            kind: kind.to_owned(),
            record_id: record_id.to_owned(),
            author: AuthorBinding {
                author_id: AUTHOR_ID.to_owned(),
                profile_scope: PROFILE_SCOPE.to_owned(),
            },
            authority_version: AUTHORITY_VERSION,
            created_unix_ms: CREATED_UNIX_MS,
            deleted: false,
            visibility_stable_id: option_id.to_owned(),
            storage_choice: STORAGE_RELAY.to_owned(),
            body,
            media,
            digest: own_digest(b"sender refusal body", b"m"),
            story,
            archive: None,
        };
        let signature: [u8; 64] = *ed25519::sign(author_secret, &own_canonical(&fields)).as_bytes();
        match kind {
            KIND_POST => SocialRecord::new_post(fields, signature, authority).ok(),
            _ => SocialRecord::new_story(fields, signature, authority).ok(),
        }
    };

    let choice = |option_id: &str, set: BTreeSet<String>| -> Option<Defaults> {
        let option = VisibilityOption::from_stable_id(option_id)?;
        let choice = VisibilityChoice { option, set };
        Some(Defaults {
            post_visibility: choice.clone(),
            story_visibility: choice,
            story_lifetime: StoryLifetime::TwentyFourHours,
        })
    };

    let rows_before = relay.row_count().unwrap_or(-1);

    // 9.1 an option that keys nobody is refused: 4651 requires every option to
    // produce at least one allowed person.
    if let (Some(record), Some(defaults)) = (
        make("vis.chosen", KIND_POST, "rec.4658.empty"),
        choice("vis.chosen", BTreeSet::new()),
    ) {
        let result = distribute_post(&record, &defaults, &world, &directory, &relay);
        report.expect(
            matches!(&result, Err(DistributionError::EmptyAudience(id)) if id == "vis.chosen"),
            "9.1",
            "an-audience-of-nobody-is-refused",
            format!("vis.chosen with nobody chosen -> {}", outcome_text(&result)),
        );
    } else {
        report.fail("9.1", "an-audience-of-nobody-is-refused", "setup".to_owned());
    }

    // 9.2 the sender's signed intent and the Settings option have to be the
    // same statement: one option cannot stand in for a sibling.
    if let (Some(record), Some(defaults)) = (
        make("vis.onlyme", KIND_POST, "rec.4658.disagree"),
        choice("vis.everyone", BTreeSet::new()),
    ) {
        let result = distribute_post(&record, &defaults, &world, &directory, &relay);
        report.expect(
            matches!(&result, Err(DistributionError::SettingsDisagree { record_says, settings_say })
                if record_says == "vis.onlyme" && settings_say == "vis.everyone"),
            "9.2",
            "settings-and-the-signed-record-must-agree",
            format!(
                "record signed vis.onlyme, Settings say vis.everyone -> {}",
                outcome_text(&result)
            ),
        );
    } else {
        report.fail(
            "9.2",
            "settings-and-the-signed-record-must-agree",
            "setup".to_owned(),
        );
    }

    // 9.3 somebody in the audience with no published fetch key is refused
    // loudly, not silently dropped from the audience.
    if let (Some(record), Some(defaults)) = (
        make("vis.chosen", KIND_POST, "rec.4658.nokey"),
        choice("vis.chosen", set_of(["r.alice", "r.nobody"])),
    ) {
        let result = distribute_post(&record, &defaults, &world, &directory, &relay);
        report.expect(
            matches!(&result, Err(DistributionError::NoFetchKey(who)) if who == "r.nobody"),
            "9.3",
            "an-unaddressable-audience-member-is-not-silently-dropped",
            format!(
                "audience {{r.alice, r.nobody}} where r.nobody published no key -> {}",
                outcome_text(&result)
            ),
        );
    } else {
        report.fail(
            "9.3",
            "an-unaddressable-audience-member-is-not-silently-dropped",
            "setup".to_owned(),
        );
    }

    // 9.4 the post path will not distribute a story and the reverse.
    if let (Some(story), Some(post), Some(defaults)) = (
        make("vis.everyone", KIND_STORY, "rec.4658.kind.story"),
        make("vis.everyone", KIND_POST, "rec.4658.kind.post"),
        choice("vis.everyone", BTreeSet::new()),
    ) {
        let as_post = distribute_post(&story, &defaults, &world, &directory, &relay);
        let as_story = distribute_story(&post, &defaults, &world, &directory, &relay, None);
        report.expect(
            matches!(&as_post, Err(DistributionError::WrongKind { expected, found })
                if expected == KIND_POST && found == KIND_STORY)
                && matches!(&as_story, Err(DistributionError::WrongKind { expected, found })
                    if expected == KIND_STORY && found == KIND_POST),
            "9.4",
            "each-content-type-has-its-own-sender-path",
            format!(
                "story through the post path -> {}; post through the story path -> {}",
                outcome_text(&as_post),
                outcome_text(&as_story)
            ),
        );
    } else {
        report.fail(
            "9.4",
            "each-content-type-has-its-own-sender-path",
            "setup".to_owned(),
        );
    }

    // 9.5 a per-story SEND TO override that does not verify is refused, and
    // does NOT quietly fall back to the Settings default.
    if let (Some(record), Some(defaults)) = (
        make("vis.chosen", KIND_STORY, "rec.4658.override.bad"),
        choice("vis.everyone", BTreeSet::new()),
    ) {
        let option = VisibilityOption::from_stable_id("vis.chosen");
        match option {
            Some(option) => {
                let mut send_to = SignedSendTo::sign(
                    VisibilityChoice {
                        option,
                        set: set_of(["r.alice"]),
                    },
                    author_secret,
                );
                send_to.choice.set = set_of(["r.alice", "r.bob", "r.carol", "r.dave"]);
                let result =
                    distribute_story(&record, &defaults, &world, &directory, &relay, Some(&send_to));
                report.expect(
                    matches!(&result, Err(DistributionError::StoryOverride(_))),
                    "9.5",
                    "a-tampered-story-override-is-refused-not-defaulted",
                    format!(
                        "override widened from {{r.alice}} to 4 people after signing -> {}",
                        outcome_text(&result)
                    ),
                );
            }
            None => report.fail(
                "9.5",
                "a-tampered-story-override-is-refused-not-defaulted",
                "settings cannot name vis.chosen".to_owned(),
            ),
        }
    } else {
        report.fail(
            "9.5",
            "a-tampered-story-override-is-refused-not-defaulted",
            "setup".to_owned(),
        );
    }

    // 9.6 a valid per-story override moves the audience away from the default
    // — proof the story path is not simply replaying the post path.
    if let (Some(record), Some(defaults), Some(option)) = (
        make("vis.chosen", KIND_STORY, "rec.4658.override.good"),
        choice("vis.everyone", BTreeSet::new()),
        VisibilityOption::from_stable_id("vis.chosen"),
    ) {
        let send_to = SignedSendTo::sign(
            VisibilityChoice {
                option,
                set: set_of(["r.carol"]),
            },
            author_secret,
        );
        let result =
            distribute_story(&record, &defaults, &world, &directory, &relay, Some(&send_to));
        let moved = match &result {
            Ok(receipt) => {
                receipt.keyed == set_of(["r.carol"])
                    && receipt.option_stable_id == "vis.chosen"
                    && !receipt.keyed.contains("r.alice")
            }
            Err(_) => false,
        };
        report.expect(
            moved,
            "9.6",
            "a-signed-story-override-replaces-the-default-audience",
            format!(
                "Settings default vis.everyone would key {{r.alice, r.bob}}; the signed override \
                 vis.chosen {{r.carol}} -> {}",
                outcome_text(&result)
            ),
        );
    } else {
        report.fail(
            "9.6",
            "a-signed-story-override-replaces-the-default-audience",
            "setup".to_owned(),
        );
    }

    // 9.7 nothing that was refused reached the server.
    let rows_after = relay.row_count().unwrap_or(-1);
    report.expect(
        rows_after == rows_before + 1,
        "9.7",
        "nothing-refused-reached-the-server",
        format!(
            "envelopes on the shelf before={rows_before} after={rows_after} (required +1: only \
             the one accepted distribution of 9.6)"
        ),
    );
    report.expect(
        relay.content_checks() == 0,
        "9.8",
        "the-server-still-read-nothing",
        format!("content_checks={} (required 0)", relay.content_checks()),
    );
}

fn outcome_text<T>(result: &Result<T, DistributionError>) -> String {
    match result {
        Ok(_) => "ACCEPTED".to_owned(),
        Err(error) => format!("refused: {error}"),
    }
}

// ---------------------------------------------------------------------------
// 10 — the real boundary, read out of the shipping source.
// ---------------------------------------------------------------------------

fn section_10_census(report: &mut Report) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/social_distribution.rs");
    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            report.fail("10.0", "shipping-module-is-readable", format!("{path:?}: {error}"));
            return;
        }
    };

    let entry_points: Vec<&str> = source
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("pub fn distribute_"))
        .collect();
    report.expect(
        entry_points.len() == KINDS.len(),
        "10.1",
        "one-production-sender-path-per-content-type",
        format!(
            "pub distribution entry points in the shipping module = {} {entry_points:?} \
             (required {})",
            entry_points.len(),
            KINDS.len()
        ),
    );

    let fixtures: Vec<&str> = source
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("pub fn ") || line.starts_with("pub async fn "))
        .filter(|line| {
            let lower = line.to_lowercase();
            lower.contains("fixture")
                || lower.contains("test_only")
                || lower.contains("for_test")
                || lower.contains("mock")
                || lower.contains("seed_")
        })
        .collect();
    report.expect(
        fixtures.is_empty(),
        "10.2",
        "no-fixture-or-seeding-entry-point-ships",
        format!(
            "fixture/mock/seed entry points in the shipping module = {} {fixtures:?} (required 0)",
            fixtures.len()
        ),
    );

    // The server's whole typed surface: it has no type through which to see a
    // record, a visibility choice or an audience.
    let block = match source.find("impl ContentRelay {") {
        Some(start) => match source[start..].find("\n}\n") {
            Some(end) => &source[start..start + end],
            None => {
                report.fail(
                    "10.3",
                    "the-servers-typed-surface-cannot-see-content",
                    "impl ContentRelay block is not delimited".to_owned(),
                );
                return;
            }
        },
        None => {
            report.fail(
                "10.3",
                "the-servers-typed-surface-cannot-see-content",
                "no impl ContentRelay block".to_owned(),
            );
            return;
        }
    };
    let forbidden = [
        "SocialRecord",
        "SocialFields",
        "VisibilityOption",
        "VisibilityChoice",
        "Audience",
        "visibility_stable_id",
        "DistributionReceipt",
        "OpenedContent",
    ];
    let found: Vec<&str> = forbidden
        .iter()
        .copied()
        .filter(|needle| block.contains(needle))
        .collect();
    let signatures: Vec<String> = block
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("pub fn "))
        .map(|line| line.trim_end_matches(" {").to_owned())
        .collect();
    report.expect(
        found.is_empty() && !signatures.is_empty(),
        "10.3",
        "the-servers-typed-surface-cannot-see-content",
        format!(
            "{} public methods on ContentRelay, none of them mentioning {forbidden:?}; found \
             {found:?}. Surface: {signatures:?}",
            signatures.len()
        ),
    );
}
