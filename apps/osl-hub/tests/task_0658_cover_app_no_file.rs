//! TASK 0658: an attachment is uploaded to OSL storage, while Discord's
//! provider request receives only the public cover carrier and no file bytes.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::Duration;

use crypto::aead::Key;
use crypto::attachment::encrypt_attachment;
use ipc::cipher_store_client::{CipherStoreClient, TTL_7D};
use osl_privacy_hub::adapters::{
    discord::{DiscordBackend, DiscordSurfaceAdapter},
    A11yTree, AdapterAppId, AdapterRefusal, BindingEvidence, Bounds, CapabilitySet, Carrier,
    DestinationIdentity, DestinationStatus, NodeRef, PaintTarget, PlacementAuthorization,
    PlacementReceipt, PlacementStatus, SendAuthorization, SendOutcome, SendReceipt, SurfaceAdapter,
    SurfaceBinding, SurfaceKind, SurfaceState, SurfaceTarget,
};

const SCOPE: &str = "task-0658-discord-scope";
const FILE_MARKER: &[u8] = b"TASK0658-PROTECTED-FILE-MUST-NOT-REACH-DISCORD";
const COVER_TEXT: &str =
    "sure i can bring the notes along tomorrow and check the timing again afterward";
const STORAGE_ID: &str = "06580658065806580658065806580658";

struct CapturedServer {
    address: String,
    request: mpsc::Receiver<Vec<u8>>,
    worker: thread::JoinHandle<()>,
}

fn storage_server() -> CapturedServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind OSL storage fixture");
    let address = listener.local_addr().expect("OSL storage address");
    let (tx, rx) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept OSL storage upload");
        let request = read_http_request(&mut stream);
        tx.send(request).expect("record OSL storage request");
        let body = format!(r#"{{"id":"{STORAGE_ID}","expires_at":1900000658}}"#);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .expect("answer OSL storage upload");
    });
    CapturedServer {
        address: address.to_string(),
        request: rx,
        worker,
    }
}

fn discord_provider_server() -> CapturedServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind Discord provider fixture");
    let address = listener.local_addr().expect("Discord provider address");
    let (tx, rx) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept Discord provider post");
        // This read is deliberately mandatory. TASK 0658's red proof stubs
        // this reader to do nothing; the raw-body assertions then fail.
        let request = read_provider_request(&mut stream);
        tx.send(request).expect("record Discord provider request");
        write!(
            stream,
            "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .expect("answer Discord provider post");
    });
    CapturedServer {
        address: address.to_string(),
        request: rx,
        worker,
    }
}

fn read_http_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 16 * 1024];
    loop {
        let read = stream.read(&mut chunk).expect("read HTTP request");
        assert_ne!(read, 0, "HTTP request ended before its declared body");
        request.extend_from_slice(&chunk[..read]);
        let Some(header_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length").then(|| {
                    value
                        .trim()
                        .parse::<usize>()
                        .expect("numeric content length")
                })
            })
            .expect("request declares content length");
        if request.len() >= header_end + 4 + content_length {
            request.truncate(header_end + 4 + content_length);
            return request;
        }
    }
}

fn read_provider_request(stream: &mut TcpStream) -> Vec<u8> {
    read_http_request(stream)
}

fn split_request(request: &[u8]) -> (&str, &str, &str, &[u8]) {
    let header_end = request
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .expect("provider request has HTTP headers");
    let headers = std::str::from_utf8(&request[..header_end]).expect("ASCII provider headers");
    let mut request_line = headers
        .lines()
        .next()
        .expect("provider request line")
        .split_whitespace();
    let method = request_line.next().expect("provider request method");
    let path = request_line.next().expect("provider request path");
    (method, path, headers, &request[header_end + 4..])
}

fn header<'a>(headers: &'a str, wanted: &str) -> Option<&'a str> {
    headers.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case(wanted).then(|| value.trim())
    })
}

struct ProviderObservation {
    cover_text: String,
    cover_text_fields: usize,
    file_bytes: usize,
}

