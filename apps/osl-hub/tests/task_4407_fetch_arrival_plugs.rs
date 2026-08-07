use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use ipc::cipher_store_client::{CipherStoreClient, DEFAULT_CIPHER_STORE_BASE_URL, TTL_1H};
use ipc::commands::encrypt_osl_phase4_to_pubkeys;
use ipc::prose_token::BridgePointer;
use osl_privacy_hub::eager_fetch::{
    CipherStoreClientTransport, EagerFetchDriver, EncryptedBurnQueue, ExistingMessageStoreOpener,
};
use store::MessageStore;

const EXACT_WORDS: &str = "TASK 4407 exact opened words";
const LIVE_EXACT_WORDS: &str = "TASK 4407 live store exact words";

fn read_request(stream: &mut TcpStream) -> String {
    let mut raw = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let count = stream.read(&mut chunk).expect("read request");
        raw.extend_from_slice(&chunk[..count]);
        if raw.windows(4).any(|window| window == b"\r\n\r\n") {
            return String::from_utf8_lossy(&raw).to_ascii_lowercase();
        }
    }
}

fn one_fetch_server(id_hex: String, object: Vec<u8>) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback cipher store");
    let address = listener.local_addr().expect("loopback address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept fetch");
        let request = read_request(&mut stream);
        assert!(request.starts_with(&format!("get /v1/blob/{id_hex} ")));
        assert!(request.contains("x-osl-fetch-token: "));
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            object.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response headers");
        stream.write_all(&object).expect("write object");
        request
    });
    (format!("http://{address}"), server)
}

fn encrypted_bridge_object(
    sender: &keystore::Identity,
    recipient: &keystore::Identity,
    plaintext: &str,
) -> Vec<u8> {
    let wire =
        encrypt_osl_phase4_to_pubkeys(&sender.x25519_secret, &[recipient.x25519_public], plaintext)
            .expect("existing opener can decrypt this wire");
    let cipher_bytes = B64
        .decode(wire.strip_prefix("DPC0::").expect("wire has DPC0 prefix"))
        .expect("wire body decodes");
    ipc::transport_padding::frame_padded_transport_object(&cipher_bytes)
        .expect("framed object fits")
}

fn install_state_and_store(
    recipient: &keystore::Identity,
    sender: &keystore::Identity,
    sender_discord_id: &str,
    temp: &tempfile::TempDir,
) -> ipc::state::AppState {
    let state = ipc::state::AppState::new();
    *state.identity_slot() = Some(recipient.clone());
    state.peer_map.lock().expect("peer map lock").insert(
        sender_discord_id.to_owned(),
        ipc::peer_map::legacy_entry(sender.user_id.clone()),
    );
    state
        .sender_pubkey_cache
        .insert(sender.user_id.clone(), sender.x25519_public);
    let store = MessageStore::open(
        &temp.path().join("store"),
        recipient.x25519_secret.as_bytes(),
    )
    .expect("real message store opens");
    *state.message_store.lock().expect("message store lock") = Some(store);
    state
}

fn bridge_id_from_hex(id_hex: &str) -> [u8; ipc::prose_token::BRIDGE_ID_BYTES] {
    assert_eq!(id_hex.len(), ipc::prose_token::BRIDGE_ID_BYTES * 2);
    let mut id = [0u8; ipc::prose_token::BRIDGE_ID_BYTES];
    for (index, slot) in id.iter_mut().enumerate() {
        let start = index * 2;
        *slot = u8::from_str_radix(&id_hex[start..start + 2], 16).expect("hex id byte");
    }
    id
}

