//! TASK 0642: the native OSL Chat send boundary admits and revalidates a tray
//! record before it opens the separately sealed Pro multipart upload file.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread;

use crypto::aead::Key;
use crypto::attachment::encrypt_attachment;
use ipc::cipher_store_client::{
    CipherStoreClient, ATTACHMENT_MULTIPART_MAX_PARTS, ATTACHMENT_MULTIPART_PART_BYTES, TTL_7D,
};
use osl_privacy_hub::osl_chat_drag_drop::OslChatAttachmentTray;
use osl_privacy_hub::osl_chat_pro_attachment_send::{
    send_pro_attachment_from_osl_chat_tray, OslChatProAttachmentSendRequest,
};

const UPLOAD_ID: &str = "0123456789abcdef0123456789abcdef";
const EXPIRES_AT: i64 = 1_900_000_642;
const PLAINTEXT_MARKER: &[u8] = b"TASK0642-PLAINTEXT-MUST-NOT-UPLOAD";

struct MultipartMock {
    base_url: String,
    started_pieces: Arc<AtomicUsize>,
    server: thread::JoinHandle<()>,
}

impl MultipartMock {
    fn start(sealed_len: u64, expected_parts: Vec<u64>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let address = listener.local_addr().expect("mock address");
        let started_pieces = Arc::new(AtomicUsize::new(0));
        let observed_pieces = Arc::clone(&started_pieces);
        let server = thread::spawn(move || {
            for request_index in 0..expected_parts.len() + 2 {
                let (mut stream, _) = listener.accept().expect("accept mock request");
                let request = read_request(&mut stream);
                let (method, path, headers, body) = split_request(&request);
                match request_index {
                    0 => {
                        assert_eq!(method, "POST");
                        assert_eq!(path, "/v1/attachment/session");
                        assert_eq!(
                            header(&headers, "x-osl-size-bytes").map(str::to_owned),
                            Some(sealed_len.to_string())
                        );
                        respond_json(
                            &mut stream,
                            &format!(
                                r#"{{"id":"{UPLOAD_ID}","expires_at":{EXPIRES_AT},"size_bytes":{sealed_len},"max_part_bytes":{ATTACHMENT_MULTIPART_PART_BYTES},"max_parts":{ATTACHMENT_MULTIPART_MAX_PARTS}}}"#
                            ),
                        );
                    }
                    index if index <= expected_parts.len() => {
                        let part_number = index as u32;
                        let expected_len = expected_parts[index - 1];
                        assert_eq!(method, "PUT");
                        assert_eq!(
                            path,
                            format!("/v1/attachment/{UPLOAD_ID}/part/{part_number}")
                        );
                        assert_eq!(body.len() as u64, expected_len);
                        assert!(
                            !body
                                .windows(PLAINTEXT_MARKER.len())
                                .any(|window| window == PLAINTEXT_MARKER),
                            "a plaintext tray file reached the uploader"
                        );
                        observed_pieces.fetch_add(1, Ordering::SeqCst);
                        respond_json(
                            &mut stream,
                            &format!(
                                r#"{{"part_number":{part_number},"size_bytes":{expected_len}}}"#
                            ),
                        );
                    }
                    _ => {
                        assert_eq!(method, "POST");
                        assert_eq!(path, format!("/v1/attachment/{UPLOAD_ID}/complete"));
                        respond_json(
                            &mut stream,
                            &format!(
                                r#"{{"id":"{UPLOAD_ID}","expires_at":{EXPIRES_AT},"size_bytes":{sealed_len}}}"#
                            ),
                        );
                    }
                }
            }
        });
        Self {
            base_url: format!("http://{address}"),
            started_pieces,
            server,
        }
    }

    fn join(self) {
        self.server.join().expect("mock server completes");
    }
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 16 * 1024];
    loop {
        let read = stream.read(&mut chunk).expect("read request");
        assert_ne!(read, 0, "request ended before body");
        request.extend_from_slice(&chunk[..read]);
        let Some(headers_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..headers_end]);
        let length = headers
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
            .expect("content length");
        if request.len() >= headers_end + 4 + length {
            return request;
        }
    }
}

