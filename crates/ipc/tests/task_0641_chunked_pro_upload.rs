//! TASK 0641: Pro uploads split a sealed attachment through the production
//! multipart HTTP path, even when the caller deliberately selects it.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use crypto::aead::Key;
use crypto::attachment::encrypt_attachment;
use ipc::cipher_store_client::{
    CipherStoreClient, ProChunkedUploadFile, ProChunkedUploadPiece, ATTACHMENT_MULTIPART_MAX_PARTS,
    ATTACHMENT_MULTIPART_PART_BYTES, TTL_7D,
};

const FIXTURE_MARKER: &[u8] = b"TASK0641-PLAINTEXT-FIXTURE-MARKER";
const UPLOAD_ID: &str = "0123456789abcdef0123456789abcdef";
const EXPIRES_AT: i64 = 1_900_000_641;

struct MultipartFixture {
    base_url: String,
    server: thread::JoinHandle<Vec<(u32, u64)>>,
}

impl MultipartFixture {
    fn start(encrypted_len: u64, expected_parts: Vec<u64>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind multipart fixture");
        let address = listener.local_addr().expect("fixture address");
        let server = thread::spawn(move || {
            let mut received = Vec::new();
            for request_index in 0..expected_parts.len() + 2 {
                let (mut stream, _) = listener.accept().expect("fixture accepts request");
                let request = read_request(&mut stream);
                let (method, path, headers, body) = split_request(&request);
                match request_index {
                    0 => {
                        assert_eq!(method, "POST");
                        assert_eq!(path, "/v1/attachment/session");
                        assert_eq!(header(&headers, "content-length"), Some("0"));
                        assert_eq!(
                            header(&headers, "x-osl-size-bytes").map(str::to_owned),
                            Some(encrypted_len.to_string())
                        );
                        respond_json(
                            &mut stream,
                            &format!(
                                r#"{{"id":"{UPLOAD_ID}","expires_at":{EXPIRES_AT},"size_bytes":{encrypted_len},"max_part_bytes":{ATTACHMENT_MULTIPART_PART_BYTES},"max_parts":{ATTACHMENT_MULTIPART_MAX_PARTS}}}"#
                            ),
                        );
                    }
                    index if index <= expected_parts.len() => {
                        let number = index as u32;
                        let expected_size = expected_parts[index - 1];
                        assert_eq!(method, "PUT");
                        assert_eq!(path, format!("/v1/attachment/{UPLOAD_ID}/part/{number}"));
                        assert_eq!(body.len() as u64, expected_size);
                        assert_eq!(
                            header(&headers, "content-length")
                                .expect("part has Content-Length")
                                .parse::<u64>()
                                .expect("numeric Content-Length"),
                            expected_size
                        );
                        assert!(
                            !body
                                .windows(FIXTURE_MARKER.len())
                                .any(|window| window == FIXTURE_MARKER),
                            "plaintext fixture marker reached multipart fixture"
                        );
                        received.push((number, expected_size));
                        respond_json(
                            &mut stream,
                            &format!(r#"{{"part_number":{number},"size_bytes":{expected_size}}}"#),
                        );
                    }
                    _ => {
                        assert_eq!(method, "POST");
                        assert_eq!(path, format!("/v1/attachment/{UPLOAD_ID}/complete"));
                        assert_eq!(header(&headers, "content-length"), Some("0"));
                        respond_json(
                            &mut stream,
                            &format!(
                                r#"{{"id":"{UPLOAD_ID}","expires_at":{EXPIRES_AT},"size_bytes":{}}}"#,
                                encrypted_len - expected_parts.last().copied().expect("at least one uploaded piece")
                            ),
                        );
                    }
                }
            }
            received
        });
        Self {
            base_url: format!("http://{address}"),
            server,
        }
    }

    fn join(self) -> Vec<(u32, u64)> {
        self.server.join().expect("multipart fixture exits cleanly")
    }
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 16 * 1024];
    loop {
        let read = stream.read(&mut chunk).expect("read fixture request");
        assert_ne!(read, 0, "request ended before its body arrived");
        request.extend_from_slice(&chunk[..read]);
        let Some(headers_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..headers_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length").then(|| {
                    value
                        .trim()
                        .parse::<usize>()
                        .expect("numeric Content-Length")
                })
            })
            .expect("request has Content-Length");
        if request.len() >= headers_end + 4 + content_length {
            return request;
        }
    }
}

