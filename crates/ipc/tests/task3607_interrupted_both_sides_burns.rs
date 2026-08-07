use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::commands::{
    cmd_osl_burn_message, cmd_osl_burn_sender_message_records_both_sides,
    cmd_osl_load_channel_history,
};
use ipc::state::AppState;
use keystore::{canonical_burn_bytes, BurnScope, KeyServerClient};
use serde_json::Value;
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const SECRET: &[u8; 32] = &[0x67; 32];
const CHANNEL: &str = "task3607-channel";
const SENDER: &str = "burner-3607";

#[derive(Clone, Copy)]
enum BurnAction {
    Succeed,
    DropAfterRequest,
}

struct ScriptedBurnServer {
    base_url: String,
    successful_removals: Arc<Mutex<Vec<String>>>,
    attempted_requests: Arc<Mutex<Vec<String>>>,
    server: JoinHandle<()>,
}

impl ScriptedBurnServer {
    fn start(
        actions: Vec<BurnAction>,
        remotely_present: Vec<String>,
        user_id: String,
        public_key: crypto::ed25519::PublicKey,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback burn server");
        let address = listener.local_addr().expect("read loopback burn address");
        let removed = Arc::new(Mutex::new(
            remotely_present.into_iter().collect::<BTreeSet<_>>(),
        ));
        let successful_removals = Arc::new(Mutex::new(Vec::new()));
        let attempted_requests = Arc::new(Mutex::new(Vec::new()));
        let removed_for_server = Arc::clone(&removed);
        let successes_for_server = Arc::clone(&successful_removals);
        let attempts_for_server = Arc::clone(&attempted_requests);
        let server = thread::spawn(move || {
            for action in actions {
                let (mut stream, _) = listener.accept().expect("accept burn request");
                let content_id =
                    verified_content_id(&mut stream, &user_id, public_key).expect("valid burn");
                attempts_for_server
                    .lock()
                    .expect("lock attempts")
                    .push(content_id.clone());
                match action {
                    BurnAction::Succeed => {
                        let deleted = removed_for_server
                            .lock()
                            .expect("lock remote rows")
                            .remove(&content_id);
                        if deleted {
                            successes_for_server
                                .lock()
                                .expect("lock successes")
                                .push(content_id);
                        }
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
                    }
                    BurnAction::DropAfterRequest => return,
                }
            }
        });
        Self {
            base_url: format!("http://{address}"),
            successful_removals,
            attempted_requests,
            server,
        }
    }

    fn join(self) -> (Vec<String>, Vec<String>) {
        self.server.join().expect("burn server exits cleanly");
        let successes = self
            .successful_removals
            .lock()
            .expect("lock successes")
            .clone();
        let attempts = self
            .attempted_requests
            .lock()
            .expect("lock attempts")
            .clone();
        (successes, attempts)
    }
}

fn sample(id: &str, sender: &str, body: &str, at: i64) -> StoredMessage {
    StoredMessage {
        discord_message_id: id.to_string(),
        channel_id: CHANNEL.to_string(),
        sender_discord_id: sender.to_string(),
        sender_osl_user_id: sender.to_string(),
        plaintext: body.to_string(),
        decrypted_at: at,
        burned: false,
        reply_parent_id: None,
        edit_revision: 0,
    }
}

fn state_with_store(
    dir: &std::path::Path,
    identity: keystore::Identity,
    keyserver_url: &str,
) -> AppState {
    let state = AppState::new();
    state.install_identity(identity);
    let store = MessageStore::open(dir, SECRET).expect("open message store");
    *state.message_store.lock().unwrap() = Some(store);
    *state.keyserver.lock().unwrap() =
        Some(KeyServerClient::new(keyserver_url).expect("install loopback keyserver"));
    state
}

