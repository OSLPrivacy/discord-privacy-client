//! TASK 0044c — every attachment's content key is separate, and the
//! compromise of one is contained to that one attachment.
//!
//! This target is deliberately *not* an at-rest-encryption test. It answers
//! one question: if an attacker is handed one attachment's complete key
//! material and metadata, exactly how many other attachments does that buy?
//!
//! Three separations are proved, and none of them is allowed to stand in for
//! another:
//!
//! 1. **Key separation.** An independent key-boundary observer prints one
//!    content-key fingerprint per attachment. The fingerprints are computed
//!    *here*, by observer-only code with its own domain separation — the
//!    production key source has no fingerprint function at all, so a
//!    separation claim can never be self-certified by the code that drew the
//!    keys. Keys are drawn by the shipping broker
//!    (`broker::begin_osl_chat_attachment`), from the OS CSPRNG, with no test
//!    seam.
//!
//! 2. **Nonce freshness is not key separation.** The observer reports nonce
//!    uniqueness *separately* and never as evidence of key separation. The
//!    `global-key` throwaway build in `scripts/task_0044c_gate.sh` keeps
//!    fresh nonces and still fails, which is what makes that distinction
//!    load-bearing.
//!
//! 3. **Compromise containment.** An outside oracle receives one
//!    attachment's complete key material plus every stored ciphertext blob
//!    and every blob's *public* metadata, and nothing else — no key/blob
//!    mapping. It tries the exposed key, the repository's own attachment
//!    key-wrap derivation, and public-metadata-only derivations against every
//!    blob. Exactly one blob may release bytes, and those bytes must equal
//!    that attachment's exact source file. Every other blob, a wrong key, a
//!    flipped ciphertext bit and a reordered part must release exactly zero
//!    bytes.
//!
//! The corpus is one account, four messages, two tiers, with the same file
//! uploaded six times — three sequential repeats and one pair of genuinely
//! concurrent uploads — so "same-account", "same-message", "same-tier" and
//! "cross-tier" are all populated relations rather than words.

#![cfg(feature = "core")]

use osl_privacy_hub::attachment_limits::AttachmentAccountTier;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Barrier, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const TEST_MAIN_PASSWORD: &str = "task-0044c-attachment-key-boundary-password";

/// The compiled production sources this proof is bound to. `include_str!` is
/// compile-time, so the digest below is a property of the binary that ran, not
/// of the working tree at the moment of printing.
const PRODUCTION_SOURCES: [(&str, &str); 6] = [
    (
        "apps/osl-hub/src/attachment_content_key.rs",
        include_str!("../src/attachment_content_key.rs"),
    ),
    (
        "apps/osl-hub/src/broker.rs",
        include_str!("../src/broker.rs"),
    ),
    (
        "apps/osl-hub/src/peer_attachment_io.rs",
        include_str!("../src/peer_attachment_io.rs"),
    ),
    (
        "crates/crypto/src/attachment.rs",
        include_str!("../../../crates/crypto/src/attachment.rs"),
    ),
    (
        "crates/crypto/src/aead.rs",
        include_str!("../../../crates/crypto/src/aead.rs"),
    ),
    (
        "crates/crypto/src/random.rs",
        include_str!("../../../crates/crypto/src/random.rs"),
    ),
];

// ---------------------------------------------------------------------------
// Independent key-boundary observer.
//
// Everything in this module is observer-owned. It shares no code with the
// production key source: `attachment_content_key.rs` contains no hash, no
// fingerprint and no identifier derived from a content key (asserted below),
// so these fingerprints cannot be a restatement of whatever the generator
// already believed.
// ---------------------------------------------------------------------------
mod observer {
    use sha2::{Digest, Sha256};

    /// Observer-only domain separation. Nothing in production uses this
    /// string.
    const FINGERPRINT_DOMAIN: &[u8] =
        b"osl-task-0044c/independent-observer/content-key-fingerprint/v1";

    /// A fingerprint of one content key, computed by the observer.
    pub fn content_key_fingerprint(key: &[u8; 32]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(FINGERPRINT_DOMAIN);
        hasher.update([0u8; 1]);
        hasher.update(key);
        let digest = hasher.finalize();
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// Fingerprint of one attachment's 20-byte base-nonce prefix, reported
    /// separately from the key fingerprint so nonce uniqueness can never be
    /// read as key separation.
    pub fn nonce_prefix_fingerprint(prefix: &[u8; 20]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(FINGERPRINT_DOMAIN);
        hasher.update([1u8; 1]);
        hasher.update(prefix);
        let digest = hasher.finalize();
        digest.iter().take(16).map(|b| format!("{b:02x}")).collect()
    }

    /// Hamming distance in bits between two content keys. Independently
    /// drawn 256-bit keys sit near 128; a structured or related pair does
    /// not.
    pub fn hamming_bits(left: &[u8; 32], right: &[u8; 32]) -> u32 {
        left.iter()
            .zip(right.iter())
            .map(|(a, b)| (a ^ b).count_ones())
            .sum()
    }

    /// Total set bits across a pool of keys.
    pub fn pooled_set_bits(keys: &[[u8; 32]]) -> u32 {
        keys.iter()
            .flat_map(|key| key.iter())
            .map(|byte| byte.count_ones())
            .sum()
    }
}

// ---------------------------------------------------------------------------
// Outside compromise oracle.
//
// The oracle is handed one attachment's complete key material and metadata,
// plus every stored ciphertext blob with its public header. It is never
// handed the mapping from keys to blobs, any other attachment's key, or any
// source plaintext. It decrypts with the shipping stream decryptor and
// releases bytes only after that decryptor's `finalize()` accepts the whole
// authenticated stream, so "released" always means "released after
// authentication".
// ---------------------------------------------------------------------------
mod oracle {
    use crypto::attachment::{StreamDecryptor, StreamHeader};
    use sha2::{Digest, Sha256};

