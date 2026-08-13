//! TASK 4817 relay: the release relay process the confidentiality inventory is
//! taken from.
//!
//! It models a carrier server that retains everything: the message queue lives
//! in process memory for the life of the process, every request body is written
//! to a SQLite database, a blob, a channel cache, an access log, a telemetry
//! stream, and on request a crash export and a dump of the relay's own process
//! memory. Nothing here tries to protect the payload — that is the point. If a
//! sync value can be recovered from any of these surfaces, the sealing failed.
//!
//! Transport is plain HTTP/1.1 with no TLS, so the packet capture in front of
//! this process records exactly the bytes the application handed the socket. A
//! confidentiality claim that survives this relay cannot be a claim about TLS.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// One message exactly as the relay received it. Held in memory for the life of
/// the process so a memory dump has something to find.
#[derive(Clone)]
struct RelayMessage {
    id: u64,
    channel: String,
    content: String,
    raw_body: Vec<u8>,
    received_ms: u128,
}

struct RelayState {
    data_dir: PathBuf,
    queue: Mutex<Vec<RelayMessage>>,
    next_id: Mutex<u64>,
    db: Mutex<rusqlite::Connection>,
    last_request: Mutex<Vec<u8>>,
    stop: AtomicBool,
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

fn append(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(bytes);
    }
}

/// Minimal JSON string-field read. The relay is not a JSON library; it needs
/// exactly one field and must keep the raw bytes anyway.
fn json_string_field(body: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\":\"");
    let start = body.find(&needle)? + needle.len();
    let rest = &body[start..];
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(out),
            '\\' => {
                let escaped = chars.next()?;
                out.push(escaped);
            }
            other => out.push(other),
        }
    }
    None
}

fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other if (other as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", other as u32)),
            other => out.push(other),
        }
    }
    out
}

fn main() {
    let mut port: u16 = 0;
    let mut data_dir = PathBuf::from("/tmp/task-4817-relay");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => port = args.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "--data-dir" => data_dir = PathBuf::from(args.next().unwrap_or_default()),
            other => {
                eprintln!("task-4817-relay: unknown argument {other}");
                std::process::exit(2);
            }
        }
    }

    fs::create_dir_all(&data_dir).expect("relay data dir");
    fs::create_dir_all(data_dir.join("blobs")).expect("relay blob dir");
    fs::create_dir_all(data_dir.join("cache")).expect("relay cache dir");

    let db_path = data_dir.join("relay.db");
    let db = rusqlite::Connection::open(&db_path).expect("relay database");
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY,
            channel TEXT NOT NULL,
            content TEXT NOT NULL,
            body_bytes INTEGER NOT NULL,
            body_digest TEXT NOT NULL,
            received_ms INTEGER NOT NULL
        );",
    )
    .expect("relay schema");

    let listener = TcpListener::bind(("127.0.0.1", port)).expect("relay bind");
    let bound = listener.local_addr().expect("relay addr").port();

    let state = Arc::new(RelayState {
        data_dir: data_dir.clone(),
        queue: Mutex::new(Vec::new()),
        next_id: Mutex::new(1),
        db: Mutex::new(db),
        last_request: Mutex::new(Vec::new()),
        stop: AtomicBool::new(false),
    });

    append(
        &data_dir.join("relay.log"),
        format!(
            "{} relay start pid={} port={} tls=false\n",
            now_ms(),
            std::process::id(),
            bound
        )
        .as_bytes(),
    );

    println!("RELAY_READY port={bound} pid={}", std::process::id());
    let _ = std::io::stdout().flush();

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = Arc::clone(&state);
                std::thread::spawn(move || handle(state, stream));
            }
            Err(err) => {
                eprintln!("task-4817-relay: accept: {err}");
            }
        }
        if state.stop.load(Ordering::SeqCst) {
            break;
        }
    }
}

