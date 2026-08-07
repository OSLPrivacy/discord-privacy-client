use std::collections::BTreeMap;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use ipc::cipher_store_client::{CipherStoreClient, DEFAULT_CIPHER_STORE_BASE_URL};
use ipc::prose_token::{
    prose_token_pointer_arrival, prose_token_send, prose_token_wire_from_object, ProseTokenSendKeys,
};
use ipc::scope::{ScopeInput, ScopeKind};
use osl_privacy_hub::eager_fetch::{
    CipherStoreTransport, EagerFetchDriver, EncryptedBurnQueue, LocalMessageStore, PointerArrival,
};
use osl_privacy_hub::osl_chat_delivery::{
    pointer_arrival_from_osl_chat_cover, receive_osl_chat_cover_pointer, receive_osl_chat_pointer,
};

const MESSAGE_KEY: [u8; 32] = [0x11; 32];
const SEND_KEY: [u8; 32] = [0x22; 32];
const CONVERSATION_KEY: [u8; 32] = [0x33; 32];
const TASK_WORDS: &str = "task 4062 words open exactly";

fn scope() -> ScopeInput {
    ScopeInput {
        kind: ScopeKind::Dm,
        id: "task-4062-dm".to_owned(),
        server_id: None,
        channel_id: Some("task-4062-channel".to_owned()),
    }
}

fn detection_key() -> [u8; 32] {
    ipc::prose_token::derive_detection_key(&[0x44; 32]).expect("test detector derives")
}

fn send_keys() -> ProseTokenSendKeys<'static> {
    ProseTokenSendKeys {
        message_key: &MESSAGE_KEY,
        send_key: &SEND_KEY,
        conversation_key: &CONVERSATION_KEY,
    }
}

fn wire_for_words(words: &str) -> String {
    format!("DPC0::{}", B64.encode(words.as_bytes()))
}

fn words_from_wire(wire: &str) -> String {
    let body = wire.strip_prefix("DPC0::").expect("wire has DPC0 prefix");
    String::from_utf8(B64.decode(body).expect("wire body is base64")).expect("words are UTF-8")
}

#[derive(Default)]
struct CountingTransport {
    blobs: BTreeMap<String, Vec<u8>>,
    fetches: usize,
}

impl CipherStoreTransport for CountingTransport {
    fn fetch(&mut self, blob_id: &str, _: &[u8]) -> Result<Vec<u8>, String> {
        self.fetches += 1;
        self.blobs
            .get(blob_id)
            .cloned()
            .ok_or_else(|| "missing blob".to_owned())
    }

    fn burn(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Default)]
struct PlainStore {
    opened: BTreeMap<String, String>,
}

impl PlainStore {
    fn open(&self, blob_id: &str) -> Option<&str> {
        self.opened.get(blob_id).map(String::as_str)
    }
}

impl LocalMessageStore for PlainStore {
    fn decrypt_and_persist(&mut self, blob_id: &str, ciphertext: &[u8]) -> Result<(), String> {
        self.opened.insert(
            blob_id.to_owned(),
            String::from_utf8(ciphertext.to_vec()).map_err(|error| error.to_string())?,
        );
        Ok(())
    }

    fn destroy_local(&mut self, blob_id: &str) -> Result<(), String> {
        self.opened.remove(blob_id);
        Ok(())
    }
}

#[test]
fn task_4062_pointer_landing_makes_exactly_one_fetch_and_open_reads_words() {
    let temp = tempfile::tempdir().unwrap();
    let pointer = PointerArrival {
        blob_id: "0011223344556677".to_owned(),
        fetch_cap: vec![7; 16],
        manage_cap: vec![8; 16],
    };
    let mut transport = CountingTransport::default();
    transport
        .blobs
        .insert(pointer.blob_id.clone(), TASK_WORDS.as_bytes().to_vec());
    let queue = EncryptedBurnQueue::new(temp.path().join("burns.enc"), [9; 32]);
    let mut driver = EagerFetchDriver::new(transport, PlainStore::default(), queue);

    receive_osl_chat_pointer(&mut driver, &pointer).expect("pointer arrival fetches");
    let (transport, store, _) = driver.into_parts();
    let opened = store.open(&pointer.blob_id).expect("open reads local copy");

    println!("TASK4062 pointer_landing_fetch_count={}", transport.fetches);
    println!("TASK4062 opened_words={opened}");
    assert_eq!(transport.fetches, 1);
    assert_eq!(opened, TASK_WORDS);
}

