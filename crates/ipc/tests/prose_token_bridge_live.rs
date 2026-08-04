//! B0-01 phase 1 — the Observable Delta Contract for the BRIDGE send path,
//! executed against the **deployed** Worker at ciphers.oslprivacy.com.
//!
//! Run with:
//!   OSL_LIVE_TESTS=1 cargo test -p ipc --test prose_token_bridge_live -- --nocapture
//!
//! WHY THIS FILE EXISTS SEPARATELY FROM `prose_token_live.rs`
//!
//! `prose_token_live.rs` asserts the *destination* contract: a 32-hex
//! client-derived blob id, and a burn only the sender's send key can perform.
//! The Worker implementing that contract (`cipher-store-cf/`) has never been
//! deployed, so those assertions describe where the system is going, not where
//! it is. They are deliberately left alone and are expected to fail against
//! production until Phase 2 ships. Do not "fix" them by relaxing them — the
//! `a_foreign_send_key_cannot_burn` case is the security property this whole
//! design exists for, and a green suite that no longer checks it would be worth
//! less than a red one that does.
//!
//! This file asserts only what the bridge actually promises, and every
//! assertion below is a status code from the real service. There is no mock
//! anywhere in this file, on purpose:
//! `apps/osl-hub/tests/native_discord_receive_e2e.rs` hand-rolls a server
//! matching `("POST", "/v1/blob")`, so it validated the client against a server
//! built to agree with it — and reported "12 passed" while production could not
//! serve a single one of those requests.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use ipc::cipher_store_client::{
    BlobCapabilities, BlobObjectClass, CipherStoreClient, CipherStoreError,
    DEFAULT_CIPHER_STORE_BASE_URL,
};
use ipc::prose_token::{prose_token_recv, prose_token_send, ProseTokenSendKeys};
use ipc::scope::{ScopeInput, ScopeKind};

const MESSAGE_KEY: [u8; 32] = [0x11; 32];
const SEND_KEY: [u8; 32] = [0x22; 32];
const CONVERSATION_KEY: [u8; 32] = [0x33; 32];

/// One hour — the shortest TTL the deployed Worker accepts, so anything this
/// suite leaves behind costs the store an hour rather than a week.
const TTL_1H: u32 = 3600;

fn send_keys() -> ProseTokenSendKeys<'static> {
    ProseTokenSendKeys {
        message_key: &MESSAGE_KEY,
        send_key: &SEND_KEY,
        conversation_key: &CONVERSATION_KEY,
    }
}

fn live_tests_enabled() -> bool {
    std::env::var("OSL_LIVE_TESTS").ok().as_deref() == Some("1")
}

/// Empty config dir, so `resolve_cipher_store_base_url` falls through to the
/// built-in production default. The point of this suite is that the address is
/// not a test fixture.
fn config_dir() -> std::path::PathBuf {
    tempfile::tempdir().unwrap().into_path()
}

fn production() -> CipherStoreClient {
    CipherStoreClient::new(DEFAULT_CIPHER_STORE_BASE_URL).expect("client builds")
}

fn dm_scope() -> ScopeInput {
    ScopeInput {
        kind: ScopeKind::Dm,
        id: "999000111222333444".to_string(),
        server_id: None,
        channel_id: None,
    }
}

fn fake_wire(payload: &[u8]) -> String {
    format!("DPC0::{}", B64.encode(payload))
}

fn detection_key() -> [u8; 32] {
    ipc::prose_token::derive_detection_key(&[0x44; 32]).expect("test secret derives detector")
}

/// Print the HTTP status a live call produced, so a failure reports the number
/// the contract is written in rather than a Rust enum name.
fn status_of(err: &CipherStoreError) -> String {
    match err {
        CipherStoreError::Status { status, body } => format!("HTTP {status} {body}"),
        CipherStoreError::NotFound => "HTTP 404 (client folds 404 to NotFound)".to_string(),
        CipherStoreError::RateLimited => "HTTP 429".to_string(),
        other => format!("no status: {other}"),
    }
}

// ---------------------------------------------------------------------------
// ODC: action — a real POST /v1/blob against production must return 2xx
// ---------------------------------------------------------------------------

/// `must_change: http_status == 2xx`
///
/// A real message travels: `prose_token_send` uploads to the real service and
/// `prose_token_recv` recovers byte-identical plaintext having been given
/// nothing but the cover text.
#[test]
#[ignore = "live: needs OSL_LIVE_TESTS=1 + network to ciphers.oslprivacy.com. Run: cargo test -p ipc --test prose_token_bridge_live -- --ignored"]
fn a_message_actually_travels_through_production() {
    if !live_tests_enabled() {
        eprintln!("skipping live test (set OSL_LIVE_TESTS=1 to run)");
        return;
    }
    let dir = config_dir();
    let scope = dm_scope();
    let key = detection_key();

    let payload: Vec<u8> = (0u8..=200).collect();
    let wire = fake_wire(&payload);

    let sent = prose_token_send(&dir, &scope, &key, send_keys(), &wire, TTL_1H)
        .expect("send reaches the live store");
    println!("[send] blob_id    = {}", sent.blob_id);
    println!("[send] cover_text = {}", sent.cover_text);
    println!("[send] expires_at = {}", sent.expires_at);

    // The deployed Worker assigns the id, so it is 16 hex chars, not the 32 the
    // destination protocol produces. Asserting the bridge shape keeps this file
    // honest about which of the two protocols it is testing.
    assert_eq!(sent.blob_id.len(), 16, "deployed Worker assigns an 8-byte id");
    assert!(sent.blob_id.bytes().all(|b| b.is_ascii_hexdigit()));
    assert!(
        !sent.cover_text.starts_with("DPC"),
        "cover must not leak the wire marker"
    );

    let recv = prose_token_recv(&dir, &scope, &key, &sent.cover_text)
        .expect("recv reaches the live store")
        .expect("the cover text carried a recoverable token");
    println!("[recv] blob_id    = {}", recv.blob_id);
    assert_eq!(recv.blob_id, sent.blob_id);
    assert_eq!(
        recv.wire, wire,
        "plaintext survived the round trip byte for byte"
    );
}