    /// Everything an attacker gets from one compromised attachment.
    #[derive(Clone)]
    pub struct ExposedMaterial {
        pub object: String,
        pub content_key: [u8; 32],
        pub content_id: [u8; 16],
        pub attachment_index: u32,
        pub original_filename: String,
        pub mime_type: String,
        pub plaintext_size: u64,
        pub tier: String,
        pub message: String,
    }

    /// One stored ciphertext object, as the service or a disk-image attacker
    /// would see it: opaque bytes plus the plaintext stream header.
    #[derive(Clone)]
    pub struct CiphertextBlob {
        pub object: String,
        pub wire: Vec<u8>,
        pub original_filename: String,
        pub mime_type: String,
        pub plaintext_size: u64,
    }

    impl CiphertextBlob {
        pub fn header(&self) -> StreamHeader {
            StreamHeader::deserialize(&self.wire)
                .expect("stored attachment carries a parseable public header")
                .0
        }

        pub fn header_bytes(&self) -> Vec<u8> {
            self.header().serialize()
        }
    }

    /// Decrypt `wire` with `key`, releasing bytes only if every chunk tag and
    /// the stream finalizer accept. Any failure releases exactly zero bytes.
    pub fn released_bytes(key: &[u8; 32], wire: &[u8]) -> Vec<u8> {
        let (mut decryptor, header_len) =
            match StreamDecryptor::new(crypto::aead::Key::from_bytes(*key), wire) {
                Ok(pair) => pair,
                Err(_) => return Vec::new(),
            };
        let chunk = crypto::attachment::ATTACHMENT_CHUNK_SIZE + crypto::aead::TAG_SIZE;
        let body = &wire[header_len..];
        let mut held = Vec::new();
        let mut offset = 0usize;
        while offset < body.len() {
            let take = std::cmp::min(chunk, body.len() - offset);
            match decryptor.write(&body[offset..offset + take]) {
                // Held, not released: nothing leaves this function until the
                // whole authenticated stream is accepted below.
                Ok(plaintext) => held.extend_from_slice(&plaintext),
                Err(_) => return Vec::new(),
            }
            offset += take;
        }
        if decryptor.finalize().is_err() {
            return Vec::new();
        }
        held
    }

    /// Every key an attacker can reach from `exposed` plus `blob`'s public
    /// header, using only constructions this repository itself ships or that
    /// need no secret at all. Each entry is `(label, key)`.
    pub fn reachable_keys(
        exposed: &ExposedMaterial,
        blob: &CiphertextBlob,
    ) -> Vec<(String, [u8; 32])> {
        let header = blob.header();
        let header_bytes = blob.header_bytes();
        let mut candidates = Vec::new();

        // (a) The exposed key itself.
        candidates.push(("exposed-key-direct".to_owned(), exposed.content_key));

        // (b) The repository's own attachment key-wrap, driven by the target's
        //     public content id and index. If any attachment's key is wrapped
        //     under another attachment's key, this reproduces it.
        if let Ok(wrapped) = crypto::attachment::wrap_attachment_key(
            &crypto::aead::Key::from_bytes(exposed.content_key),
            &header.content_id,
            header.attachment_index,
        ) {
            candidates.push((
                "sibling-wrap-of-exposed-key".to_owned(),
                *wrapped.as_bytes(),
            ));
        }

        // (c) Public-metadata-only derivations. These use no key material at
        //     all: if a content key is a function of what the wire already
        //     shows, one of these finds it.
        candidates.push((
            "public-metadata-name-mime-size".to_owned(),
            sha256_of(&[
                b"osl-attachment-content-key",
                blob.original_filename.as_bytes(),
                blob.mime_type.as_bytes(),
                &blob.plaintext_size.to_be_bytes(),
            ]),
        ));
        candidates.push((
            "public-metadata-content-id".to_owned(),
            sha256_of(&[
                b"osl-attachment-content-key",
                &header.content_id,
                &header.attachment_index.to_be_bytes(),
                &header.plaintext_len.to_be_bytes(),
            ]),
        ));
        candidates.push((
            "public-stream-header".to_owned(),
            sha256_of(&[&header_bytes]),
        ));
        candidates.push((
            "public-nonce-prefix".to_owned(),
            sha256_of(&[&header.base_nonce_prefix]),
        ));

        // (d) Cheap mixes of the exposed key with the target's public facts.
        candidates.push((
            "exposed-key-mixed-with-target-content-id".to_owned(),
            sha256_of(&[&exposed.content_key, &header.content_id]),
        ));
        candidates.push((
            "exposed-key-mixed-with-target-header".to_owned(),
            sha256_of(&[&exposed.content_key, &header_bytes]),
        ));
        if let Ok(derived) =
            crypto::hkdf::derive_32(&exposed.content_key, b"attachment-key-wrap", &header_bytes)
        {
            candidates.push(("exposed-key-hkdf-over-target-header".to_owned(), derived));
        }
        candidates
    }