fn split_request(request: &[u8]) -> (String, String, String, &[u8]) {
    let headers_end = request
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .expect("header end");
    let headers = String::from_utf8_lossy(&request[..headers_end]).into_owned();
    let mut request_line = headers
        .lines()
        .next()
        .expect("request line")
        .split_whitespace();
    (
        request_line.next().expect("method").to_owned(),
        request_line.next().expect("path").to_owned(),
        headers,
        &request[headers_end + 4..],
    )
}

fn header<'a>(headers: &'a str, wanted: &str) -> Option<&'a str> {
    headers.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case(wanted).then(|| value.trim())
    })
}

fn respond_json(stream: &mut TcpStream, body: &str) {
    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).expect("mock response");
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
        fetch_token: [0x42; 16],
    }
}

#[test]
fn task_0642_native_pro_send_starts_pieces_only_after_admission_and_tray_validation() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plaintext_path = temp.path().join("tray-plaintext.bin");
    let mut plaintext = Vec::with_capacity(ATTACHMENT_MULTIPART_PART_BYTES as usize);
    while plaintext.len() < ATTACHMENT_MULTIPART_PART_BYTES as usize {
        let remaining = ATTACHMENT_MULTIPART_PART_BYTES as usize - plaintext.len();
        plaintext.extend_from_slice(&PLAINTEXT_MARKER[..remaining.min(PLAINTEXT_MARKER.len())]);
    }
    std::fs::write(&plaintext_path, &plaintext).expect("write plaintext tray fixture");
    let sealed = encrypt_attachment(
        Key::from_bytes([0x64; 32]),
        &plaintext,
        b"task-0642".to_vec(),
        0,
    )
    .expect("seal fixture");
    assert!(!sealed
        .windows(PLAINTEXT_MARKER.len())
        .any(|window| window == PLAINTEXT_MARKER));
    let sealed_path = temp.path().join("already-sealed.bin");
    std::fs::write(&sealed_path, &sealed).expect("write sealed fixture");

    let mut valid_tray = OslChatAttachmentTray::default();
    valid_tray
        .accept_dropped_files([plaintext_path.as_path()])
        .expect("valid tray intake");
    let expected_parts = (0..sealed.len() as u64)
        .step_by(ATTACHMENT_MULTIPART_PART_BYTES as usize)
        .map(|offset| ((sealed.len() as u64) - offset).min(ATTACHMENT_MULTIPART_PART_BYTES))
        .collect::<Vec<_>>();
    let mock = MultipartMock::start(sealed.len() as u64, expected_parts.clone());
    let client = CipherStoreClient::new(&mock.base_url).expect("cipher-store client");

    let mut over_limit_tray = OslChatAttachmentTray::default();
    over_limit_tray
        .accept_dropped_files(std::iter::repeat_n(plaintext_path.as_path(), 17))
        .expect("over-limit tray intake");
    let invalid = send_pro_attachment_from_osl_chat_tray(
        &over_limit_tray,
        &request_for(&over_limit_tray, &sealed_path),
        &client,
    );
    assert!(
        invalid.is_err(),
        "count admission must refuse before upload"
    );
    let invalid_started = mock.started_pieces.load(Ordering::SeqCst);
    assert_eq!(invalid_started, 0);

    let mut stale_request = request_for(&valid_tray, &sealed_path);
    stale_request.tray_size_bytes += 1;
    let stale = send_pro_attachment_from_osl_chat_tray(&valid_tray, &stale_request, &client);
    assert!(stale.is_err(), "tray validation must refuse before upload");
    let invalid_started = mock.started_pieces.load(Ordering::SeqCst);
    println!("TASK0642 invalid_started_piece_count={invalid_started}");
    assert_eq!(invalid_started, 0);

    let report = send_pro_attachment_from_osl_chat_tray(
        &valid_tray,
        &request_for(&valid_tray, &sealed_path),
        &client,
    )
    .expect("valid Pro send");
    let valid_started = mock.started_pieces.load(Ordering::SeqCst);
    println!("TASK0642 valid_started_piece_count={valid_started}");
    assert_eq!(valid_started, expected_parts.len());
    assert_eq!(report.finished_pieces.len(), expected_parts.len());
    mock.join();
}
