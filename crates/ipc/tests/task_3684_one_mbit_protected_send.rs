//! TASK 3684: exercise a resumable 25 MiB protected attachment over a measured
//! one-megabit loopback link, then prove a second active copy can be cancelled.

use std::fs::File;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use ipc::cipher_store_client::{
    CipherStoreClient, CipherStoreError, ProChunkedUploadProgress, ProChunkedUploadResumeRecord,
    ATTACHMENT_MULTIPART_MAX_PARTS, ATTACHMENT_MULTIPART_PART_BYTES, TTL_7D,
};
use sha2::{Digest, Sha256};

const FILE_BYTES: u64 = 25 * 1024 * 1024;
const BITS_PER_SECOND: f64 = 1_000_000.0;
const FIRST_UPLOAD_ID: &str = "0123456789abcdef0123456789abcdef";
const SECOND_UPLOAD_ID: &str = "fedcba9876543210fedcba9876543210";
const EXPIRES_AT: i64 = 1_900_003_684;
const DROP_BYTES: u64 = 512 * 1024;

#[derive(Debug)]
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
        assert!(headers.len() < 32 * 1024, "bounded request headers");
    }
    let headers = String::from_utf8(headers).expect("UTF-8 fixture headers");
    let mut lines = headers.lines();
    let mut request_line = lines.next().expect("request line").split_whitespace();
    let method = request_line.next().expect("request method").to_owned();
    let path = request_line.next().expect("request path").to_owned();
    let content_length = lines
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

fn accept_request(listener: &TcpListener, method: &str, path: &str) -> (TcpStream, RequestHead) {
    let (mut stream, _) = listener.accept().expect("accept fixture request");
    let request = read_request_head(&mut stream);
    assert_eq!(request.method, method);
    assert_eq!(request.path, path);
    (stream, request)
}

fn respond(stream: &mut TcpStream, body: &str) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    )
    .expect("write fixture response");
}

fn respond_session(stream: &mut TcpStream, upload_id: &str) {
    respond(
        stream,
        &format!(
            r#"{{"id":"{upload_id}","expires_at":{EXPIRES_AT},"size_bytes":{FILE_BYTES},"max_part_bytes":{ATTACHMENT_MULTIPART_PART_BYTES},"max_parts":{ATTACHMENT_MULTIPART_MAX_PARTS}}}"#,
        ),
    );
}

/// Drain one request at exactly one decimal megabit per second. The returned
/// duration includes the final pacing interval, so the measured rate cannot be
/// an artifact of a full localhost socket buffer.
fn read_at_one_mbit(stream: &mut TcpStream, length: u64, stored: &mut Vec<u8>) -> Duration {
    let started = Instant::now();
    let mut received = 0_u64;
    let mut buffer = [0_u8; 16 * 1024];
    while received < length {
        let wanted = usize::try_from((length - received).min(buffer.len() as u64))
            .expect("bounded body read");
        stream
            .read_exact(&mut buffer[..wanted])
            .expect("read throttled request body");
        stored.extend_from_slice(&buffer[..wanted]);
        received += wanted as u64;

        let due = Duration::from_secs_f64(received as f64 * 8.0 / BITS_PER_SECOND);
        if let Some(remaining) = due.checked_sub(started.elapsed()) {
            thread::sleep(remaining);
        }
    }
    started.elapsed()
}

fn drop_after_bytes(stream: &mut TcpStream, length: u64) {
    let started = Instant::now();
    let mut received = 0_u64;
    let mut buffer = [0_u8; 16 * 1024];
    while received < length {
        let wanted = usize::try_from((length - received).min(buffer.len() as u64))
            .expect("bounded dropped body read");
        stream
            .read_exact(&mut buffer[..wanted])
            .expect("read body before simulated interruption");
        received += wanted as u64;
        let due = Duration::from_secs_f64(received as f64 * 8.0 / BITS_PER_SECOND);
        if let Some(remaining) = due.checked_sub(started.elapsed()) {
            thread::sleep(remaining);
        }
    }
    stream
        .shutdown(Shutdown::Both)
        .expect("interrupt first upload connection");
}