// ---------------------------------------------------------------------------
// ODC: mutants. Each is a status code from the live service.
//
// These drive the store directly rather than through `prose_token_*`, because
// the credential being mutated is a store credential: going through the send
// path would only prove the client can be made to build a bad request, whereas
// what has to be proven is that the deployed service refuses one.
// ---------------------------------------------------------------------------

/// Upload one object the honest way and hand back `(id, fetch token)`.
fn upload_live(body: &[u8], token: &[u8; 16]) -> String {
    let result = production()
        .upload(body, TTL_1H, token)
        .expect("baseline upload succeeds against production");
    println!("[baseline] HTTP 201 id = {}", result.id_hex);
    result.id_hex
}

/// `mutant: wrong capability digest -> 4xx`
#[test]
#[ignore = "live: needs OSL_LIVE_TESTS=1 + network to ciphers.oslprivacy.com. Run: cargo test -p ipc --test prose_token_bridge_live -- --ignored"]
fn a_wrong_capability_is_refused_by_production() {
    if !live_tests_enabled() {
        eprintln!("skipping live test (set OSL_LIVE_TESTS=1 to run)");
        return;
    }
    let token = [0x5Au8; 16];
    let id = upload_live(b"wrong-capability-mutant", &token);

    let mut wrong = token;
    wrong[0] ^= 0x01;
    let err = production()
        .fetch_legacy_token(&id, &wrong)
        .expect_err("a wrong capability must not read the object");
    println!("[mutant wrong-capability] {}", status_of(&err));
    match &err {
        CipherStoreError::Status { status, .. } => {
            assert!((400..500).contains(status), "expected 4xx, got {status}")
        }
        other => panic!("expected a 4xx status, got {other}"),
    }

    // The correct capability still reads it, so the refusal above was about the
    // capability and not about the object having gone missing.
    let bytes = production()
        .fetch_legacy_token(&id, &token)
        .expect("the real capability still reads the object");
    assert_eq!(bytes, b"wrong-capability-mutant");
    let _ = production().delete(&id, &token);
}

/// `mutant: absent blob id -> 4xx`
#[test]
#[ignore = "live: needs OSL_LIVE_TESTS=1 + network to ciphers.oslprivacy.com. Run: cargo test -p ipc --test prose_token_bridge_live -- --ignored"]
fn an_absent_blob_id_is_refused_by_production() {
    if !live_tests_enabled() {
        eprintln!("skipping live test (set OSL_LIVE_TESTS=1 to run)");
        return;
    }
    let err = production()
        .fetch_legacy_token("0000000000000000", &[0xABu8; 16])
        .expect_err("an id that was never uploaded must not resolve");
    println!("[mutant absent-blob-id] {}", status_of(&err));
    assert!(
        matches!(err, CipherStoreError::NotFound)
            || matches!(&err, CipherStoreError::Status { status, .. } if (400..500).contains(status)),
        "expected 4xx / NotFound, got {err}"
    );
}

/// `mutant: absent grant -> 503 or 4xx`
///
/// Production has no grant concept — the grant belongs to the undeployed
/// capability Worker. The live equivalent of "no grant" is therefore the
/// destination protocol itself: `upload_pointer` sends capability digests and
/// no fetch token, and production rejects it.
///
/// This is the assertion that starts failing the day Phase 2 deploys, which is
/// exactly when someone must be forced back here to delete the bridge.
#[test]
#[ignore = "live: needs OSL_LIVE_TESTS=1 + network to ciphers.oslprivacy.com. Run: cargo test -p ipc --test prose_token_bridge_live -- --ignored"]
fn the_destination_protocol_is_still_not_deployed() {
    if !live_tests_enabled() {
        eprintln!("skipping live test (set OSL_LIVE_TESTS=1 to run)");
        return;
    }
    let caps = BlobCapabilities {
        fetch_cap: [0x01; 16],
        ack_cap: [0x02; 16],
        manage_cap: [0x03; 16],
        delivery_tag: [0x04; 16],
    };
    let err = production()
        .upload_pointer(
            b"destination-protocol-probe",
            TTL_1H,
            &[0x05; 16],
            caps,
            BlobObjectClass::SingleAck,
            // No grant: production has no grant verifier on this route at all,
            // and phase 2 could not mint a blob-upload grant even with one —
            // see `a_link_creation_grant_cannot_be_spent_on_a_blob_upload`.
            None,
        )
        .expect_err("production does not implement the capability upload");
    println!("[mutant absent-grant / destination-protocol] {}", status_of(&err));
    match &err {
        CipherStoreError::Status { status, .. } => assert!(
            (400..500).contains(status) || *status == 503,
            "expected 4xx or 503, got {status}"
        ),
        other => panic!("expected a status, got {other}"),
    }
}
