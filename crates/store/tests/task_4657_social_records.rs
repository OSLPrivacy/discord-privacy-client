//! TASK 4657 — the check for the post, story and archive-item records.
//!
//! `harness = false`: a starved production path has to exit **1**, and a
//! libtest binary exits 101 with cargo wrapping it. This binary owns
//! `fn main()` and its own exit code.
//!
//! The check is deliberately independent of the module it checks:
//!
//! - the 4650/4651/4652 vocabulary is written out here as this file's own
//!   literals, so a production source that renamed or widened a constant
//!   cannot drag the control along with it;
//! - the body/media digest is hashed here with `sha2` over this file's own
//!   preimage, not by calling the store's helper;
//! - the durable row is read straight out of `social.sqlite` with this file's
//!   own SQLite handle, unsealed with keys this file derives from the identity
//!   secret using its own HKDF literals, and parsed with this file's own
//!   canonical parser;
//! - `diff_at`, the byte-offset comparator every failure quotes, is this
//!   file's own.

use crypto::{aead, ed25519, hkdf};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;
use store::social::{
    canonical_bytes, ArchiveTerms, AuthorBinding, AuthorityDirectory, SocialFields, SocialRecord,
    SocialRecordStore, StoryTerms,
};

// ---------------------------------------------------------------------------
// The independent authority: what 4650, 4651 and 4652 decided, as literals.
// ---------------------------------------------------------------------------

/// TASK 4651's four frozen visibility stable IDs, in the order the note lists
/// them. Not a superset, not a subset.
const VISIBILITY_IDS_4651: [&str; 4] = ["vis.everyone", "vis.chosen", "vis.except", "vis.onlyme"];

/// TASK 4650: a story lives at most 168 hours, on both tiers.
const STORY_CAP_SECONDS_4650: u64 = 168 * 60 * 60;

/// TASK 4652: the archive is a local saved copy on the poster's own disk.
const ARCHIVE_KIND_4652: &str = "archive.local_saved_copy";
const ARCHIVE_PAYER_4652: &str = "payer.poster_disk";
/// Named only so the check can watch them be refused.
const ARCHIVE_KIND_REFUSED_4652: &str = "archive.relay_pointer";
const ARCHIVE_PAYER_REFUSED_4652: &str = "payer.osl_data_allowance";

const STORAGE_RELAY: &str = "store.relay";
const STORAGE_LOCAL_ARCHIVE: &str = "store.local_archive";

const KIND_POST: &str = "social.post";
const KIND_STORY: &str = "social.story";
const KIND_ARCHIVE: &str = "social.archive";

// The check's own copies of the storage literals, used to unseal the durable
// row without asking the store for anything.
const OWN_SEAL_INFO: &[u8] = b"osl-social-record-store-v1";
const OWN_INDEX_INFO: &[u8] = b"osl-social-record-store-index-v1";
const OWN_BI_DOMAIN: &[u8] = b"osl-social-bi/record_id-v1";
const OWN_ROW_AAD: &[u8] = b"osl-social-record/row/v1";
const OWN_DIGEST_DOMAIN: &[u8] = b"osl-social-record/digest/v1";
const OWN_CANONICAL_MAGIC: &[u8] = b"osl-social-record/v1";

const AUTHOR_ID: &str = "osl:author:liam";
const PROFILE_SCOPE: &str = "osl_chats";
const OTHER_AUTHOR_ID: &str = "osl:author:mallory";
const AUTHORITY_VERSION: u32 = 7;
const CREATED_MS: i64 = 1_770_000_000_000;

const IDENTITY_SECRET: [u8; 32] = [0x4a; 32];

// ---------------------------------------------------------------------------
// Reporting
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

    fn pass(&mut self, id: &str, message: String) {
        self.passed += 1;
        println!("TASK4657 PASS  {id} :: {message}");
    }

    fn fail(&mut self, id: &str, message: String) {
        self.failed += 1;
        println!("TASK4657 FAIL  {id} :: {message}");
    }

    fn check(&mut self, ok: bool, id: &str, message: String) -> bool {
        if ok {
            self.pass(id, message);
        } else {
            self.fail(id, message);
        }
        ok
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn short(bytes: &[u8]) -> String {
    let full = hex(bytes);
    if full.len() <= 12 {
        full
    } else {
        format!("{}…{}", &full[..8], &full[full.len() - 4..])
    }
}

/// The check's own byte-offset comparator. `None` means identical.
fn diff_at(left: &[u8], right: &[u8]) -> Option<usize> {
    let n = left.len().min(right.len());
    for i in 0..n {
        if left[i] != right[i] {
            return Some(i);
        }
    }
    if left.len() == right.len() {
        None
    } else {
        Some(n)
    }
}

/// A nine-byte window of `bytes` starting at `at`, as hex — the "content"
/// every field/offset failure quotes.
fn window(bytes: &[u8], at: usize) -> String {
    let end = (at + 9).min(bytes.len());
    if at >= bytes.len() {
        return "<past end>".to_owned();
    }
    hex(&bytes[at..end])
}

/// The check's own body/media digest, over its own length-prefixed,
/// domain-separated preimage.
fn own_digest(body: &[u8], media: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(OWN_DIGEST_DOMAIN);
    hasher.update((body.len() as u64).to_be_bytes());
    hasher.update(body);
    hasher.update((media.len() as u64).to_be_bytes());
    hasher.update(media);
    hasher.finalize().into()
}

// ---------------------------------------------------------------------------
// The check's own canonical parser and durable-row reader
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedRecord {
    kind: String,
    record_id: String,
    author_id: String,
    profile_scope: String,
    authority_version: u32,
    created_unix_ms: i64,
    deleted: bool,
    visibility_stable_id: String,
    storage_choice: String,
    body: Vec<u8>,
    media: Vec<u8>,
    digest: Vec<u8>,
    story: Option<(u64, i64)>,
    archive: Option<(String, String)>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    /// Fallible on purpose. A durable row whose bytes moved must be reported
    /// as a named failure carrying the field and the byte offset, not raised
    /// as a panic — a panicking check exits 101, and the finish line asks for
    /// exit 1 by content, field and byte offset.
    fn take(&mut self, n: usize, field: &str) -> Result<&'a [u8], String> {
        if self.at + n > self.bytes.len() {
            return Err(format!(
                "field {field}: wanted {n} bytes at byte offset {}, only {} of {} remain; \
                 tail content [{}]",
                self.at,
                self.bytes.len().saturating_sub(self.at),
                self.bytes.len(),
                hex(&self.bytes[self.at.min(self.bytes.len())..]),
            ));
        }
        let out = &self.bytes[self.at..self.at + n];
        self.at += n;
        Ok(out)
    }
    fn lp(&mut self, field: &str) -> Result<&'a [u8], String> {
        let raw = self.take(4, field)?;
        let n = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
        self.take(n, field)
    }
    fn text(&mut self, field: &str) -> Result<String, String> {
        let at = self.at;
        let raw = self.lp(field)?;
        String::from_utf8(raw.to_vec()).map_err(|e| {
            format!(
                "field {field}: not UTF-8 at byte offset {at}, content [{}]: {e}",
                hex(raw)
            )
        })
    }
    fn u8(&mut self, field: &str) -> Result<u8, String> {
        Ok(self.take(1, field)?[0])
    }
    fn u32(&mut self, field: &str) -> Result<u32, String> {
        let raw = self.take(4, field)?;
        Ok(u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]))
    }
    fn u64(&mut self, field: &str) -> Result<u64, String> {
        let raw = self.take(8, field)?;
        let mut buf = [0u8; 8];
        buf.copy_from_slice(raw);
        Ok(u64::from_be_bytes(buf))
    }
}