fn accept_part(listener: &TcpListener, upload_id: &str, number: u32, length: u64) -> TcpStream {
    let path = format!("/v1/attachment/{upload_id}/part/{number}");
    let (stream, request) = accept_request(listener, "PUT", &path);
    assert_eq!(request.content_length, length);
    stream
}

fn observe_progress_until<T>(
    progress: &ProChunkedUploadProgress,
    result: &mpsc::Receiver<T>,
) -> (T, Vec<u64>) {
    let deadline = Instant::now() + Duration::from_secs(300);
    let mut rises = Vec::new();
    let mut previous = 0;
    loop {
        let uploaded = progress.query().uploaded_bytes;
        if uploaded > previous {
            rises.push(uploaded);
            previous = uploaded;
        }
        match result.try_recv() {
            Ok(value) => return (value, rises),
            Err(mpsc::TryRecvError::Empty) => {
                assert!(Instant::now() < deadline, "upload observation timed out");
                thread::sleep(Duration::from_millis(25));
            }
            Err(mpsc::TryRecvError::Disconnected) => panic!("upload worker disappeared"),
        }
    }
}

fn write_protected_fixture(path: &Path) -> String {
    let mut file = File::create(path).expect("create protected fixture");
    let mut hasher = Sha256::new();
    let mut written = 0_u64;
    let mut block = [0_u8; 64 * 1024];
    while written < FILE_BYTES {
        for (offset, byte) in block.iter_mut().enumerate() {
            let position = written + offset as u64;
            *byte = (position.wrapping_mul(131).wrapping_add(position >> 9) as u64) as u8;
        }
        let count = usize::try_from((FILE_BYTES - written).min(block.len() as u64))
            .expect("fixture block size");
        file.write_all(&block[..count])
            .expect("write protected fixture bytes");
        hasher.update(&block[..count]);
        written += count as u64;
    }
    file.flush().expect("flush protected fixture");
    format!("{:x}", hasher.finalize())
}

