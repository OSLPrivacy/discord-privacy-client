use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::osl_mail::{get_status, open_thread, provision, send, OslMailState};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::{Arc, Mutex};

const THREAD_ID: &str = "thread4330readonly";
const SUBJECT: &str = "OSL protected message";
const READER_USER_ID: &str = "task-4330-reader";
const READER_ADDRESS: &str = "task4330reader@oslprivacy.com";
const WORDS: [&str; 4] = ["alpha", "bravo", "charlie", "delta"];

#[test]
fn task4330_reading_a_mail_thread_preserves_the_whole_mailbox() {
    let proof =
        run_check(false).expect("an ordinary read must leave the mailbox byte-for-byte equal");

    println!(
        "TASK4330 before_messages={} before_unread={} returned_words={} words={} differences={} after_messages={} after_unread={}",
        proof.before_messages,
        proof.before_unread,
        proof.returned_words.len(),
        proof.returned_words.join("|"),
        proof.differences,
        proof.after_messages,
        proof.after_unread,
    );

    assert_eq!(proof.before_messages, 4);
    assert_eq!(proof.before_unread, 4);
    assert_eq!(proof.returned_words, WORDS);
    assert_eq!(proof.differences, 0);
    assert_eq!(proof.after_messages, 4);
    assert_eq!(proof.after_unread, 4);

    let child = Command::new(std::env::current_exe().expect("current test executable"))
        .args([
            "--ignored",
            "--exact",
            "task4330_mark_during_read_probe",
            "--nocapture",
            "--test-threads=1",
        ])
        .output()
        .expect("run the isolated mutation probe");
    let stderr = String::from_utf8_lossy(&child.stderr);
    let stdout = String::from_utf8_lossy(&child.stdout);
    let combined = format!("{stdout}{stderr}");

    println!(
        "TASK4330 mutation_exit={} mutation_error={}",
        child.status.code().unwrap_or(-1),
        combined
            .lines()
            .find(|line| line.contains("TASK4330 mailbox changed"))
            .unwrap_or("missing")
    );

    assert_eq!(child.status.code(), Some(1), "mutation output:\n{combined}");
    assert!(
        combined.contains(
            "TASK4330 mailbox changed: messages[0] message_id=mail4330m1 field=unread before=true after=false"
        ),
        "mutation must name the exact changed mailbox field:\n{combined}"
    );
}

