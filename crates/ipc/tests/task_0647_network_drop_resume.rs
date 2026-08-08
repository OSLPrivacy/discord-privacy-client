//! TASK 0647: a one-gibibyte Pro upload resumes after a dropped part request.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use ipc::cipher_store_client::{
    CipherStoreClient, CipherStoreError, ProChunkedUploadResumeRecord,
    ATTACHMENT_MULTIPART_PART_BYTES, MAX_SEALED_ATTACHMENT_BYTES, TTL_7D,
};

const UPLOAD_ID: &str = "0123456789abcdef0123456789abcdef";
const EXPIRES_AT: i64 = 1_900_000_647;
const TASK3583_CHILD_ENV: &str = "TASK3583_CHILD";
const TASK3583_ROOT_ENV: &str = "TASK3583_ROOT";
const TASK3583_CONTROL_ID: &str = "35833583358335833583358335833583";
const TASK3583_FAILED_ID: &str = "83588358835883588358835883588358";
const TASK3583_FILE_BYTES: u64 = ATTACHMENT_MULTIPART_PART_BYTES * 2;

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

fn respond_status(stream: &mut TcpStream, status: &str, body: &str) {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    )
    .expect("fixture error response");
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

fn task3583_session_path(root: &Path, upload_id: &str) -> PathBuf {
    root.join("multipart-sessions").join(upload_id)
}

fn task3583_regular_entry_count(path: &Path) -> usize {
    fs::read_dir(path)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .count()
}

fn task3583_directory_entry_count(path: &Path) -> usize {
    fs::read_dir(path)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .count()
}

fn task3583_completed_upload_count(root: &Path) -> usize {
    task3583_directory_entry_count(&root.join("completed-uploads"))
}

fn task3583_retained_key_count(root: &Path) -> usize {
    task3583_regular_entry_count(&root.join("retained-keys"))
}

fn task3583_part_count(root: &Path, upload_id: &str) -> usize {
    task3583_regular_entry_count(&task3583_session_path(root, upload_id))
}

fn task3583_read_part_to_path(stream: &mut TcpStream, length: u64, path: &Path) -> io::Result<()> {
    let mut file = File::create(path)?;
    let mut remaining = length;
    let mut buffer = [0_u8; 64 * 1024];
    while remaining != 0 {
        let wanted =
            usize::try_from(remaining.min(buffer.len() as u64)).expect("part read fits buffer");
        stream.read_exact(&mut buffer[..wanted])?;
        file.write_all(&buffer[..wanted])?;
        remaining -= wanted as u64;
    }
    file.sync_all()
}

fn task3583_accept_part_to_disk(
    listener: &TcpListener,
    root: &Path,
    upload_id: &str,
    part_number: u32,
) {
    let (mut stream, _) = listener.accept().expect("accept disk-backed part request");
    let request = read_request_head(&mut stream);
    assert_eq!(request.method, "PUT");
    assert_eq!(
        request.path,
        format!("/v1/attachment/{upload_id}/part/{part_number}")
    );
    assert_eq!(request.content_length, ATTACHMENT_MULTIPART_PART_BYTES);
    let session = task3583_session_path(root, upload_id);
    fs::create_dir_all(&session).expect("create multipart session directory");
    task3583_read_part_to_path(
        &mut stream,
        request.content_length,
        &session.join(format!("part-{part_number}")),
    )
    .expect("persist complete multipart part");
    respond(
        &mut stream,
        &format!(
            r#"{{"part_number":{part_number},"size_bytes":{}}}"#,
            request.content_length
        ),
    );
}

fn task3583_accept_session(listener: &TcpListener, upload_id: &str) {
    let (mut stream, _) = listener.accept().expect("accept multipart session request");
    let request = read_request_head(&mut stream);
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/attachment/session");
    assert_eq!(request.content_length, 0);
    respond(
        &mut stream,
        &format!(
            r#"{{"id":"{upload_id}","expires_at":{EXPIRES_AT},"size_bytes":{TASK3583_FILE_BYTES},"max_part_bytes":{ATTACHMENT_MULTIPART_PART_BYTES},"max_parts":128}}"#
        ),
    );
}

