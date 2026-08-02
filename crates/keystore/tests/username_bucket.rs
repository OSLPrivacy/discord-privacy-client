use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use keystore::username::{ResolvedIdentity, Resolver, UsernameResolveError};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

const DOMAIN: &[u8] = b"OSL-USERNAME-BUCKET-v1";

fn digest_hex(name: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(DOMAIN);
    digest.update(name.as_bytes());
    digest
        .finalize()
        .iter()
        .flat_map(|byte| [format!("{:x}", byte >> 4), format!("{:x}", byte & 0x0f)])
        .collect()
}

fn bucket_for(name: &str, expected_key: [u8; 32]) -> String {
    let target = &digest_hex(name)[4..];
    let mut suffixes: Vec<String> = (0..1023).map(|index| format!("{:060x}", index)).collect();
    suffixes.push(target.to_owned());
    suffixes.sort();
    suffixes
        .into_iter()
        .map(|suffix| {
            let (user, key) = if suffix == target {
                ("osl_match", expected_key)
            } else {
                ("osl_decoy", [0_u8; 32])
            };
            format!("{suffix}:{user}:{}\n", STANDARD.encode(key))
        })
        .collect()
}

fn fixture_server(body: String) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let bytes = stream.read(&mut request).unwrap();
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(), body
                )
                .as_bytes(),
            )
            .unwrap();
        String::from_utf8(request[..bytes].to_vec()).unwrap()
    });
    (format!("http://{address}"), handle)
}

#[test]
fn resolves_a_matching_suffix_after_requesting_its_four_hex_bucket() {
    let name = "alice_01";
    let expected_key = [7_u8; 32];
    let (base_url, server) = fixture_server(bucket_for(name, expected_key));

    let resolved = Resolver::new(base_url).unwrap().resolve(name).unwrap();

    assert_eq!(
        resolved,
        Some(ResolvedIdentity {
            user_id: "osl_match".into(),
            ed25519_public: expected_key,
        })
    );
    let request = server.join().unwrap();
    assert!(request.starts_with(&format!(
        "GET /v1/username-bucket/{} HTTP/1.1",
        &digest_hex(name)[..4]
    )));
}

#[test]
fn rejects_short_bucket_before_returning_a_match() {
    let (base_url, server) =
        fixture_server("0".repeat(28) + ":osl_match:" + &STANDARD.encode([7_u8; 32]) + "\n");

    let error = Resolver::new(base_url)
        .unwrap()
        .resolve("alice_01")
        .unwrap_err();

    assert!(matches!(
        error,
        UsernameResolveError::WrongRowCount {
            expected: 1024,
            actual: 1
        }
    ));
    server.join().unwrap();
}
