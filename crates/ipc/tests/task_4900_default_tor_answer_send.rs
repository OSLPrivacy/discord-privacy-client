use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use ipc::cipher_store_client::TTL_1H;
use ipc::prose_token::{derive_detection_key, prose_token_send, ProseTokenSendKeys};
use ipc::scope::{ScopeInput, ScopeKind};
use serde_json::json;

const MESSAGE_KEY: [u8; 32] = [0x49; 32];
const SEND_KEY: [u8; 32] = [0x00; 32];
const CONVERSATION_KEY: [u8; 32] = [0x90; 32];
const TEST_SERVICE_ID: &str = "4900490049004900";

fn send_keys() -> ProseTokenSendKeys<'static> {
    ProseTokenSendKeys {
        message_key: &MESSAGE_KEY,
        send_key: &SEND_KEY,
        conversation_key: &CONVERSATION_KEY,
    }
}

fn scope() -> ScopeInput {
    ScopeInput {
        kind: ScopeKind::Dm,
        id: "4900000000000000000".to_owned(),
        server_id: None,
        channel_id: None,
    }
}

struct CountingCipherStore {
    base_url: String,
    arrivals: Arc<AtomicUsize>,
    body_bytes: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl CountingCipherStore {
    fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind task 4900 service");
        listener
            .set_nonblocking(true)
            .expect("make task 4900 service nonblocking");
        let base_url = format!("http://{}", listener.local_addr().expect("service address"));
        let arrivals = Arc::new(AtomicUsize::new(0));
        let body_bytes = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_arrivals = arrivals.clone();
        let thread_body_bytes = body_bytes.clone();
        let thread_stop = stop.clone();
        let handle = thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        if let Some((request, body_len)) = read_request(&mut stream) {
                            if request.to_ascii_lowercase().starts_with("post /v1/blob ") {
                                thread_arrivals.fetch_add(1, Ordering::AcqRel);
                                thread_body_bytes.fetch_add(body_len, Ordering::AcqRel);
                            }
                        }
                        let body = json!({
                            "id": TEST_SERVICE_ID,
                            "expires_at": 2_000_000_000i64,
                        })
                        .to_string();
                        let response = format!(
                            "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = stream.write_all(response.as_bytes());
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            base_url,
            arrivals,
            body_bytes,
            stop,
            handle: Some(handle),
        }
    }

    fn arrivals(&self) -> usize {
        self.arrivals.load(Ordering::Acquire)
    }

    fn body_bytes(&self) -> usize {
        self.body_bytes.load(Ordering::Acquire)
    }
}

impl Drop for CountingCipherStore {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn read_request(stream: &mut TcpStream) -> Option<(String, usize)> {
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    let mut raw = Vec::new();
    let mut chunk = [0u8; 2048];
    loop {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            return None;
        }
        raw.extend_from_slice(&chunk[..read]);
        let headers_end = raw.windows(4).position(|window| window == b"\r\n\r\n")?;
        let headers = String::from_utf8_lossy(&raw[..headers_end]).to_string();
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        if raw.len() >= headers_end + 4 + content_length {
            return Some((headers, content_length));
        }
    }
}

fn accept_onboarding_selected_answer(force_direct: bool) -> &'static str {
    if force_direct {
        keystore::egress::permit_clearnet();
        "Direct"
    } else {
        keystore::egress::seal();
        "Tor"
    }
}

fn send_one_osl_message(config_dir: &std::path::Path) -> Result<String, String> {
    let plaintext = [0x64; 64];
    let wire = format!("DPC0::{}", B64.encode(plaintext));
    let detection_key =
        derive_detection_key(&CONVERSATION_KEY).map_err(|error| error.to_string())?;
    let sent = prose_token_send(config_dir, &scope(), &detection_key, send_keys(), &wire, TTL_1H)
        .map_err(|error| error.to_string())?;
    assert_eq!(plaintext.len(), 64);
    assert!(
        sent.blob_id == TEST_SERVICE_ID,
        "the message must be acknowledged by the task 4900 service"
    );
    Ok(sent.blob_id)
}

#[test]
fn task_4900_default_tor_answer_can_send_one_osl_message() {
    let _restore = keystore::egress::restore_clearnet_on_drop();
    std::env::remove_var("OSL_ARTI_PROXY_PATH");
    std::env::remove_var("OSL_ARTI_PROXY");
    std::env::remove_var("OSL_ARTI_PROXY_ARGS");

    let force_direct = std::env::var("TOR_4900_FORCE_DIRECT").ok().as_deref() == Some("1");
    let temp = tempfile::tempdir().expect("task 4900 tempdir");
    let service = CountingCipherStore::spawn();
    std::fs::write(
        temp.path().join("keyserver.json"),
        json!({ "cipher_store_url": service.base_url }).to_string(),
    )
    .expect("write task 4900 service override");
    let selected = accept_onboarding_selected_answer(force_direct);
    let send_result = send_one_osl_message(temp.path());
    thread::sleep(Duration::from_millis(100));

    let arrivals = service.arrivals();
    let body_bytes = service.body_bytes();
    println!(
        "TOR-4900 selected_route={selected} force_direct={force_direct} osl_message_bytes=64 messages_arrived={arrivals} uploaded_body_bytes={body_bytes}"
    );

    if force_direct {
        println!(
            "TOR-4900-INSTRUMENT messages_arrived={arrivals} osl_message_bytes=64 uploaded_body_bytes={body_bytes}"
        );
        assert_eq!(
            arrivals, 1,
            "forced Direct must reach the task 4900 service exactly once"
        );
        assert!(
            send_result.is_ok(),
            "forced Direct send failed: {send_result:?}"
        );
    } else if send_result.is_err() && arrivals == 0 {
        println!(
            "TOR-4900-RED messages_arrived=0 error={}",
            send_result.as_ref().expect_err("red path has an error")
        );
    }

    assert_eq!(
        arrivals, 1,
        "the default pre-selected Tor answer must send exactly one OSL message"
    );
    assert!(
        send_result.is_ok(),
        "default pre-selected Tor send failed: {send_result:?}"
    );
}