fn split_request(request: &[u8]) -> (String, String, String, &[u8]) {
    let headers_end = request
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .unwrap();
    let headers = String::from_utf8_lossy(&request[..headers_end]).into_owned();
    let mut first_line = headers
        .lines()
        .next()
        .expect("request line")
        .split_whitespace();
    let method = first_line.next().expect("method").to_owned();
    let path = first_line.next().expect("path").to_owned();
    (method, path, headers, &request[headers_end + 4..])
}

fn header<'a>(headers: &'a str, wanted: &str) -> Option<&'a str> {
    headers.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case(wanted).then(|| value.trim())
    })
}

fn respond_json(stream: &mut TcpStream, body: &str) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .expect("fixture response");
}

#[test]
fn task_0641_pro_chunked_upload_reports_encrypted_pieces_and_completion() {
    let plaintext_len = 20 * 1024 * 1024;
    let mut plaintext = Vec::with_capacity(plaintext_len);
    while plaintext.len() < plaintext_len {
        let take = (plaintext_len - plaintext.len()).min(FIXTURE_MARKER.len());
        plaintext.extend_from_slice(&FIXTURE_MARKER[..take]);
    }
    let encrypted = encrypt_attachment(
        Key::from_bytes([0x64; 32]),
        &plaintext,
        b"task-0641".to_vec(),
        0,
    )
    .expect("encrypt real attachment fixture");
    assert_ne!(encrypted, plaintext, "fixture must be sealed before upload");
    assert!(
        !encrypted
            .windows(FIXTURE_MARKER.len())
            .any(|window| window == FIXTURE_MARKER),
        "sealed file must not contain the plaintext fixture marker"
    );

    let encrypted_len = encrypted.len() as u64;
    let expected_parts: Vec<u64> = (0..encrypted_len.div_ceil(ATTACHMENT_MULTIPART_PART_BYTES))
        .map(|number| {
            (encrypted_len - number * ATTACHMENT_MULTIPART_PART_BYTES)
                .min(ATTACHMENT_MULTIPART_PART_BYTES)
        })
        .collect();
    let fixture = MultipartFixture::start(encrypted_len, expected_parts.clone());
    let mut file = tempfile::NamedTempFile::new().expect("sealed temp file");
    file.write_all(&encrypted).expect("write sealed attachment");
    file.flush().expect("flush sealed attachment");

    let report = CipherStoreClient::new(&fixture.base_url)
        .expect("cipher-store client")
        .upload_attachment_file_pro_chunked(
            file.reopen().expect("reopen sealed attachment"),
            TTL_7D,
            &[0x41; 16],
        )
        .expect("Pro chunked upload");
    let fixture_pieces = fixture.join();
    let expected_receipts: Vec<ProChunkedUploadPiece> = expected_parts
        .iter()
        .enumerate()
        .map(|(index, &size_bytes)| ProChunkedUploadPiece {
            upload_id: UPLOAD_ID.to_owned(),
            piece_number: index as u32 + 1,
            size_bytes,
        })
        .collect();
    let expected_file = ProChunkedUploadFile {
        file_id: UPLOAD_ID.to_owned(),
        total_size_bytes: encrypted_len,
        piece_count: expected_parts.len() as u32,
        expires_at: EXPIRES_AT,
    };

    assert_eq!(report.finished_pieces, expected_receipts);
    assert_eq!(report.completed_file, expected_file);
    assert_eq!(
        fixture_pieces,
        expected_parts
            .iter()
            .enumerate()
            .map(|(index, &size)| (index as u32 + 1, size))
            .collect::<Vec<_>>()
    );
    println!("TASK0641 fixture_bytes={plaintext_len}");
    println!("TASK0641 encrypted_bytes={encrypted_len}");
    println!(
        "TASK0641 finished_piece_records={:?}",
        report.finished_pieces
    );
    println!("TASK0641 fixture_received_order={fixture_pieces:?}");
    println!("TASK0641 completed_file={:?}", report.completed_file);
}
