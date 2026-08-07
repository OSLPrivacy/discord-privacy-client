use ipc::cipher_store_client::{CipherStoreClient, FETCH_TOKEN_BYTES, TTL_1H};
use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const ATTACHMENT_BYTES: u64 = 8 * 1024 * 1024;
const TOR_CAP_BITS_PER_SECOND: u64 = 300_000;
const TOR_CAP_BYTES_PER_SECOND: u64 = TOR_CAP_BITS_PER_SECOND / 8;
const RECV_BUFFER_BYTES: usize = 4096;

#[derive(Clone, Copy, Default)]
struct CircuitStats {
    declared_content_length: u64,
    body_received: u64,
    completed_body: bool,
}

struct TorMarkedSlowCircuit {
    address: String,
    stop: Arc<AtomicBool>,
    stats: Arc<Mutex<CircuitStats>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl TorMarkedSlowCircuit {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind Tor-marked slow circuit");
        listener
            .set_nonblocking(true)
            .expect("make Tor-marked fixture listener nonblocking");
        let address = listener.local_addr().unwrap().to_string();
        let stop = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(Mutex::new(CircuitStats::default()));
        let thread_stop = Arc::clone(&stop);
        let thread_stats = Arc::clone(&stats);
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        serve_slow_attachment_request(stream, &thread_stop, &thread_stats);
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            address,
            stop,
            stats,
            thread: Some(thread),
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    fn snapshot(&self) -> CircuitStats {
        *self.stats.lock().unwrap_or_else(|error| error.into_inner())
    }
}

impl Drop for TorMarkedSlowCircuit {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect(&self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve_slow_attachment_request(
    stream: TcpStream,
    stop: &AtomicBool,
    stats: &Mutex<CircuitStats>,
) {
    let mut stream = shrink_receive_buffer(stream);
    let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
    let mut request = Vec::with_capacity(8192);
    let mut buffer = [0_u8; 1024];
    let header_end = loop {
        match stream.read(&mut buffer) {
            Ok(0) => return,
            Ok(read) => {
                request.extend_from_slice(&buffer[..read]);
                if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    break index + 4;
                }
                if request.len() > 64 * 1024 {
                    return;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                if stop.load(Ordering::Acquire) {
                    return;
                }
            }
            Err(_) => return,
        }
    };
    let header = String::from_utf8_lossy(&request[..header_end]);
    let content_length = header
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<u64>().ok())
                .flatten()
        })
        .unwrap_or(0);
    {
        let mut stats = stats.lock().unwrap_or_else(|error| error.into_inner());
        stats.declared_content_length = content_length;
        stats.body_received = request.len().saturating_sub(header_end) as u64;
    }