fn verified_content_id(
    stream: &mut TcpStream,
    user_id: &str,
    public_key: crypto::ed25519::PublicKey,
) -> Result<String, String> {
    let request = read_request(stream);
    let request_text = String::from_utf8_lossy(&request);
    let (head, body) = request_text
        .split_once("\r\n\r\n")
        .ok_or_else(|| "request has no body".to_string())?;
    assert!(
        head.starts_with("DELETE /v1/wrapped-keys HTTP/1.1"),
        "remote burn must call the wrapped-key delete endpoint"
    );
    let burn = serde_json::from_str::<Value>(body).map_err(|e| e.to_string())?;
    assert_eq!(burn["scope"], "single");
    assert_eq!(burn["user_id"], user_id);
    assert!(burn["target_user_id"].is_null());
    let content_id = burn["target_content_id"]
        .as_str()
        .ok_or_else(|| "single-scope burn names no content id".to_string())?;
    let timestamp_ms = burn["timestamp_ms"]
        .as_i64()
        .ok_or_else(|| "signed burn has no timestamp".to_string())?;
    let request_id = burn["request_id"]
        .as_str()
        .ok_or_else(|| "signed burn has no request id".to_string())?;
    let signature_b64 = burn["burn_signature_b64"]
        .as_str()
        .ok_or_else(|| "signed burn has no signature".to_string())?;
    let signature_bytes = STANDARD.decode(signature_b64).map_err(|e| e.to_string())?;
    let signature_array: [u8; crypto::ed25519::SIGNATURE_SIZE] = signature_bytes
        .try_into()
        .map_err(|_| "signature is not 64 bytes".to_string())?;
    let signature = crypto::ed25519::Signature::from_bytes(signature_array);
    let canonical = canonical_burn_bytes(
        user_id,
        timestamp_ms,
        request_id,
        &BurnScope::Single {
            content_id: content_id.to_owned(),
        },
    );
    assert!(
        crypto::ed25519::verify(&public_key, &canonical, &signature).unwrap(),
        "remote burn request signature must verify"
    );
    Ok(content_id.to_owned())
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

fn selected_remaining(state: &AppState, ids: &[String]) -> usize {
    let history = cmd_osl_load_channel_history(state, CHANNEL.to_string(), Some(20)).unwrap();
    history
        .iter()
        .filter(|row| ids.iter().any(|id| id == &row.discord_message_id))
        .count()
}

fn burned_row_fingerprint(dir: &std::path::Path) -> String {
    let store = MessageStore::open(dir, SECRET).expect("open message store for fingerprint");
    store
        .burned_message_record_fingerprint("task3607-unrelated-marked")
        .expect("fingerprint burned row")
        .expect("burned row exists")
}

fn burn_journal_count(dir: &std::path::Path) -> usize {
    let store = MessageStore::open(dir, SECRET).expect("open message store for journal count");
    store
        .sender_message_burn_journal_count()
        .expect("count burn journals")
}

#[test]
fn task3607_restart_finishes_interrupted_both_sides_burn_without_retouching_targets() {
    let tmp = TempDir::new().unwrap();
    let identity = keystore::identity_from_entropy([0x37; 16], SENDER.to_owned());
    let public_key = identity.ed25519_public;
    let selected = vec![
        "task3607-target-1".to_owned(),
        "task3607-target-2".to_owned(),
        "task3607-target-3".to_owned(),
    ];
    let unrelated_marked = "task3607-unrelated-marked";

    {
        let setup = AppState::new();
        setup.install_identity(identity.clone());
        let store = MessageStore::open(tmp.path(), SECRET).expect("open setup message store");
        *setup.message_store.lock().unwrap() = Some(store);
        {
            let guard = setup.message_store.lock().unwrap();
            let store = guard.as_ref().expect("message store installed");
            for (index, id) in selected.iter().enumerate() {
                store
                    .put(&sample(
                        id,
                        SENDER,
                        &format!("TASK3607 selected sender record {}", index + 1),
                        1_900_100_000 + index as i64,
                    ))
                    .unwrap();
            }
            store
                .put(&sample(
                    unrelated_marked,
                    "sender-3607-other",
                    "TASK3607 unrelated marked row",
                    1_900_100_100,
                ))
                .unwrap();
        }
        cmd_osl_burn_message(&setup, unrelated_marked.to_owned()).unwrap();
    }
    let fingerprint_before = burned_row_fingerprint(tmp.path());

    let server1 = ScriptedBurnServer::start(
        vec![BurnAction::Succeed, BurnAction::DropAfterRequest],
        selected.clone(),
        SENDER.to_owned(),
        public_key,
    );
    {
        let state = state_with_store(tmp.path(), identity.clone(), &server1.base_url);
        let err =
            cmd_osl_burn_sender_message_records_both_sides(&state, selected.clone()).unwrap_err();
        assert!(
            err.contains("remote wrapped-key removal failed"),
            "first interruption must be remote failure after local step: {err}"
        );
        assert_eq!(selected_remaining(&state, &selected), 1);
    }
    let (success1, attempts1) = server1.join();

    let server2 = ScriptedBurnServer::start(
        vec![BurnAction::Succeed, BurnAction::DropAfterRequest],
        selected.clone(),
        SENDER.to_owned(),
        public_key,
    );
    {
        let state = state_with_store(tmp.path(), identity.clone(), &server2.base_url);
        let err =
            cmd_osl_burn_sender_message_records_both_sides(&state, selected.clone()).unwrap_err();
        assert!(
            err.contains("remote wrapped-key removal failed"),
            "second interruption must be remote failure after local step: {err}"
        );
        assert_eq!(selected_remaining(&state, &selected), 0);
    }
    let (success2, attempts2) = server2.join();

    let server3 = ScriptedBurnServer::start(
        vec![BurnAction::Succeed],
        selected.clone(),
        SENDER.to_owned(),
        public_key,
    );
    let final_result = {
        let state = state_with_store(tmp.path(), identity.clone(), &server3.base_url);
        let result =
            cmd_osl_burn_sender_message_records_both_sides(&state, selected.clone()).unwrap();
        assert_eq!(selected_remaining(&state, &selected), 0);
        result
    };
    let (success3, attempts3) = server3.join();

    let fingerprint_after = burned_row_fingerprint(tmp.path());
    let remote_successes = [success1.clone(), success2.clone(), success3.clone()].concat();
    let remote_attempts = [attempts1.clone(), attempts2.clone(), attempts3.clone()].concat();
    let mut per_target_success_counts = selected
        .iter()
        .map(|id| {
            (
                id.clone(),
                remote_successes
                    .iter()
                    .filter(|success| *success == id)
                    .count(),
            )
        })
        .collect::<Vec<_>>();
    per_target_success_counts.sort();

    assert_eq!(burn_journal_count(tmp.path()), 1);
    assert_eq!(final_result.requested_count, 3);
    assert_eq!(final_result.local_removal_count, 3);
    assert_eq!(final_result.remote_removal_count, 3);
    assert_eq!(final_result.remaining_local_count, 0);
    assert!(final_result.equal_removal_counts);
    assert_eq!(
        per_target_success_counts,
        vec![
            ("task3607-target-1".to_owned(), 1),
            ("task3607-target-2".to_owned(), 1),
            ("task3607-target-3".to_owned(), 1),
        ]
    );
    assert_eq!(fingerprint_after, fingerprint_before);

    println!("TASK3607_BURN_ID={}", final_result.burn_id);
    println!("TASK3607_RESTART_COUNT=3");
    println!(
        "TASK3607_JOURNAL_BURN_COUNT={}",
        burn_journal_count(tmp.path())
    );
    println!("TASK3607_REMOTE_ATTEMPTS={}", remote_attempts.join(","));
    println!(
        "TASK3607_REMOTE_SUCCESSFUL_REMOVALS={}",
        remote_successes.join(",")
    );
    println!(
        "TASK3607_TARGET_SUCCESS_COUNTS={}",
        per_target_success_counts
            .iter()
            .map(|(id, count)| format!("{id}:{count}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    println!(
        "TASK3607_FINAL_COUNTS requested={} local_done={} remote_done={} remaining_local={} equal={}",
        final_result.requested_count,
        final_result.local_removal_count,
        final_result.remote_removal_count,
        final_result.remaining_local_count,
        final_result.equal_removal_counts
    );
    println!(
        "TASK3607_UNRELATED_MARKED_FINGERPRINT_BEFORE={}",
        fingerprint_before
    );
    println!(
        "TASK3607_UNRELATED_MARKED_FINGERPRINT_AFTER={}",
        fingerprint_after
    );
}