fn task3583_finish_control(listener: &TcpListener, root: &Path) {
    let (mut stream, _) = listener
        .accept()
        .expect("accept control completion request");
    let request = read_request_head(&mut stream);
    assert_eq!(request.method, "POST");
    assert_eq!(
        request.path,
        format!("/v1/attachment/{TASK3583_CONTROL_ID}/complete")
    );
    assert_eq!(request.content_length, 0);

    fs::create_dir_all(root.join("completed-uploads")).expect("create completed-upload directory");
    fs::rename(
        task3583_session_path(root, TASK3583_CONTROL_ID),
        root.join("completed-uploads").join(TASK3583_CONTROL_ID),
    )
    .expect("atomically publish completed control upload");
    fs::create_dir_all(root.join("retained-keys")).expect("create retained-key directory");
    fs::write(
        root.join("retained-keys")
            .join(format!("{TASK3583_CONTROL_ID}.key")),
        [0x83_u8; 16],
    )
    .expect("retain the control upload key");
    respond(
        &mut stream,
        &format!(
            r#"{{"id":"{TASK3583_CONTROL_ID}","expires_at":{EXPIRES_AT},"size_bytes":{TASK3583_FILE_BYTES}}}"#
        ),
    );
}

fn task3583_fill_disk(root: &Path) -> (bool, u64) {
    let mut filler = File::create(root.join("disk-filler.bin")).expect("create disk filler");
    let block = [0x5a_u8; 64 * 1024];
    let mut written = 0_u64;
    let saw_enospc = loop {
        match filler.write(&block) {
            Ok(0) => break false,
            Ok(count) => written += count as u64,
            Err(error) if error.raw_os_error() == Some(28) => break true,
            Err(error) => panic!("disk fill failed before ENOSPC: {error}"),
        }
    };
    (saw_enospc, written)
}

fn task3583_fail_next_part(listener: &TcpListener, root: &Path) -> (bool, u64) {
    let (mut stream, _) = listener.accept().expect("accept full-disk part request");
    let request = read_request_head(&mut stream);
    assert_eq!(request.method, "PUT");
    assert_eq!(
        request.path,
        format!("/v1/attachment/{TASK3583_FAILED_ID}/part/2")
    );
    assert_eq!(request.content_length, ATTACHMENT_MULTIPART_PART_BYTES);

    let session = task3583_session_path(root, TASK3583_FAILED_ID);
    let partial_path = session.join("part-2.pending");
    let mut partial = File::create(&partial_path).expect("create next-part partial");
    let halfway = request.content_length / 2;
    let mut buffer = [0_u8; 64 * 1024];
    let mut received = 0_u64;
    while received < halfway {
        let wanted = usize::try_from((halfway - received).min(buffer.len() as u64))
            .expect("half-part read fits buffer");
        stream
            .read_exact(&mut buffer[..wanted])
            .expect("read first half of next part");
        partial
            .write_all(&buffer[..wanted])
            .expect("write first half of next part");
        received += wanted as u64;
    }
    partial.sync_all().expect("sync first half of next part");

    let (filler_enospc, filler_bytes) = task3583_fill_disk(root);
    assert!(
        filler_enospc,
        "disk filler must reach the real ENOSPC boundary"
    );
    let next_write = partial.write(&[0x35_u8; 64 * 1024]);
    let write_enospc = next_write
        .as_ref()
        .is_err_and(|error| error.raw_os_error() == Some(28));
    assert!(
        write_enospc,
        "the next multipart write must report ENOSPC, got {next_write:?}"
    );
    drop(partial);
    fs::remove_file(&partial_path).expect("discard failed next-part partial while disk is full");

    // Drain the body the client has already committed to this request, then
    // return an ordinary server refusal. The durable part count remains one:
    // only the first, fully acknowledged part is eligible for later cleanup.
    let mut remaining = request.content_length - received;
    while remaining != 0 {
        let wanted = usize::try_from(remaining.min(buffer.len() as u64))
            .expect("remaining part read fits buffer");
        stream
            .read_exact(&mut buffer[..wanted])
            .expect("drain refused next-part body");
        remaining -= wanted as u64;
    }
    respond_status(
        &mut stream,
        "507 Insufficient Storage",
        r#"{"error":"disk_full"}"#,
    );
    (write_enospc, filler_bytes)
}

