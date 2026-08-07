//! T14-T11: OSL Chat fetches on pointer arrival, before a conversation opens.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use ipc::{
    cipher_store_client::{CipherStoreClient, TTL_1H},
    prose_token::{
        derive_detection_key, prose_token_bridge_pointer, prose_token_send_with_client,
        ProseTokenSendKeys,
    },
    scope::{ScopeInput, ScopeKind},
};
use osl_privacy_hub::{
    eager_fetch::{
        CipherStoreTransport, EagerFetchDriver, EncryptedBurnQueue, LocalMessageStore,
        PointerArrival,
    },
    osl_chat_delivery::receive_osl_chat_pointer,
};

fn pointer() -> PointerArrival {
    PointerArrival {
        server_blob_id: [0x11; ipc::prose_token::BRIDGE_ID_BYTES],
        seed: [0x22; ipc::prose_token::BRIDGE_SEED_BYTES],
    }
}

#[derive(Default)]
struct Transport {
    blobs: BTreeMap<String, Vec<u8>>,
    expected_fetch_token: Option<Vec<u8>>,
}
impl CipherStoreTransport for Transport {
    fn fetch(&mut self, blob_id: &str, fetch_token: &[u8]) -> Result<Vec<u8>, String> {
        if let Some(expected) = &self.expected_fetch_token {
            assert_eq!(fetch_token, expected.as_slice());
        }
        self.blobs
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

#[test]
fn t14_t11_payload_is_local_before_the_user_opens_the_conversation() {
    let path = std::env::temp_dir().join(format!("osl-chat-eager-{}", uuid::Uuid::new_v4()));
    let pointer = pointer();
    let mut transport = Transport::default();
    transport
        .blobs
        .insert("c882e13e918656da".to_owned(), b"ciphertext".to_vec());
    let queue = EncryptedBurnQueue::new(&path, [7; 32]);
    let mut driver = EagerFetchDriver::new(transport, Store::default(), queue);
    let pointer = PointerArrival {
        blob_id: "c882e13e918656da".to_owned(),
        fetch_seed: [1; ipc::prose_token::BRIDGE_SEED_BYTES],
    };
        .0
        .insert(pointer.blob_id_hex(), b"ciphertext".to_vec());
    let queue = EncryptedBurnQueue::new(&path, [7; 32]);
    let mut driver = EagerFetchDriver::new(transport, Store::default(), queue);

    // No chat view/open callback exists in this call: pointer arrival itself
    // must put the payload in the durable local store.
    receive_osl_chat_pointer(&mut driver, &pointer).expect("arrival fetches and persists");
    let (_, store, _) = driver.into_parts();
    assert_eq!(
        store.0.get("c882e13e918656da"),
        Some(&b"ciphertext".to_vec())
    );
}

#[test]
fn t14_t11_accepts_the_bridge_pointer_shape_from_a_real_send() {
    let (base_url, upload_rx) = spawn_upload_server("c882e13e918656da");
    let client = CipherStoreClient::new(base_url).expect("local cipher-store client builds");
    let scope = dm_scope();
    let detection_key = derive_detection_key(&[0x44; 32]).expect("detector derives");
    let wire = format!("DPC0::{}", B64.encode(b"real send payload"));

    let sent =
        prose_token_send_with_client(&client, &scope, &detection_key, send_keys(), &wire, TTL_1H)
            .expect("real send path uploads through the bridge POST shape");
    let uploaded = upload_rx.recv().expect("upload request captured");
    let bridge_pointer = prose_token_bridge_pointer(&scope, &detection_key, &sent.cover_text)
        .expect("cover decode does not error")
        .expect("cover carries a bridge pointer");
    let pointer = PointerArrival {
        blob_id: bridge_pointer.blob_id,
        fetch_seed: bridge_pointer.fetch_seed,
    };
    let expected_fetch_token = pointer.fetch_token().to_vec();
    assert_eq!(sent.blob_id, pointer.blob_id);
    assert_eq!(uploaded.fetch_token_hex, hex_lower(&expected_fetch_token));

    let path = std::env::temp_dir().join(format!("osl-chat-real-send-{}", uuid::Uuid::new_v4()));
    let transport = Transport {
        blobs: BTreeMap::from([(pointer.blob_id.clone(), uploaded.body)]),
        expected_fetch_token: Some(expected_fetch_token),
    };
    let mut driver = EagerFetchDriver::new(
        transport,
        Store::default(),
        EncryptedBurnQueue::new(&path, [7; 32]),
    );

    receive_osl_chat_pointer(&mut driver, &pointer).expect("arrival accepts the real send pointer");
    let (_, store, _) = driver.into_parts();
    assert!(
        store.0.contains_key(&sent.blob_id),
        "a real bridge pointer from the shipping send path is persisted"
    );
}

#[test]
fn t14_t11_refuses_the_retired_split_permission_shape_by_name() {
    let old_shape = serde_json::json!({
        "blob_id": "c882e13e918656da",
        "fetch_cap": [1],
        "manage_cap": [2]
    });
    let error = serde_json::from_value::<PointerArrival>(old_shape)
        .expect_err("retired fetch_cap/manage_cap pointer shape must be refused");

    assert!(
        error.to_string().contains("fetch_cap"),
        "the old field name must be named in the refusal: {error}"
    );
}

struct UploadRequest {
    fetch_token_hex: String,
    body: Vec<u8>,
}

fn spawn_upload_server(id_hex: &'static str) -> (String, mpsc::Receiver<UploadRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local upload server");
    let addr = listener.local_addr().expect("local addr");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept upload");
        let request = read_request(&mut stream);
        tx.send(request).expect("send captured upload");
        let body = format!(r#"{{"id":"{id_hex}","expires_at":1}}"#);
        write!(
            stream,
            "HTTP/1.1 201 Created\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .expect("write response");
    });
    (format!("http://{addr}"), rx)
}

fn read_request(stream: &mut std::net::TcpStream) -> UploadRequest {
    let mut bytes = Vec::new();
    let header_end = loop {
        let mut chunk = [0u8; 1024];
        let read = stream.read(&mut chunk).expect("read request");
        assert_ne!(read, 0, "connection closed before headers");
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index;
        }
    };
    let header_bytes = &bytes[..header_end];
    let headers = String::from_utf8(header_bytes.to_vec()).expect("headers are utf8");
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().expect("content-length"))
        })
        .expect("content-length header");
    let fetch_token_hex = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("x-osl-fetch-token")
                .then(|| value.trim().to_owned())
        })
        .expect("fetch token header");

    let body_start = header_end + 4;
    let mut body = bytes[body_start..].to_vec();
    while body.len() < content_length {
        let mut chunk = vec![0u8; content_length - body.len()];
        stream.read_exact(&mut chunk).expect("read body");
        body.extend_from_slice(&chunk);
    }
    body.truncate(content_length);
    UploadRequest {
        fetch_token_hex,
        body,
    }
}

fn dm_scope() -> ScopeInput {
    ScopeInput {
        kind: ScopeKind::Dm,
        id: "999000111222333444".to_owned(),
        server_id: None,
        channel_id: None,
    }
}

fn send_keys() -> ProseTokenSendKeys<'static> {
    static MESSAGE_KEY: [u8; 32] = [0x11; 32];
    static SEND_KEY: [u8; 32] = [0x22; 32];
    static CONVERSATION_KEY: [u8; 32] = [0x33; 32];
    ProseTokenSendKeys {
        message_key: &MESSAGE_KEY,
        send_key: &SEND_KEY,
        conversation_key: &CONVERSATION_KEY,
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
        store.0.get(&pointer.blob_id_hex()),
        Some(&b"ciphertext".to_vec())
    );
}