    /// Derivations in [`reachable_keys`] that need no key material at all.
    pub fn is_public_only(label: &str) -> bool {
        label.starts_with("public-")
    }

    fn sha256_of(parts: &[&[u8]]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        for part in parts {
            hasher.update((part.len() as u64).to_be_bytes());
            hasher.update(part);
        }
        let digest = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        out
    }
}

// ---------------------------------------------------------------------------
// Isolated account fixture. No network: this proof needs the sender-side seal
// path (draw the key, stream-encrypt the file) and nothing else.
// ---------------------------------------------------------------------------

struct TestStorage {
    root: PathBuf,
}

impl TestStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-0044c-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create isolated OSL test root");
        keystore::set_base_dir_override(Some(root.clone()));
        ipc::main_password::set_file_storage_key(None);
        ipc::main_password::set_main_password(&root, TEST_MAIN_PASSWORD)
            .expect("set isolated OSL main password");
        Self { root }
    }

    fn account(&self, name: &str) -> PathBuf {
        let dir = self.root.join(name);
        fs::create_dir(&dir).expect("create isolated OSL account dir");
        fs::write(
            dir.join("keyserver.json"),
            serde_json::to_vec(&json!({ "cipher_store_url": "http://127.0.0.1:9/" })).unwrap(),
        )
        .expect("write isolated cipher-store configuration");
        dir
    }

    fn local_root(&self, name: &str) -> PathBuf {
        let dir = self.root.join(format!("{name}-local"));
        fs::create_dir(&dir).expect("create isolated OSL local data root");
        dir
    }

    fn activate(dir: &Path) {
        keystore::set_active_account_dir(Some(dir.to_owned()));
    }
}

impl Drop for TestStorage {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Peer {
    dir: PathBuf,
    local_root: PathBuf,
    identity_id: String,
    core: osl_privacy_hub::core_bridge::HubCoreState,
    security: osl_privacy_hub::security::HubSecurityState,
    broker: osl_privacy_hub::broker::HubBrokerState,
    friend_code: String,
}

impl Peer {
    fn new(storage: &TestStorage, name: &str) -> Self {
        let dir = storage.account(name);
        let local_root = storage.local_root(name);
        let identity = keystore::generate_identity(format!("osl-{name}-0044c"));
        let identity_id = identity.user_id.clone();
        let core = osl_privacy_hub::core_bridge::HubCoreState::default();
        *core.osl.identity.lock().unwrap() = Some(identity);
        *core.osl.keyserver.lock().unwrap() =
            Some(keystore::KeyServerClient::new("http://127.0.0.1:9/").unwrap());
        TestStorage::activate(&dir);
        let exported =
            osl_privacy_hub::security::export_friend_code(&core).expect("export friend code");
        Self {
            dir,
            local_root,
            identity_id,
            core,
            security: osl_privacy_hub::security::HubSecurityState::default(),
            broker: osl_privacy_hub::broker::HubBrokerState::default(),
            friend_code: exported.friend_code,
        }
    }

    fn activate(&self) {
        TestStorage::activate(&self.dir);
    }

    fn open_context_to(&self, other_code: &str) {
        self.activate();
        let friend = osl_privacy_hub::security::add_friend_code(
            &self.core,
            &self.security,
            other_code.to_owned(),
            Some("0044c peer".to_owned()),
        )
        .expect("add friend code");
        osl_privacy_hub::security::verify_friend_safety_number(
            &self.core,
            &self.security,
            friend.person_id.clone(),
            friend.safety_number.clone(),
        )
        .expect("verify safety number");
        let binding = osl_privacy_hub::security::manual_peer_binding(&self.core, friend.person_id)
            .expect("manual peer binding");
        let activated = osl_privacy_hub::broker::activate_owned_osl_chat_context(
            &self.broker,
            &self.identity_id,
            binding,
        )
        .expect("activate OSL chat context");
        osl_privacy_hub::security::set_friend_account_reach_choice(
            &self.security,
            activated.person_id.clone(),
            "osl-chat".to_owned(),
            "osl-main".to_owned(),
            true,
        )
        .expect("allow OSL Chat account reach");
        osl_privacy_hub::security::set_manual_peer_scope_permission(
            &self.core,
            &self.security,
            "osl-chat",
            "osl-main",
            activated.person_id.clone(),
            activated.scope.clone(),
            true,
        )
        .expect("approve manual peer scope");
        osl_privacy_hub::security::set_scope_security(&self.security, activated.scope, 3600, true)
            .expect("enable decrypted display for this scope");
    }