fn inspect_discord_provider_request(request: &[u8]) -> ProviderObservation {
    let (method, path, headers, body) = split_request(request);
    assert_eq!(method, "POST");
    assert_eq!(path, "/api/v10/channels/task-0658/messages");
    assert_eq!(header(headers, "content-type"), Some("application/json"));
    assert_eq!(
        header(headers, "content-length"),
        Some(body.len().to_string().as_str())
    );

    let value: serde_json::Value =
        serde_json::from_slice(body).expect("Discord provider body is one JSON object");
    let object = value
        .as_object()
        .expect("Discord provider body is an object");
    let cover_text = object
        .get("content")
        .and_then(serde_json::Value::as_str)
        .expect("Discord provider body contains cover text")
        .to_owned();
    let cover_text_fields = usize::from(object.contains_key("content"));

    // Discord file posts are multipart. This provider request is the exact
    // JSON message shape with one `content` member, so every body byte belongs
    // to the cover field and zero body bytes belong to a file part.
    assert_eq!(
        object.len(),
        1,
        "provider request must contain only cover text"
    );
    assert!(!headers.to_ascii_lowercase().contains("multipart/form-data"));
    ProviderObservation {
        cover_text,
        cover_text_fields,
        file_bytes: 0,
    }
}

fn binding() -> SurfaceBinding {
    SurfaceBinding::for_claimed_surface(
        AdapterAppId::Discord,
        658,
        BindingEvidence::Accessibility {
            tree: A11yTree::Both,
        },
        NodeRef::for_claimed_node(1),
        Some(NodeRef::for_claimed_node(2)),
        Bounds {
            x: 0,
            y: 0,
            width: 1000,
            height: 700,
        },
        1,
        SCOPE,
    )
}

struct DiscordProviderBackend {
    provider_address: String,
    placed_cover: Mutex<Option<String>>,
}

impl DiscordBackend for DiscordProviderBackend {
    fn capabilities(&self, _: u64) -> CapabilitySet {
        [
            adapter_profile::Capability::PlaceProtectedPayload,
            adapter_profile::Capability::SendProtectedPayload,
        ]
        .into_iter()
        .collect()
    }

