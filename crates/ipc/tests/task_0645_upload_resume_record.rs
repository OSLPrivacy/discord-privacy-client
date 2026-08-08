//! TASK 0645: each accepted multipart piece immediately checkpoints resume state.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use ipc::cipher_store_client::{
    CipherStoreClient, ProChunkedUploadResumeRecord, ATTACHMENT_MULTIPART_MAX_PARTS,
    ATTACHMENT_MULTIPART_PART_BYTES, TTL_7D,
};

const UPLOAD_ID: &str = "0123456789abcdef0123456789abcdef";
const EXPIRES_AT: i64 = 1_900_000_645;

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
            .unwrap_or(0);
        if request.len() >= headers_end + 4 + content_length {
            return request;
        }
    }
}

fn request_line(request: &[u8]) -> (String, String) {
    let end = request
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .unwrap();
    let headers = String::from_utf8_lossy(&request[..end]);
    let mut fields = headers.lines().next().unwrap().split_whitespace();
    (
        fields.next().unwrap().to_owned(),
        fields.next().unwrap().to_owned(),
    )
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    )
    .expect("fixture response");
}

#[test]
fn task_0645_stop_after_piece_three_keeps_upload_id_and_pieces_one_through_three() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
    let base_url = format!("http://{}", listener.local_addr().expect("fixture address"));
    let server = thread::spawn(move || {
        for request_index in 0..5 {
            let (mut stream, _) = listener.accept().expect("accept request");
            let request = read_request(&mut stream);
            let (method, path) = request_line(&request);
            match request_index {
                0 => {
                    assert_eq!(
                        (method.as_str(), path.as_str()),
                        ("POST", "/v1/attachment/session")
                    );
                    respond(
                        &mut stream,
                        "200 OK",
                        &format!(
                            r#"{{"id":"{UPLOAD_ID}","expires_at":{EXPIRES_AT},"size_bytes":{},"max_part_bytes":{ATTACHMENT_MULTIPART_PART_BYTES},"max_parts":{ATTACHMENT_MULTIPART_MAX_PARTS}}}"#,
                            ATTACHMENT_MULTIPART_PART_BYTES * 3 + 1,
                        ),
                    );
                }
                1..=3 => {
                    let piece = request_index as u32;
                    assert_eq!(method, "PUT");
                    assert_eq!(path, format!("/v1/attachment/{UPLOAD_ID}/part/{piece}"));
                    respond(
                        &mut stream,
                        "200 OK",
                        &format!(
                            r#"{{"part_number":{piece},"size_bytes":{ATTACHMENT_MULTIPART_PART_BYTES}}}"#
                        ),
                    );
                }
                4 => {
                    assert_eq!(path, format!("/v1/attachment/{UPLOAD_ID}/part/4"));
                    respond(
                        &mut stream,
                        "503 Service Unavailable",
                        "stopped after piece 3",
                    );
                }
                _ => unreachable!("the resumable stop must retain its server upload"),
            }
        }
    });

    let mut sealed = tempfile::NamedTempFile::new().expect("sealed fixture");
    sealed
        .write_all(&vec![
            0x64;
            (ATTACHMENT_MULTIPART_PART_BYTES * 3 + 1) as usize
        ])
        .expect("write sealed fixture");
    sealed.flush().expect("flush sealed fixture");
    let resume_dir = tempfile::tempdir().expect("resume directory");
    let resume_path = resume_dir.path().join("upload-resume.json");

    let stopped = CipherStoreClient::new(base_url)
        .expect("client")
        .upload_attachment_file_pro_chunked_with_resume_record(
            sealed.reopen().expect("reopen sealed fixture"),
            TTL_7D,
            &[0x45; 16],
            &resume_path,
        );
    assert!(stopped.is_err(), "fixture must stop after piece 3");
    server.join().expect("fixture exits cleanly");

    let record =
        ProChunkedUploadResumeRecord::load(&resume_path).expect("resume record survives stop");
    assert_eq!(record.upload_id, UPLOAD_ID);
    assert_eq!(record.completed_piece_numbers, vec![1, 2, 3]);
    println!(
        "TASK0645 upload_id={} completed_pieces={:?} stop_after_piece=3",
        record.upload_id, record.completed_piece_numbers
    );
}