    fn set_tier(&self, tier: AttachmentAccountTier) {
        let (state, raw) = match tier {
            AttachmentAccountTier::Free => (keystore::LicenseState::Free, "Unconfigured"),
            AttachmentAccountTier::Pro => (keystore::LicenseState::Paid, "ACTIVE"),
        };
        *self
            .core
            .osl
            .license_state
            .lock()
            .expect("license state mutex poisoned") = keystore::LicenseStateDto {
            state,
            raw_status: raw.to_owned(),
            current_period_end: None,
            last_validated_at: None,
        };
    }
}

// ---------------------------------------------------------------------------
// Corpus
// ---------------------------------------------------------------------------

/// One sealed attachment as this proof tracks it.
struct Attachment {
    object: String,
    message: String,
    tier: &'static str,
    source: &'static str,
    original_filename: String,
    mime_type: String,
    plaintext_size: u64,
    content_key: [u8; 32],
    content_id: [u8; 16],
    attachment_index: u32,
    base_nonce_prefix: [u8; 20],
    wire: Vec<u8>,
    concurrent: bool,
}

/// A fixture source file: deterministic, non-sparse, high-entropy filler so a
/// released byte is unmistakable and a zero-release is not an artefact of a
/// blank file.
fn write_source(path: &Path, seed: u64, len: usize) {
    let mut file = File::create(path).expect("create fixture source");
    let mut state = seed | 1;
    let mut block = vec![0u8; 8192];
    let mut written = 0usize;
    while written < len {
        for slot in block.iter_mut() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *slot = (state >> 24) as u8;
        }
        let take = std::cmp::min(block.len(), len - written);
        file.write_all(&block[..take])
            .expect("write fixture source");
        written += take;
    }
    file.sync_all().expect("flush fixture source");
}