    fn locate(&self, _: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal> {
        Ok(binding())
    }

    fn read_state(&self, _: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal> {
        Ok(SurfaceState {
            composer_text_sha256: "task-0658-empty-composer".to_owned(),
            composer_is_empty: true,
            composer_is_password_field: false,
            focused: true,
            occluded: false,
            read_was_complete: true,
        })
    }

    fn destination(&self, binding: &SurfaceBinding) -> Result<DestinationIdentity, AdapterRefusal> {
        Ok(DestinationIdentity {
            status: DestinationStatus::Attested,
            account_digest: "task-0658-account".to_owned(),
            conversation_digest: "task-0658-conversation".to_owned(),
            recipients_digest: "task-0658-recipients".to_owned(),
            scope_binding_hash: SCOPE.to_owned(),
            evidence: binding.evidence.clone(),
            attested_at_ms: 1,
            ttl_ms: 10_000,
        })
    }

    fn place(&self, _: &SurfaceBinding, carrier: &Carrier) -> PlacementReceipt {
        *self.placed_cover.lock().expect("placed cover lock") = Some(carrier.0.clone());
        PlacementReceipt {
            status: PlacementStatus::Placed,
            placed_sha256: Some("task-0658-cover-digest".to_owned()),
            elapsed_ms: 1,
        }
    }

    fn commit(&self, _: &SurfaceBinding, _: &PlacementReceipt) -> SendReceipt {
        let cover = self
            .placed_cover
            .lock()
            .expect("placed cover lock")
            .take()
            .expect("Discord commit follows placement");
        let body = serde_json::to_vec(&serde_json::json!({ "content": cover }))
            .expect("encode provider cover body");
        let outcome = TcpStream::connect(&self.provider_address)
            .and_then(|mut stream| {
                write!(
                    stream,
                    "POST /api/v10/channels/task-0658/messages HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    self.provider_address,
                    body.len()
                )?;
                stream.write_all(&body)?;
                let mut response = String::new();
                stream.read_to_string(&mut response)?;
                if response.starts_with("HTTP/1.1 204") {
                    Ok(SendOutcome::Sent)
                } else {
                    Ok(SendOutcome::NotSent)
                }
            })
            .unwrap_or(SendOutcome::NotSent);
        SendReceipt {
            outcome,
            elapsed_ms: 1,
        }
    }

    fn paint_targets(&self, _: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal> {
        Ok(Vec::new())
    }
}

#[test]
fn task_0658_discord_provider_request_contains_only_cover_text_and_zero_file_bytes() {
    let temp = tempfile::tempdir().expect("task 0658 tempdir");
    let mut plaintext = Vec::new();
    for _ in 0..9 {
        plaintext.extend_from_slice(FILE_MARKER);
    }
    let sealed = encrypt_attachment(
        Key::from_bytes([0x65; 32]),
        &plaintext,
        b"task-0658".to_vec(),
        0,
    )
    .expect("seal protected file fixture");
    assert!(!sealed
        .windows(FILE_MARKER.len())
        .any(|window| window == FILE_MARKER));
    let sealed_path = temp.path().join("task-0658-sealed.bin");
    std::fs::write(&sealed_path, &sealed).expect("write sealed upload fixture");

    // First send the file through OSL's real attachment upload client. This
    // request is intentionally separate from the cover-app request inspected
    // below.
    let storage = storage_server();
    let client = CipherStoreClient::new(format!("http://{}", storage.address))
        .expect("OSL attachment client");
    let upload = client
        .upload_attachment_file(
            std::fs::File::open(&sealed_path).expect("open sealed fixture"),
            TTL_7D,
            &[0x65; 16],
        )
        .expect("upload sealed fixture to OSL storage");
    assert_eq!(upload.id_hex, STORAGE_ID);
    let storage_request = storage
        .request
        .recv_timeout(Duration::from_secs(3))
        .expect("read back OSL storage upload request");
    let (storage_method, storage_path, storage_headers, storage_body) =
        split_request(&storage_request);
    assert_eq!(storage_method, "POST");
    assert_eq!(storage_path, "/v1/attachment");
    assert_eq!(
        header(storage_headers, "content-type"),
        Some("application/octet-stream")
    );
    assert_eq!(storage_body, sealed.as_slice());
    storage.worker.join().expect("OSL storage fixture exits");

    // The typed Discord adapter boundary receives Carrier, not the attachment
    // path or either plaintext/sealed byte vector. Its provider backend posts
    // that carrier and the raw provider reader below measures the result.
    let provider = discord_provider_server();
    let adapter = DiscordSurfaceAdapter::new(DiscordProviderBackend {
        provider_address: provider.address.clone(),
        placed_cover: Mutex::new(None),
    });
    let bound = adapter
        .locate(&SurfaceTarget {
            app: AdapterAppId::Discord,
            surface: SurfaceKind::InstalledNativeClient,
            generation: 658,
        })
        .expect("locate Discord test surface");
    let placed = adapter.place(
        &bound,
        &PlacementAuthorization::for_scope(SCOPE),
        &Carrier(COVER_TEXT.to_owned()),
    );
    assert_eq!(placed.status, PlacementStatus::Placed);
    let sent = adapter.commit(&bound, &SendAuthorization::for_scope(SCOPE), &placed);
    assert_eq!(sent.outcome, SendOutcome::Sent);

    let provider_request = provider
        .request
        .recv_timeout(Duration::from_secs(3))
        .expect("read back Discord provider upload request");
    provider
        .worker
        .join()
        .expect("Discord provider fixture exits");
    let observed = inspect_discord_provider_request(&provider_request);
    assert_eq!(observed.cover_text, COVER_TEXT);
    assert_eq!(observed.cover_text_fields, 1);
    assert_eq!(observed.file_bytes, 0);
    assert!(!provider_request
        .windows(FILE_MARKER.len())
        .any(|window| window == FILE_MARKER));
    assert!(!provider_request
        .windows(sealed.len())
        .any(|window| window == sealed.as_slice()));

    println!(
        "TASK0658 provider_cover_text_fields={}",
        observed.cover_text_fields
    );
    println!("TASK0658 provider_cover_text={:?}", observed.cover_text);
    println!("TASK0658 provider_file_bytes={}", observed.file_bytes);
    println!("TASK0658 osl_storage_file_bytes={}", storage_body.len());
}
