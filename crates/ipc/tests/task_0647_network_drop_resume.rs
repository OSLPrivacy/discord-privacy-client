//! TASK 0647: a one-gibibyte Pro upload resumes after a dropped part request.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::thread;

use ipc::cipher_store_client::{
    CipherStoreClient, CipherStoreError, ProChunkedUploadResumeRecord,
    ATTACHMENT_MULTIPART_PART_BYTES, MAX_SEALED_ATTACHMENT_BYTES, TTL_7D,
};

const UPLOAD_ID: &str = "0123456789abcdef0123456789abcdef";
const EXPIRES_AT: i64 = 1_900_000_647;

struct RequestHead {
    method: String,
    path: String,
    content_length: u64,
}

fn read_request_head(stream: &mut TcpStream) -> RequestHead {
    let mut headers = Vec::new();
    let mut byte = [0_u8; 1];
    while !headers.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).expect("read fixture headers");
        headers.push(byte[0]);
    }
    let headers = String::from_utf8(headers).expect("UTF-8 fixture headers");
    let mut request = headers.lines();
    let mut request_line = request.next().expect("request line").split_whitespace();
    let method = request_line.next().expect("request method").to_owned();
    let path = request_line.next().expect("request path").to_owned();
    let content_length = request
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<u64>().expect("numeric Content-Length"))
        })
        .unwrap_or(0);
    RequestHead {
        method,
        path,
        content_length,
    }
}

fn discard_request_body(stream: &mut TcpStream, length: u64) {
    let mut remaining = length;
    let mut body = [0_u8; 8 * 1024];
    while remaining != 0 {
        let to_read = usize::try_from(remaining.min(body.len() as u64)).expect("body chunk size");
        stream
            .read_exact(&mut body[..to_read])
            .expect("read fixture request body");
        remaining -= to_read as u64;
    }
}

fn accept_part(listener: &TcpListener, expected_part: u32) -> TcpStream {
    let (mut stream, _) = listener.accept().expect("accept part request");
    let request = read_request_head(&mut stream);
    assert_eq!(request.method, "PUT");
    assert_eq!(
        request.path,
        format!("/v1/attachment/{UPLOAD_ID}/part/{expected_part}")
    );
    assert_eq!(request.content_length, ATTACHMENT_MULTIPART_PART_BYTES);
    discard_request_body(&mut stream, request.content_length);
    stream
}

fn respond(stream: &mut TcpStream, body: &str) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    )
    .expect("fixture response");
}

fn assert_resume_record(path: &Path) {
    let record = ProChunkedUploadResumeRecord::load(path).expect("load durable checkpoint");
    assert_eq!(record.upload_id, UPLOAD_ID);
    assert_eq!(record.completed_piece_numbers, (1..=64).collect::<Vec<_>>());
}

#[test]
fn task_0647_network_drop_after_piece_64_resumes_at_65_without_duplicate_completion() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
    let base_url = format!("http://{}", listener.local_addr().expect("fixture address"));
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept session request");
        let request = read_request_head(&mut stream);
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/v1/attachment/session");
        assert_eq!(request.content_length, 0);
        respond(
            &mut stream,
            &format!(
                r#"{{"id":"{UPLOAD_ID}","expires_at":{EXPIRES_AT},"size_bytes":{MAX_SEALED_ATTACHMENT_BYTES},"max_part_bytes":{ATTACHMENT_MULTIPART_PART_BYTES},"max_parts":128}}"#
            ),
        );
        let mut completed_pieces = Vec::new();
        for part in 1..=64 {
            let mut stream = accept_part(&listener, part);
            respond(
                &mut stream,
                &format!(
                    r#"{{"part_number":{part},"size_bytes":{ATTACHMENT_MULTIPART_PART_BYTES}}}"#
                ),
            );
            completed_pieces.push(part);
        }
        let drop_stream = accept_part(&listener, 65);
        drop(drop_stream);

        let mut resumed_first_piece = None;
        for part in 65..=128 {
            let mut stream = accept_part(&listener, part);
            resumed_first_piece.get_or_insert(part);
            respond(
                &mut stream,
                &format!(
                    r#"{{"part_number":{part},"size_bytes":{ATTACHMENT_MULTIPART_PART_BYTES}}}"#
                ),
            );
            completed_pieces.push(part);
        }
        let (mut stream, _) = listener.accept().expect("accept completion request");
        let request = read_request_head(&mut stream);
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, format!("/v1/attachment/{UPLOAD_ID}/complete"));
        assert_eq!(request.content_length, 0);
        respond(
            &mut stream,
            &format!(
                r#"{{"id":"{UPLOAD_ID}","expires_at":{EXPIRES_AT},"size_bytes":{MAX_SEALED_ATTACHMENT_BYTES}}}"#
            ),
        );
        (
            completed_pieces,
            resumed_first_piece.expect("resumed part request"),
            1_u32,
        )
    });

    let file_dir = tempfile::tempdir().expect("sparse file directory");
    let file_path = file_dir.path().join("sealed-1gib.bin");
    File::create(&file_path)
        .expect("create sparse fixture")
        .set_len(MAX_SEALED_ATTACHMENT_BYTES)
        .expect("set sparse fixture size");
    assert_eq!(
        fs::metadata(&file_path)
            .expect("sparse file metadata")
            .len(),
        MAX_SEALED_ATTACHMENT_BYTES
    );
    let resume_path = file_dir.path().join("upload-resume.json");
    let client = CipherStoreClient::new(base_url).expect("client");

    let first = client.upload_attachment_file_pro_chunked_with_resume_record(
        File::open(&file_path).expect("open sparse fixture"),
        TTL_7D,
        &[0x47; 16],
        &resume_path,
    );
    assert!(
        matches!(&first, Err(CipherStoreError::Network(_))),
        "network drop must surface as a network error: {first:?}"
    );
    assert_resume_record(&resume_path);

    let report = client
        .upload_attachment_file_pro_chunked_with_resume_record(
            File::open(&file_path).expect("reopen sparse fixture"),
            TTL_7D,
            &[0x47; 16],
            &resume_path,
        )
        .expect("resume succeeds");
    let (completed_pieces, resumed_first_piece, session_requests) =
        server.join().expect("fixture exits cleanly");

    assert_eq!(completed_pieces, (1..=128).collect::<Vec<_>>());
    assert_eq!(
        completed_pieces
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        128,
        "no completed piece may be logged twice"
    );
    assert_eq!(resumed_first_piece, 65);
    assert_eq!(session_requests, 1, "resume must not open another session");
    assert_eq!(
        report.completed_file.total_size_bytes,
        MAX_SEALED_ATTACHMENT_BYTES
    );
    assert_eq!(report.completed_file.piece_count, 128);
    assert!(!resume_path.exists(), "completion removes the checkpoint");
    println!(
        "TASK0647 fixture_size_bytes=1073741824 network_drop_after_piece=64 resumed_first_piece=65 completed_pieces=128 duplicate_completed_pieces=0 session_requests=1 upload_completed=true"
    );
}