/// Drive the shipping sender-side seal for one attachment: the broker draws
/// the content key and content id, and `peer_attachment_io` streams the file
/// through the production AEAD. Nothing here supplies, seeds or overrides a
/// key.
fn seal_one(
    sender: &Peer,
    corpus_dir: &Path,
    message: &str,
    tier: AttachmentAccountTier,
    source_label: &'static str,
    source_path: &Path,
    filename: &str,
    concurrent: bool,
) -> Attachment {
    sender.activate();
    let mut source = File::open(source_path).expect("open fixture source");
    let plaintext_size = source.metadata().expect("source metadata").len();

    let plan = osl_privacy_hub::broker::begin_osl_chat_attachment(
        &sender.core,
        &sender.broker,
        filename.to_owned(),
        plaintext_size,
        false,
    )
    .expect("begin OSL chat attachment");

    let content_key = plan.attachment_key;
    let content_id = plan.content_id;
    let attachment_index = 0u32;

    let staged = osl_privacy_hub::peer_attachment_io::encrypt_file_for_account_tier(
        &sender.local_root,
        &mut source,
        &plan.original_filename,
        &plan.mime_type,
        tier,
        crypto::aead::Key::from_bytes(content_key),
        content_id.to_vec(),
        attachment_index,
    )
    .expect("stream-encrypt the attachment");

    let wire = fs::read(staged.path()).expect("read the sealed attachment");
    let object = plan.attachment_id.clone();
    let stored = corpus_dir.join(format!("{object}.osl"));
    fs::write(&stored, &wire).expect("store the sealed attachment in the corpus");
    osl_privacy_hub::peer_attachment_io::remove_staged_file(staged).expect("clear sealed staging");

    let header = crypto::attachment::StreamHeader::deserialize(&wire)
        .expect("sealed attachment carries a parseable header")
        .0;

    Attachment {
        object,
        message: message.to_owned(),
        tier: match tier {
            AttachmentAccountTier::Free => "free",
            AttachmentAccountTier::Pro => "pro",
        },
        source: source_label,
        original_filename: plan.original_filename.clone(),
        mime_type: plan.mime_type.clone(),
        plaintext_size,
        content_key,
        content_id,
        attachment_index,
        base_nonce_prefix: header.base_nonce_prefix,
        wire,
        concurrent,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn short(object: &str) -> String {
    let tail: String = object.chars().rev().take(8).collect();
    format!("...{}", tail.chars().rev().collect::<String>())
}

fn source_digest() -> String {
    let mut hasher = Sha256::new();
    for (path, text) in PRODUCTION_SOURCES {
        hasher.update((path.len() as u64).to_be_bytes());
        hasher.update(path.as_bytes());
        hasher.update((text.len() as u64).to_be_bytes());
        hasher.update(text.as_bytes());
    }
    hex(&hasher.finalize())[..12].to_owned()
}

/// Digest of one compiled production source, so a throwaway build says which
/// object it changed rather than only that something changed.
fn file_digest(path: &str) -> String {
    let text = PRODUCTION_SOURCES
        .iter()
        .find(|(name, _)| *name == path)
        .map(|(_, text)| *text)
        .expect("named production source is compiled into this test");
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hex(&hasher.finalize())[..12].to_owned()
}

fn git_head() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .filter(|head| !head.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

// ---------------------------------------------------------------------------
// The proof
// ---------------------------------------------------------------------------

#[test]
fn task_0044c_every_attachment_key_is_separate_and_its_compromise_is_contained() {
    let revision = format!("{}+sources-{}", git_head(), source_digest());
    eprintln!(
        "TASK0044C revision={revision} key_source={} broker={} stream_aead={}",
        file_digest("apps/osl-hub/src/attachment_content_key.rs"),
        file_digest("apps/osl-hub/src/broker.rs"),
        file_digest("crates/crypto/src/attachment.rs"),
    );

    // ---- Guard: the key source must not be able to certify itself. --------
    let generator = PRODUCTION_SOURCES[0].1;
    let generator_code: String = generator
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in ["Sha256", "sha2", "fingerprint", "digest"] {
        assert!(
            !generator_code.contains(forbidden),
            "TASK0044C the production key source must not fingerprint its own keys \
             (found `{forbidden}` in apps/osl-hub/src/attachment_content_key.rs)"
        );
    }
    assert!(
        generator_code.contains("crypto::random::random_bytes"),
        "TASK0044C the production content key must come from the OS CSPRNG"
    );
    // No cooperative key source: nothing may hand this module a key, and no
    // environment or caller-supplied value may steer the draw.
    for seam in ["override", "env::var", "set_content_key", "seed"] {
        assert!(
            !generator_code.contains(seam),
            "TASK0044C the production key source must have no test seam (found `{seam}`)"
        );
    }

    let storage = TestStorage::new();
    let alice = Peer::new(&storage, "alice");
    let bob = Peer::new(&storage, "bob");
    alice.open_context_to(&bob.friend_code);
    bob.open_context_to(&alice.friend_code);

    let sources = storage.root.join("sources");
    fs::create_dir(&sources).expect("create fixture source dir");
    let corpus_dir = storage.root.join("corpus");
    fs::create_dir(&corpus_dir).expect("create ciphertext corpus dir");

    let repeated = sources.join("quarterly.pdf");
    write_source(&repeated, 0x0044_c001, 41_137);
    let notes = sources.join("notes.txt");
    write_source(&notes, 0x0044_c002, 12_289);
    let photo = sources.join("photo.png");
    write_source(&photo, 0x0044_c003, 63_871);
    let clip = sources.join("clip.mp4");
    write_source(&clip, 0x0044_c004, 97_003);
    let sheet = sources.join("sheet.csv");
    write_source(&sheet, 0x0044_c005, 25_601);

    let mut corpus: Vec<Attachment> = Vec::new();

    // Message 1, Free tier: three distinct files in one message.
    alice.set_tier(AttachmentAccountTier::Free);
    for (label, path, name) in [
        ("repeated", &repeated, "quarterly.pdf"),
        ("notes", &notes, "notes.txt"),
        ("photo", &photo, "photo.png"),
    ] {
        corpus.push(seal_one(
            &alice,
            &corpus_dir,
            "msg-free-1",
            AttachmentAccountTier::Free,
            label,
            path,
            name,
            false,
        ));
    }

    // Message 2, Free tier: the SAME file uploaded twice inside one message.
    // Identical public metadata, identical bytes — the keys must still differ.
    for _ in 0..2 {
        corpus.push(seal_one(
            &alice,
            &corpus_dir,
            "msg-free-2",
            AttachmentAccountTier::Free,
            "repeated",
            &repeated,
            "quarterly.pdf",
            false,
        ));
    }

    // Message 3, Pro tier: the same repeated file again, cross-tier, plus two
    // more distinct files.
    alice.set_tier(AttachmentAccountTier::Pro);
    for (label, path, name) in [
        ("repeated", &repeated, "quarterly.pdf"),
        ("clip", &clip, "clip.mp4"),
        ("sheet", &sheet, "sheet.csv"),
    ] {
        corpus.push(seal_one(
            &alice,
            &corpus_dir,
            "msg-pro-1",
            AttachmentAccountTier::Pro,
            label,
            path,
            name,
            false,
        ));
    }

    // Message 4, Pro tier: two uploads of the same file that genuinely
    // overlap. A barrier makes both threads enter the broker draw at the same
    // moment, so a key source that is not per-call would collide here.
    {
        let barrier = Barrier::new(2);
        let results: Mutex<Vec<Attachment>> = Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            for _ in 0..2 {
                scope.spawn(|| {
                    barrier.wait();
                    let sealed = seal_one(
                        &alice,
                        &corpus_dir,
                        "msg-pro-2",
                        AttachmentAccountTier::Pro,
                        "repeated",
                        &repeated,
                        "quarterly.pdf",
                        true,
                    );
                    results
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .push(sealed);
                });
            }
        });
        let mut sealed = results
            .into_inner()
            .unwrap_or_else(|error| error.into_inner());
        sealed.sort_by(|a, b| a.object.cmp(&b.object));
        corpus.extend(sealed);
    }

    // ---- Starvation guards on the corpus itself. --------------------------
    assert!(
        corpus.len() >= 8,
        "TASK0044C the corpus must hold more than one attachment (got {})",
        corpus.len()
    );
    let concurrent_pairs = corpus.iter().filter(|a| a.concurrent).count();
    assert_eq!(
        concurrent_pairs, 2,
        "TASK0044C the concurrent same-file upload pair must be present"
    );
    let repeated_uploads = corpus.iter().filter(|a| a.source == "repeated").count();
    assert!(
        repeated_uploads >= 4,
        "TASK0044C the same file must be uploaded repeatedly (got {repeated_uploads})"
    );
    let tiers: BTreeSet<&str> = corpus.iter().map(|a| a.tier).collect();
    assert_eq!(
        tiers.len(),
        2,
        "TASK0044C the corpus must span both shipping tiers"
    );
    let messages: BTreeSet<String> = corpus.iter().map(|a| a.message.clone()).collect();
    assert!(
        messages.len() >= 3,
        "TASK0044C the corpus must span several messages"
    );
    let multi_attachment_messages = messages
        .iter()
        .filter(|message| corpus.iter().filter(|a| &a.message == *message).count() > 1)
        .count();
    assert!(
        multi_attachment_messages >= 3,
        "TASK0044C same-message attachment pairs must exist"
    );

    let mut failures: Vec<String> = Vec::new();
    // Objects named in a cross-object compromise. Everything outside this set
    // is "unrelated" for the purposes of the throwaway-build reports.
    let mut implicated: BTreeSet<String> = BTreeSet::new();

    // ---- 1. Independent key-boundary observer. ----------------------------
    let mut fingerprints: BTreeMap<String, String> = BTreeMap::new();
    let mut by_fingerprint: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut nonce_fingerprints: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for attachment in &corpus {
        let fingerprint = observer::content_key_fingerprint(&attachment.content_key);
        let nonce_fingerprint = observer::nonce_prefix_fingerprint(&attachment.base_nonce_prefix);
        eprintln!(
            "TASK0044C content_key object={} message={} tier={} source={} concurrent={} \
             bytes={} key_fingerprint={} nonce_prefix_fingerprint={}",
            short(&attachment.object),
            attachment.message,
            attachment.tier,
            attachment.source,
            attachment.concurrent,
            attachment.plaintext_size,
            fingerprint,
            nonce_fingerprint,
        );
        fingerprints.insert(attachment.object.clone(), fingerprint.clone());
        by_fingerprint
            .entry(fingerprint)
            .or_default()
            .push(attachment.object.clone());
        nonce_fingerprints
            .entry(nonce_fingerprint)
            .or_default()
            .push(attachment.object.clone());
    }
    assert_eq!(
        fingerprints.len(),
        corpus.len(),
        "TASK0044C the observer must print one fingerprint per attachment"
    );
    for (fingerprint, objects) in &by_fingerprint {
        if objects.len() > 1 {
            for object in objects {
                let others: Vec<String> = objects
                    .iter()
                    .filter(|other| *other != object)
                    .map(|other| short(other))
                    .collect();
                implicated.insert(object.clone());
                failures.push(format!(
                    "object={} cross-object compromise: content key fingerprint {} is shared with object={}",
                    short(object),
                    &fingerprint[..16],
                    others.join(",")
                ));
            }
        }
    }

    // Nonce uniqueness is reported on its own and is never counted as key
    // separation.
    let duplicate_nonce_prefixes = nonce_fingerprints
        .values()
        .filter(|objects| objects.len() > 1)
        .count();
    eprintln!(
        "TASK0044C observer attachments={} distinct_key_fingerprints={} \
         distinct_nonce_prefixes={} duplicate_nonce_prefixes={} \
         nonce_uniqueness_counts_as_key_separation=false",
        corpus.len(),
        by_fingerprint.len(),
        nonce_fingerprints.len(),
        duplicate_nonce_prefixes,
    );

    // Randomness, not merely distinctness.
    let keys: Vec<[u8; 32]> = corpus.iter().map(|a| a.content_key).collect();
    let mut min_hamming = u32::MAX;
    let mut max_hamming = 0u32;
    let mut pairs = 0usize;
    for (i, left) in keys.iter().enumerate() {
        for right in keys.iter().skip(i + 1) {
            let distance = observer::hamming_bits(left, right);
            min_hamming = min_hamming.min(distance);
            max_hamming = max_hamming.max(distance);
            pairs += 1;
        }
    }
    let pooled_bits = observer::pooled_set_bits(&keys);
    let total_bits = (keys.len() * 256) as u32;
    eprintln!(
        "TASK0044C observer_randomness pairs={pairs} min_hamming_bits={min_hamming} \
         max_hamming_bits={max_hamming} pooled_set_bits={pooled_bits}/{total_bits}"
    );
    assert!(pairs > 0, "TASK0044C the observer must compare key pairs");
    if min_hamming < 64 || max_hamming > 192 {
        failures.push(format!(
            "content keys are not independently random: pairwise Hamming distance range \
             [{min_hamming},{max_hamming}] bits falls outside [64,192]"
        ));
    }
    let low = total_bits / 2 - total_bits / 8;
    let high = total_bits / 2 + total_bits / 8;
    if pooled_bits < low || pooled_bits > high {
        failures.push(format!(
            "pooled content-key bit balance {pooled_bits}/{total_bits} is outside [{low},{high}]"
        ));
    }
    for attachment in &corpus {
        if attachment.content_key == [0u8; 32] {
            failures.push(format!(
                "object={} content key is all zero",
                short(&attachment.object)
            ));
        }
        // A key that is a function of the ciphertext is not a separate key.
        if attachment
            .wire
            .windows(32)
            .any(|window| window == attachment.content_key)
        {
            failures.push(format!(
                "object={} content key appears verbatim inside its own ciphertext",
                short(&attachment.object)
            ));
        }
    }

    // The same file, same filename, same size, same MIME, uploaded six times:
    // if any key were a function of public metadata these would collide.
    let repeated_keys: BTreeSet<[u8; 32]> = corpus
        .iter()
        .filter(|a| a.source == "repeated")
        .map(|a| a.content_key)
        .collect();
    eprintln!(
        "TASK0044C repeated_identical_uploads uploads={repeated_uploads} distinct_keys={}",
        repeated_keys.len()
    );
    if repeated_keys.len() != repeated_uploads {
        failures.push(format!(
            "repeated uploads of one identical file produced {} distinct keys for {} uploads",
            repeated_keys.len(),
            repeated_uploads
        ));
    }

    // ---- 2. Compromise oracle. -------------------------------------------
    let blobs: Vec<oracle::CiphertextBlob> = corpus
        .iter()
        .map(|attachment| oracle::CiphertextBlob {
            object: attachment.object.clone(),
            wire: attachment.wire.clone(),
            original_filename: attachment.original_filename.clone(),
            mime_type: attachment.mime_type.clone(),
            plaintext_size: attachment.plaintext_size,
        })
        .collect();
    let plaintexts: BTreeMap<&str, Vec<u8>> = [
        ("repeated", fs::read(&repeated).unwrap()),
        ("notes", fs::read(&notes).unwrap()),
        ("photo", fs::read(&photo).unwrap()),
        ("clip", fs::read(&clip).unwrap()),
        ("sheet", fs::read(&sheet).unwrap()),
    ]
    .into_iter()
    .collect();

    let mut exposures = 0usize;
    let mut decrypted_objects = 0usize;
    let mut readable_objects: BTreeSet<String> = BTreeSet::new();
    let mut cross_key_trials = 0usize;
    let mut cross_key_zero_release = 0usize;
    let mut derivation_trials = 0usize;
    let mut derivation_zero_release = 0usize;
    let mut public_only_trials = 0usize;
    let mut relation_zero: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut wrong_key_zero_release = 0usize;
    let mut flipped_bit_zero_release = 0usize;
    let mut reordered_part_zero_release = 0usize;
    let mut truncated_zero_release = 0usize;
    let mut cross_object_bytes_released = 0usize;

    for attachment in &corpus {
        let exposed = oracle::ExposedMaterial {
            object: attachment.object.clone(),
            content_key: attachment.content_key,
            content_id: attachment.content_id,
            attachment_index: attachment.attachment_index,
            original_filename: attachment.original_filename.clone(),
            mime_type: attachment.mime_type.clone(),
            plaintext_size: attachment.plaintext_size,
            tier: attachment.tier.to_owned(),
            message: attachment.message.clone(),
        };
        exposures += 1;

        for blob in &blobs {
            let matching = blob.object == exposed.object;
            for (label, candidate) in oracle::reachable_keys(&exposed, blob) {
                let released = oracle::released_bytes(&candidate, &blob.wire);
                let direct = label == "exposed-key-direct";
                if oracle::is_public_only(&label) {
                    public_only_trials += 1;
                }
                if matching && direct {
                    // The one attachment the exposure is supposed to buy.
                    let expected = plaintexts
                        .get(attachment.source)
                        .expect("fixture source plaintext");
                    if released.as_slice() == expected.as_slice() {
                        decrypted_objects += 1;
                        readable_objects.insert(blob.object.clone());
                    } else {
                        failures.push(format!(
                            "object={} the right key did not release its exact file \
                             (released {} bytes, expected {})",
                            short(&blob.object),
                            released.len(),
                            expected.len()
                        ));
                    }
                    continue;
                }
                if matching {
                    // Derivations against the exposed attachment's own blob
                    // are not a cross-object result; they are only counted
                    // when they wrongly succeed.
                    if !released.is_empty() && candidate != exposed.content_key {
                        failures.push(format!(
                            "object={} the content key is reachable from public data by `{label}`",
                            short(&blob.object)
                        ));
                    }
                    continue;
                }

                // Every trial from here down is cross-object.
                let target = corpus
                    .iter()
                    .find(|other| other.object == blob.object)
                    .expect("blob belongs to the corpus");
                if direct {
                    cross_key_trials += 1;
                } else {
                    derivation_trials += 1;
                }
                if released.is_empty() {
                    if direct {
                        cross_key_zero_release += 1;
                        *relation_zero.entry("same_account").or_default() += 1;
                        if target.message == exposed.message {
                            *relation_zero.entry("same_message").or_default() += 1;
                        }
                        if target.tier == exposed.tier {
                            *relation_zero.entry("same_tier").or_default() += 1;
                        } else {
                            *relation_zero.entry("cross_tier").or_default() += 1;
                        }
                    } else {
                        derivation_zero_release += 1;
                    }
                } else {
                    cross_object_bytes_released += released.len();
                    implicated.insert(blob.object.clone());
                    implicated.insert(exposed.object.clone());
                    failures.push(format!(
                        "object={} cross-object compromise: exposing object={} \
                         (tier={} message={}) released {} bytes of object={} \
                         (tier={} message={}) via `{label}`",
                        short(&blob.object),
                        short(&exposed.object),
                        exposed.tier,
                        exposed.message,
                        released.len(),
                        short(&blob.object),
                        target.tier,
                        target.message,
                    ));
                }
            }
        }

        // Tamper cases, each against the exposed attachment's own blob.
        let own = blobs
            .iter()
            .find(|blob| blob.object == exposed.object)
            .expect("exposed attachment is in the corpus");

        let mut wrong = [0u8; 32];
        wrong.copy_from_slice(&crypto::random::random_bytes(32));
        assert_ne!(wrong, exposed.content_key);
        if oracle::released_bytes(&wrong, &own.wire).is_empty() {
            wrong_key_zero_release += 1;
        } else {
            failures.push(format!(
                "object={} a wrong key released plaintext",
                short(&own.object)
            ));
        }

        let header_len = own.header_bytes().len();
        let mut flipped = own.wire.clone();
        let flip_at = header_len + 512;
        flipped[flip_at] ^= 0x01;
        if oracle::released_bytes(&exposed.content_key, &flipped).is_empty() {
            flipped_bit_zero_release += 1;
        } else {
            failures.push(format!(
                "object={} a flipped ciphertext bit released plaintext",
                short(&own.object)
            ));
        }

        let part = crypto::attachment::ATTACHMENT_CHUNK_SIZE + crypto::aead::TAG_SIZE;
        let mut reordered = own.wire.clone();
        assert!(
            reordered.len() >= header_len + 2 * part,
            "TASK0044C a sealed attachment must hold at least two parts to reorder"
        );
        let first: Vec<u8> = reordered[header_len..header_len + part].to_vec();
        let second: Vec<u8> = reordered[header_len + part..header_len + 2 * part].to_vec();
        reordered[header_len..header_len + part].copy_from_slice(&second);
        reordered[header_len + part..header_len + 2 * part].copy_from_slice(&first);
        if oracle::released_bytes(&exposed.content_key, &reordered).is_empty() {
            reordered_part_zero_release += 1;
        } else {
            failures.push(format!(
                "object={} a reordered part released plaintext",
                short(&own.object)
            ));
        }

        let truncated = own.wire[..own.wire.len() - part].to_vec();
        if oracle::released_bytes(&exposed.content_key, &truncated).is_empty() {
            truncated_zero_release += 1;
        } else {
            failures.push(format!(
                "object={} a truncated stream released plaintext",
                short(&own.object)
            ));
        }
    }

    let expected_cross = corpus.len() * (corpus.len() - 1);
    eprintln!(
        "TASK0044C compromise_oracle exposed_objects={exposures} decrypted_objects={decrypted_objects} \
         cross_key_trials={cross_key_trials} cross_key_zero_release={cross_key_zero_release} \
         derivation_trials={derivation_trials} derivation_zero_release={derivation_zero_release} \
         public_only_derivation_trials={public_only_trials} \
         wrong_key_zero_release={wrong_key_zero_release} \
         flipped_bit_zero_release={flipped_bit_zero_release} \
         reordered_part_zero_release={reordered_part_zero_release} \
         truncated_zero_release={truncated_zero_release}"
    );
    eprintln!(
        "TASK0044C relation_zero_release same_account={} same_message={} same_tier={} cross_tier={}",
        relation_zero.get("same_account").copied().unwrap_or(0),
        relation_zero.get("same_message").copied().unwrap_or(0),
        relation_zero.get("same_tier").copied().unwrap_or(0),
        relation_zero.get("cross_tier").copied().unwrap_or(0),
    );
    eprintln!(
        "TASK0044C positive_attachments_readable={} implicated_objects={} \
         unrelated_positive_attachments_readable={}",
        readable_objects.len(),
        implicated.len(),
        readable_objects
            .iter()
            .filter(|object| !implicated.contains(*object))
            .count(),
    );

    // ---- Starvation guards on the oracle and every tamper case. -----------
    assert_eq!(
        exposures,
        corpus.len(),
        "TASK0044C every attachment must be exposed in turn"
    );
    assert_eq!(
        cross_key_trials, expected_cross,
        "TASK0044C the oracle must try every exposed key against every other object"
    );
    assert!(
        derivation_trials >= expected_cross * 6,
        "TASK0044C the oracle must try the reachable-key derivations against every other object \
         (got {derivation_trials})"
    );
    assert!(
        public_only_trials >= corpus.len() * corpus.len() * 4,
        "TASK0044C the oracle must try the public-only derivations against every object \
         (got {public_only_trials})"
    );
    assert_eq!(
        wrong_key_zero_release,
        corpus.len(),
        "TASK0044C the wrong-key case must run for every attachment"
    );
    assert_eq!(
        flipped_bit_zero_release,
        corpus.len(),
        "TASK0044C the flipped-bit case must run for every attachment"
    );
    assert_eq!(
        reordered_part_zero_release,
        corpus.len(),
        "TASK0044C the reordered-part case must run for every attachment"
    );
    assert_eq!(
        truncated_zero_release,
        corpus.len(),
        "TASK0044C the truncated-stream case must run for every attachment"
    );

    if !failures.is_empty() {
        for failure in &failures {
            eprintln!("TASK0044C FAIL {failure}");
        }
    }
    eprintln!(
        "TASK0044C finish revision={revision} attachments={} messages={} tiers={} \
         repeated_identical_uploads={} concurrent_same_file_uploads={} \
         distinct_content_keys={} exposures={} decrypted_per_exposure={} \
         cross_object_bytes_released={cross_object_bytes_released} failures={}",
        corpus.len(),
        messages.len(),
        tiers.len(),
        repeated_uploads,
        concurrent_pairs,
        by_fingerprint.len(),
        exposures,
        if exposures == 0 {
            0
        } else {
            decrypted_objects / exposures
        },
        failures.len(),
    );

    assert!(
        failures.is_empty(),
        "TASK0044C {} failure(s): {}",
        failures.len(),
        failures.join(" | ")
    );
    assert_eq!(
        decrypted_objects, exposures,
        "TASK0044C each exposure must decrypt exactly one attachment"
    );
    assert_eq!(
        cross_key_zero_release, expected_cross,
        "TASK0044C every cross-object trial must release exactly zero bytes"
    );
    assert_eq!(
        derivation_zero_release, derivation_trials,
        "TASK0044C every reachable-key derivation must release exactly zero bytes"
    );
    assert_eq!(
        by_fingerprint.len(),
        corpus.len(),
        "TASK0044C every attachment must have a different content key"
    );
}
