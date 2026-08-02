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
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};

    use serde_json::Value;

    use super::*;

    struct WrappedKeyRelay {
        base_url: String,
        wrapped_key_exists: Arc<Mutex<bool>>,
        server: JoinHandle<()>,
    }

    impl WrappedKeyRelay {
        fn start(content_id: &'static str) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback relay");
            let address = listener.local_addr().expect("read relay address");
            let wrapped_key_exists = Arc::new(Mutex::new(true));
            let relay_state = Arc::clone(&wrapped_key_exists);
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept burn request");
                let request = read_request(&mut stream);
                let request_text = String::from_utf8_lossy(&request);
                let (head, body) = request_text
                    .split_once("\r\n\r\n")
                    .expect("request has headers and body");
                let burn = serde_json::from_str::<Value>(body).expect("burn request is JSON");
                let single_target_matches = burn["scope"] == "single"
                    && burn["target_content_id"] == content_id
                    && burn["target_user_id"].is_null();
                let deleted = head.starts_with("DELETE /v1/wrapped-keys HTTP/1.1")
                    && single_target_matches
                    && std::mem::replace(
                        &mut *relay_state.lock().expect("lock wrapped-key relay state"),
                        false,
                    );
                let body = format!(
                    r#"{{"scope":"single","deleted_count":{}}}"#,
                    u32::from(deleted)
                );
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
                wrapped_key_exists,
                server,
            }
        }

        fn wrapped_key_exists(&self) -> bool {
            *self
                .wrapped_key_exists
                .lock()
                .expect("lock wrapped-key relay state")
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
        let relay = WrappedKeyRelay::start(CONTENT_ID);
        let client = KeyServerClient::new(&relay.base_url).expect("construct relay client");
        let identity = keystore::identity_from_entropy([22; 16], "burner".to_owned());

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
        assert!(
            !relay.wrapped_key_exists(),
            "a successful burn must remove the relay's wrapped key"
        );
        relay.join();
    }
}
