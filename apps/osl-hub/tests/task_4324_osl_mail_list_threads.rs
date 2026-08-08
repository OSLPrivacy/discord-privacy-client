use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::osl_mail::{
    list_my_threads, provision, OslMailState, OSL_MAIL_THREAD_LIST_CAP,
};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;

#[test]
fn task4324_osl_mail_list_my_threads_caps_and_refuses_locked_account() {
    let four = run_list_case(
        "task-4324-four",
        "task4324four",
        json!({ "messages": [
            listed_thread("thread4324four000", "sender0@example.com", 1_970_000_004_000_i64, "Subject zero"),
            listed_thread("thread4324four001", "sender1@example.com", 1_970_000_003_000_i64, "Subject one"),
            listed_thread("thread4324four002", "sender2@example.com", 1_970_000_002_000_i64, "Subject two"),
            listed_thread("thread4324four003", "sender3@example.com", 1_970_000_001_000_i64, "Subject three")
        ]}),
    );
    println!(
        "TASK4324 four_count={} first_sender={} first_time={} first_subject={} requested_limit={}",
        four.threads.len(),
        four.threads[0].correspondent,
        four.threads[0].latest_at,
        four.threads[0].subject,
        four.requested_limit
    );
    assert_eq!(four.threads.len(), 4);
    assert_eq!(four.threads[0].correspondent, "sender0@example.com");
    assert_eq!(four.threads[0].latest_at, 1_970_000_004_000);
    assert_eq!(four.threads[0].subject, "Subject zero");
    assert_eq!(four.requested_limit, OSL_MAIL_THREAD_LIST_CAP as u64);

    let empty = run_list_case(
        "task-4324-empty",
        "task4324empty",
        json!({ "messages": [] }),
    );
    println!(
        "TASK4324 empty_count={} empty_error=false requested_limit={}",
        empty.threads.len(),
        empty.requested_limit
    );
    assert!(empty.threads.is_empty());

    let over_cap_messages = (0..(OSL_MAIL_THREAD_LIST_CAP + 3))
        .map(|index| {
            listed_thread(
                &format!("thread4324cap{index:03}"),
                &format!("sender{index}@example.com"),
                1_970_001_000_000_i64 - i64::try_from(index).unwrap(),
                &format!("Cap subject {index}"),
            )
        })
        .collect::<Vec<_>>();
    let over_cap = run_list_case(
        "task-4324-cap",
        "task4324cap",
        json!({ "messages": over_cap_messages }),
    );
    println!(
        "TASK4324 cap={} over_cap_input={} returned={} first_subject={} last_subject={} requested_limit={}",
        OSL_MAIL_THREAD_LIST_CAP,
        OSL_MAIL_THREAD_LIST_CAP + 3,
        over_cap.threads.len(),
        over_cap.threads[0].subject,
        over_cap.threads.last().unwrap().subject,
        over_cap.requested_limit
    );
    assert_eq!(over_cap.threads.len(), OSL_MAIL_THREAD_LIST_CAP);

    let locked_core = HubCoreState::default();
    let locked_state = OslMailState::default();
    let locked = list_my_threads(&locked_core, &locked_state).unwrap_err();
    println!("TASK4324 locked_refusal={locked}");
    assert_eq!(locked, "Unlock an OSL identity before using OSL Mail");
}

struct ListCaseResult {
    threads: Vec<osl_privacy_hub::osl_mail::OslMailThreadSummary>,
    requested_limit: u64,
}

fn run_list_case(user_id: &str, username: &str, list_response: Value) -> ListCaseResult {
    let (base_url, list_request_rx) = spawn_mail_list_service(user_id, username, list_response);
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("keyserver.json"),
        format!(r#"{{"base_url":"{base_url}"}}"#),
    )
    .unwrap();
    keystore::set_base_dir_override(Some(temp.path().to_path_buf()));
    keystore::set_active_account_dir(None);

    let core = HubCoreState::default();
    *core.osl.identity.lock().unwrap() = Some(keystore::generate_identity(user_id.to_owned()));
    let state = OslMailState::default();
    provision(&core, &state, username.to_owned()).unwrap();
    let threads = list_my_threads(&core, &state).unwrap();
    let list_request = list_request_rx.recv().unwrap();
    keystore::set_base_dir_override(None);

    ListCaseResult {
        threads,
        requested_limit: list_request["limit"].as_u64().unwrap(),
    }
}

fn listed_thread(thread_id: &str, sender: &str, received_at: i64, subject: &str) -> Value {
    json!({
        "message_id": format!("mail_{thread_id}"),
        "kind": "external_envelope",
        "sender_address": sender,
        "opaque_thread_token": thread_id,
        "recipient_key_fingerprint": "fingerprint",
        "received_at": received_at,
        "expires_at": received_at + 1_000,
        "byte_length": 12,
        "subject": subject
    })
}

fn spawn_mail_list_service(
    user_id: &str,
    username: &str,
    list_response: Value,
) -> (String, mpsc::Receiver<Value>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let user_id = user_id.to_owned();
    let username = username.to_owned();
    std::thread::spawn(move || {
        for request_index in 0..4 {
            let mut stream = listener.accept().unwrap().0;
            let request = read_http_request(&mut stream);
            match request_index {
                0 | 2 => {
                    assert!(request.starts_with("GET /v1/mail/capabilities "));
                    write_json(
                        &mut stream,
                        r#"{"version":1,"addressDomain":"oslprivacy.com","oslToOslE2ee":true}"#,
                    );
                }
                1 => {
                    assert!(request.starts_with("POST /v1/mail/address "));
                    write_json(
                        &mut stream,
                        &json!({
                            "address": format!("{username}@oslprivacy.com"),
                            "username": username,
                            "user_id": user_id,
                            "state": "active"
                        })
                        .to_string(),
                    );
                }
                3 => {
                    assert!(request.starts_with("POST /v1/mail/list "));
                    let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
                    tx.send(serde_json::from_str(body).unwrap()).unwrap();
                    write_json(&mut stream, &list_response.to_string());
                }
                _ => unreachable!(),
            }
        }
    });
    (format!("http://{address}"), rx)
}

fn read_http_request(stream: &mut TcpStream) -> String {
    let mut buffer = [0u8; 8192];
    let mut bytes = Vec::new();
    loop {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "connection closed before request completed");
        bytes.extend_from_slice(&buffer[..read]);
        if let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            let content_len = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length: "))
                .or_else(|| {
                    headers
                        .lines()
                        .find_map(|line| line.strip_prefix("Content-Length: "))
                })
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if bytes.len() >= header_end + 4 + content_len {
                return String::from_utf8_lossy(&bytes).to_string();
            }
        }
    }
}

fn write_json(stream: &mut TcpStream, body: &str) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.as_bytes().len(),
        body
    )
    .unwrap();
}
