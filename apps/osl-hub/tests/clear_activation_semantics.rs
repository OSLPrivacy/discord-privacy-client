#![cfg(feature = "core")]

use osl_privacy_hub::core_bridge::{clear_activation_code, validate_activation_code, HubCoreState};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::Mutex;
use std::thread;
use tempfile::tempdir;

const ACTIVATION_CODE: &str = "OSL-2222-3333-4444-5555";
const PERIOD_END: i64 = 1_800_000_000;

// The config-dir resolver is process-global. This test changes it so the Hub
// command path can be exercised without touching a developer's real cache.
static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

struct ConfigDirReset;

impl Drop for ConfigDirReset {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn active_response() -> Vec<u8> {
    let body =
        format!(r#"{{"status":"ACTIVE","current_period_end":{PERIOD_END},"checksum_ok":true}}"#);
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn read_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let mut buffer = [0_u8; 4096];
    let mut request = Vec::new();
    let header_end = loop {
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0, "activation client closed before sending headers");
        request.extend_from_slice(&buffer[..read]);
        if let Some(offset) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            break offset;
        }
    };
    let header = std::str::from_utf8(&request[..header_end]).unwrap();
    let content_length = header
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    while request[header_end + 4..].len() < content_length {
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0, "activation client closed before sending body");
        request.extend_from_slice(&buffer[..read]);
    }
    request
}

fn two_validation_server() -> (String, thread::JoinHandle<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let mut requests = Vec::new();
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            requests.push(read_request(&mut stream));
            stream.write_all(&active_response()).unwrap();
        }
        requests
    });
    (url, server)
}

fn configure_loopback_keyserver(dir: &Path, url: &str) {
    std::fs::write(
        dir.join("keyserver.json"),
        format!(r#"{{"base_url":"{url}"}}"#),
    )
    .unwrap();
}

#[test]
fn clearing_activation_only_forgets_local_cache_and_reentering_keeps_the_period() {
    let _serial = CONFIG_DIR_LOCK.lock().unwrap();
    let cache_dir = tempdir().unwrap();
    let _reset = ConfigDirReset;
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(cache_dir.path().to_path_buf()));

    let (url, server) = two_validation_server();
    configure_loopback_keyserver(cache_dir.path(), &url);
    let core = HubCoreState::default();

    let first_activation = validate_activation_code(&core, ACTIVATION_CODE.to_owned()).unwrap();
    assert_eq!(first_activation.access, "pro");
    assert_eq!(first_activation.current_period_end, Some(PERIOD_END));
    assert!(cache_dir.path().join("license.json").is_file());

    let cleared = clear_activation_code(&core).unwrap();
    assert_eq!(cleared.access, "free");
    assert_eq!(cleared.current_period_end, None);
    assert!(!cache_dir.path().join("license.json").exists());

    let restored_activation = validate_activation_code(&core, ACTIVATION_CODE.to_owned()).unwrap();
    assert_eq!(restored_activation.access, "pro");
    assert_eq!(restored_activation.current_period_end, Some(PERIOD_END));
    assert_eq!(
        restored_activation.current_period_end,
        first_activation.current_period_end
    );

    let requests = server.join().unwrap();
    assert_eq!(
        requests.len(),
        2,
        "only the two validations contact the server"
    );
    for request in requests {
        assert!(request.starts_with(b"POST /v1/license/validate HTTP/1.1\r\n"));
    }
}
