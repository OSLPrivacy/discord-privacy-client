use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::osl_mail::{open_thread, provision, send, OslMailState};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

const REAL_THREAD: &str = "thread4325real";
const MADE_UP_THREAD: &str = "thread4325madeup";
const FOREIGN_THREAD: &str = "thread4325foreign";
const SUBJECT: &str = "OSL protected message";
const READER_USER_ID: &str = "task-4325-reader";
const READER_ADDRESS: &str = "task4325reader@oslprivacy.com";

#[test]
fn task4325_osl_mail_open_thread_lists_groups_and_fetches_each_message_once() {
    let service = spawn_mail_service();
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("keyserver.json"),
        format!(r#"{{"base_url":"{}"}}"#, service.base_url),
    )
    .unwrap();
    keystore::set_base_dir_override(Some(temp.path().to_path_buf()));
    keystore::set_active_account_dir(None);

    let identity = keystore::generate_identity(READER_USER_ID.to_owned());
    let core = HubCoreState::default();
    *core.osl.identity.lock().unwrap() = Some(identity.clone());
    let state = OslMailState::default();
    provision(&core, &state, "task4325reader".to_owned())
        .expect("task 4325 setup provision should be accepted");

    let sent_words = [
        "TASK4325 exact words message one",
        "TASK4325 exact words message two",
        "TASK4325 exact words message three",
    ];
    for words in sent_words {
        send(
            &core,
            &state,
            READER_ADDRESS.to_owned(),
            SUBJECT.to_owned(),
            words.to_owned(),
        )
        .expect("task 4325 setup send should be accepted");
    }

    let opened = open_thread(&core, &state, REAL_THREAD.to_owned())
        .expect("real thread should open by listing then fetching each message");
    let returned_words = opened
        .messages
        .iter()
        .map(|message| message.body.as_str())
        .collect::<Vec<_>>();
    let fetch_ids_after_open = service.fetch_message_ids();

    println!(
        "TASK4325 real_thread={} returned_messages={} words=[{}] one_message_fetch=/v1/mail/fetch fetch_call_count={} fetch_message_ids={}",
        opened.thread_id,
        opened.messages.len(),
        returned_words.join(" | "),
        fetch_ids_after_open.len(),
        fetch_ids_after_open.join(",")
    );

    assert_eq!(opened.thread_id, REAL_THREAD);
    assert_eq!(returned_words, sent_words);
    assert_eq!(fetch_ids_after_open.len(), 3);
    assert_eq!(
        fetch_ids_after_open,
        ["mail4325m1", "mail4325m2", "mail4325m3"]
    );

    let made_up_refusal = open_thread(&core, &state, MADE_UP_THREAD.to_owned()).unwrap_err();
    println!("TASK4325 made_up_refusal={made_up_refusal}");
    assert!(made_up_refusal.contains(MADE_UP_THREAD));

    let foreign_refusal = open_thread(&core, &state, FOREIGN_THREAD.to_owned()).unwrap_err();
    println!("TASK4325 foreign_refusal={foreign_refusal}");
    assert!(foreign_refusal.contains(FOREIGN_THREAD));
    assert_eq!(
        service.fetch_message_ids().len(),
        3,
        "refused thread names must not be fetched by guessing"
    );

    keystore::set_base_dir_override(None);
}

struct MailService {
    base_url: String,
    state: Arc<Mutex<ServiceState>>,
}

impl MailService {
    fn fetch_message_ids(&self) -> Vec<String> {
        self.state.lock().unwrap().fetch_message_ids.clone()
    }
}

#[derive(Default)]
struct ServiceState {
    uploads: Vec<Value>,
    fetch_message_ids: Vec<String>,
}

fn spawn_mail_service() -> MailService {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let state = Arc::new(Mutex::new(ServiceState::default()));
    let thread_state = Arc::clone(&state);
    std::thread::spawn(move || {
        for _ in 0..17 {
            let mut stream = listener.accept().unwrap().0;
            let request = read_http_request(&mut stream);
            serve_request(&mut stream, &thread_state, &request);
        }
    });
    MailService {
        base_url: format!("http://{address}"),
        state,
    }
}

fn serve_request(stream: &mut TcpStream, state: &Arc<Mutex<ServiceState>>, request: &str) {
    if request.starts_with("GET /v1/mail/capabilities ") {
        write_json(
            stream,
            json!({"version":1,"addressDomain":"oslprivacy.com","oslToOslE2ee":true}),
        );
        return;
    }

    let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
    let request_body: Value = serde_json::from_str(body).unwrap_or_else(|_| json!({}));
    if request.starts_with("POST /v1/mail/address ") {
        write_json(
            stream,
            json!({
                "address": READER_ADDRESS,
                "username": "task4325reader",
                "user_id": READER_USER_ID,
                "state": "active"
            }),
        );
    } else if request.starts_with("POST /v1/mail/send/osl ") {
        let mut locked = state.lock().unwrap();
        let message_index = locked.uploads.len() + 1;
        locked.uploads.push(request_body);
        write_json(
            stream,
            json!({"message_id": format!("mail4325m{message_index}"), "accepted": true}),
        );
    } else if request.starts_with("POST /v1/mail/list ") {
        assert_eq!(request_body["user_id"], READER_USER_ID);
        let messages = state
            .lock()
            .unwrap()
            .uploads
            .iter()
            .enumerate()
            .map(|(index, _upload)| {
                json!({
                    "message_id": format!("mail4325m{}", index + 1),
                    "kind": "osl_message",
                    "sender_user_id": READER_USER_ID,
                    "opaque_thread_token": REAL_THREAD,
                    "received_at": 1_970_002_000_000_i64 + i64::try_from(index).unwrap(),
                    "expires_at": 1_970_002_100_000_i64,
                    "byte_length": 64,
                    "subject": SUBJECT
                })
            })
            .collect::<Vec<_>>();
        write_json(stream, json!({ "messages": messages }));
    } else if request.starts_with("POST /v1/mail/fetch ") {
        assert!(request_body.get("thread_id").is_none());
        let message_id = request_body["message_id"].as_str().unwrap().to_owned();
        let index = message_id
            .strip_prefix("mail4325m")
            .unwrap()
            .parse::<usize>()
            .unwrap()
            - 1;
        let mut locked = state.lock().unwrap();
        locked.fetch_message_ids.push(message_id.clone());
        let upload = locked.uploads[index].clone();
        write_json(
            stream,
            json!({
                "message_id": message_id,
                "kind": "osl_message",
                "sender_user_id": READER_USER_ID,
                "subject": SUBJECT,
                "ciphertext_b64": upload["ciphertext_b64"],
                "envelope": upload["envelope"],
                "received_at": 1_970_002_000_000_i64 + i64::try_from(index).unwrap()
            }),
        );
    } else {
        write!(
            stream,
            "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
        )
        .unwrap();
    }
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

fn write_json(stream: &mut TcpStream, body: Value) {
    let body = body.to_string();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.as_bytes().len(),
        body
    )
    .unwrap();
}