#[test]
fn task_3684_one_mbit_protected_send_retries_then_second_copy_cancels() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind one-megabit fixture");
    let base_url = format!("http://{}", listener.local_addr().expect("fixture address"));
    let received_messages = Arc::new(AtomicUsize::new(0));
    let server_received_messages = Arc::clone(&received_messages);
    let (second_part_started_tx, second_part_started_rx) = mpsc::channel();
    let (second_cancel_set_tx, second_cancel_set_rx) = mpsc::channel();
    let (second_body_finished_tx, second_body_finished_rx) = mpsc::channel();

    let server = thread::spawn(move || {
        let (mut session, request) = accept_request(&listener, "POST", "/v1/attachment/session");
        assert_eq!(request.content_length, 0);
        respond_session(&mut session, FIRST_UPLOAD_ID);

        let mut stored = Vec::with_capacity(FILE_BYTES as usize);
        let mut measured = Duration::ZERO;

        let mut part_one = accept_part(
            &listener,
            FIRST_UPLOAD_ID,
            1,
            ATTACHMENT_MULTIPART_PART_BYTES,
        );
        measured += read_at_one_mbit(&mut part_one, ATTACHMENT_MULTIPART_PART_BYTES, &mut stored);
        respond(
            &mut part_one,
            &format!(r#"{{"part_number":1,"size_bytes":{ATTACHMENT_MULTIPART_PART_BYTES}}}"#),
        );

        let mut interrupted_part = accept_part(
            &listener,
            FIRST_UPLOAD_ID,
            2,
            ATTACHMENT_MULTIPART_PART_BYTES,
        );
        drop_after_bytes(&mut interrupted_part, DROP_BYTES);

        for (number, length) in [
            (2, ATTACHMENT_MULTIPART_PART_BYTES),
            (3, ATTACHMENT_MULTIPART_PART_BYTES),
            (4, 1024 * 1024),
        ] {
            let mut part = accept_part(&listener, FIRST_UPLOAD_ID, number, length);
            measured += read_at_one_mbit(&mut part, length, &mut stored);
            respond(
                &mut part,
                &format!(r#"{{"part_number":{number},"size_bytes":{length}}}"#),
            );
        }

        let complete_path = format!("/v1/attachment/{FIRST_UPLOAD_ID}/complete");
        let (mut complete, request) = accept_request(&listener, "POST", &complete_path);
        assert_eq!(request.content_length, 0);
        assert_eq!(server_received_messages.load(Ordering::SeqCst), 0);
        assert_eq!(stored.len() as u64, FILE_BYTES);
        let received_hash = format!("{:x}", Sha256::digest(&stored));
        server_received_messages.fetch_add(1, Ordering::SeqCst);
        respond(
            &mut complete,
            &format!(
                r#"{{"id":"{FIRST_UPLOAD_ID}","expires_at":{EXPIRES_AT},"size_bytes":{FILE_BYTES}}}"#
            ),
        );

        let (mut second_session, request) =
            accept_request(&listener, "POST", "/v1/attachment/session");
        assert_eq!(request.content_length, 0);
        respond_session(&mut second_session, SECOND_UPLOAD_ID);

        let mut second_part = accept_part(
            &listener,
            SECOND_UPLOAD_ID,
            1,
            ATTACHMENT_MULTIPART_PART_BYTES,
        );
        let mut one_byte = [0_u8; 1];
        second_part
            .read_exact(&mut one_byte)
            .expect("second copy starts its first body");
        second_part_started_tx
            .send(())
            .expect("announce second copy body");
        second_cancel_set_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("wait until Cancel is set before dropping connection");
        let mut second_body_bytes = 1_u64;
        let mut buffer = [0_u8; 16 * 1024];
        while second_body_bytes < ATTACHMENT_MULTIPART_PART_BYTES {
            match second_part.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => second_body_bytes += read as u64,
            }
        }
        let body_continued_after_cancel = second_body_bytes == ATTACHMENT_MULTIPART_PART_BYTES;
        second_body_finished_tx
            .send(body_continued_after_cancel)
            .expect("report whether Cancel interrupted the body");
        if body_continued_after_cancel {
            // Keep a Cancel-ignoring mutant active long enough for the check
            // to observe the forbidden 1 instead of manufacturing a network
            // error that would clear the handle for an unrelated reason.
            thread::sleep(Duration::from_secs(10));
        }
        drop(second_part);

        listener.set_nonblocking(true).expect("set nonblocking");
        thread::sleep(Duration::from_millis(250));
        assert!(
            listener.accept().is_err(),
            "Cancel must prevent another part or completion request"
        );
        assert_eq!(server_received_messages.load(Ordering::SeqCst), 1);

        let measured_mbit = FILE_BYTES as f64 * 8.0 / measured.as_secs_f64() / 1_000_000.0;
        (measured_mbit, received_hash)
    });

    let directory = tempfile::tempdir().expect("protected-send directory");
    let protected_path = directory.path().join("protected-25mb.osl");
    let original_hash = write_protected_fixture(&protected_path);
    assert_eq!(
        protected_path.metadata().expect("fixture metadata").len(),
        FILE_BYTES
    );
    let resume_path = directory.path().join("first-upload-resume.json");
    let first_progress = ProChunkedUploadProgress::default();

    assert_eq!(received_messages.load(Ordering::SeqCst), 0);
    let (first_tx, first_rx) = mpsc::channel();
    let first_url = base_url.clone();
    let first_file = protected_path.clone();
    let first_resume = resume_path.clone();
    let first_worker_progress = first_progress.clone();
    let first_worker = thread::spawn(move || {
        let result = CipherStoreClient::new(first_url)
            .expect("first client")
            .upload_attachment_file_pro_chunked_with_resume_record_and_progress(
                File::open(first_file).expect("open protected fixture"),
                TTL_7D,
                &[0x84; 16],
                first_resume,
                &first_worker_progress,
            );
        first_tx.send(result).expect("return interrupted result");
    });
    let (first_result, _) = observe_progress_until(&first_progress, &first_rx);
    first_worker.join().expect("interrupted worker exits");
    assert!(
        matches!(first_result, Err(CipherStoreError::Network(_))),
        "the simulated connection interruption must reach Retry: {first_result:?}"
    );
    let checkpoint = ProChunkedUploadResumeRecord::load(&resume_path)
        .expect("interrupted send leaves its checkpoint");
    assert_eq!(checkpoint.upload_id, FIRST_UPLOAD_ID);
    assert_eq!(checkpoint.completed_piece_numbers, vec![1]);

    let (retry_tx, retry_rx) = mpsc::channel();
    let retry_url = base_url.clone();
    let retry_file = protected_path.clone();
    let retry_resume = resume_path.clone();
    let retry_worker_progress = first_progress.clone();
    let retry_worker = thread::spawn(move || {
        let result = CipherStoreClient::new(retry_url)
            .expect("retry client")
            .upload_attachment_file_pro_chunked_with_resume_record_and_progress(
                File::open(retry_file).expect("reopen protected fixture"),
                TTL_7D,
                &[0x84; 16],
                retry_resume,
                &retry_worker_progress,
            );
        retry_tx.send(result).expect("return retry result");
    });
    let (retry_result, progress_rises) = observe_progress_until(&first_progress, &retry_rx);
    retry_worker.join().expect("retry worker exits");
    let retry_report = retry_result.expect("Retry completes");
    assert_eq!(retry_report.completed_file.file_id, FIRST_UPLOAD_ID);
    assert_eq!(retry_report.completed_file.total_size_bytes, FILE_BYTES);
    assert!(!resume_path.exists(), "successful Retry removes checkpoint");
    assert!(
        progress_rises.len() >= 5,
        "visible progress must rise at least five times: {progress_rises:?}"
    );
    assert!(
        progress_rises.windows(2).all(|pair| pair[0] < pair[1]),
        "visible progress samples must rise strictly"
    );

    let second_progress = ProChunkedUploadProgress::default();
    let second_resume = directory.path().join("second-upload-resume.json");
    let (second_tx, second_rx) = mpsc::channel();
    let second_url = base_url;
    let second_file = protected_path;
    let second_worker_progress = second_progress.clone();
    let second_worker = thread::spawn(move || {
        let result = CipherStoreClient::new(second_url)
            .expect("second client")
            .upload_attachment_file_pro_chunked_with_resume_record_and_progress(
                File::open(second_file).expect("open second protected copy"),
                TTL_7D,
                &[0x85; 16],
                second_resume,
                &second_worker_progress,
            );
        second_tx.send(result).expect("return cancelled result");
    });

    second_part_started_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("second copy reaches first request body");
    let active_before_cancel = second_progress.active_upload_count();
    assert_eq!(active_before_cancel, 1);
    assert!(second_progress.cancel(), "Cancel reaches second copy");
    second_cancel_set_tx
        .send(())
        .expect("release cancelled fixture request");
    let body_continued_after_cancel = second_body_finished_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("fixture observes Cancel at the request-body boundary");
    if body_continued_after_cancel {
        assert_eq!(
            second_progress.active_upload_count(),
            0,
            "active-upload count stays 1 after Cancel while upload pieces continue"
        );
    }
    let second_result = second_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("cancelled worker returns");
    second_worker.join().expect("cancelled worker exits");
    assert!(matches!(
        second_result,
        Err(CipherStoreError::UploadCancelled)
    ));
    let active_after_cancel = second_progress.active_upload_count();
    assert_eq!(active_before_cancel, 1);
    assert_eq!(active_after_cancel, 0);

    let (measured_mbit, received_hash) = server.join().expect("fixture exits cleanly");
    assert!(
        (0.95..=1.05).contains(&measured_mbit),
        "measured speed {measured_mbit:.6} Mbit/s is outside 0.95..=1.05"
    );
    assert_eq!(
        received_hash, original_hash,
        "Retry preserves original hash"
    );
    assert_eq!(received_messages.load(Ordering::SeqCst), 1);

    let shown_samples = progress_rises.iter().take(6).copied().collect::<Vec<_>>();
    println!(
        "TASK3684 file_bytes={FILE_BYTES} measured_speed_mbit={measured_mbit:.6} progress_rises={} progress_samples={shown_samples:?} interrupted=true retry_completed=true original_sha256={original_hash} received_sha256={received_hash} received_message_count=0->{} active_upload_count={active_before_cancel}->{active_after_cancel}",
        progress_rises.len(),
        received_messages.load(Ordering::SeqCst),
    );
}
