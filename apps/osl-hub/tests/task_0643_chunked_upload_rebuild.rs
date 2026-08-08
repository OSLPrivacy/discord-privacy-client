//! TASK 0643: prove the Pro upload pieces rebuild the exact stored file.

#![cfg(feature = "core")]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::thread;

use ipc::cipher_store_client::{
    CipherStoreClient, ATTACHMENT_MULTIPART_MAX_PARTS, ATTACHMENT_MULTIPART_PART_BYTES, TTL_7D,
};
use osl_privacy_hub::osl_chat_drag_drop::OslChatAttachmentTray;
use osl_privacy_hub::osl_chat_pro_attachment_send::{
    send_pro_attachment_from_osl_chat_tray, OslChatProAttachmentSendRequest,
};
use sha2::{Digest, Sha256};

const UPLOAD_ID: &str = "06430643064306430643064306430643";
const EXPIRES_AT: i64 = 1_900_000_643;
const MUTATION_ENV: &str = "TASK0643_MUTATE_FIXTURE";
const TRAILING_BYTES: usize = 257;

struct OslStorageFixture {
    base_url: String,
    server: thread::JoinHandle<()>,
}

impl OslStorageFixture {
    fn start(expected_size: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind OSL storage fixture");
        let address = listener.local_addr().expect("OSL storage fixture address");
        let server = thread::spawn(move || {
            let mut stored = Vec::with_capacity(expected_size);
            for request_index in 0..5 {
                let (mut stream, _) = listener.accept().expect("accept OSL storage request");
                let request = read_request(&mut stream);
                match request_index {
                    0 => {
                        assert_eq!(request.method, "POST");
                        assert_eq!(request.path, "/v1/attachment/session");
                        assert_eq!(
                            header(&request.headers, "x-osl-size-bytes"),
                            Some(expected_size.to_string())
                        );
                        respond_json(
                            &mut stream,
                            &format!(
                                r#"{{"id":"{UPLOAD_ID}","expires_at":{EXPIRES_AT},"size_bytes":{expected_size},"max_part_bytes":{ATTACHMENT_MULTIPART_PART_BYTES},"max_parts":{ATTACHMENT_MULTIPART_MAX_PARTS}}}"#
                            ),
                        );
                    }
                    1 | 2 => {
                        let piece_number = request_index;
                        assert_eq!(request.method, "PUT");
                        assert_eq!(
                            request.path,
                            format!("/v1/attachment/{UPLOAD_ID}/part/{piece_number}")
                        );
                        let expected_piece_size = if piece_number == 1 {
                            ATTACHMENT_MULTIPART_PART_BYTES as usize
                        } else {
                            TRAILING_BYTES
                        };
                        assert_eq!(request.body.len(), expected_piece_size);
                        // TASK 0644 deliberately drops the final numbered
                        // piece after acknowledging it, so the rebuild hash
                        // check must detect the incomplete stored file.
                        if piece_number != 2 {
                            stored.extend_from_slice(&request.body);
                        } else {
                            println!("TASK0644 skipped_piece_number={piece_number}");
                        }
                        respond_json(
                            &mut stream,
                            &format!(
                                r#"{{"part_number":{piece_number},"size_bytes":{expected_piece_size}}}"#
                            ),
                        );
                    }
                    3 => {
                        assert_eq!(request.method, "POST");
                        assert_eq!(request.path, format!("/v1/attachment/{UPLOAD_ID}/complete"));
                        respond_json(
                            &mut stream,
                            &format!(
                                r#"{{"id":"{UPLOAD_ID}","expires_at":{EXPIRES_AT},"size_bytes":{expected_size}}}"#
                            ),
                        );
                    }
                    _ => {
                        assert_eq!(request.method, "GET");
                        assert_eq!(request.path, format!("/v1/attachment/{UPLOAD_ID}"));
                        respond_bytes(&mut stream, &stored);
                    }
                }
            }
        });
        Self {
            base_url: format!("http://{address}"),
            server,
        }
    }

    fn join(self) {
        self.server.join().expect("OSL storage fixture completes");
    }
}

struct HttpRequest {
    method: String,
    path: String,
    headers: String,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> HttpRequest {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = stream.read(&mut buffer).expect("read OSL storage request");
        assert_ne!(read, 0, "OSL storage request ended before its body");
        request.extend_from_slice(&buffer[..read]);
        let Some(headers_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..headers_end]).into_owned();
        let content_length = header(&headers, "content-length")
            .map(|value| {
                value
                    .parse::<usize>()
                    .expect("numeric request content length")
            })
            .unwrap_or(0);
        if request.len() < headers_end + 4 + content_length {
            continue;
        }
        let mut request_line = headers
            .lines()
            .next()
            .expect("request line")
            .split_whitespace();
        return HttpRequest {
            method: request_line.next().expect("request method").to_owned(),
            path: request_line.next().expect("request path").to_owned(),
            headers,
            body: request[headers_end + 4..headers_end + 4 + content_length].to_vec(),
        };
    }
}

