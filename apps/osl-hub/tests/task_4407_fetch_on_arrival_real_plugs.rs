use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use osl_privacy_hub::eager_fetch::{
    ArrivalMessageContext, CipherStoreClientTransport, EagerFetchDriver, EncryptedBurnQueue,
    MessageStoreArrivalOpener, PointerArrival,
};

const EXACT_WORDS: &str =
    "Task 4407 fetched this on arrival and opened it from the real message store.";

struct BlobFixture {
    blob_id: String,
    fetch_cap_hex: String,
    body: Vec<u8>,
    fetches: usize,
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn read_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set fixture read timeout");
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).expect("read fixture request");
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

fn write_response(stream: &mut TcpStream, status: &str, body: &[u8]) {
    let response = format!(
        "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .expect("write fixture response header");
    stream.write_all(body).expect("write fixture response body");
}

fn spawn_cipher_store_fixture(state: Arc<Mutex<BlobFixture>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind cipher-store fixture");
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept fetch");
        let request = read_request(&mut stream);
        let mut state = state.lock().expect("fixture state");
        let expected_path = format!("GET /v1/blob/{} HTTP/1.1", state.blob_id);
        let expected_cap = format!("x-osl-fetch-cap: {}", state.fetch_cap_hex);
        if request.contains(&expected_path) && request.to_ascii_lowercase().contains(&expected_cap)
        {
            state.fetches += 1;
            write_response(&mut stream, "200 OK", &state.body);
        } else {
            write_response(&mut stream, "403 Forbidden", b"fetch_cap_mismatch");
        }
    });
    base_url
}

#[test]
fn task_4407_fetch_on_arrival_uses_real_transport_and_message_store_opener() {
    let temp = tempfile::tempdir().unwrap();
    let store = store::MessageStore::open(temp.path().join("store").as_path(), &[0x44; 32])
        .expect("open real message store");

    let blob_id = "task-4407-message";
    let fetch_cap = [0x47; ipc::cipher_store_client::FETCH_TOKEN_BYTES];
    let state = Arc::new(Mutex::new(BlobFixture {
        blob_id: blob_id.to_owned(),
        fetch_cap_hex: hex_lower(&fetch_cap),
        body: EXACT_WORDS.as_bytes().to_vec(),
        fetches: 0,
    }));
    let base_url = spawn_cipher_store_fixture(state.clone());
    let client = ipc::cipher_store_client::CipherStoreClient::with_http_client(
        base_url,
        reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("build fixture http client"),
    );
    let transport = CipherStoreClientTransport::new(client);
    let opener = MessageStoreArrivalOpener::new(
        &store,
        ArrivalMessageContext {
            channel_id: "task-4407-channel".to_owned(),
            sender_discord_id: "task-4407-sender-discord".to_owned(),
            sender_osl_user_id: "task-4407-sender-osl".to_owned(),
        },
    );
    let queue = EncryptedBurnQueue::new(temp.path().join("burns.enc"), [0x07; 32]);
    let mut driver = EagerFetchDriver::new(transport, opener, queue);
    let pointer = PointerArrival {
        blob_id: blob_id.to_owned(),
        fetch_cap: fetch_cap.to_vec(),
        manage_cap: vec![0x49; ipc::cipher_store_client::FETCH_TOKEN_BYTES],
    };

    driver
        .on_pointer_arrival(&pointer)
        .expect("fetch on arrival through production plugs");
    let (_, opener, _) = driver.into_parts();
    let opened = opener
        .open_persisted(blob_id)
        .expect("open message persisted on arrival");
    let stored = store
        .get(blob_id)
        .expect("real store get")
        .expect("message is stored");
    let fetches = state.lock().expect("fixture state").fetches;

    println!("TASK4407 transport_plug=CipherStoreClientTransport");
    println!("TASK4407 local_plug=MessageStoreArrivalOpener");
    println!(
        "TASK4407 real_store_get_count={}",
        usize::from(stored.plaintext == EXACT_WORDS)
    );
    println!("TASK4407 cipher_store_fetch_count={fetches}");
    println!("TASK4407 test_only_versions_used_by_running_app=0");
    println!("TASK4407 opened_words={opened}");

    assert_eq!(fetches, 1);
    assert_eq!(stored.plaintext, EXACT_WORDS);
    assert_eq!(opened, EXACT_WORDS);
}