fn respond(stream: &mut TcpStream, status: &str, body: &str, state: &RelayState) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    append(
        &state.data_dir.join("responses.log"),
        format!("{} {}\n", now_ms(), body).as_bytes(),
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn handle(state: Arc<RelayState>, mut stream: TcpStream) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone relay stream"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() || request_line.trim().is_empty() {
        return;
    }
    let mut content_length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).is_err() {
            return;
        }
        if header.trim().is_empty() {
            break;
        }
        let lower = header.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-length:") {
            content_length = rest.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 && reader.read_exact(&mut body).is_err() {
        return;
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    let method = parts.first().copied().unwrap_or("");
    let target = parts.get(1).copied().unwrap_or("");

    // Access log: a relay that logs whole request bodies. Deliberate.
    append(
        &state.data_dir.join("relay.log"),
        format!(
            "{} {method} {target} bytes={} body={}\n",
            now_ms(),
            body.len(),
            String::from_utf8_lossy(&body)
        )
        .as_bytes(),
    );
    append(
        &state.data_dir.join("requests.log"),
        format!("{} {method} {target}\n", now_ms()).as_bytes(),
    );
    append(&state.data_dir.join("requests.log"), &body);
    append(&state.data_dir.join("requests.log"), b"\n");
    *state.last_request.lock().expect("last request") = body.clone();

    let path = target.split('?').next().unwrap_or(target);
    let query = target.split('?').nth(1).unwrap_or("");

    if method == "POST" && path.starts_with("/relay/v1/channels/") && path.ends_with("/messages") {
        let channel = path
            .trim_start_matches("/relay/v1/channels/")
            .trim_end_matches("/messages")
            .to_owned();
        let text = String::from_utf8_lossy(&body).into_owned();
        let content = json_string_field(&text, "content").unwrap_or_default();
        let id = {
            let mut next = state.next_id.lock().expect("next id");
            let id = *next;
            *next += 1;
            id
        };
        let message = RelayMessage {
            id,
            channel: channel.clone(),
            content: content.clone(),
            raw_body: body.clone(),
            received_ms: now_ms(),
        };
        state.queue.lock().expect("queue").push(message.clone());

        let digest = format!("{:016x}", fnv1a64(&body));
        {
            let db = state.db.lock().expect("db");
            let _ = db.execute(
                "INSERT INTO messages (id, channel, content, body_bytes, body_digest, received_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    id as i64,
                    channel,
                    content,
                    body.len() as i64,
                    digest,
                    message.received_ms as i64
                ],
            );
        }

        let blob_path = state.data_dir.join("blobs").join(format!("{id}.blob"));
        let _ = fs::write(&blob_path, &body);

        // Channel cache: the newest messages for this channel, rewritten each
        // time, exactly as an edge cache would keep them.
        let cache_path = state.data_dir.join("cache").join(format!("{channel}.cache"));
        let cached: Vec<String> = state
            .queue
            .lock()
            .expect("queue")
            .iter()
            .filter(|m| m.channel == channel)
            .rev()
            .take(32)
            .map(|m| format!("{}\t{}", m.id, m.content))
            .collect();
        let _ = fs::write(&cache_path, cached.join("\n"));

        let prefix: String = content.chars().take(96).collect();
        append(
            &state.data_dir.join("telemetry.jsonl"),
            format!(
                "{{\"event\":\"message_received\",\"ms\":{},\"channel\":\"{}\",\"id\":{},\"bytes\":{},\"digest\":\"{}\",\"content_prefix\":\"{}\"}}\n",
                now_ms(),
                json_escape(&channel),
                id,
                body.len(),
                digest,
                json_escape(&prefix)
            )
            .as_bytes(),
        );

        let response = format!(
            "{{\"id\":{id},\"channel\":\"{}\",\"content\":\"{}\"}}",
            json_escape(&channel),
            json_escape(&content)
        );
        respond(&mut stream, "200 OK", &response, &state);
        return;
    }

    if method == "GET" && path.starts_with("/relay/v1/channels/") && path.ends_with("/messages") {
        let channel = path
            .trim_start_matches("/relay/v1/channels/")
            .trim_end_matches("/messages")
            .to_owned();
        let after: u64 = query
            .split('&')
            .find_map(|pair| pair.strip_prefix("after="))
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let rows: Vec<String> = state
            .queue
            .lock()
            .expect("queue")
            .iter()
            .filter(|m| m.channel == channel && m.id > after)
            .map(|m| {
                format!(
                    "{{\"id\":{},\"content\":\"{}\"}}",
                    m.id,
                    json_escape(&m.content)
                )
            })
            .collect();
        respond(&mut stream, "200 OK", &format!("[{}]", rows.join(",")), &state);
        return;
    }

    if method == "POST" && path == "/relay/v1/admin/dump" {
        let written = dump_surfaces(&state);
        let rows: Vec<String> = written
            .iter()
            .map(|(name, bytes)| format!("{{\"surface\":\"{name}\",\"bytes\":{bytes}}}"))
            .collect();
        respond(&mut stream, "200 OK", &format!("[{}]", rows.join(",")), &state);
        return;
    }

    if method == "POST" && path == "/relay/v1/admin/shutdown" {
        respond(&mut stream, "200 OK", "{\"ok\":true}", &state);
        state.stop.store(true, Ordering::SeqCst);
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(50));
            std::process::exit(0);
        });
        return;
    }

    respond(&mut stream, "404 Not Found", "{\"error\":\"no route\"}", &state);
}