    let started = Instant::now();
    let mut received = request.len().saturating_sub(header_end) as u64;
    while received < content_length && !stop.load(Ordering::Acquire) {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                received += read as u64;
                {
                    let mut stats = stats.lock().unwrap_or_else(|error| error.into_inner());
                    stats.body_received = received;
                }
                pace_to_cap(started, received);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
    }
    if received == content_length {
        {
            let mut stats = stats.lock().unwrap_or_else(|error| error.into_inner());
            stats.completed_body = true;
        }
        let body = format!(
            r#"{{"id":"00000000000000000000000000004904","expires_at":4102444800,"size_bytes":{content_length}}}"#
        );
        let response = format!(
            "HTTP/1.1 201 Created\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
    }
}

fn shrink_receive_buffer(stream: TcpStream) -> TcpStream {
    set_recv_buffer_size(&stream, RECV_BUFFER_BYTES);
    stream
}

#[cfg(unix)]
fn set_recv_buffer_size(stream: &TcpStream, bytes: usize) {
    use std::ffi::c_void;
    use std::os::fd::AsRawFd;
    use std::os::raw::c_int;

    const SOL_SOCKET: c_int = 1;
    const SO_RCVBUF: c_int = 8;

    extern "C" {
        fn setsockopt(
            socket: c_int,
            level: c_int,
            option_name: c_int,
            option_value: *const c_void,
            option_len: u32,
        ) -> c_int;
    }

    let value = bytes as c_int;
    unsafe {
        let _ = setsockopt(
            stream.as_raw_fd(),
            SOL_SOCKET,
            SO_RCVBUF,
            (&value as *const c_int).cast(),
            std::mem::size_of_val(&value) as u32,
        );
    }
}

#[cfg(not(unix))]
fn set_recv_buffer_size(_stream: &TcpStream, _bytes: usize) {}

fn pace_to_cap(started: Instant, received: u64) {
    let expected = Duration::from_secs_f64(received as f64 / TOR_CAP_BYTES_PER_SECOND as f64);
    if expected > started.elapsed() {
        thread::sleep(expected - started.elapsed());
    }
}

fn write_attachment(path: &Path) {
    let mut file = File::create(path).expect("create 8 MiB attachment fixture");
    let mut block = [0_u8; 64 * 1024];
    let mut state = 0x4904_4904_4904_4904_u64;
    let mut written = 0_u64;
    while written < ATTACHMENT_BYTES {
        for byte in &mut block {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = (state >> 32) as u8;
        }
        let take = (ATTACHMENT_BYTES - written).min(block.len() as u64) as usize;
        file.write_all(&block[..take])
            .expect("write 8 MiB attachment fixture");
        written += take as u64;
    }
    file.sync_all().expect("sync 8 MiB attachment fixture");
}

fn required_seconds_at_cap() -> f64 {
    (ATTACHMENT_BYTES * 8) as f64 / TOR_CAP_BITS_PER_SECOND as f64
}

#[test]
fn tor_marked_8_mib_attachment_survives_the_expected_slow_circuit() {
    let required_seconds = required_seconds_at_cap();
    assert!(
        required_seconds > 200.0,
        "8 MiB at 0.30 Mbit/s must prove a transfer longer than 200 seconds"
    );

    let temp = tempfile::tempdir().expect("create task 4904 attachment tempdir");
    let attachment = temp.path().join("task-4904-8mib.bin");
    write_attachment(&attachment);

    let circuit = TorMarkedSlowCircuit::start();
    let _restore = keystore::egress::restore_clearnet_on_drop();
    let tor_http = reqwest::blocking::Client::builder()
        .build()
        .expect("build Tor-marked fixture HTTP client");
    keystore::egress::route_through_tor(tor_http);
    let client =
        CipherStoreClient::new(circuit.base_url()).expect("adopt Tor-marked fixture circuit");

    let started = Instant::now();
    let result = client.upload_attachment_file(
        File::open(&attachment).expect("open 8 MiB attachment fixture"),
        TTL_1H,
        &[0x49_u8; FETCH_TOKEN_BYTES],
    );
    let elapsed = started.elapsed();
    let failure_second = elapsed.as_secs();
    let stats = circuit.snapshot();

    match result {
        Ok(upload) => {
            assert_eq!(upload.id_hex, "00000000000000000000000000004904");
            assert_eq!(stats.declared_content_length, ATTACHMENT_BYTES);
            assert_eq!(stats.body_received, ATTACHMENT_BYTES);
            assert!(stats.completed_body);
        }
        Err(error) => {
            println!(
                "TOR-4904-RED tor_marked=true attachment_bytes={} cap_mbit_s=0.30 required_seconds={required_seconds:.3} failure_second={failure_second} received_bytes={} completed_body={} error={error}",
                ATTACHMENT_BYTES,
                stats.body_received,
                stats.completed_body
            );
            assert!(
                failure_second <= 120,
                "TOR-4904-RED expected failure at or before second 120, got second {failure_second}"
            );
            panic!(
                "TOR-4904-RED app turned a Tor-slow 8 MiB attachment into failed before the fixture could finish"
            );
        }
    }
}