#[test]
fn task_4062_search_finds_exactly_one_fetcher_and_no_running_test_plugs() {
    let eager = include_str!("../src/eager_fetch.rs");
    let delivery = include_str!("../src/osl_chat_delivery.rs");
    let fetchers = eager.matches("pub fn on_pointer_arrival(").count();
    let running_test_only_plugs =
        eager.matches("#[cfg(test)]").count() + delivery.matches("#[cfg(test)]").count();

    println!("TASK4062 fetcher_search_count={fetchers}");
    println!("TASK4062 running_app_test_only_plugs={running_test_only_plugs}");
    assert_eq!(fetchers, 1);
    assert_eq!(running_test_only_plugs, 0);
}

struct CountingRealTransport {
    inner: CipherStoreClient,
    fetches: usize,
}

impl CipherStoreTransport for CountingRealTransport {
    fn fetch(&mut self, blob_id: &str, fetch_cap: &[u8]) -> Result<Vec<u8>, String> {
        self.fetches += 1;
        CipherStoreTransport::fetch(&mut self.inner, blob_id, fetch_cap)
    }

    fn burn(&mut self, blob_id: &str, manage_cap: &[u8]) -> Result<(), String> {
        CipherStoreTransport::burn(&mut self.inner, blob_id, manage_cap)
    }
}

#[derive(Default)]
struct WireStore {
    opened_words: BTreeMap<String, String>,
}

impl WireStore {
    fn open(&self, blob_id: &str) -> Option<&str> {
        self.opened_words.get(blob_id).map(String::as_str)
    }
}

impl LocalMessageStore for WireStore {
    fn decrypt_and_persist(&mut self, blob_id: &str, ciphertext: &[u8]) -> Result<(), String> {
        let wire = prose_token_wire_from_object(ciphertext).map_err(|error| error.to_string())?;
        self.opened_words
            .insert(blob_id.to_owned(), words_from_wire(&wire));
        Ok(())
    }

    fn destroy_local(&mut self, blob_id: &str) -> Result<(), String> {
        self.opened_words.remove(blob_id);
        Ok(())
    }
}

fn live_tests_enabled() -> bool {
    std::env::var("OSL_LIVE_TESTS").ok().as_deref() == Some("1")
}

#[test]
#[ignore = "live: set OSL_LIVE_TESTS=1 and run with --ignored"]
fn task_4062_live_deployed_pointer_shape_runs_through_the_eager_driver() {
    if !live_tests_enabled() {
        eprintln!("skipping live test (set OSL_LIVE_TESTS=1 to run)");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let config_dir = tempfile::tempdir().unwrap();
    let scope = scope();
    let detection_key = detection_key();
    let wire = wire_for_words(TASK_WORDS);
    let sent = prose_token_send(
        config_dir.path(),
        &scope,
        &detection_key,
        send_keys(),
        &wire,
        ipc::cipher_store_client::TTL_1H,
    )
    .expect("live bridge send uploads through production");
    let pointer = pointer_arrival_from_osl_chat_cover(&scope, &detection_key, &sent.cover_text)
        .expect("cover pointer parses")
        .expect("cover contains a pointer");
    let direct_pointer = prose_token_pointer_arrival(&scope, &detection_key, &sent.cover_text)
        .expect("direct parser succeeds")
        .expect("direct parser finds pointer");

    println!("TASK4062 live_blob_id={}", pointer.blob_id);
    println!("TASK4062 live_blob_id_len={}", pointer.blob_id.len());
    assert_eq!(pointer.blob_id, sent.blob_id);
    assert_eq!(pointer.blob_id.len(), ipc::prose_token::BRIDGE_ID_BYTES * 2);
    assert_eq!(pointer.fetch_cap, direct_pointer.fetch_cap);

    let transport = CountingRealTransport {
        inner: CipherStoreClient::new(DEFAULT_CIPHER_STORE_BASE_URL).expect("real client builds"),
        fetches: 0,
    };
    let queue = EncryptedBurnQueue::new(temp.path().join("burns.enc"), [3; 32]);
    let mut driver = EagerFetchDriver::new(transport, WireStore::default(), queue);

    receive_osl_chat_cover_pointer(&mut driver, &scope, &detection_key, &sent.cover_text)
        .expect("arrival runs through the eager driver");
    let (mut transport, store, _) = driver.into_parts();
    let opened = store
        .open(&pointer.blob_id)
        .expect("open reads local words");

    println!("TASK4062 live_fetch_count={}", transport.fetches);
    println!("TASK4062 live_opened_words={opened}");
    assert_eq!(transport.fetches, 1);
    assert_eq!(opened, TASK_WORDS);

    let _ = transport.burn(&pointer.blob_id, &pointer.manage_cap);
}
