use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use ipc::cipher_store_client::{CipherStoreClient, TTL_1H};
use ipc::prose_token::{
    derive_detection_key, prose_token_bridge_pointer, prose_token_send_with_client,
    ProseTokenSendKeys,
};
use ipc::scope::{ScopeInput, ScopeKind};
use osl_privacy_hub::eager_fetch::{
    CipherStoreTransport, EagerFetchDriver, EncryptedBurnQueue, LocalMessageStore, PointerArrival,
};
use serde_json::json;

const MESSAGE_KEY: [u8; 32] = [0x11; 32];
const SEND_KEY: [u8; 32] = [0x22; 32];
const CONVERSATION_KEY: [u8; 32] = [0x33; 32];

#[derive(Default)]
struct Transport {
    payloads: BTreeMap<String, Vec<u8>>,
    fetched: Vec<(String, Vec<u8>)>,
}

impl CipherStoreTransport for Transport {
    fn fetch(&mut self, blob_id: &str, fetch_token: &[u8]) -> Result<Vec<u8>, String> {
        self.fetched
            .push((blob_id.to_owned(), fetch_token.to_vec()));
        self.payloads
            .get(blob_id)
            .cloned()
            .ok_or_else(|| "missing payload".to_owned())
    }

    fn burn(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Default)]
struct Store(BTreeMap<String, Vec<u8>>);

impl LocalMessageStore for Store {
    fn decrypt_and_persist(&mut self, blob_id: &str, ciphertext: &[u8]) -> Result<(), String> {
        self.0.insert(blob_id.to_owned(), ciphertext.to_vec());
        Ok(())
    }

    fn destroy_local(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut raw = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let count = stream.read(&mut chunk).expect("read request");
        raw.extend_from_slice(&chunk[..count]);
        let Some(headers_end) = raw.windows(4).position(|v| v == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&raw[..headers_end]).to_ascii_lowercase();
        let length = headers
            .lines()
            .find_map(|line| line.strip_prefix("content-length: "))
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        if raw.len() >= headers_end + 4 + length {
            return String::from_utf8_lossy(&raw).to_ascii_lowercase();
        }
    }
}

fn one_upload_server(id_hex: &'static str) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback cipher store");
    let address = listener.local_addr().expect("loopback address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept upload");
        let request = read_request(&mut stream);
        let body = format!(r#"{{"id":"{id_hex}","expires_at":1}}"#);
        let response = format!(
            "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write upload response");
        request
    });
    (format!("http://{address}"), server)
}

fn scope() -> ScopeInput {
    ScopeInput {
        kind: ScopeKind::Dm,
        id: "999000111222333444".to_owned(),
        server_id: None,
        channel_id: None,
    }
}

fn send_keys() -> ProseTokenSendKeys<'static> {
    ProseTokenSendKeys {
        message_key: &MESSAGE_KEY,
        send_key: &SEND_KEY,
        conversation_key: &CONVERSATION_KEY,
    }
}

#[test]
fn task_4508_fetch_arrival_accepts_shipping_bridge_pointer_and_refuses_old_shape() {
    let id_hex = "0102030405060708";
    let (url, server) = one_upload_server(id_hex);
    let client = CipherStoreClient::new(url).expect("client builds");
    let scope = scope();
    let detection_key = derive_detection_key(&CONVERSATION_KEY).expect("detector derives");
    let wire = format!("DPC0::{}", B64.encode(b"message ciphertext"));

    let sent =
        prose_token_send_with_client(&client, &scope, &detection_key, send_keys(), &wire, TTL_1H)
            .expect("real send helper produces a bridge cover");
    let upload_request = server.join().expect("upload fixture exits");
    assert!(upload_request.starts_with("post /v1/blob "));
    assert!(upload_request.contains("x-osl-fetch-token: "));

    let pointer = prose_token_bridge_pointer(&scope, &detection_key, &sent.cover_text)
        .expect("cover decode runs")
        .expect("real send cover carries a bridge pointer");
    assert_eq!(pointer.blob_id_hex(), sent.blob_id);
    assert_eq!(pointer.blob_id_hex(), id_hex);

    let shape = serde_json::to_value(pointer).expect("pointer serializes");
    let mut fields = shape
        .as_object()
        .expect("pointer serializes as object")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    fields.sort();
    assert_eq!(fields, vec!["seed".to_owned(), "server_blob_id".to_owned()]);

    let payload = b"eager payload".to_vec();
    let transport = Transport {
        payloads: BTreeMap::from([(pointer.blob_id_hex(), payload.clone())]),
        fetched: Vec::new(),
    };
    let temp = tempfile::tempdir().expect("tempdir");
    let queue = EncryptedBurnQueue::new(temp.path().join("burns.enc"), [0x45; 32]);
    let mut driver = EagerFetchDriver::new(transport, Store::default(), queue);
    driver
        .on_pointer_arrival(&pointer)
        .expect("real send pointer is accepted by eager fetch");

    let (transport, store, _) = driver.into_parts();
    assert_eq!(
        transport.fetched,
        vec![(pointer.blob_id_hex(), pointer.fetch_token().to_vec())]
    );
    assert_eq!(store.0.get(&pointer.blob_id_hex()), Some(&payload));

    let old_shape = json!({
        "blob_id": sent.blob_id,
        "fetch_cap": [1, 2, 3],
        "manage_cap": [4, 5, 6]
    });
    let refusal = serde_json::from_value::<PointerArrival>(old_shape)
        .expect_err("old arrival shape must be refused by field name")
        .to_string();
    assert!(
        refusal.contains("unknown field `blob_id`")
            || refusal.contains("unknown field `fetch_cap`")
            || refusal.contains("unknown field `manage_cap`"),
        "{refusal}"
    );

    println!("TASK4508_SHAPE_FIELDS={}", fields.join(","));
    println!("TASK4508_REAL_SEND_POINTER_ACCEPTED=1");
    println!("TASK4508_OLD_SHAPE_REFUSAL={refusal}");
}