struct Task3583BeforeCleanup {
    completed_upload_count: usize,
    retained_key_count: usize,
    failed_upload_part_count: usize,
    write_enospc: bool,
    filler_bytes: u64,
}

struct Task3583AfterCleanup {
    completed_upload_count: usize,
    retained_key_count: usize,
    failed_upload_part_count: usize,
}

fn task3583_start_server(
    root: PathBuf,
    before_tx: Sender<Task3583BeforeCleanup>,
    cleanup_rx: Receiver<()>,
) -> (String, thread::JoinHandle<Task3583AfterCleanup>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind task 3583 upload fixture");
    let address = listener.local_addr().expect("task 3583 fixture address");
    let server = thread::spawn(move || {
        task3583_accept_session(&listener, TASK3583_CONTROL_ID);
        task3583_accept_part_to_disk(&listener, &root, TASK3583_CONTROL_ID, 1);
        task3583_accept_part_to_disk(&listener, &root, TASK3583_CONTROL_ID, 2);
        task3583_finish_control(&listener, &root);

        task3583_accept_session(&listener, TASK3583_FAILED_ID);
        task3583_accept_part_to_disk(&listener, &root, TASK3583_FAILED_ID, 1);
        let (write_enospc, filler_bytes) = task3583_fail_next_part(&listener, &root);
        before_tx
            .send(Task3583BeforeCleanup {
                completed_upload_count: task3583_completed_upload_count(&root),
                retained_key_count: task3583_retained_key_count(&root),
                failed_upload_part_count: task3583_part_count(&root, TASK3583_FAILED_ID),
                write_enospc,
                filler_bytes,
            })
            .expect("report state before cleanup");

        cleanup_rx.recv().expect("cleanup requested");
        fs::remove_dir_all(task3583_session_path(&root, TASK3583_FAILED_ID))
            .expect("cleanup aborts the abandoned multipart upload");
        fs::remove_file(root.join("disk-filler.bin")).expect("remove disk filler after cleanup");
        Task3583AfterCleanup {
            completed_upload_count: task3583_completed_upload_count(&root),
            retained_key_count: task3583_retained_key_count(&root),
            failed_upload_part_count: task3583_part_count(&root, TASK3583_FAILED_ID),
        }
    });
    (format!("http://{address}"), server)
}

fn task3583_mount_private_disk(root: &Path) {
    let status = Command::new("mount")
        .args(["-t", "tmpfs", "-o", "size=48m,nr_inodes=1024", "tmpfs"])
        .arg(root)
        .status()
        .expect("start task 3583 private tmpfs mount");
    assert!(
        status.success(),
        "task 3583 private tmpfs mount failed: {status}"
    );
}

#[test]
fn task_3583_full_disk_during_upload_part_write_cleans_only_the_failed_upload() {
    if std::env::var_os(TASK3583_CHILD_ENV).is_some() {
        return;
    }
    let mountpoint = tempfile::tempdir().expect("create task 3583 mountpoint");
    let status = Command::new("unshare")
        .args(["-U", "-r", "-m"])
        .arg(std::env::current_exe().expect("current test executable"))
        .args([
            "--ignored",
            "--exact",
            "task_3583_private_disk_child",
            "--nocapture",
        ])
        .env(TASK3583_CHILD_ENV, "1")
        .env(TASK3583_ROOT_ENV, mountpoint.path())
        .status()
        .expect("run task 3583 private-disk child");
    assert!(
        status.success(),
        "task 3583 private-disk child failed: {status}"
    );
}

