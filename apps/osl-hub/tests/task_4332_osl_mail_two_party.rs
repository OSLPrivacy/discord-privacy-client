use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::osl_mail::{
    osl_mail_acknowledge_retrieval, osl_mail_agree_to_sender, osl_mail_list_threads,
    osl_mail_provision, osl_mail_retrieve_thread, osl_mail_send, OslMailState,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

const ALICE_USER: &str = "alice4332";
const ALICE_NAME: &str = "alice4332";
const ALICE_ADDRESS: &str = "alice4332@oslprivacy.com";
const BOB_USER: &str = "bob4332";
const BOB_NAME: &str = "bob4332";
const BOB_ADDRESS: &str = "bob4332@oslprivacy.com";
const SUBJECT: &str = "OSL protected message";
const EXACT_WORDS: &str = "TASK4332 orange lantern meets quiet river at seven.";

#[test]
fn task4332_one_protected_message_goes_through_app_commands_and_is_acknowledged() {
    assert_shipping_command_surface();
    let service = MailService::spawn();
    let temp = tempfile::tempdir().expect("temporary OSL account directory");
    std::fs::write(
        temp.path().join("keyserver.json"),
        format!(r#"{{"base_url":"{}"}}"#, service.base_url),
    )
    .expect("write loopback keyserver configuration");
    keystore::set_base_dir_override(Some(temp.path().to_path_buf()));
    keystore::set_active_account_dir(None);

    let alice_identity = keystore::generate_identity(ALICE_USER.to_owned());
    let bob_identity = keystore::generate_identity(BOB_USER.to_owned());
    let alice = core_with_identity(alice_identity);
    let bob = core_with_identity(bob_identity);
    let app_mail_state = OslMailState::default();
    let mut commands = Vec::new();

    commands.push("osl_mail_provision");
    let alice_status = osl_mail_provision(&alice, &app_mail_state, ALICE_NAME.to_owned())
        .expect("first OSL Mail address provisions through the app command");
    commands.push("osl_mail_provision");
    let bob_status = osl_mail_provision(&bob, &app_mail_state, BOB_NAME.to_owned())
        .expect("second OSL Mail address provisions through the app command");
    assert_eq!(alice_status.address.as_deref(), Some(ALICE_ADDRESS));
    assert_eq!(bob_status.address.as_deref(), Some(BOB_ADDRESS));

    commands.push("osl_mail_agree_to_sender");
    let agreement = osl_mail_agree_to_sender(&bob, &app_mail_state, ALICE_NAME.to_owned(), true)
        .expect("second address agrees to hear from the first through the app command");
    assert!(agreement.allowed);
    assert_eq!(agreement.sender_address, ALICE_ADDRESS);

    commands.push("osl_mail_list_threads");
    let before = osl_mail_list_threads(&bob, &app_mail_state)
        .expect("second address lists its mailbox through the app command");
    let opened_protected_before = 0usize;
    assert!(before.is_empty());

    commands.push("osl_mail_send");
    let sent = osl_mail_send(
        &alice,
        &app_mail_state,
        BOB_ADDRESS.to_owned(),
        SUBJECT.to_owned(),
        EXACT_WORDS.to_owned(),
    )
    .expect("first address sends the marked protected message through the app command");
    assert_eq!(sent.transit, "oslE2ee");

    commands.push("osl_mail_list_threads");
    let after_send = osl_mail_list_threads(&bob, &app_mail_state)
        .expect("second address receives one protected thread through the app command");
    assert_eq!(after_send.len(), 1);

    commands.push("osl_mail_retrieve_thread");
    let opened = osl_mail_retrieve_thread(&bob, &app_mail_state, after_send[0].thread_id.clone())
        .expect("second address opens the protected thread through the app command");
    let opened_protected_after = opened
        .messages
        .iter()
        .filter(|message| message.transit == "oslE2ee")
        .count();
    let returned_words = opened.messages[0].body.clone();
    assert_eq!(opened_protected_before, 0);
    assert_eq!(opened_protected_after, 1);
    assert_eq!(returned_words, EXACT_WORDS);

    commands.push("osl_mail_acknowledge_retrieval");
    let message_ids = opened
        .messages
        .iter()
        .map(|message| message.message_id.clone())
        .collect::<Vec<_>>();
    let acknowledgement =
        osl_mail_acknowledge_retrieval(&bob, &app_mail_state, opened.retrieval_id, message_ids)
            .expect(
                "second address acknowledges the opened protected message through the app command",
            );
    assert!(acknowledgement.server_delete_confirmed);

    let service_held_after_ack = service.message_count();
    assert_eq!(service_held_after_ack, 0);
    service.finish();
    keystore::set_base_dir_override(None);

    println!("TASK4332_addresses={ALICE_ADDRESS}|{BOB_ADDRESS}");
    println!("TASK4332_opened_protected_before={opened_protected_before}");
    println!("TASK4332_opened_protected_after={opened_protected_after}");
    println!("TASK4332_marked_protected_transit={}", sent.transit);
    println!("TASK4332_exact_words={returned_words}");
    println!("TASK4332_service_copies_after_ack={service_held_after_ack}");
    println!("TASK4332_commands={}", commands.join(" -> "));
}

fn assert_shipping_command_surface() {
    let main = include_str!("../src/main.rs");
    let registry = include_str!("../src/hub_command_surface.rs");
    let permissions = include_str!("../permissions/hub.toml");
    let capability = include_str!("../capabilities/hub.json");
    for command in [
        "osl_mail_provision",
        "osl_mail_agree_to_sender",
        "osl_mail_list_threads",
        "osl_mail_retrieve_thread",
        "osl_mail_acknowledge_retrieval",
        "osl_mail_send",
    ] {
        assert!(
            main.contains(&format!("async fn {command}")),
            "{command} must be a shipping Tauri command wrapper"
        );
        assert!(
            registry.contains(command),
            "{command} must be in the shipping invoke handler"
        );
        let permission = format!("allow-{}", command.replace('_', "-"));
        assert!(permissions.contains(command));
        assert!(capability.contains(&permission));
    }
}

fn core_with_identity(identity: keystore::Identity) -> HubCoreState {
    let core = HubCoreState::default();
    *core.osl.identity.lock().expect("identity state") = Some(identity);
    core
}

#[derive(Clone)]
struct HeldMessage {
    message_id: String,
    sender_user_id: String,
    sender_address: String,
    recipient_user_id: String,
    thread_id: String,
    ciphertext_b64: String,
    envelope: Value,
    received_at: i64,
}

#[derive(Default)]
struct ServiceState {
    addresses: BTreeMap<String, (String, String)>,
    agreements: BTreeSet<(String, String)>,
    fetched: BTreeSet<String>,
    messages: Vec<HeldMessage>,
}

struct MailService {
    base_url: String,
    state: Arc<Mutex<ServiceState>>,
    server: std::thread::JoinHandle<()>,
}

impl MailService {
    fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback mail service");
        let address = listener.local_addr().expect("loopback mail address");
        let state = Arc::new(Mutex::new(ServiceState::default()));
        let server_state = Arc::clone(&state);
        let server = std::thread::spawn(move || {
            for _ in 0..17 {
                let (mut stream, _) = listener.accept().expect("accept app command request");
                let request = read_request(&mut stream);
                serve_request(&mut stream, &server_state, &request);
            }
        });
        Self {
            base_url: format!("http://{address}"),
            state,
            server,
        }
    }

    fn message_count(&self) -> usize {
        self.state
            .lock()
            .expect("mail service state")
            .messages
            .len()
    }

    fn finish(self) {
        self.server.join().expect("loopback mail service completes");
    }
}

fn serve_request(stream: &mut TcpStream, state: &Arc<Mutex<ServiceState>>, request: &str) {
    if request.starts_with("GET /v1/mail/capabilities ") {
        return write_json(
            stream,
            200,
            json!({"version":1,"addressDomain":"oslprivacy.com","oslToOslE2ee":true}),
        );
    }
    let body: Value = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap_or("{}"))
        .expect("app command JSON");
    let user_id = body["user_id"].as_str().unwrap_or_default();
    if request.starts_with("POST /v1/mail/address ") {
        let username = body["username"].as_str().expect("provision username");
        let address = format!("{username}@oslprivacy.com");
        state
            .lock()
            .expect("mail state")
            .addresses
            .insert(address.clone(), (user_id.to_owned(), username.to_owned()));
        return write_json(
            stream,
            200,
            json!({"address":address,"username":username,"user_id":user_id,"state":"active"}),
        );
    }
    if request.starts_with("POST /v1/mail/consent ") {
        let sender_username = body["sender_username"].as_str().expect("sender username");
        let allowed = body["allowed"].as_bool().expect("allowed flag");
        let mut locked = state.lock().expect("mail state");
        let (sender_address, sender_user_id) = locked
            .addresses
            .iter()
            .find_map(|(address, (id, username))| {
                (username == sender_username).then(|| (address.clone(), id.clone()))
            })
            .expect("named sender address exists");
        if allowed {
            locked
                .agreements
                .insert((user_id.to_owned(), sender_user_id.clone()));
        } else {
            locked
                .agreements
                .remove(&(user_id.to_owned(), sender_user_id.clone()));
        }
        return write_json(
            stream,
            200,
            json!({"sender_user_id":sender_user_id,"sender_username":sender_username,"sender_address":sender_address,"allowed":allowed}),
        );
    }
    if request.starts_with("POST /v1/mail/send/osl ") {
        let recipient = body["recipient_address"]
            .as_str()
            .expect("recipient address");
        let mut locked = state.lock().expect("mail state");
        let recipient_user = locked
            .addresses
            .get(recipient)
            .expect("recipient provisioned")
            .0
            .clone();
        if !locked
            .agreements
            .contains(&(recipient_user.clone(), user_id.to_owned()))
        {
            return write_json(
                stream,
                403,
                json!({"error":"recipient has not allowed this sender"}),
            );
        }
        let sender_address = locked
            .addresses
            .iter()
            .find_map(|(address, (id, _))| (id == user_id).then(|| address.clone()))
            .expect("sender provisioned");
        locked.messages.push(HeldMessage {
            message_id: "mail4332protected".to_owned(),
            sender_user_id: user_id.to_owned(),
            sender_address,
            recipient_user_id: recipient_user,
            thread_id: body["opaque_thread_token"]
                .as_str()
                .expect("thread token")
                .to_owned(),
            ciphertext_b64: body["ciphertext_b64"]
                .as_str()
                .expect("ciphertext")
                .to_owned(),
            envelope: body["envelope"].clone(),
            received_at: 1_970_043_320_000,
        });
        return write_json(
            stream,
            200,
            json!({"message_id":"mail4332protected","accepted":true}),
        );
    }
    if request.starts_with("POST /v1/mail/list ") {
        let locked = state.lock().expect("mail state");
        let messages = locked
            .messages
            .iter()
            .filter(|message| message.recipient_user_id == user_id)
            .map(|message| {
                json!({"message_id":message.message_id,"kind":"osl_e2ee","sender_user_id":message.sender_user_id,"sender_address":message.sender_address,"subject":SUBJECT,"opaque_thread_token":message.thread_id,"received_at":message.received_at})
            })
            .collect::<Vec<_>>();
        return write_json(stream, 200, json!({"messages":messages}));
    }
    if request.starts_with("POST /v1/mail/fetch ") {
        let message_id = body["message_id"].as_str().expect("fetch message id");
        let mut locked = state.lock().expect("mail state");
        let message = locked
            .messages
            .iter()
            .find(|message| {
                message.message_id == message_id && message.recipient_user_id == user_id
            })
            .cloned()
            .expect("held recipient message");
        locked.fetched.insert(message_id.to_owned());
        return write_json(
            stream,
            200,
            json!({"message_id":message.message_id,"kind":"osl_e2ee","sender_user_id":message.sender_user_id,"sender_address":message.sender_address,"subject":SUBJECT,"ciphertext_b64":message.ciphertext_b64,"envelope":message.envelope,"received_at":message.received_at}),
        );
    }
    if request.starts_with("POST /v1/mail/ack ") {
        let message_id = body["message_id"].as_str().expect("ack message id");
        let mut locked = state.lock().expect("mail state");
        if !locked.fetched.contains(message_id) {
            return write_json(stream, 409, json!({"error":"message was never opened"}));
        }
        let before = locked.messages.len();
        locked.messages.retain(|message| {
            message.message_id != message_id || message.recipient_user_id != user_id
        });
        return write_json(
            stream,
            200,
            json!({"deleted":locked.messages.len() < before}),
        );
    }
    write_json(stream, 404, json!({"error":"not found"}));
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = stream.read(&mut buffer).expect("read app command request");
        assert!(read > 0, "request ended before its declared body");
        bytes.extend_from_slice(&buffer[..read]);
        if let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(str::to_owned)
                })
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if bytes.len() >= header_end + 4 + content_length {
                return String::from_utf8(bytes).expect("UTF-8 HTTP request");
            }
        }
    }
}

fn write_json(stream: &mut TcpStream, status: u16, body: Value) {
    let body = body.to_string();
    let reason = if status == 200 { "OK" } else { "Error" };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("write mail response");
}
