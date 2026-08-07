#[allow(dead_code)]
#[path = "../src/attachment_limits.rs"]
mod attachment_limits;

use attachment_limits::{attachment_limit_command, AttachmentAccountTier};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[derive(Clone, Copy)]
struct RejectedInput {
    label: &'static str,
    tier: AttachmentAccountTier,
    bytes_per_file: u64,
    file_count: usize,
}

struct RecordingUploadService {
    address: String,
    uploads: Arc<AtomicU32>,
    stopping: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl RecordingUploadService {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind recording upload service");
        listener
            .set_nonblocking(true)
            .expect("make recording upload service nonblocking");
        let address = listener.local_addr().unwrap().to_string();
        let uploads = Arc::new(AtomicU32::new(0));
        let stopping = Arc::new(AtomicBool::new(false));
        let thread_uploads = Arc::clone(&uploads);
        let thread_stopping = Arc::clone(&stopping);
        let thread = thread::spawn(move || {
            while !thread_stopping.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => record_request(&mut stream, &thread_uploads),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            address,
            uploads,
            stopping,
            thread: Some(thread),
        }
    }

    fn upload_count(&self) -> u32 {
        self.uploads.load(Ordering::Acquire)
    }
}

impl Drop for RecordingUploadService {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        let _ = TcpStream::connect(&self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn record_request(stream: &mut TcpStream, uploads: &AtomicU32) {
    let mut buffer = [0u8; 1024];
    let read = stream.read(&mut buffer).unwrap_or(0);
    let first_line = std::str::from_utf8(&buffer[..read])
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("");
    if first_line.starts_with("POST /upload ") {
        uploads.fetch_add(1, Ordering::AcqRel);
    }
    let response = b"HTTP/1.1 201 Created\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
    let _ = stream.write_all(response);
}

fn send_upload_request(address: &str) {
    let mut stream = TcpStream::connect(address).expect("connect to recording upload service");
    let request = b"POST /upload HTTP/1.1\r\nhost: recording-service\r\ncontent-length: 7\r\nconnection: close\r\n\r\npayload";
    stream
        .write_all(request)
        .expect("send upload request to recording service");
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
}

fn submit_rejected_direct_attachment_command(
    input: RejectedInput,
    service: &RecordingUploadService,
) -> &'static str {
    let result = attachment_limit_command(input.tier, input.bytes_per_file, input.file_count);
    if result == "accept" {
        send_upload_request(&service.address);
    }
    result
}

#[test]
fn task0052_rejected_direct_attachment_inputs_never_upload() {
    const MIB: u64 = 1024 * 1024;
    let service = RecordingUploadService::start();
    let rejected_inputs = [
        RejectedInput {
            label: "26 MB",
            tier: AttachmentAccountTier::Free,
            bytes_per_file: 26 * MIB,
            file_count: 1,
        },
        RejectedInput {
            label: "1.1 GB",
            tier: AttachmentAccountTier::Pro,
            bytes_per_file: 1127 * MIB,
            file_count: 1,
        },
        RejectedInput {
            label: "17 files",
            tier: AttachmentAccountTier::Free,
            bytes_per_file: MIB,
            file_count: 17,
        },
    ];

    let mut rejected = 0usize;
    for input in rejected_inputs {
        let result = submit_rejected_direct_attachment_command(input, &service);
        if result == "reject" {
            rejected += 1;
        }
        println!(
            "TASK0052 rejected_direct_attachment case={} result={} bytes_per_file={} file_count={}",
            input.label, result, input.bytes_per_file, input.file_count
        );
    }

    if rejected != rejected_inputs.len() {
        for _ in 0..50 {
            if service.upload_count() > 0 {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
    let upload_requests = service.upload_count();
    println!(
        "TASK0052 rejected_direct_attachment rejected_inputs={} upload_requests={} direct_uploads={}",
        rejected, upload_requests, upload_requests
    );
    assert_eq!(upload_requests, 0);
    assert_eq!(rejected, rejected_inputs.len());
}