#[test]
#[ignore = "subprocess helper requiring its own mount namespace"]
fn task_3583_private_disk_child() {
    let root = PathBuf::from(
        std::env::var(TASK3583_ROOT_ENV).expect("TASK3583_ROOT is set for private-disk child"),
    );
    task3583_mount_private_disk(&root);

    let input_root = tempfile::tempdir().expect("create upload input root outside private disk");
    let input = input_root.path().join("two-parts.bin");
    File::create(&input)
        .expect("create sparse two-part upload")
        .set_len(TASK3583_FILE_BYTES)
        .expect("size sparse two-part upload");
    let control_resume = input_root.path().join("control-resume.json");
    let failed_resume = input_root.path().join("failed-resume.json");

    let (before_tx, before_rx) = mpsc::channel();
    let (cleanup_tx, cleanup_rx) = mpsc::channel();
    let (base_url, server) = task3583_start_server(root.clone(), before_tx, cleanup_rx);
    let client = CipherStoreClient::new(base_url).expect("build task 3583 upload client");
    let mut success_count = 0usize;

    let control = client
        .upload_attachment_file_pro_chunked_with_resume_record(
            File::open(&input).expect("open control upload"),
            TTL_7D,
            &[0x83; 16],
            &control_resume,
        )
        .expect("finish control multipart upload");
    assert_eq!(control.completed_file.file_id, TASK3583_CONTROL_ID);
    success_count += 1;
    let completed_before_failure = task3583_completed_upload_count(&root);
    let retained_before_failure = task3583_retained_key_count(&root);
    assert_eq!(completed_before_failure, 1);
    assert_eq!(retained_before_failure, 1);
    assert_eq!(success_count, 1);
    println!(
        "TASK3583 stage=before_failure completed_upload_count={completed_before_failure} retained_key_count={retained_before_failure} success_count={success_count}"
    );

    let failed = client.upload_attachment_file_pro_chunked_with_resume_record(
        File::open(&input).expect("open second upload"),
        TTL_7D,
        &[0x58; 16],
        &failed_resume,
    );
    assert!(
        matches!(&failed, Err(CipherStoreError::Status { status: 507, .. })),
        "the second upload must fail on the disk-full part: {failed:?}"
    );
    let before_cleanup = before_rx.recv().expect("receive pre-cleanup counts");
    assert!(before_cleanup.write_enospc);
    assert_eq!(before_cleanup.completed_upload_count, 1);
    assert_eq!(before_cleanup.retained_key_count, 1);
    assert_eq!(before_cleanup.failed_upload_part_count, 1);
    assert_eq!(success_count, 1);
    println!(
        "TASK3583 stage=after_failure_before_cleanup write_enospc={} filler_bytes={} completed_upload_count={} retained_key_count={} failed_upload_part_count={} success_count={success_count}",
        before_cleanup.write_enospc,
        before_cleanup.filler_bytes,
        before_cleanup.completed_upload_count,
        before_cleanup.retained_key_count,
        before_cleanup.failed_upload_part_count,
    );

    cleanup_tx.send(()).expect("run abandoned-upload cleanup");
    let after_cleanup = server.join().expect("task 3583 fixture exits cleanly");
    assert_eq!(after_cleanup.completed_upload_count, 1);
    assert_eq!(after_cleanup.retained_key_count, 1);
    assert_eq!(after_cleanup.failed_upload_part_count, 0);
    assert_eq!(success_count, 1);
    println!(
        "TASK3583 stage=after_cleanup completed_upload_count={} retained_key_count={} failed_upload_part_count={} success_count={success_count}",
        after_cleanup.completed_upload_count,
        after_cleanup.retained_key_count,
        after_cleanup.failed_upload_part_count,
    );
    println!(
        "TASK3583 finish_line completed_upload_count_before=1 completed_upload_count_after_failure=1 completed_upload_count_after_cleanup=1 retained_key_count_before=1 retained_key_count_after_failure=1 retained_key_count_after_cleanup=1 failed_upload_part_count_before_cleanup=1 failed_upload_part_count_after_cleanup=0 success_count_before=1 success_count_after=1"
    );

    let status = Command::new("umount")
        .arg(&root)
        .status()
        .expect("unmount task 3583 private tmpfs");
    assert!(
        status.success(),
        "task 3583 private tmpfs unmount failed: {status}"
    );
}
