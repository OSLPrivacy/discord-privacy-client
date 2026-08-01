//! Behavioral tests for the separate license-redemption wire call.

use keystore::KeyServerClient;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;

fn response(body: &[u8]) -> Vec<u8> {
    let mut response =
        b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json\r\n".to_vec();
    response.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
    response.extend_from_slice(body);
    response
}

fn read_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let mut buffer = [0u8; 4096];
    let mut request = Vec::new();
    let header_end = loop {
        let count = stream.read(&mut buffer).unwrap();
        assert_ne!(count, 0, "request ended before headers");
        request.extend_from_slice(&buffer[..count]);
        if let Some(position) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break position;
        }
    };
    let headers = std::str::from_utf8(&request[..header_end]).unwrap();
    let content_length = headers
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    while request.len() - header_end - 4 < content_length {
        let count = stream.read(&mut buffer).unwrap();
        assert_ne!(count, 0, "request ended before body");
        request.extend_from_slice(&buffer[..count]);
    }
    request
}

fn two_request_server(responses: [Vec<u8>; 2]) -> (u16, mpsc::Receiver<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut requests = Vec::new();
        for response in responses {
            let (mut stream, _) = listener.accept().unwrap();
            requests.push(read_request(&mut stream));
            stream.write_all(&response).unwrap();
        }
        sender.send(requests).unwrap();
    });
    (port, receiver)
}

#[test]
fn fresh_redemption_uses_redeem_then_refresh_uses_validate() {
    let (port, requests) = two_request_server([
        response(br#"{"status":"ACTIVE","redeemed_at":1735689600,"expires_at":1738281600,"checksum_ok":true}"#),
        response(br#"{"status":"ACTIVE","redeemed_at":1735689600,"expires_at":1738281600,"checksum_ok":true}"#),
    ]);
    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();

    let redeemed = client.redeem_license("OSL-2222-3333-4444-5555").unwrap();
    assert_eq!(redeemed.status, "ACTIVE");
    assert_eq!(redeemed.redeemed_at, Some(1_735_689_600));
    assert_eq!(redeemed.expires_at, Some(1_738_281_600));

    let refreshed = client.validate_license("OSL-2222-3333-4444-5555").unwrap();
    assert_eq!(refreshed.status, "ACTIVE");

    let requests = requests.recv().unwrap();
    assert_eq!(requests.len(), 2);
    let redemption_request = std::str::from_utf8(&requests[0]).unwrap();
    let refresh_request = std::str::from_utf8(&requests[1]).unwrap();
    assert!(redemption_request.starts_with("POST /v1/license/redeem HTTP/1.1\r\n"));
    assert!(refresh_request.starts_with("POST /v1/license/validate HTTP/1.1\r\n"));
    for request in [redemption_request, refresh_request] {
        assert!(request.contains("Content-Type: application/json\r\n"));
        assert!(request.contains(r#"{"license_key":"OSL-2222-3333-4444-5555"}"#));
    }
}