#[test]
#[ignore = "spawned by the main check to prove a mark makes the verifier exit 1"]
fn task4330_mark_during_read_probe() {
    match run_check(true) {
        Ok(_) => {
            eprintln!("TASK4330 mutation was not detected");
            std::process::exit(2);
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ReadProof {
    before_messages: usize,
    before_unread: usize,
    returned_words: Vec<String>,
    differences: usize,
    after_messages: usize,
    after_unread: usize,
}

fn run_check(mark_during_read: bool) -> Result<ReadProof, String> {
    let service = spawn_mail_service(mark_during_read);
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    std::fs::write(
        temp.path().join("keyserver.json"),
        format!(r#"{{"base_url":"{}"}}"#, service.base_url),
    )
    .map_err(|error| error.to_string())?;
    keystore::set_base_dir_override(Some(temp.path().to_path_buf()));
    keystore::set_active_account_dir(None);

    let identity = keystore::generate_identity(READER_USER_ID.to_owned());
    let core = HubCoreState::default();
    *core
        .osl
        .identity
        .lock()
        .map_err(|error| error.to_string())? = Some(identity);
    let state = OslMailState::default();
    provision(&core, &state, "task4330reader".to_owned())?;

    for word in WORDS {
        send(
            &core,
            &state,
            READER_ADDRESS.to_owned(),
            SUBJECT.to_owned(),
            word.to_owned(),
        )?;
    }

    let before = service.mailbox_copy();
    let before_status = get_status(&core, &state)?;
    let opened = open_thread(&core, &state, THREAD_ID.to_owned())?;
    let after_status = get_status(&core, &state)?;
    let after = service.mailbox_copy();
    service.finish()?;
    keystore::set_base_dir_override(None);

    let returned_words = opened
        .messages
        .iter()
        .map(|message| message.body.clone())
        .collect::<Vec<_>>();
    let differences = mailbox_differences(&before, &after);
    if let Some(change) = differences.first() {
        return Err(format!("TASK4330 mailbox changed: {change}"));
    }

    let proof = ReadProof {
        before_messages: before.len(),
        before_unread: before.iter().filter(|message| message.unread).count(),
        returned_words,
        differences: differences.len(),
        after_messages: after.len(),
        after_unread: after.iter().filter(|message| message.unread).count(),
    };
    if before_status.unread_count != 4 || after_status.unread_count != 4 {
        return Err(format!(
            "TASK4330 unread count changed: before={} after={}",
            before_status.unread_count, after_status.unread_count
        ));
    }
    Ok(proof)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MailboxMessage {
    message_id: String,
    kind: String,
    sender_user_id: String,
    opaque_thread_token: String,
    ciphertext_b64: String,
    envelope: Value,
    recipient_key_fingerprint: String,
    received_at: i64,
    expires_at: i64,
    byte_length: usize,
    subject: String,
    unread: bool,
}

fn mailbox_differences(before: &[MailboxMessage], after: &[MailboxMessage]) -> Vec<String> {
    let mut differences = Vec::new();
    if before.len() != after.len() {
        differences.push(format!(
            "message_count before={} after={}",
            before.len(),
            after.len()
        ));
    }
    for (index, (before_message, after_message)) in before.iter().zip(after).enumerate() {
        macro_rules! changed {
            ($field:ident) => {
                if before_message.$field != after_message.$field {
                    differences.push(format!(
                        "messages[{index}] message_id={} field={} before={:?} after={:?}",
                        before_message.message_id,
                        stringify!($field),
                        before_message.$field,
                        after_message.$field
                    ));
                }
            };
        }
        changed!(message_id);
        changed!(kind);
        changed!(sender_user_id);
        changed!(opaque_thread_token);
        changed!(ciphertext_b64);
        changed!(envelope);
        changed!(recipient_key_fingerprint);
        changed!(received_at);
        changed!(expires_at);
        changed!(byte_length);
        changed!(subject);
        if before_message.unread != after_message.unread {
            differences.push(format!(
                "messages[{index}] message_id={} field=unread before={} after={}",
                before_message.message_id, before_message.unread, after_message.unread
            ));
        }
    }
    differences
}

struct MailService {
    base_url: String,
    state: Arc<Mutex<ServiceState>>,
    server: std::thread::JoinHandle<()>,
}

impl MailService {
    fn mailbox_copy(&self) -> Vec<MailboxMessage> {
        self.state.lock().unwrap().mailbox.clone()
    }

    fn finish(self) -> Result<(), String> {
        self.server
            .join()
            .map_err(|_| "TASK4330 mail service thread panicked".to_owned())
    }
}

#[derive(Default)]
struct ServiceState {
    mailbox: Vec<MailboxMessage>,
    inject_mark: bool,
    mark_injected: bool,
}

fn spawn_mail_service(inject_mark: bool) -> MailService {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let state = Arc::new(Mutex::new(ServiceState {
        mailbox: Vec::new(),
        inject_mark,
        mark_injected: false,
    }));
    let thread_state = Arc::clone(&state);
    let server = std::thread::spawn(move || {
        for _ in 0..20 {
            let mut stream = listener.accept().unwrap().0;
            let request = read_http_request(&mut stream);
            serve_request(&mut stream, &thread_state, &request);
        }
    });
    MailService {
        base_url: format!("http://{address}"),
        state,
        server,
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
                "username": "task4330reader",
                "user_id": READER_USER_ID,
                "state": "active"
            }),
        );
    } else if request.starts_with("POST /v1/mail/send/osl ") {
        let mut locked = state.lock().unwrap();
        let message_index = locked.mailbox.len() + 1;
        let ciphertext = request_body["ciphertext_b64"].as_str().unwrap().to_owned();
        locked.mailbox.push(MailboxMessage {
            message_id: format!("mail4330m{message_index}"),
            kind: "osl_message".to_owned(),
            sender_user_id: READER_USER_ID.to_owned(),
            opaque_thread_token: THREAD_ID.to_owned(),
            byte_length: ciphertext.len(),
            ciphertext_b64: ciphertext,
            envelope: request_body["envelope"].clone(),
            recipient_key_fingerprint: request_body["recipient_key_fingerprint"]
                .as_str()
                .unwrap()
                .to_owned(),
            received_at: 1_970_003_000_000_i64 + i64::try_from(message_index).unwrap(),
            expires_at: 1_970_003_100_000_i64,
            subject: SUBJECT.to_owned(),
            unread: true,
        });
        write_json(
            stream,
            json!({"message_id": format!("mail4330m{message_index}"), "accepted": true}),
        );
    } else if request.starts_with("POST /v1/mail/list ") {
        let locked = state.lock().unwrap();
        let messages = locked
            .mailbox
            .iter()
            .map(|message| {
                json!({
                    "message_id": message.message_id,
                    "kind": message.kind,
                    "sender_user_id": message.sender_user_id,
                    "opaque_thread_token": message.opaque_thread_token,
                    "received_at": message.received_at,
                    "expires_at": message.expires_at,
                    "byte_length": message.byte_length,
                    "subject": message.subject,
                    "unread": message.unread
                })
            })
            .collect::<Vec<_>>();
        let unread_count = locked
            .mailbox
            .iter()
            .filter(|message| message.unread)
            .count();
        write_json(
            stream,
            json!({ "messages": messages, "unread_count": unread_count }),
        );
    } else if request.starts_with("POST /v1/mail/fetch ") {
        let message_id = request_body["message_id"].as_str().unwrap();
        let mut locked = state.lock().unwrap();
        let index = locked
            .mailbox
            .iter()
            .position(|message| message.message_id == message_id)
            .unwrap();
        if locked.inject_mark && !locked.mark_injected {
            locked.mailbox[index].unread = false;
            locked.mark_injected = true;
        }
        let message = locked.mailbox[index].clone();
        write_json(
            stream,
            json!({
                "message_id": message.message_id,
                "kind": message.kind,
                "sender_user_id": message.sender_user_id,
                "subject": message.subject,
                "ciphertext_b64": message.ciphertext_b64,
                "envelope": message.envelope,
                "received_at": message.received_at
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
        body.len(),
        body
    )
    .unwrap();
}
