use ipc::commands::{cmd_osl_check_site_for_update, SiteUpdateCheckResult};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

fn spawn_site(routes: HashMap<String, (u16, String)>, requests: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    thread::spawn(move || {
        for _ in 0..requests {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 2048];
            let read = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let (status, body) = routes
                .get(path)
                .cloned()
                .unwrap_or_else(|| (404, "not found".to_owned()));
            let reason = match status {
                200 => "OK",
                204 => "No Content",
                404 => "Not Found",
                _ => "Error",
            };
            let response = if status == 204 {
                format!("HTTP/1.1 {status} {reason}\r\nContent-Length: 0\r\n\r\n")
            } else {
                format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                )
            };
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    base
}

#[test]
fn newer_site_version_returns_version_download_address_and_expected_fingerprint() {
    let digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let download_address = format!("{base}/download/osl-hub-0.2.0-x64-nsis.exe");
    let latest = serde_json::json!({
        "version": "0.2.0",
        "platforms": {
            "windows-x86_64": {
                "url": download_address
            }
        }
    })
    .to_string();
    let checksums = format!("{digest}  osl-hub-0.2.0-x64-nsis.exe\n");
    let routes = HashMap::from([
        ("/latest.json".to_owned(), (200, latest)),
        ("/SHA256SUMS.txt".to_owned(), (200, checksums)),
    ]);
    thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 2048];
            let read = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let (status, body) = routes
                .get(path)
                .cloned()
                .unwrap_or_else(|| (404, "not found".to_owned()));
            let response = format!(
                "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });

    let result = cmd_osl_check_site_for_update(
        "0.1.0".to_owned(),
        format!("{base}/latest.json"),
        "windows".to_owned(),
        "x86_64".to_owned(),
    );

    assert_eq!(
        result,
        SiteUpdateCheckResult::UpdateAvailable {
            version: "0.2.0".to_owned(),
            download_address: download_address.clone(),
            expected_fingerprint: format!("sha256:{digest}"),
        }
    );
    let output = result.direct_command_output();
    assert!(output.contains("version=0.2.0"));
    assert!(output.contains(&format!("download_address={download_address}")));
    assert!(output.contains(&format!("expected_fingerprint=sha256:{digest}")));
    println!("TASK3169_NEWER_VERSION=0.2.0");
    println!("TASK3169_DOWNLOAD_ADDRESS={download_address}");
    println!("TASK3169_EXPECTED_FINGERPRINT=sha256:{digest}");
}

#[test]
fn current_or_newer_version_returns_exact_up_to_date_string() {
    let latest = serde_json::json!({
        "version": "0.2.0",
        "download_address": "https://example.test/osl-hub-0.2.0-x64-nsis.exe",
        "sha256": "f".repeat(64)
    })
    .to_string();
    let base = spawn_site(
        HashMap::from([("/latest.json".to_owned(), (200, latest))]),
        1,
    );

    let result = cmd_osl_check_site_for_update(
        "0.2.0".to_owned(),
        format!("{base}/latest.json"),
        "windows".to_owned(),
        "x86_64".to_owned(),
    );

    assert_eq!(
        result,
        SiteUpdateCheckResult::UpToDate {
            message: "you are up to date".to_owned(),
        }
    );
    assert_eq!(result.direct_command_output(), "you are up to date");
    println!(
        "TASK3169_UP_TO_DATE_STRING={}",
        result.direct_command_output()
    );
}

#[test]
fn unreachable_site_returns_plain_error_without_panic() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/latest.json", listener.local_addr().unwrap());
    drop(listener);

    let result = cmd_osl_check_site_for_update(
        "0.1.0".to_owned(),
        url,
        "windows".to_owned(),
        "x86_64".to_owned(),
    );

    let output = result.direct_command_output();
    assert!(matches!(result, SiteUpdateCheckResult::Error { .. }));
    assert!(output.starts_with("error: site cannot be reached:"));
    println!("TASK3169_ERROR_PREFIX=error:");
    println!("TASK3169_NO_CRASH=true");
}
