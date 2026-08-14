use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::app_preferences::AskBeforeIrreversibleActionsChoice;
use ipc::commands::{
    cmd_osl_chat_burn_sender_message_records_choice,
    cmd_osl_chat_burn_sender_message_records_choice_confirmed, cmd_osl_load_channel_history,
};
use ipc::irreversible_action::{COMPLETED_ANSWER, NEEDS_CONFIRMING_ANSWER};
use ipc::AppState;
use keystore::{canonical_burn_bytes, BurnScope, KeyServerClient};
use serde_json::Value;
use store::{MessageStore, StoredMessage};

const ASKSTEP: &str = "ASKSTEP";
const CHAT_ID: &str = "task3156-ask-step-chat";
const SELF_ID: &str = "task3156-owner";
const MESSAGE_IDS: [&str; 3] = [
    "task3156-ask-step-1",
    "task3156-ask-step-2",
    "task3156-ask-step-3",
];

struct SignedBurnServer {
    base_url: String,
    requests: Arc<Mutex<Vec<String>>>,
    server: JoinHandle<()>,
}

impl SignedBurnServer {
    fn start(identity: &keystore::Identity) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback burn server");
        let address = listener.local_addr().expect("read loopback burn address");
        let expected = MESSAGE_IDS
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        let user_id = identity.user_id.clone();
        let public_key = identity.ed25519_public;
        let requests = Arc::new(Mutex::new(Vec::new()));
        let requests_for_server = Arc::clone(&requests);
        let server = thread::spawn(move || {
            for _ in 0..expected.len() {
                let (mut stream, _) = listener.accept().expect("accept confirmed burn request");
                let request = read_request(&mut stream);
                let request_text = String::from_utf8_lossy(&request);
                let (head, body) = request_text
                    .split_once("\r\n\r\n")
                    .expect("request has headers and body");
                assert!(
                    head.starts_with("DELETE /v1/wrapped-keys HTTP/1.1"),
                    "confirmed burn must call the wrapped-key delete endpoint"
                );
                let burn = serde_json::from_str::<Value>(body).expect("burn request is JSON");
                assert_eq!(burn["scope"], "single");
                assert_eq!(burn["user_id"], user_id);
                let content_id = burn["target_content_id"]
                    .as_str()
                    .expect("single burn names a content id");
                assert!(expected.contains(content_id));

                let timestamp_ms = burn["timestamp_ms"]
                    .as_i64()
                    .expect("signed burn carries a timestamp");
                let request_id = burn["request_id"]
                    .as_str()
                    .expect("signed burn carries a request id");
                let signature = STANDARD
                    .decode(
                        burn["burn_signature_b64"]
                            .as_str()
                            .expect("signed burn carries a signature"),
                    )
                    .expect("signature is base64");
                let signature = crypto::ed25519::Signature::from_bytes(
                    signature.try_into().expect("signature is 64 bytes"),
                );
                let canonical = canonical_burn_bytes(
                    &user_id,
                    timestamp_ms,
                    request_id,
                    &BurnScope::Single {
                        content_id: content_id.to_owned(),
                    },
                );
                assert!(
                    crypto::ed25519::verify(&public_key, &canonical, &signature).unwrap(),
                    "confirmed burn signature must verify"
                );
                requests_for_server
                    .lock()
                    .expect("lock recorded requests")
                    .push(content_id.to_owned());

                let response = r#"{"scope":"single","deleted_count":1}"#;
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.len(),
                    response
                )
                .expect("respond to confirmed burn request");
            }
        });
        Self {
            base_url: format!("http://{address}"),
            requests,
            server,
        }
    }

    fn join(self) -> Vec<String> {
        self.server.join().expect("burn server exits cleanly");
        self.requests
            .lock()
            .expect("lock recorded requests")
            .clone()
    }
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream.read(&mut chunk).expect("read burn request");
        assert_ne!(read, 0, "burn request ended before its body arrived");
        request.extend_from_slice(&chunk[..read]);
        let Some(headers_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
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

fn askstep_count(state: &AppState) -> usize {
    cmd_osl_load_channel_history(state, CHAT_ID.to_owned(), Some(10))
        .expect("load ASKSTEP chat")
        .into_iter()
        .filter(|message| message.plaintext == ASKSTEP)
        .count()
}

#[test]
fn task3156_ask_step_blocks_chat_burn_until_confirmed() {
    let state = AppState::new();
    state
        .app_preferences
        .lock()
        .expect("app preferences mutex poisoned")
        .ask_before_irreversible_actions = AskBeforeIrreversibleActionsChoice::On;

    let identity = keystore::identity_from_entropy([0x56; 16], SELF_ID.to_owned());
    let server = SignedBurnServer::start(&identity);
    state.install_identity(identity);
    *state.keyserver.lock().expect("keyserver mutex poisoned") =
        Some(KeyServerClient::new(&server.base_url).expect("install loopback keyserver"));

    let store_dir = tempfile::tempdir().expect("isolated message store");
    let store = MessageStore::open(store_dir.path(), &[0x56; 32]).expect("open message store");
    for (index, message_id) in MESSAGE_IDS.into_iter().enumerate() {
        store
            .put(&StoredMessage {
                discord_message_id: message_id.to_owned(),
                channel_id: CHAT_ID.to_owned(),
                sender_discord_id: SELF_ID.to_owned(),
                sender_osl_user_id: SELF_ID.to_owned(),
                plaintext: ASKSTEP.to_owned(),
                decrypted_at: 1_903_156_000 + i64::try_from(index).unwrap(),
                burned: false,
                reply_parent_id: None,
                edit_revision: 0,
            })
            .expect("seed ASKSTEP message");
    }
    *state
        .message_store
        .lock()
        .expect("message store mutex poisoned") = Some(store);

    let before = askstep_count(&state);
    assert_eq!(before, 3);

    let unconfirmed =
        cmd_osl_chat_burn_sender_message_records_choice(&state, CHAT_ID.to_owned(), "both-sides")
            .expect("unconfirmed burn returns an answer");
    let after_unconfirmed = askstep_count(&state);
    assert_eq!(after_unconfirmed, 3);
    assert_eq!(unconfirmed.answer(), NEEDS_CONFIRMING_ANSWER);
    assert!(unconfirmed.result().is_none());

    let confirmed = cmd_osl_chat_burn_sender_message_records_choice_confirmed(
        &state,
        CHAT_ID.to_owned(),
        "both-sides",
        true,
    )
    .expect("confirmed burn succeeds");
    let result = confirmed.result().expect("confirmed burn has a result");
    assert_eq!(confirmed.answer(), COMPLETED_ANSWER);
    assert_eq!(result.requested_count, 3);
    assert_eq!(result.local_removal_count, 3);
    assert_eq!(result.remote_removal_count, 3);
    assert_eq!(result.remaining_local_count, 0);
    let after_confirmed = askstep_count(&state);
    assert_eq!(after_confirmed, 0);

    let remote_requests = server.join();
    assert_eq!(
        remote_requests.into_iter().collect::<BTreeSet<_>>(),
        MESSAGE_IDS
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
    );

    println!(
        "TASK3156 marker={ASKSTEP} askstep_matching_before={before} unconfirmed_answer={} after_unconfirmed={after_unconfirmed} confirmed_answer={} after_confirmed={after_confirmed} requested_count={} local_removal_count={} remote_removal_count={}",
        unconfirmed.answer(),
        confirmed.answer(),
        result.requested_count,
        result.local_removal_count,
        result.remote_removal_count,
    );
}
