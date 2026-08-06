//! Server-side half of a burn.
//!
//! A local wipe only removes this device's ability to decrypt its cached copy.
//! This module performs the separate, authenticated keyserver deletion that
//! makes the sender's wrapped key unavailable from the relay as well.

use keystore::{BurnResponse, BurnScope, Identity, KeyServerClient};

/// Destroy wrapped keys held by the relay for `scope`.
///
/// The caller has already selected and authorized the scope through the burn
/// contract.  Keeping this operation at the Hub boundary ensures the shipping
/// app, rather than only the retired shell, reaches the server-enforced tier.
pub fn destroy_server_wrapped_keys(
    client: &KeyServerClient,
    identity: &Identity,
    scope: &BurnScope,
) -> keystore::Result<BurnResponse> {
    client.burn(identity, scope)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};

    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use serde_json::Value;

    use super::*;

    #[derive(Clone)]
    struct WrappedKeyRecord {
        sender_id: String,
    }

    struct WrappedKeyRelay {
        base_url: String,
        wrapped_keys: Arc<Mutex<BTreeMap<String, WrappedKeyRecord>>>,
        server: JoinHandle<()>,
    }

    impl WrappedKeyRelay {
        fn start(identity: &Identity, content_id: &'static str) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback relay");
            let address = listener.local_addr().expect("read relay address");
            let wrapped_keys = Arc::new(Mutex::new(BTreeMap::from([
                (
                    content_id.to_owned(),
                    WrappedKeyRecord {
                        sender_id: identity.user_id.clone(),
                    },
                ),
                (
                    "peer-owned-message".to_owned(),
                    WrappedKeyRecord {
                        sender_id: "peer-user".to_owned(),
                    },
                ),
            ])));
            let relay_state = Arc::clone(&wrapped_keys);
            let expected_user_id = identity.user_id.clone();
            let expected_public_key = identity.ed25519_public;
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept burn request");
                let request = read_request(&mut stream);
                let request_text = String::from_utf8_lossy(&request);
                let (head, body) = request_text
                    .split_once("\r\n\r\n")
                    .expect("request has headers and body");
                let burn = serde_json::from_str::<Value>(body).expect("burn request is JSON");
                let timestamp_ms = burn["timestamp_ms"]
                    .as_i64()
                    .expect("burn request has timestamp_ms");
                let request_id = burn["request_id"]
                    .as_str()
                    .expect("burn request has request_id");
                let signature_b64 = burn["burn_signature_b64"]
                    .as_str()
                    .expect("burn request has burn_signature_b64");
                let single_target_matches = burn["scope"] == "single"
                    && burn["user_id"] == expected_user_id
                    && burn["target_content_id"] == content_id
                    && burn["target_user_id"].is_null();
                let signature_bytes: [u8; crypto::ed25519::SIGNATURE_SIZE] = STANDARD
                    .decode(signature_b64)
                    .expect("burn signature is base64")
                    .try_into()
                    .expect("burn signature is 64 bytes");
                let signature = crypto::ed25519::Signature::from_bytes(signature_bytes);
                let canonical = keystore::canonical_burn_bytes(
                    &expected_user_id,
                    timestamp_ms,
                    request_id,
                    &BurnScope::Single {
                        content_id: content_id.to_owned(),
                    },
                );
                let signature_valid =
                    crypto::ed25519::verify(&expected_public_key, &canonical, &signature)
                        .expect("burn signature verification runs");
                let deleted = head.starts_with("DELETE /v1/wrapped-keys HTTP/1.1")
                    && single_target_matches
                    && signature_valid;
                let removed = if deleted {
                    let mut rows = relay_state.lock().expect("lock wrapped-key relay state");
                    match rows.get(content_id) {
                        Some(record) if record.sender_id == expected_user_id => {
                            rows.remove(content_id);
                            1
                        }
                        _ => 0,
                    }
                } else {
                    0
                };
                let body = format!(r#"{{"scope":"single","deleted_count":{}}}"#, removed);
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .expect("respond to burn request");
            });
            Self {
                base_url: format!("http://{address}"),
                wrapped_keys,
                server,
            }
        }

        fn wrapped_key_exists(&self, content_id: &str) -> bool {
            self.wrapped_keys
                .lock()
                .expect("lock wrapped-key relay state")
                .contains_key(content_id)
        }

        fn join(self) {
            self.server.join().expect("relay exits cleanly");
        }
    }

    fn read_request(stream: &mut TcpStream) -> Vec<u8> {
        let mut request = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            let read = stream.read(&mut chunk).expect("read burn request");
            assert_ne!(read, 0, "burn request ended before its body arrived");
            request.extend_from_slice(&chunk[..read]);
            let Some(headers_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n")
            else {
                continue;
            };
            let headers = String::from_utf8_lossy(&request[..headers_end]);
            let content_length = headers
                .lines()
                .find_map(|line| line.strip_prefix("Content-Length: "))
                .expect("burn request has content length")
                .parse::<usize>()
                .expect("content length is numeric");
            if request.len() >= headers_end + 4 + content_length {
                return request;
            }
        }
    }

    #[test]
    fn tf_22_burn_removes_the_server_held_wrapped_key() {
        const CONTENT_ID: &str = "message-to-burn";
        let identity = keystore::identity_from_entropy([22; 16], "burner".to_owned());
        let relay = WrappedKeyRelay::start(&identity, CONTENT_ID);
        let client = KeyServerClient::new(&relay.base_url).expect("construct relay client");

        let result = destroy_server_wrapped_keys(
            &client,
            &identity,
            &BurnScope::Single {
                content_id: CONTENT_ID.to_owned(),
            },
        )
        .expect("server-side wrapped-key burn succeeds");

        assert_eq!(result.scope, "single");
        assert_eq!(result.deleted_count, 1);
        println!("TASK0508 remote_burn_response.scope={}", result.scope);
        println!(
            "TASK0508 remote_burn_response.deleted_count={}",
            result.deleted_count
        );
        assert!(
            !relay.wrapped_key_exists(CONTENT_ID),
            "a successful burn must remove the sender's selected wrapped key"
        );
        println!(
            "TASK0508 selected_sender_owned_record_exists_after={}",
            relay.wrapped_key_exists(CONTENT_ID)
        );
        assert!(
            relay.wrapped_key_exists("peer-owned-message"),
            "the signed burn must not remove a peer-owned wrapped key"
        );
        println!(
            "TASK0508 peer_owned_record_exists_after={}",
            relay.wrapped_key_exists("peer-owned-message")
        );
        relay.join();
    }
}
