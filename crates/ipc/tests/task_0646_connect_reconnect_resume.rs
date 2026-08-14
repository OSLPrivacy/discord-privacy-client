//! TASK 0646: a reconnect continues the saved multipart upload at its first
//! unfinished piece instead of opening a new session or resending receipts.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use ipc::cipher_store_client::{
    CipherStoreClient, ProChunkedUploadResumeRecord, ATTACHMENT_MULTIPART_PART_BYTES, TTL_7D,
};

const UPLOAD_ID: &str = "0123456789abcdef0123456789abcdef";
const EXPIRES_AT: i64 = 1_900_000_646;

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
        .expect("request headers");
    let headers = String::from_utf8_lossy(&request[..end]);
    let mut fields = headers
        .lines()
        .next()
        .expect("request line")
        .split_whitespace();
    (
        fields.next().expect("method").to_owned(),
        fields.next().expect("path").to_owned(),
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
fn task_0646_resume_after_piece_three_starts_at_piece_four_without_resending_one_to_three() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
    let base_url = format!("http://{}", listener.local_addr().expect("fixture address"));
    let server = thread::spawn(move || {
        let (mut part_stream, _) = listener.accept().expect("accept resumed part");
        let part_request = read_request(&mut part_stream);
        assert_eq!(
            request_line(&part_request),
            (
                "PUT".to_owned(),
                format!("/v1/attachment/{UPLOAD_ID}/part/4"),
            ),
            "the first reconnect request must be the only unfinished piece"
        );
        respond(
            &mut part_stream,
            "200 OK",
            r#"{"part_number":4,"size_bytes":1}"#,
        );

        let (mut complete_stream, _) = listener.accept().expect("accept completion");
        let complete_request = read_request(&mut complete_stream);
        assert_eq!(
            request_line(&complete_request),
            (
                "POST".to_owned(),
                format!("/v1/attachment/{UPLOAD_ID}/complete"),
            ),
            "only completion may follow piece 4"
        );
        respond(
            &mut complete_stream,
            "200 OK",
            &format!(
                r#"{{"id":"{UPLOAD_ID}","expires_at":{EXPIRES_AT},"size_bytes":{}}}"#,
                ATTACHMENT_MULTIPART_PART_BYTES * 3 + 1
            ),
        );
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
    fs::write(
        &resume_path,
        serde_json::to_vec(&ProChunkedUploadResumeRecord {
            upload_id: UPLOAD_ID.to_owned(),
            completed_piece_numbers: vec![1, 2, 3],
        })
        .expect("serialize checkpoint"),
    )
    .expect("save checkpoint");

    let report = CipherStoreClient::new(base_url)
        .expect("client")
        .upload_attachment_file_pro_chunked_with_resume_record(
            sealed.reopen().expect("reopen sealed fixture"),
            TTL_7D,
            &[0x46; 16],
            &resume_path,
        )
        .expect("resume succeeds");
    server.join().expect("fixture exits cleanly");

    assert_eq!(report.completed_file.file_id, UPLOAD_ID);
    assert_eq!(report.finished_pieces.len(), 1);
    assert_eq!(report.finished_pieces[0].piece_number, 4);
    assert!(
        !resume_path.exists(),
        "completion removes the now-obsolete checkpoint"
    );
    println!(
        "TASK0646 upload_id={UPLOAD_ID} first_requested_piece=4 resent_pieces=0 completed_pieces=4"
    );
}