/// Parse a canonical record with this file's own reader, so a production
/// encoder that changed its layout is caught rather than followed.
fn parse_independently(bytes: &[u8]) -> Result<ParsedRecord, String> {
    if !bytes.starts_with(OWN_CANONICAL_MAGIC) {
        return Err(format!(
            "field magic: canonical bytes do not begin with {} at byte offset 0, content [{}]",
            String::from_utf8_lossy(OWN_CANONICAL_MAGIC),
            hex(&bytes[..bytes.len().min(OWN_CANONICAL_MAGIC.len())]),
        ));
    }
    let mut r = Reader {
        bytes,
        at: OWN_CANONICAL_MAGIC.len(),
    };
    let kind = r.text("kind")?;
    let record_id = r.text("record_id")?;
    let author_id = r.text("author_id")?;
    let profile_scope = r.text("profile_scope")?;
    let authority_version = r.u32("authority_version")?;
    let created_unix_ms = r.u64("created_unix_ms")? as i64;
    let deleted = r.u8("deleted")? != 0;
    let visibility_stable_id = r.text("visibility_stable_id")?;
    let storage_choice = r.text("storage_choice")?;
    let body = r.lp("body")?.to_vec();
    let media = r.lp("media")?.to_vec();
    let digest = r.lp("digest")?.to_vec();
    let story = if r.u8("story.present")? == 0 {
        None
    } else {
        Some((
            r.u64("story.lifetime_seconds")?,
            r.u64("story.expires_unix_ms")? as i64,
        ))
    };
    let archive = if r.u8("archive.present")? == 0 {
        None
    } else {
        Some((r.text("archive.archive_kind")?, r.text("archive.payer")?))
    };
    if r.at != bytes.len() {
        return Err(format!(
            "field trailer: {} trailing byte(s) after the record at byte offset {}, content [{}]",
            bytes.len() - r.at,
            r.at,
            hex(&bytes[r.at..]),
        ));
    }
    Ok(ParsedRecord {
        kind,
        record_id,
        author_id,
        profile_scope,
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

/// Read one durable row out of `social.sqlite` with this file's own SQLite
/// handle, unseal it with keys this file derives, and parse it with this
/// file's own parser. Nothing on this path calls the repository.
fn read_durable_row(db_path: &Path, record_id: &str) -> Result<(ParsedRecord, [u8; 64]), String> {
    let index_key = hkdf::derive_32(&[], &IDENTITY_SECRET, OWN_INDEX_INFO).expect("index key");
    let seal_key = hkdf::derive_32(&[], &IDENTITY_SECRET, OWN_SEAL_INFO).expect("seal key");
    let bi = hkdf::derive_32(&index_key, record_id.as_bytes(), OWN_BI_DOMAIN).expect("blind index");

    let conn = Connection::open(db_path).expect("open social.sqlite directly");
    let row: Option<(Vec<u8>, Vec<u8>)> = conn
        .query_row(
            "SELECT nonce, sealed FROM social_records WHERE record_bi = ?1",
            rusqlite::params![bi.to_vec()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();
    let Some((nonce, sealed)) = row else {
        return Err(format!(
            "field record_bi: no durable row at byte offset 0 under blind index [{}]",
            hex(&bi)
        ));
    };

    let mut aad = OWN_ROW_AAD.to_vec();
    aad.extend_from_slice(&bi);
    if nonce.len() != 24 {
        return Err(format!(
            "field nonce: durable nonce is {} bytes at byte offset 0, want 24; content [{}]",
            nonce.len(),
            hex(&nonce)
        ));
    }
    let mut nb = [0u8; 24];
    nb.copy_from_slice(&nonce);
    let payload = aead::open(
        &aead::Key::from_bytes(seal_key),
        &aead::Nonce::from_bytes(nb),
        &aad,
        &sealed,
    )
    .map_err(|e| {
        format!("field sealed: durable row does not unseal at byte offset 0 under an independently derived key: {e}")
    })?;

    if payload.len() < 4 + 64 {
        return Err(format!(
            "field payload: durable payload is {} bytes at byte offset 0, too short to hold a \
             record and a 64-byte signature; content [{}]",
            payload.len(),
            hex(&payload)
        ));
    }
    let n = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
    if payload.len() != 4 + n + 64 {
        return Err(format!(
            "field payload: durable payload is {} bytes at byte offset 0, want {} for a \
             {n}-byte record plus a 64-byte signature",
            payload.len(),
            4 + n + 64
        ));
    }
    let canonical = &payload[4..4 + n];
    let mut signature = [0u8; 64];
    signature.copy_from_slice(&payload[4 + n..]);
    Ok((parse_independently(canonical)?, signature))
}

fn durable_row_count(db_path: &Path) -> i64 {
    let conn = Connection::open(db_path).expect("open social.sqlite directly");
    conn.query_row("SELECT COUNT(*) FROM social_records", [], |row| row.get(0))
        .expect("count rows")
}

fn durable_distinct_ids(db_path: &Path) -> i64 {
    let conn = Connection::open(db_path).expect("open social.sqlite directly");
    conn.query_row(
        "SELECT COUNT(DISTINCT record_bi) FROM social_records",
        [],
        |row| row.get(0),
    )
    .expect("count distinct rows")
}

// ---------------------------------------------------------------------------
// Building and signing submissions
// ---------------------------------------------------------------------------

fn base_fields(kind: &str, record_id: &str, body: &[u8], media: &[u8]) -> SocialFields {
    SocialFields {
        kind: kind.to_owned(),
        record_id: record_id.to_owned(),
        author: AuthorBinding {
            author_id: AUTHOR_ID.to_owned(),
            profile_scope: PROFILE_SCOPE.to_owned(),
        },
        authority_version: AUTHORITY_VERSION,
        created_unix_ms: CREATED_MS,
        deleted: false,
        visibility_stable_id: VISIBILITY_IDS_4651[1].to_owned(),
        storage_choice: STORAGE_RELAY.to_owned(),
        body: body.to_vec(),
        media: media.to_vec(),
        digest: own_digest(body, media).to_vec(),
        story: None,
        archive: None,
    }
}

fn post_fields(record_id: &str, body: &[u8], media: &[u8]) -> SocialFields {
    base_fields(KIND_POST, record_id, body, media)
}

fn story_fields(record_id: &str, body: &[u8], media: &[u8], lifetime: u64) -> SocialFields {
    let mut fields = base_fields(KIND_STORY, record_id, body, media);
    fields.story = Some(StoryTerms {
        lifetime_seconds: lifetime,
        expires_unix_ms: CREATED_MS + (lifetime as i64) * 1000,
    });
    fields
}

fn archive_fields(record_id: &str, body: &[u8], media: &[u8]) -> SocialFields {
    let mut fields = base_fields(KIND_ARCHIVE, record_id, body, media);
    fields.storage_choice = STORAGE_LOCAL_ARCHIVE.to_owned();
    fields.archive = Some(ArchiveTerms {
        archive_kind: ARCHIVE_KIND_4652.to_owned(),
        payer: ARCHIVE_PAYER_4652.to_owned(),
    });
    fields
}

fn sign(fields: &SocialFields, secret: &ed25519::SecretKey) -> [u8; 64] {
    *ed25519::sign(secret, &canonical_bytes(fields)).as_bytes()
}

// ---------------------------------------------------------------------------
// The eight independently generated fidelity controls
// ---------------------------------------------------------------------------

struct Control {
    name: &'static str,
    body: Vec<u8>,
    media: Vec<u8>,
}

fn controls() -> Vec<Control> {
    let all_bytes: Vec<u8> = (0..=255u8).collect();
    let mut blob = Vec::with_capacity(4096);
    let mut state: u32 = 0x4657_0001;
    for _ in 0..4096 {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        blob.push((state >> 16) as u8);
    }
    vec![
        Control {
            // Leading tab+spaces, a CRLF, an interior run, and a trailing
            // space+tab+newline: every edge a "helpful" trim would eat.
            name: "text/whitespace-edges-and-interior",
            body: b"\t  leading\r\n  interior   run  \ttrailing \t\n".to_vec(),
            media: all_bytes.clone(),
        },
        Control {
            name: "text/whitespace-only",
            body: b" \t\r\n \t\r\n".to_vec(),
            media: vec![0x00, 0x20, 0x09, 0x0a],
        },
        Control {
            // U+00E9 next to e + U+0301: byte-different, visually identical,
            // so a normalising layer is caught.
            name: "text/unicode-nfc-vs-nfd-pair",
            body: "\u{00e9}|e\u{0301}".as_bytes().to_vec(),
            media: vec![0xef, 0xbb, 0xbf, 0x00, 0xff, 0x7f, 0x80, 0x01],
        },
        Control {
            name: "text/unicode-emoji-zwj-and-rtl",
            body: "\u{1f469}\u{200d}\u{1f4bb}\u{202e}gnidoc\u{202c}"
                .as_bytes()
                .to_vec(),
            media: vec![0x00, 0x01, 0xfe, 0xff, 0x7f, 0x80],
        },
        Control {
            name: "text/unicode-combining-and-zero-width",
            body: "a\u{0301}\u{0328}\u{200b}\u{feff}b".as_bytes().to_vec(),
            media: vec![0xff, 0x00, 0xff, 0x00],
        },
        Control {
            name: "binary/all-256-byte-values",
            body: all_bytes.clone(),
            media: all_bytes.iter().rev().copied().collect(),
        },
        Control {
            name: "binary/embedded-nul-and-high-bytes",
            body: vec![0x00, 0xff, 0x00, 0xfe, 0x80, 0x7f, 0x00, 0x01],
            media: (0..64u8).map(|b| b ^ 0xa5).collect(),
        },
        Control {
            name: "media/large-pseudorandom-blob",
            body: b"media control, 4096 bytes below".to_vec(),
            media: blob,
        },
    ]
}

/// One submission that must be refused before anything reaches the disk.
struct Refusal {
    name: &'static str,
    field: &'static str,
    kind: &'static str,
    canonical: Vec<u8>,
    signature: [u8; 64],
    /// 0 = the live directory, 1 = the author's authority revoked,
    /// 2 = the author's key rotated out from under the signature.
    directory: usize,
    record_id: String,
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    let mut report = Report::new();

    // -- 1. the vocabulary is the frozen one -------------------------------
    let shipped_visibility: Vec<&str> = content_defaults::VisibilityOption::ALL
        .iter()
        .map(|option| option.stable_id())
        .collect();
    report.check(
        shipped_visibility == VISIBILITY_IDS_4651.to_vec(),
        "1.1 visibility-ids-are-exactly-4651s-four",
        format!("shipped={shipped_visibility:?} 4651={VISIBILITY_IDS_4651:?}"),
    );
    report.check(
        store::social::STORY_MAX_LIFETIME_SECONDS == STORY_CAP_SECONDS_4650,
        "1.2 story-cap-is-4650s-168h",
        format!(
            "shipped={} 4650={STORY_CAP_SECONDS_4650} seconds",
            store::social::STORY_MAX_LIFETIME_SECONDS
        ),
    );
    report.check(
        store::social::ARCHIVE_KIND_LOCAL_SAVED_COPY == ARCHIVE_KIND_4652
            && store::social::ARCHIVE_PAYER_POSTER_DISK == ARCHIVE_PAYER_4652
            && store::social::ARCHIVE_KIND_RELAY_POINTER == ARCHIVE_KIND_REFUSED_4652
            && store::social::ARCHIVE_PAYER_OSL_DATA_ALLOWANCE == ARCHIVE_PAYER_REFUSED_4652,
        "1.3 archive-kind-and-payer-are-4652s",
        format!(
            "shipped kind={} payer={} (refusable kind={} payer={})",
            store::social::ARCHIVE_KIND_LOCAL_SAVED_COPY,
            store::social::ARCHIVE_PAYER_POSTER_DISK,
            store::social::ARCHIVE_KIND_RELAY_POINTER,
            store::social::ARCHIVE_PAYER_OSL_DATA_ALLOWANCE,
        ),
    );

    // -- the authorized author, and the directory that says so -------------
    let author_secret = ed25519::SecretKey::from_bytes([0x11; 32]);
    let author_public = ed25519::derive_public(&author_secret);
    let other_secret = ed25519::SecretKey::from_bytes([0x22; 32]);
    let other_public = ed25519::derive_public(&other_secret);
    let rotated_secret = ed25519::SecretKey::from_bytes([0x33; 32]);
    let rotated_public = ed25519::derive_public(&rotated_secret);

    let mut directory = AuthorityDirectory::new();
    directory.grant(AUTHOR_ID, PROFILE_SCOPE, author_public, AUTHORITY_VERSION);
    // A second, genuinely authorized author, so "substituted author" means an
    // author who really does hold authority — not merely an unknown name.
    directory.grant(
        OTHER_AUTHOR_ID,
        PROFILE_SCOPE,
        other_public,
        AUTHORITY_VERSION,
    );

    // -- 2. three production constructors, three unique records ------------
    let post_media: Vec<u8> = (0..=255u8).collect();
    let post = post_fields("rec.post.4657.a", b"post body, TASK 4657, lane c", &post_media);
    let story = story_fields(
        "rec.story.4657.a",
        b"story body, TASK 4657",
        &[9, 8, 7, 6, 5, 4],
        STORY_CAP_SECONDS_4650,
    );
    let archive = archive_fields(
        "rec.archive.4657.a",
        b"archive body, TASK 4657",
        &[1, 2, 3, 4, 5, 6, 7],
    );
    let post_sig = sign(&post, &author_secret);
    let story_sig = sign(&story, &author_secret);
    let archive_sig = sign(&archive, &author_secret);

    let mut constructors_used: BTreeSet<&str> = BTreeSet::new();
    let built_post = SocialRecord::new_post(post.clone(), post_sig, &directory);
    match &built_post {
        Ok(record) => {
            constructors_used.insert("new_post");
            report.pass(
                "2.1 production-constructor-admits",
                format!("new_post -> kind={}", record.fields().kind),
            );
        }
        Err(error) => report.fail(
            "2.1 production-constructor-admits",
            format!("new_post refused a valid post: {error}"),
        ),
    }
    let built_story = SocialRecord::new_story(story.clone(), story_sig, &directory);
    match &built_story {
        Ok(record) => {
            constructors_used.insert("new_story");
            report.pass(
                "2.1 production-constructor-admits",
                format!("new_story -> kind={}", record.fields().kind),
            );
        }
        Err(error) => report.fail(
            "2.1 production-constructor-admits",
            format!("new_story refused a valid story: {error}"),
        ),
    }
    let built_archive = SocialRecord::new_archive_item(archive.clone(), archive_sig, &directory);
    match &built_archive {
        Ok(record) => {
            constructors_used.insert("new_archive_item");
            report.pass(
                "2.1 production-constructor-admits",
                format!("new_archive_item -> kind={}", record.fields().kind),
            );
        }
        Err(error) => report.fail(
            "2.1 production-constructor-admits",
            format!("new_archive_item refused a valid archive item: {error}"),
        ),
    }
    report.check(
        constructors_used.len() == 3,
        "2.2 three-production-constructors-used",
        format!(
            "constructors that admitted a record = {} {:?} (required 3)",
            constructors_used.len(),
            constructors_used
        ),
    );

    let main_dir = tempfile::tempdir().expect("temp dir");
    let db_path = main_dir.path().join("social.sqlite");
    let store = SocialRecordStore::open(main_dir.path(), &IDENTITY_SECRET).expect("open store");
    report.pass(
        "2.3 repository-opens",
        format!("path={}", store.path().display()),
    );

    let submissions: Vec<(&str, &SocialFields, [u8; 64])> = vec![
        (KIND_POST, &post, post_sig),
        (KIND_STORY, &story, story_sig),
        (KIND_ARCHIVE, &archive, archive_sig),
    ];
    for (kind, fields, signature) in &submissions {
        match store.put_canonical(kind, &canonical_bytes(fields), signature, &directory) {
            Ok(()) => report.pass(
                "2.4 repository-writes",
                format!("{kind} record_id={}", fields.record_id),
            ),
            Err(error) => report.fail(
                "2.4 repository-writes",
                format!("{kind} record_id={} refused: {error}", fields.record_id),
            ),
        }
    }
    let rows = durable_row_count(&db_path);
    report.check(
        rows == 3,
        "2.5 three-unique-records-on-disk",
        format!("written=3 rows_on_disk={rows} (required 3 and 3)"),
    );
    let distinct = durable_distinct_ids(&db_path);
    report.check(
        rows == 3 && distinct == 3,
        "2.6 record-ids-are-unique",
        format!("rows={rows} distinct={distinct} (required 3 and 3)"),
    );

    // -- 3. restart --------------------------------------------------------
    drop(store);
    let store = SocialRecordStore::open(main_dir.path(), &IDENTITY_SECRET).expect("reopen store");
    report.check(
        store.row_count().unwrap_or(-1) == 3,
        "3.1 reopen-after-restart",
        format!("rows_visible={}", store.row_count().unwrap_or(-1)),
    );

    let mut read_back = Vec::new();
    for (kind, fields, _) in &submissions {
        match store.get(&fields.record_id, &directory) {
            Ok(Some(record)) => {
                let read = record.fields().clone();
                report.pass(
                    "3.2 read-after-restart",
                    format!(
                        "{kind} record_id={} kind={} body_len={} media_len={} digest={}",
                        read.record_id,
                        read.kind,
                        read.body.len(),
                        read.media.len(),
                        hex(&read.digest)
                    ),
                );
                read_back.push(read);
            }
            Ok(None) => report.fail(
                "3.2 read-after-restart",
                format!("{kind} record_id={} is absent after restart", fields.record_id),
            ),
            Err(error) => report.fail(
                "3.2 read-after-restart",
                format!("{kind} record_id={} refused on read: {error}", fields.record_id),
            ),
        }
    }
    report.check(
        read_back.len() == 3,
        "3.3 all-three-survive-restart",
        format!(
            "records read back after restart = {} (required 3)",
            read_back.len()
        ),
    );

    match read_back.iter().find(|f| f.kind == KIND_STORY) {
        Some(fields) => {
            let terms = fields.story;
            let ok = terms.is_some_and(|t| {
                t.lifetime_seconds > 0
                    && t.lifetime_seconds <= STORY_CAP_SECONDS_4650
                    && t.expires_unix_ms == fields.created_unix_ms + (t.lifetime_seconds as i64) * 1000
            });
            report.check(
                ok,
                "3.4 story-has-expiry-and-lifetime",
                match terms {
                    Some(t) => format!(
                        "lifetime_seconds={} expires_unix_ms={} created_unix_ms={} cap={STORY_CAP_SECONDS_4650}",
                        t.lifetime_seconds, t.expires_unix_ms, fields.created_unix_ms
                    ),
                    None => "the story read back off disk has no expiry or lifetime".to_owned(),
                },
            );
        }
        None => report.fail(
            "3.4 story-has-expiry-and-lifetime",
            "no story survived the restart".to_owned(),
        ),
    }

    match read_back.iter().find(|f| f.kind == KIND_ARCHIVE) {
        Some(fields) => {
            let terms = fields.archive.clone();
            let ok = terms.as_ref().is_some_and(|t| {
                t.archive_kind == ARCHIVE_KIND_4652
                    && t.archive_kind != ARCHIVE_KIND_REFUSED_4652
                    && t.payer == ARCHIVE_PAYER_4652
            }) && fields.storage_choice == STORAGE_LOCAL_ARCHIVE;
            report.check(
                ok,
                "3.5 archive-has-non-pointer-kind-and-payer",
                match terms {
                    Some(t) => format!(
                        "archive_kind={} payer={} storage_choice={} (pointer kind {} is not it)",
                        t.archive_kind, t.payer, fields.storage_choice, ARCHIVE_KIND_REFUSED_4652
                    ),
                    None => "the archive item read back off disk has no chosen kind".to_owned(),
                },
            );
        }
        None => report.fail(
            "3.5 archive-has-non-pointer-kind-and-payer",
            "no archive item survived the restart".to_owned(),
        ),
    }

    // -- 4. every record authenticates under the authorized key ------------
    report.check(
        directory
            .live(AUTHOR_ID, PROFILE_SCOPE)
            .map(|grant| *grant.public_key.as_bytes())
            == Some(*author_public.as_bytes()),
        "4.0 verifying-key-is-the-currently-authorized-one",
        format!(
            "directory key={} author key={}",
            directory
                .live(AUTHOR_ID, PROFILE_SCOPE)
                .map(|g| hex(g.public_key.as_bytes()))
                .unwrap_or_else(|| "<none>".to_owned()),
            hex(author_public.as_bytes())
        ),
    );

    let authorized_key = directory
        .live(AUTHOR_ID, PROFILE_SCOPE)
        .expect("a live grant")
        .public_key;
    let mut authenticated = 0usize;
    for (kind, fields, _) in &submissions {
        let (parsed, signature) = match read_durable_row(&db_path, &fields.record_id) {
            Ok(row) => row,
            Err(why) => {
                report.fail(
                    "4.1 signature-verifies",
                    format!(
                        "{kind} record_id={} has no readable durable row: {why}",
                        fields.record_id
                    ),
                );
                continue;
            }
        };
        // Rebuild the canonical bytes from the DURABLE row, then verify the
        // durable signature over them under the directory's key.
        let canonical = canonical_bytes(&read_back_fields(&parsed));
        let verified = ed25519::verify(
            &authorized_key,
            &canonical,
            &ed25519::Signature::from_bytes(signature),
        )
        .unwrap_or(false);
        if verified {
            authenticated += 1;
        }
        report.check(
            verified,
            "4.1 signature-verifies",
            format!(
                "{kind} verifies={verified} canonical_bytes={} author_id={} profile_scope={} authority_version={}",
                canonical.len(),
                parsed.author_id,
                parsed.profile_scope,
                parsed.authority_version
            ),
        );
    }
    report.check(
        authenticated == 3,
        "4.2 all-three-authenticate",
        format!(
            "records whose signature verified under the authorized key = {authenticated} (required 3)"
        ),
    );

    // -- 5. every signed field is bound ------------------------------------
    //
    // Each mutation moves exactly one signed field of a record already on
    // disk, re-encodes, and resubmits under the UNTOUCHED signature. Body and
    // media additionally have their digest recomputed, so those two land on
    // the signature gate rather than stopping at the digest gate — the digest
    // gate is proved separately in section 7.
    let rows_before_mutations = durable_row_count(&db_path);
    let mutations: Vec<(&str, &str, SocialFields, [u8; 64], SocialFields)> = {
        let mut out: Vec<(&str, &str, SocialFields, [u8; 64], SocialFields)> = Vec::new();

        let mut m = post.clone();
        m.record_id = "rec.post.4657.a.moved".to_owned();
        out.push(("record_id", KIND_POST, post.clone(), post_sig, m));

        let mut m = post.clone();
        m.author.author_id = OTHER_AUTHOR_ID.to_owned();
        out.push(("author_id", KIND_POST, post.clone(), post_sig, m));

        let mut m = post.clone();
        m.author.profile_scope = "global".to_owned();
        out.push(("profile_scope", KIND_POST, post.clone(), post_sig, m));

        let mut m = post.clone();
        m.authority_version = AUTHORITY_VERSION + 1;
        out.push(("authority_version", KIND_POST, post.clone(), post_sig, m));

        let mut m = post.clone();
        m.created_unix_ms = CREATED_MS + 1;
        out.push(("created_unix_ms", KIND_POST, post.clone(), post_sig, m));

        let mut m = post.clone();
        m.deleted = true;
        out.push(("deleted", KIND_POST, post.clone(), post_sig, m));

        let mut m = post.clone();
        m.visibility_stable_id = VISIBILITY_IDS_4651[3].to_owned();
        out.push((
            "visibility_stable_id",
            KIND_POST,
            post.clone(),
            post_sig,
            m,
        ));

        let mut m = post.clone();
        m.storage_choice = STORAGE_LOCAL_ARCHIVE.to_owned();
        out.push(("storage_choice", KIND_POST, post.clone(), post_sig, m));

        let mut m = post.clone();
        m.body[0] ^= 0x01;
        m.digest = own_digest(&m.body, &m.media).to_vec();
        out.push(("body", KIND_POST, post.clone(), post_sig, m));

        let mut m = post.clone();
        let last = m.media.len() - 1;
        m.media[last] ^= 0x80;
        m.digest = own_digest(&m.body, &m.media).to_vec();
        out.push(("media", KIND_POST, post.clone(), post_sig, m));

        let mut m = post.clone();
        m.digest[31] ^= 0x01;
        out.push(("digest", KIND_POST, post.clone(), post_sig, m));

        let mut m = story.clone();
        if let Some(terms) = m.story.as_mut() {
            terms.lifetime_seconds = 3_600;
            terms.expires_unix_ms = CREATED_MS + 3_600 * 1000;
        }
        out.push((
            "story.lifetime_seconds",
            KIND_STORY,
            story.clone(),
            story_sig,
            m,
        ));

        let mut m = story.clone();
        if let Some(terms) = m.story.as_mut() {
            terms.expires_unix_ms += 1;
        }
        out.push((
            "story.expires_unix_ms",
            KIND_STORY,
            story.clone(),
            story_sig,
            m,
        ));

        let mut m = archive.clone();
        if let Some(terms) = m.archive.as_mut() {
            terms.archive_kind = ARCHIVE_KIND_REFUSED_4652.to_owned();
        }
        out.push((
            "archive.archive_kind",
            KIND_ARCHIVE,
            archive.clone(),
            archive_sig,
            m,
        ));

        let mut m = archive.clone();
        if let Some(terms) = m.archive.as_mut() {
            terms.payer = ARCHIVE_PAYER_REFUSED_4652.to_owned();
        }
        out.push((
            "archive.payer",
            KIND_ARCHIVE,
            archive.clone(),
            archive_sig,
            m,
        ));

        out
    };

    let mutation_total = mutations.len();
    let mut mutations_refused = 0usize;
    for (field, kind, original, signature, mutated) in &mutations {
        let before = canonical_bytes(original);
        let after = canonical_bytes(mutated);
        let offset = diff_at(&before, &after).unwrap_or(usize::MAX);
        let result = store.put_canonical(kind, &after, signature, &directory);
        let refused = result.is_err();
        if refused {
            mutations_refused += 1;
        }
        let detail = match &result {
            Ok(()) => "was PERSISTED under the untouched signature".to_owned(),
            Err(error) => format!("refused: {error}"),
        };
        report.check(
            refused,
            "5.1 field-binding",
            format!(
                "field {field} first differing byte offset {offset} content [{}] -> [{}] {detail}",
                window(&before, offset.min(before.len())),
                window(&after, offset.min(after.len())),
            ),
        );
    }
    report.check(
        mutations_refused == mutation_total,
        "5.2 every-signed-field-is-bound",
        format!(
            "signed fields whose alteration was refused = {mutations_refused} of {mutation_total}"
        ),
    );
    let rows_after_mutations = durable_row_count(&db_path);
    report.check(
        rows_before_mutations == 3 && rows_after_mutations == 3,
        "5.3 no-mutation-reached-the-disk",
        format!("rows before={rows_before_mutations} after={rows_after_mutations} (required 3 and 3)"),
    );

    // -- 6. fidelity controls ----------------------------------------------
    let mut identical = 0usize;
    let mut digests_agree = 0usize;
    let control_list = controls();
    let control_total = control_list.len();
    for (index, control) in control_list.iter().enumerate() {
        let control_dir = tempfile::tempdir().expect("control temp dir");
        let control_db = control_dir.path().join("social.sqlite");
        let record_id = format!("rec.control.{index}");
        let fields = post_fields(&record_id, &control.body, &control.media);
        let signature = sign(&fields, &author_secret);
        let input_digest = own_digest(&control.body, &control.media);

        let control_store =
            SocialRecordStore::open(control_dir.path(), &IDENTITY_SECRET).expect("open control");
        if let Err(error) =
            control_store.put_canonical(KIND_POST, &canonical_bytes(&fields), &signature, &directory)
        {
            report.fail(
                "6.1 control-round-trip",
                format!("control {} was refused: {error}", control.name),
            );
            continue;
        }
        // restart
        drop(control_store);

        let durable = match read_durable_row(&control_db, &record_id) {
            Ok((durable, _)) => durable,
            Err(why) => {
                report.fail(
                    "6.1 control-round-trip",
                    format!(
                        "control {} has no readable durable row after restart: {why}",
                        control.name
                    ),
                );
                continue;
            }
        };
        let control_store =
            SocialRecordStore::open(control_dir.path(), &IDENTITY_SECRET).expect("reopen control");
        let read = match control_store.get(&record_id, &directory) {
            Ok(Some(record)) => record.fields().clone(),
            Ok(None) => {
                report.fail(
                    "6.1 control-round-trip",
                    format!("control {} is absent on read", control.name),
                );
                continue;
            }
            Err(error) => {
                report.fail(
                    "6.1 control-round-trip",
                    format!("control {} refused on read: {error}", control.name),
                );
                continue;
            }
        };

        let mut problems: Vec<String> = Vec::new();
        for (label, input, durable_bytes, read_bytes) in [
            ("body", &control.body, &durable.body, &read.body),
            ("media", &control.media, &durable.media, &read.media),
        ] {
            if let Some(at) = diff_at(input, durable_bytes) {
                problems.push(format!(
                    "field {label} input->durable: first differing byte offset {at}, input [{}] durable [{}] (len {} vs {})",
                    window(input, at), window(durable_bytes, at), input.len(), durable_bytes.len()
                ));
            }
            if let Some(at) = diff_at(input, read_bytes) {
                problems.push(format!(
                    "field {label} input->read: first differing byte offset {at}, input [{}] read [{}] (len {} vs {})",
                    window(input, at), window(read_bytes, at), input.len(), read_bytes.len()
                ));
            }
        }
        let byte_identical = problems.is_empty();
        if byte_identical {
            identical += 1;
        }
        report.check(
            byte_identical,
            "6.1 control-round-trip",
            if byte_identical {
                format!(
                    "control {} body_len={} media_len={} byte-identical at constructor input, durable stored record and read output",
                    control.name,
                    control.body.len(),
                    control.media.len()
                )
            } else {
                format!("control {} {}", control.name, problems.join(" | "))
            },
        );

        let durable_digest = own_digest(&durable.body, &durable.media);
        let read_digest = own_digest(&read.body, &read.media);
        let stored_field = durable.digest.clone();
        let agree = durable_digest == input_digest
            && read_digest == input_digest
            && stored_field.as_slice() == input_digest.as_slice()
            && read.digest.as_slice() == input_digest.as_slice();
        if agree {
            digests_agree += 1;
        }
        report.check(
            agree,
            "6.2 control-digests-agree",
            if agree {
                format!("control {} digest={}", control.name, hex(&input_digest))
            } else {
                let at = diff_at(&input_digest, &read_digest).unwrap_or(0);
                format!(
                    "control {} field digest: input={} durable={} read={} stored={} first differing byte offset {at}",
                    control.name,
                    short(&input_digest),
                    short(&durable_digest),
                    short(&read_digest),
                    short(&stored_field)
                )
            },
        );
    }
    report.check(
        identical == control_total,
        "6.3 every-control-is-byte-identical",
        format!("controls byte-identical across all three stages = {identical} of {control_total}"),
    );
    report.check(
        digests_agree == control_total,
        "6.4 every-control-digest-agrees",
        format!(
            "controls whose independent digests agree = {digests_agree} of {control_total}"
        ),
    );

    // -- 7. refusals, all before persistence -------------------------------
    let mut revoked_directory = AuthorityDirectory::new();
    revoked_directory.grant(
        OTHER_AUTHOR_ID,
        PROFILE_SCOPE,
        other_public,
        AUTHORITY_VERSION,
    );
    let mut rotated_directory = AuthorityDirectory::new();
    rotated_directory.grant(AUTHOR_ID, PROFILE_SCOPE, rotated_public, AUTHORITY_VERSION);

    let mut refusals: Vec<Refusal> = Vec::new();
    macro_rules! push {
        ($name:expr, $field:expr, $kind:expr, $fields:expr, $secret:expr, $dir:expr) => {{
            let fields = $fields;
            let canonical = canonical_bytes(&fields);
            let signature = *ed25519::sign($secret, &canonical).as_bytes();
            refusals.push(Refusal {
                name: $name,
                field: $field,
                kind: $kind,
                canonical,
                signature,
                directory: $dir,
                record_id: fields.record_id.clone(),
            });
        }};
    }

    // missing fields
    let mut f = post_fields("rec.refuse.missing-record-id", b"body", &[1, 2]);
    f.record_id = String::new();
    push!("missing-record-id", "record_id", KIND_POST, f, &author_secret, 0);

    let mut f = post_fields("rec.refuse.missing-author-id", b"body", &[1, 2]);
    f.author.author_id = String::new();
    push!("missing-author-id", "author_id", KIND_POST, f, &author_secret, 0);

    let mut f = post_fields("rec.refuse.missing-profile-scope", b"body", &[1, 2]);
    f.author.profile_scope = String::new();
    push!("missing-profile-scope", "profile_scope", KIND_POST, f, &author_secret, 0);

    let mut f = post_fields("rec.refuse.missing-visibility", b"body", &[1, 2]);
    f.visibility_stable_id = String::new();
    push!("missing-visibility", "visibility_stable_id", KIND_POST, f, &author_secret, 0);

    let mut f = post_fields("rec.refuse.missing-storage", b"body", &[1, 2]);
    f.storage_choice = String::new();
    push!("missing-storage-choice", "storage_choice", KIND_POST, f, &author_secret, 0);

    let mut f = post_fields("rec.refuse.missing-body", b"body", &[1, 2]);
    f.body = Vec::new();
    push!("missing-body", "body", KIND_POST, f, &author_secret, 0);

    let mut f = post_fields("rec.refuse.missing-digest", b"body", &[1, 2]);
    f.digest = Vec::new();
    push!("missing-digest", "digest", KIND_POST, f, &author_secret, 0);

    let mut f = post_fields("rec.refuse.missing-created", b"body", &[1, 2]);
    f.created_unix_ms = 0;
    push!("missing-created-time", "created_unix_ms", KIND_POST, f, &author_secret, 0);

    let mut f = post_fields("rec.refuse.missing-authority-version", b"body", &[1, 2]);
    f.authority_version = 0;
    push!("missing-authority-version", "authority_version", KIND_POST, f, &author_secret, 0);

    let mut f = story_fields("rec.refuse.missing-story-lifetime", b"body", &[1, 2], 3_600);
    f.story = Some(StoryTerms {
        lifetime_seconds: 0,
        expires_unix_ms: CREATED_MS + 3_600_000,
    });
    push!("missing-story-lifetime", "story.lifetime_seconds", KIND_STORY, f, &author_secret, 0);

    let mut f = archive_fields("rec.refuse.missing-archive-choice", b"body", &[1, 2]);
    f.archive = Some(ArchiveTerms {
        archive_kind: String::new(),
        payer: ARCHIVE_PAYER_4652.to_owned(),
    });
    push!("missing-archive-choice", "archive.archive_kind", KIND_ARCHIVE, f, &author_secret, 0);

    // vocabulary
    let mut f = post_fields("rec.refuse.unknown-visibility", b"body", &[1, 2]);
    f.visibility_stable_id = "vis.public".to_owned();
    push!("unknown-visibility-id", "visibility_stable_id", KIND_POST, f, &author_secret, 0);

    let mut f = post_fields("rec.refuse.unknown-storage", b"body", &[1, 2]);
    f.storage_choice = "store.somewhere".to_owned();
    push!("unknown-storage-choice", "storage_choice", KIND_POST, f, &author_secret, 0);

    // 4650 / 4652 policy
    let f = story_fields(
        "rec.refuse.story-over-cap",
        b"body",
        &[1, 2],
        STORY_CAP_SECONDS_4650 + 1,
    );
    push!("story-lifetime-over-4650-cap", "story.lifetime_seconds", KIND_STORY, f, &author_secret, 0);

    let mut f = story_fields("rec.refuse.story-expiry-disagrees", b"body", &[1, 2], 3_600);
    if let Some(terms) = f.story.as_mut() {
        terms.expires_unix_ms += 1;
    }
    push!("story-expiry-disagrees-with-lifetime", "story.expires_unix_ms", KIND_STORY, f, &author_secret, 0);

    let mut f = archive_fields("rec.refuse.archive-pointer", b"body", &[1, 2]);
    if let Some(terms) = f.archive.as_mut() {
        terms.archive_kind = ARCHIVE_KIND_REFUSED_4652.to_owned();
    }
    push!("archive-pointer-kind", "archive.archive_kind", KIND_ARCHIVE, f, &author_secret, 0);

    let mut f = archive_fields("rec.refuse.archive-payer", b"body", &[1, 2]);
    if let Some(terms) = f.archive.as_mut() {
        terms.payer = ARCHIVE_PAYER_REFUSED_4652.to_owned();
    }
    push!("archive-payer-is-osl-allowance", "archive.payer", KIND_ARCHIVE, f, &author_secret, 0);

    // author authenticity and owner authority
    let f = post_fields("rec.refuse.forged-signature", b"body", &[1, 2]);
    push!("forged-author-signature", "signature", KIND_POST, f, &other_secret, 0);

    let mut f = post_fields("rec.refuse.substituted-author", b"body", &[1, 2]);
    // Signed by liam, then the author id is swapped to a DIFFERENT, genuinely
    // authorized author. Done after signing, below.
    f.author.author_id = AUTHOR_ID.to_owned();
    {
        let mut signed_as_liam = f.clone();
        signed_as_liam.kind = KIND_POST.to_owned();
        let signature = *ed25519::sign(&author_secret, &canonical_bytes(&signed_as_liam)).as_bytes();
        let mut substituted = signed_as_liam.clone();
        substituted.author.author_id = OTHER_AUTHOR_ID.to_owned();
        refusals.push(Refusal {
            name: "substituted-author-identity",
            field: "author_id",
            kind: KIND_POST,
            canonical: canonical_bytes(&substituted),
            signature,
            directory: 0,
            record_id: substituted.record_id.clone(),
        });
    }

    let mut f = post_fields("rec.refuse.stale-authority", b"body", &[1, 2]);
    f.authority_version = AUTHORITY_VERSION - 1;
    push!("stale-authority-version", "authority_version", KIND_POST, f, &author_secret, 0);

    let f = post_fields("rec.refuse.revoked-authority", b"body", &[1, 2]);
    push!("revoked-authority", "author_id", KIND_POST, f, &author_secret, 1);

    let f = post_fields("rec.refuse.rotated-key", b"body", &[1, 2]);
    push!("rotated-author-key", "signature", KIND_POST, f, &author_secret, 2);

    // digest and byte alterations
    let mut f = post_fields("rec.refuse.digest-mismatch", b"body", &[1, 2]);
    f.digest[0] ^= 0xee;
    push!("digest-does-not-cover-the-bytes", "digest", KIND_POST, f, &author_secret, 0);

    {
        // A body byte moves on the wire with the digest left alone.
        let signed_fields = post_fields("rec.refuse.altered-body", b"body bytes", &[1, 2, 3]);
        let signature = sign(&signed_fields, &author_secret);
        let mut altered = signed_fields.clone();
        altered.body[0] ^= 0x01;
        refusals.push(Refusal {
            name: "altered-body-byte",
            field: "body",
            kind: KIND_POST,
            canonical: canonical_bytes(&altered),
            signature,
            directory: 0,
            record_id: altered.record_id.clone(),
        });

        let signed_fields = post_fields("rec.refuse.altered-media", b"body bytes", &[1, 2, 3]);
        let signature = sign(&signed_fields, &author_secret);
        let mut altered = signed_fields.clone();
        altered.media[2] ^= 0x40;
        refusals.push(Refusal {
            name: "altered-media-byte",
            field: "media",
            kind: KIND_POST,
            canonical: canonical_bytes(&altered),
            signature,
            directory: 0,
            record_id: altered.record_id.clone(),
        });

        let signed_fields = post_fields("rec.refuse.altered-visibility", b"body bytes", &[1, 2, 3]);
        let signature = sign(&signed_fields, &author_secret);
        let mut altered = signed_fields.clone();
        altered.visibility_stable_id = VISIBILITY_IDS_4651[0].to_owned();
        refusals.push(Refusal {
            name: "altered-visibility-byte",
            field: "visibility_stable_id",
            kind: KIND_POST,
            canonical: canonical_bytes(&altered),
            signature,
            directory: 0,
            record_id: altered.record_id.clone(),
        });

        let signed_fields = post_fields("rec.refuse.altered-deleted", b"body bytes", &[1, 2, 3]);
        let signature = sign(&signed_fields, &author_secret);
        let mut altered = signed_fields.clone();
        altered.deleted = true;
        refusals.push(Refusal {
            name: "altered-delete-state-byte",
            field: "deleted",
            kind: KIND_POST,
            canonical: canonical_bytes(&altered),
            signature,
            directory: 0,
            record_id: altered.record_id.clone(),
        });
    }

    let rows_before_refusals = durable_row_count(&db_path);
    let refusal_total = refusals.len();
    let mut refused_count = 0usize;
    for case in &refusals {
        let dir_ref = match case.directory {
            1 => &revoked_directory,
            2 => &rotated_directory,
            _ => &directory,
        };
        let result = store.put_canonical(case.kind, &case.canonical, &case.signature, dir_ref);
        let landed = store
            .get(&case.record_id, &directory)
            .ok()
            .flatten()
            .is_some();
        let refused = result.is_err() && !landed;
        if refused {
            refused_count += 1;
        }
        let detail = match &result {
            Ok(()) => format!(
                "was ACCEPTED and persisted: record_id={} canonical_bytes={}",
                case.record_id,
                case.canonical.len()
            ),
            Err(error) => format!("refused before persistence: {error}"),
        };
        report.check(
            refused,
            "7.1 refusal",
            format!("{} field {} {detail}", case.name, case.field),
        );
    }
    report.check(
        refused_count == refusal_total,
        "7.2 every-refusal-case-refused",
        format!(
            "refusal cases refused before persistence = {refused_count} of {refusal_total}"
        ),
    );
    let rows_after_refusals = durable_row_count(&db_path);
    report.check(
        rows_before_refusals == 3 && rows_after_refusals == 3,
        "7.3 nothing-refused-reached-the-disk",
        format!("rows before={rows_before_refusals} after={rows_after_refusals} (required 3 and 3)"),
    );

    // Non-vacuity: the same shape, signed correctly under the live authority.
    let good = post_fields("rec.refuse.control-good", b"a correctly signed post", &[1, 2]);
    let good_sig = sign(&good, &author_secret);
    let accepted = store
        .put_canonical(KIND_POST, &canonical_bytes(&good), &good_sig, &directory)
        .is_ok();
    let readable = store
        .get(&good.record_id, &directory)
        .ok()
        .flatten()
        .is_some();
    report.check(
        accepted && readable,
        "7.4 refusals-are-not-vacuous",
        format!(
            "a correctly signed record under the live authority (key {}) was accepted={accepted} readable={readable}; rows now {}",
            hex(author_public.as_bytes()),
            durable_row_count(&db_path)
        ),
    );

    // -- 8. constructor census over the shipping source --------------------
    //
    // `include_str!` is a compile-time read of the real production module, so
    // this counts what ships, not what a fixture says ships.
    let source = include_str!("../src/social.rs");
    let shipping = match source.find("#[cfg(test)]") {
        Some(at) => &source[..at],
        None => source,
    };
    let production_constructors = ["pub fn new_post", "pub fn new_story", "pub fn new_archive_item"]
        .iter()
        .filter(|needle| shipping.contains(**needle))
        .count();
    report.check(
        production_constructors == 3,
        "8.1 production-constructors-present",
        format!(
            "production constructors defined in the shipping module = {production_constructors} (required 3)"
        ),
    );

    let fixture_markers = [
        "fn fixture", "fn fake_", "fn dummy_", "fn stub_", "fn mock_", "fn sample_",
        "for_test", "for_tests", "test_only", "testing_only", "_unchecked",
    ];
    let fixture_hits: Vec<&str> = fixture_markers
        .iter()
        .filter(|needle| shipping.contains(**needle))
        .copied()
        .collect();
    report.check(
        fixture_hits.is_empty(),
        "8.2 fixture-constructor-census",
        format!(
            "fixture constructors in the shipping module = {} {:?} (required 0)",
            fixture_hits.len(),
            fixture_hits
        ),
    );

    let private_fields = shipping.contains("pub struct SocialRecord {\n    fields: SocialFields,\n    signature: [u8; 64],\n}")
        && !shipping.contains("pub fields: SocialFields");
    report.check(
        private_fields,
        "8.3 no-unchecked-construction-path",
        if private_fields {
            "SocialRecord's fields and signature are private, so admit() is unavoidable".to_owned()
        } else {
            "SocialRecord exposes its fields, so a record can be built without admit()".to_owned()
        },
    );

    println!();
    println!(
        "TASK4657 SUMMARY checks_passed={} checks_failed={}",
        report.passed, report.failed
    );
    if report.failed == 0 {
        println!("TASK4657 RESULT ok");
        std::process::exit(0);
    }
    println!("TASK4657 RESULT failed");
    std::process::exit(1);
}

/// Rebuild the production `SocialFields` shape from this file's independently
/// parsed durable row, so the signature check in section 4 verifies over bytes
/// that came off the disk rather than bytes the check kept in memory.
fn read_back_fields(parsed: &ParsedRecord) -> SocialFields {
    SocialFields {
        kind: parsed.kind.clone(),
        record_id: parsed.record_id.clone(),
        author: AuthorBinding {
            author_id: parsed.author_id.clone(),
            profile_scope: parsed.profile_scope.clone(),
        },
        authority_version: parsed.authority_version,
        created_unix_ms: parsed.created_unix_ms,
        deleted: parsed.deleted,
        visibility_stable_id: parsed.visibility_stable_id.clone(),
        storage_choice: parsed.storage_choice.clone(),
        body: parsed.body.clone(),
        media: parsed.media.clone(),
        digest: parsed.digest.clone(),
        story: parsed
            .story
            .map(|(lifetime_seconds, expires_unix_ms)| StoryTerms {
                lifetime_seconds,
                expires_unix_ms,
            }),
        archive: parsed
            .archive
            .as_ref()
            .map(|(archive_kind, payer)| ArchiveTerms {
                archive_kind: archive_kind.clone(),
                payer: payer.clone(),
            }),
    }
}
