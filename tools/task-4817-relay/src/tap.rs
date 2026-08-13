//! TASK 4817 packet capture: a recording TCP tap in front of the relay.
//!
//! The device talks to this process, this process talks to the relay, and every
//! byte in both directions is appended to a capture file before it is forwarded.
//! There is no TLS anywhere on the path, so the capture holds exactly the
//! application bytes the sending device produced — the strongest position an
//! observer on the network could ever occupy.
//!
//! Capture record layout:
//!
//! ```text
//! magic     : b"PKT1"
//! direction : u8    0 = device -> relay, 1 = relay -> device
//! length    : u32 LE
//! payload   : length bytes
//! ```

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn record(capture: &Arc<Mutex<std::fs::File>>, direction: u8, payload: &[u8]) {
    let mut file = capture.lock().expect("capture file");
    let mut frame = Vec::with_capacity(9 + payload.len());
    frame.extend_from_slice(b"PKT1");
    frame.push(direction);
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(payload);
    let _ = file.write_all(&frame);
    let _ = file.flush();
}

fn pump(
    mut from: TcpStream,
    mut to: TcpStream,
    direction: u8,
    capture: Arc<Mutex<std::fs::File>>,
) {
    let mut buffer = vec![0u8; 16 * 1024];
    loop {
        match from.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                record(&capture, direction, &buffer[..n]);
                if to.write_all(&buffer[..n]).is_err() {
                    break;
                }
                let _ = to.flush();
            }
        }
    }
    let _ = to.shutdown(Shutdown::Write);
}

fn main() {
    let mut listen: u16 = 0;
    let mut upstream: u16 = 0;
    let mut capture_path = PathBuf::from("/tmp/task-4817-capture.bin");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--listen" => listen = args.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "--upstream" => upstream = args.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "--capture" => capture_path = PathBuf::from(args.next().unwrap_or_default()),
            other => {
                eprintln!("task-4817-tap: unknown argument {other}");
                std::process::exit(2);
            }
        }
    }

    let capture = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&capture_path)
        .expect("capture file");
    let capture = Arc::new(Mutex::new(capture));

    let listener = TcpListener::bind(("127.0.0.1", listen)).expect("tap bind");
    let bound = listener.local_addr().expect("tap addr").port();
    println!("TAP_READY port={bound} upstream={upstream} pid={}", std::process::id());
    let _ = std::io::stdout().flush();

    for stream in listener.incoming() {
        let client = match stream {
            Ok(stream) => stream,
            Err(_) => continue,
        };
        let server = match TcpStream::connect(("127.0.0.1", upstream)) {
            Ok(server) => server,
            Err(err) => {
                eprintln!("task-4817-tap: upstream connect: {err}");
                continue;
            }
        };
        let capture_up = Arc::clone(&capture);
        let capture_down = Arc::clone(&capture);
        let client_read = client.try_clone().expect("clone client");
        let server_write = server.try_clone().expect("clone server");
        std::thread::spawn(move || pump(client_read, server_write, 0, capture_up));
        std::thread::spawn(move || pump(server, client, 1, capture_down));
    }
}
