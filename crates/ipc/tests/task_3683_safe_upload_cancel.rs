//! TASK 3683: cancelling a resumable upload stops before a new part, removes
//! its local checkpoint, and cannot disturb a file that was already stored.

use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use ipc::cipher_store_client::{
    CipherStoreClient, CipherStoreError, ProChunkedUploadProgress,
    ATTACHMENT_MULTIPART_PART_BYTES, TTL_7D,
};

const UPLOAD_ID: &str = "0123456789abcdef0123456789abcdef";
const EXISTING_MARK: &[u8] = b"task-3683-earlier-stored-file-mark";

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
                    value.trim().parse::<usize>().expect("numeric Content-Length")
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

fn wait_for_active(progress: &ProChunkedUploadProgress) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while progress.active_upload_count() == 0 {
        assert!(Instant::now() < deadline, "upload did not become active");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn task_3683_cancel_stops_new_parts_removes_checkpoint_and_preserves_stored_file() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
    let base_url = format!("http://{}", listener.local_addr().expect("fixture address"));
    let directory = tempfile::tempdir().expect("temporary files");
    let resume_path = directory.path().join("upload-resume.json");
    let earlier_file = directory.path().join("earlier-stored-file");
    fs::write(&earlier_file, EXISTING_MARK).expect("write earlier stored file");

    let progress = ProChunkedUploadProgress::default();
    assert_eq!(progress.active_upload_count(), 0);
    let (before_cancel_tx, before_cancel_rx) = mpsc::channel();
    let server_progress = progress.clone();
    let server_resume_path = resume_path.clone();
    let server = thread::spawn(move || {
        let (mut session_stream, _) = listener.accept().expect("accept session");
        let session_request = read_request(&mut session_stream);
        assert_eq!(request_line(&session_request).0, "POST");
        assert_eq!(request_line(&session_request).1, "/v1/attachment/session");
        respond(
            &mut session_stream,
            "200 OK",
            &format!(
                r#"{{"id":"{UPLOAD_ID}","expires_at":1900003683,"size_bytes":{},"max_part_bytes":{},"max_parts":65}}"#,
                ATTACHMENT_MULTIPART_PART_BYTES * 4 + 1,
                ATTACHMENT_MULTIPART_PART_BYTES,
            ),
        );

        for part_number in 1..=4 {
            let (mut part_stream, _) = listener.accept().expect("accept numbered part");
            let part_request = read_request(&mut part_stream);
            assert_eq!(
                request_line(&part_request),
                (
                    "PUT".to_owned(),
                    format!("/v1/attachment/{UPLOAD_ID}/part/{part_number}"),
                ),
                "only the first four parts may reach the store before Cancel"
            );
            respond(
                &mut part_stream,
                "200 OK",
                &format!(
                    r#"{{"part_number":{part_number},"size_bytes":{ATTACHMENT_MULTIPART_PART_BYTES}}}"#
                ),
            );
        }

        let deadline = Instant::now() + Duration::from_secs(10);
        while server_progress.unfinished_local_part_count() != 4 {
            assert!(Instant::now() < deadline, "four receipts were not checkpointed");
            thread::yield_now();
        }
        assert!(server_resume_path.exists(), "four-part resume record exists before Cancel");
        before_cancel_tx
            .send((
                server_progress.active_upload_count(),
                server_progress.unfinished_local_part_count(),
            ))
            .expect("report pre-cancel counts");
        assert!(server_progress.cancel(), "Cancel reaches the active upload");

        listener.set_nonblocking(true).expect("set nonblocking");
        thread::sleep(Duration::from_millis(250));
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock),
            "Cancel must stop a fifth part or completion request"
        );
    });

    let mut sealed = tempfile::NamedTempFile::new().expect("sealed fixture");
    sealed
        .write_all(&vec![0x68; (ATTACHMENT_MULTIPART_PART_BYTES * 4 + 1) as usize])
        .expect("write five-part fixture");
    sealed.flush().expect("flush sealed fixture");
    let client_progress = progress.clone();
    let client_resume_path = resume_path.clone();
    let client = thread::spawn(move || {
        CipherStoreClient::new(base_url)
            .expect("client")
            .upload_attachment_file_pro_chunked_with_resume_record_and_progress(
                sealed.reopen().expect("reopen sealed fixture"),
                TTL_7D,
                &[0x83; 16],
                client_resume_path,
                &client_progress,
            )
    });

    wait_for_active(&progress);
    let active_after_start = progress.active_upload_count();
    assert_eq!(active_after_start, 1);
    let (active_before_cancel, unfinished_before_cancel) = before_cancel_rx
        .recv_timeout(Duration::from_secs(15))
        .expect("server reaches four checkpointed parts");
    assert_eq!(active_before_cancel, 1);
    assert_eq!(unfinished_before_cancel, 4);

    let error = client.join().expect("client worker exits").unwrap_err();
    assert!(matches!(error, CipherStoreError::UploadCancelled), "{error}");
    server.join().expect("server exits cleanly");

    assert_eq!(progress.active_upload_count(), 0);
    assert_eq!(progress.unfinished_local_part_count(), 0);
    assert!(!resume_path.exists(), "Cancel removes unfinished local parts");
    assert_eq!(fs::read(&earlier_file).expect("read earlier file"), EXISTING_MARK);
    println!(
        "TASK3683 active_before_start=0 active_after_start={active_after_start} active_after_cancel={} unfinished_local_parts_before_cancel={unfinished_before_cancel} unfinished_local_parts_after_cancel={} earlier_stored_file_mark={}",
        progress.active_upload_count(),
        progress.unfinished_local_part_count(),
        String::from_utf8_lossy(EXISTING_MARK),
    );
}