#[test]
fn task_4407_fetch_arrival_runs_with_real_client_and_real_message_store() {
    let recipient = keystore::generate_identity("task-4407-recipient".to_owned());
    let sender = keystore::generate_identity("task-4407-sender".to_owned());
    let sender_discord_id = "440700000000000001";
    let channel_id = "440700000000000002";
    let object = encrypted_bridge_object(&sender, &recipient, EXACT_WORDS);
    let pointer = BridgePointer {
        server_blob_id: [0x44; ipc::prose_token::BRIDGE_ID_BYTES],
        seed: [0x07; ipc::prose_token::BRIDGE_SEED_BYTES],
    };
    let blob_id = pointer.blob_id_hex();
    let (url, server) = one_fetch_server(blob_id.clone(), object);

    let temp = tempfile::tempdir().expect("tempdir");
    let state = install_state_and_store(&recipient, &sender, sender_discord_id, &temp);

    let client = CipherStoreClient::new(url).expect("client builds");
    let transport = CipherStoreClientTransport::new(client);
    let local = ExistingMessageStoreOpener::new(&state, channel_id, sender_discord_id);
    let queue = EncryptedBurnQueue::new(temp.path().join("burns.enc"), [0x44; 32]);
    let mut driver = EagerFetchDriver::new(transport, local, queue);

    driver
        .on_pointer_arrival(&pointer)
        .expect("fetch-on-arrival runs with production plugs");
    let request = server.join().expect("server exits");
    assert!(request.contains("x-osl-fetch-token: "));

    let guard = state.message_store.lock().expect("message store lock");
    let persisted = guard
        .as_ref()
        .expect("message store remains installed")
        .get(&blob_id)
        .expect("real store reads")
        .expect("fetched message persisted");
    assert_eq!(persisted.plaintext, EXACT_WORDS);

    println!("TASK4407_REAL_FETCH_TRANSPORT=CipherStoreClientTransport");
    println!("TASK4407_REAL_LOCAL_OPENER=ExistingMessageStoreOpener");
    println!("TASK4407_OPENED_WORDS={}", persisted.plaintext);
}

fn live_tests_enabled() -> bool {
    std::env::var("OSL_LIVE_TESTS").ok().as_deref() == Some("1")
}

#[test]
#[ignore = "live: uploads to the deployed cipher-store; run with OSL_LIVE_TESTS=1"]
fn task_4407_fetch_arrival_runs_against_deployed_cipher_store() {
    if !live_tests_enabled() {
        eprintln!("skipping live test (set OSL_LIVE_TESTS=1)");
        return;
    }

    let recipient = keystore::generate_identity("task-4407-live-recipient".to_owned());
    let sender = keystore::generate_identity("task-4407-live-sender".to_owned());
    let sender_discord_id = "440700000000000101";
    let channel_id = "440700000000000102";
    let seed = [0x94; ipc::prose_token::BRIDGE_SEED_BYTES];
    let fetch_token = ipc::prose_token::bridge_fetch_token(&seed);
    let object = encrypted_bridge_object(&sender, &recipient, LIVE_EXACT_WORDS);

    let upload_client =
        CipherStoreClient::new(DEFAULT_CIPHER_STORE_BASE_URL).expect("production client builds");
    let uploaded = upload_client
        .upload(&object, TTL_1H, &fetch_token)
        .expect("production cipher-store upload succeeds");
    let pointer = BridgePointer {
        server_blob_id: bridge_id_from_hex(&uploaded.id_hex),
        seed,
    };
    let blob_id = pointer.blob_id_hex();

    let temp = tempfile::tempdir().expect("tempdir");
    let state = install_state_and_store(&recipient, &sender, sender_discord_id, &temp);
    let client =
        CipherStoreClient::new(DEFAULT_CIPHER_STORE_BASE_URL).expect("production client builds");
    let transport = CipherStoreClientTransport::new(client);
    let local = ExistingMessageStoreOpener::new(&state, channel_id, sender_discord_id);
    let queue = EncryptedBurnQueue::new(temp.path().join("burns.enc"), [0x45; 32]);
    let mut driver = EagerFetchDriver::new(transport, local, queue);

    driver
        .on_pointer_arrival(&pointer)
        .expect("fetch-on-arrival runs against deployed cipher-store");
    let _ = upload_client.delete(&blob_id, &fetch_token);

    let guard = state.message_store.lock().expect("message store lock");
    let persisted = guard
        .as_ref()
        .expect("message store remains installed")
        .get(&blob_id)
        .expect("real store reads")
        .expect("live fetched message persisted");
    assert_eq!(persisted.plaintext, LIVE_EXACT_WORDS);

    println!("TASK4407_DEPLOYED_CIPHER_STORE={DEFAULT_CIPHER_STORE_BASE_URL}");
    println!("TASK4407_DEPLOYED_STORE_BLOB_ID={blob_id}");
    println!(
        "TASK4407_DEPLOYED_STORE_OPENED_WORDS={}",
        persisted.plaintext
    );
}