fn header(headers: &str, wanted: &str) -> Option<String> {
    headers.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case(wanted)
            .then(|| value.trim().to_owned())
    })
}

fn respond_json(stream: &mut TcpStream, body: &str) {
    respond(stream, "application/json", body.as_bytes());
}

fn respond_bytes(stream: &mut TcpStream, body: &[u8]) {
    respond(stream, "application/octet-stream", body);
}

fn respond(stream: &mut TcpStream, content_type: &str, body: &[u8]) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .expect("write OSL storage response headers");
    stream
        .write_all(body)
        .expect("write OSL storage response body");
}

fn deterministic_fixture() -> Vec<u8> {
    let size = ATTACHMENT_MULTIPART_PART_BYTES as usize + TRAILING_BYTES;
    (0..size)
        .map(|index| {
            let mixed = (index as u64)
                .wrapping_mul(0x9e37_79b9_7f4a_7c15)
                .rotate_left((index % 61) as u32);
            (mixed ^ (mixed >> 29) ^ 0x43) as u8
        })
        .collect()
}

fn request_for(
    tray: &OslChatAttachmentTray,
    sealed_path: &Path,
) -> OslChatProAttachmentSendRequest {
    let attachment = tray.attachments().first().expect("tray attachment");
    OslChatProAttachmentSendRequest {
        tray_id: attachment.tray_id.clone(),
        tray_path: attachment.path.clone(),
        tray_size_bytes: attachment.size_bytes,
        sealed_path: sealed_path.to_path_buf(),
        ttl_seconds: TTL_7D,
        fetch_token: [0x43; 16],
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn task_0643_chunked_upload_rebuilds_exact_file_from_osl_storage() {
    let temp = tempfile::tempdir().expect("task 0643 tempdir");
    let tray_path = temp.path().join("tray-source.bin");
    std::fs::write(&tray_path, b"TASK0643 native tray record").expect("write tray fixture");

    let fixture = deterministic_fixture();
    let original_hash = sha256_hex(&fixture);
    let sealed_path = temp.path().join("prepared-sealed-fixture.bin");
    std::fs::write(&sealed_path, &fixture).expect("write prepared upload fixture");

    let mut tray = OslChatAttachmentTray::default();
    tray.accept_dropped_files([tray_path.as_path()])
        .expect("admit tray fixture");
    let storage = OslStorageFixture::start(fixture.len());
    let client = CipherStoreClient::new(&storage.base_url).expect("OSL storage client");
    let request = request_for(&tray, &sealed_path);

    let report = send_pro_attachment_from_osl_chat_tray(&tray, &request, &client)
        .expect("upload prepared fixture through pieces");
    assert_eq!(report.finished_pieces.len(), 2);
    let mut read_back = Vec::new();
    let fetched = client
        .fetch_attachment_to_writer(
            &report.completed_file.file_id,
            &request.fetch_token,
            &mut read_back,
        )
        .expect("read rebuilt fixture from OSL storage");
    storage.join();

    let read_back_hash = sha256_hex(&read_back);
    println!(
        "TASK0643 uploaded_piece_count={}",
        report.finished_pieces.len()
    );
    println!("TASK0643 original_fixture_bytes={}", fixture.len());
    println!("TASK0643 read_back_bytes={fetched}");
    println!("TASK0643 original_fixture_sha256={original_hash}");
    println!("TASK0643 read_back_sha256={read_back_hash}");
    println!(
        "TASK0643 original_hash_match={}",
        original_hash == read_back_hash
    );
    assert_eq!(
        read_back_hash, original_hash,
        "rebuilt OSL storage file hash"
    );

    let mut comparison_fixture = fixture;
    if std::env::var_os(MUTATION_ENV).is_some() {
        let mutation_index = ATTACHMENT_MULTIPART_PART_BYTES as usize + 17;
        let before = comparison_fixture[mutation_index];
        comparison_fixture[mutation_index] ^= 1;
        println!(
            "TASK0643 one_byte_mutation=index:{mutation_index},before:{before},after:{}",
            comparison_fixture[mutation_index]
        );
    }
    let comparison_hash = sha256_hex(&comparison_fixture);
    println!("TASK0643 comparison_fixture_sha256={comparison_hash}");
    println!(
        "TASK0643 comparison_hash_match={}",
        comparison_hash == read_back_hash
    );
    assert_eq!(
        comparison_hash, read_back_hash,
        "prepared fixture hash must equal the rebuilt OSL storage file hash"
    );
}
