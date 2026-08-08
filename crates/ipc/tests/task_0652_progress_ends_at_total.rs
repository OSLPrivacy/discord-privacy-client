//! TASK 0652: a direct multipart upload is visible while active, reaches its
//! exact total on completion, and a stalled upload cannot masquerade as done.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc, Barrier};
use std::thread;

use ipc::cipher_store_client::{
    ActiveProChunkedUploadProgress, CipherStoreClient, ProChunkedUploadProgress,
    ATTACHMENT_MULTIPART_MAX_PARTS, ATTACHMENT_MULTIPART_PART_BYTES, TTL_7D,
};

const BYTES: u64 = 37;
const UPLOAD_ID: &str = "0123456789abcdef0123456789abcdef";

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream.read(&mut chunk).expect("read fixture request");
        assert_ne!(read, 0, "fixture request ended early");
        request.extend_from_slice(&chunk[..read]);
        let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") else { continue };
        let headers = String::from_utf8_lossy(&request[..end]);
        let length = headers.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().expect("numeric content length"))
        }).expect("content length");
        if request.len() >= end + 4 + length { return request; }
    }
}

fn request_path(request: &[u8]) -> String {
    String::from_utf8_lossy(request).lines().next().expect("request line")
        .split_whitespace().nth(1).expect("path").to_owned()
}

fn respond_json(stream: &mut TcpStream, body: &str) {
    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).expect("fixture response");
}

#[test]
fn task_0652_direct_upload_progress_ends_at_total_and_stall_is_not_complete() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
    let address = listener.local_addr().expect("fixture address");
    let stalled = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let fixture_stalled = Arc::clone(&stalled);
    let fixture_release = Arc::clone(&release);
    let server = thread::spawn(move || {
        let (mut session, _) = listener.accept().expect("session request");
        assert_eq!(request_path(&read_request(&mut session)), "/v1/attachment/session");
        fixture_stalled.wait();
        fixture_release.wait();
        respond_json(&mut session, &format!(r#"{{"id":"{UPLOAD_ID}","expires_at":1900000652,"size_bytes":{BYTES},"max_part_bytes":{ATTACHMENT_MULTIPART_PART_BYTES},"max_parts":{ATTACHMENT_MULTIPART_MAX_PARTS}}}"#));

        let (mut part, _) = listener.accept().expect("part request");
        let part_request = read_request(&mut part);
        assert_eq!(request_path(&part_request), format!("/v1/attachment/{UPLOAD_ID}/part/1"));
        respond_json(&mut part, &format!(r#"{{"part_number":1,"size_bytes":{BYTES}}}"#));

        let (mut complete, _) = listener.accept().expect("complete request");
        assert_eq!(request_path(&read_request(&mut complete)), format!("/v1/attachment/{UPLOAD_ID}/complete"));
        respond_json(&mut complete, &format!(r#"{{"id":"{UPLOAD_ID}","expires_at":1900000652,"size_bytes":{BYTES}}}"#));
    });

    let mut sealed = tempfile::NamedTempFile::new().expect("sealed fixture");
    sealed.write_all(&[0x65; BYTES as usize]).expect("write fixture");
    sealed.flush().expect("flush fixture");
    let progress = ProChunkedUploadProgress::default();
    let active = ActiveProChunkedUploadProgress::default();
    let (done_tx, done_rx) = mpsc::channel();
    let url = format!("http://{address}");
    let upload_file = sealed.reopen().expect("reopen fixture");
    let upload_progress = progress.clone();
    let upload_active = active.clone();
    thread::spawn(move || {
        let result = CipherStoreClient::new(url).expect("client")
            .upload_attachment_file_pro_chunked_with_active_progress(upload_file, TTL_7D, &[0x52; 16], &upload_progress, &upload_active);
        done_tx.send(result).expect("return upload result");
    });

    stalled.wait();
    let stalled_entries = active.query();
    assert_eq!(stalled_entries.len(), 1);
    let stalled_progress = stalled_entries[0];
    println!("TASK0652 stalled active_entries={} uploaded_bytes={} total_bytes={}", stalled_entries.len(), stalled_progress.uploaded_bytes, stalled_progress.total_bytes);
    assert_ne!(stalled_progress.uploaded_bytes, stalled_progress.total_bytes, "stalled upload must not look complete");

    release.wait();
    let (report, final_progress) = done_rx.recv().expect("upload thread result").expect("direct fixture upload completes");
    assert_eq!(report.completed_file.total_size_bytes, BYTES);
    assert_eq!(final_progress.uploaded_bytes, final_progress.total_bytes);
    assert_eq!(final_progress.total_bytes, BYTES);
    let after_completion = active.query();
    println!("TASK0652 completed uploaded_bytes={} total_bytes={} active_entries_before=1 active_entries_after={}", final_progress.uploaded_bytes, final_progress.total_bytes, after_completion.len());
    assert!(after_completion.is_empty());
    server.join().expect("fixture exits");
}