/// Write the on-demand surfaces: the queue snapshot, a crash export and a dump
/// of this process's own memory. Returns each surface and its byte count.
fn dump_surfaces(state: &RelayState) -> BTreeMap<String, u64> {
    let mut written = BTreeMap::new();

    let queue_path = state.data_dir.join("queue.bin");
    {
        let queue = state.queue.lock().expect("queue");
        let mut out = Vec::new();
        out.extend_from_slice(b"TASK4817-RELAY-QUEUE\n");
        for message in queue.iter() {
            out.extend_from_slice(
                format!(
                    "id={} channel={} received_ms={} bytes={}\n",
                    message.id,
                    message.channel,
                    message.received_ms,
                    message.raw_body.len()
                )
                .as_bytes(),
            );
            out.extend_from_slice(&message.raw_body);
            out.push(b'\n');
        }
        let _ = fs::write(&queue_path, &out);
        written.insert("queue".to_owned(), out.len() as u64);
    }

    let crash_path = state.data_dir.join("crash-export.bin");
    {
        let queue = state.queue.lock().expect("queue");
        let last = state.last_request.lock().expect("last request");
        let mut out = Vec::new();
        out.extend_from_slice(b"TASK4817-RELAY-CRASH-EXPORT\n");
        out.extend_from_slice(format!("pid={}\n", std::process::id()).as_bytes());
        out.extend_from_slice(format!("queue_depth={}\n", queue.len()).as_bytes());
        out.extend_from_slice(b"last_request_buffer=\n");
        out.extend_from_slice(&last);
        out.push(b'\n');
        out.extend_from_slice(b"retained_message_buffers=\n");
        for message in queue.iter() {
            out.extend_from_slice(message.content.as_bytes());
            out.push(b'\n');
        }
        let _ = fs::write(&crash_path, &out);
        written.insert("crash_export".to_owned(), out.len() as u64);
    }

    let (memory_bytes, regions) = dump_process_memory(&state.data_dir);
    written.insert("process_memory".to_owned(), memory_bytes);
    append(
        &state.data_dir.join("telemetry.jsonl"),
        format!(
            "{{\"event\":\"admin_dump\",\"ms\":{},\"memory_bytes\":{memory_bytes},\"memory_regions\":{regions}}}\n",
            now_ms()
        )
        .as_bytes(),
    );

    written
}

/// Dump this process's own readable anonymous memory (heap, stack, private
/// mappings). The relay dumps itself rather than being ptraced so the dump is
/// always available and is unambiguously this release binary's memory.
fn dump_process_memory(data_dir: &Path) -> (u64, u64) {
    let maps = match fs::read_to_string("/proc/self/maps") {
        Ok(maps) => maps,
        Err(_) => return (0, 0),
    };
    let mem = match File::open("/proc/self/mem") {
        Ok(mem) => mem,
        Err(_) => return (0, 0),
    };
    let dump_path = data_dir.join("memory-dump.bin");
    let index_path = data_dir.join("memory-dump.index");
    let mut dump = match File::create(&dump_path) {
        Ok(file) => file,
        Err(_) => return (0, 0),
    };
    let mut index = String::new();
    let mut total: u64 = 0;
    let mut regions: u64 = 0;
    // 1 GiB ceiling so a pathological mapping cannot fill the disk.
    const MAX_TOTAL: u64 = 1 << 30;

    for line in maps.lines() {
        let mut fields = line.split_whitespace();
        let range = match fields.next() {
            Some(range) => range,
            None => continue,
        };
        let perms = fields.next().unwrap_or("");
        let label = fields.nth(3).unwrap_or("");
        if !perms.starts_with('r') {
            continue;
        }
        // Anonymous private mappings plus the heap and stack: where a server's
        // request buffers, queue and string allocations actually live.
        let interesting = label.is_empty() || label == "[heap]" || label == "[stack]";
        if !interesting {
            continue;
        }
        let mut bounds = range.split('-');
        let start = bounds
            .next()
            .and_then(|value| u64::from_str_radix(value, 16).ok());
        let end = bounds
            .next()
            .and_then(|value| u64::from_str_radix(value, 16).ok());
        let (start, end) = match (start, end) {
            (Some(start), Some(end)) if end > start => (start, end),
            _ => continue,
        };
        let len = end - start;
        if len == 0 || total + len > MAX_TOTAL {
            continue;
        }
        let mut buffer = vec![0u8; len as usize];
        if mem.read_exact_at(&mut buffer, start).is_err() {
            continue;
        }
        if dump.write_all(&buffer).is_err() {
            continue;
        }
        index.push_str(&format!(
            "region start={start:#x} end={end:#x} len={len} label={}\n",
            if label.is_empty() { "anon" } else { label }
        ));
        total += len;
        regions += 1;
    }
    let _ = dump.flush();
    let _ = fs::write(&index_path, index);
    (total, regions)
}
