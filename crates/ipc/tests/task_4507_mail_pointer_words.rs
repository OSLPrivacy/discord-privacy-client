use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use ipc::cipher_store_client::{CipherStoreClient, TTL_1H};
use ipc::prose_token::{
    derive_detection_key, prose_token_mail_body_from_cover, prose_token_recover_pointer,
    prose_token_send_with_client, ProseTokenSendKeys,
};
use ipc::scope::{ScopeInput, ScopeKind};

const MESSAGE_KEY: [u8; 32] = [0x11; 32];
const SEND_KEY: [u8; 32] = [0x22; 32];
const CONVERSATION_KEY: [u8; 32] = [0x33; 32];

fn scope() -> ScopeInput {
    ScopeInput {
        kind: ScopeKind::Dm,
        id: "task-4507-osl-mail-thread".to_owned(),
        server_id: None,
        channel_id: Some("task-4507-osl-mail-thread".to_owned()),
    }
}

fn send_keys() -> ProseTokenSendKeys<'static> {
    ProseTokenSendKeys {
        message_key: &MESSAGE_KEY,
        send_key: &SEND_KEY,
        conversation_key: &CONVERSATION_KEY,
    }
}

fn ordinary_emails() -> [&'static str; 20] {
    [
        "I moved the planning notes into the folder for tomorrow.",
        "Thanks for the quick review. The invoice number is correct.",
        "The room is available after the morning team meeting.",
        "Please send the agenda when the draft is ready.",
        "I will be ten minutes late but can still join the call.",
        "The printer is working again near the east stairs.",
        "Can you confirm whether the package arrived today?",
        "The spreadsheet totals match the signed receipt.",
        "Lunch moved to the smaller table by the window.",
        "The deployment note should mention the maintenance window.",
        "I found the badge form and left it on your desk.",
        "The customer asked for a copy of the original estimate.",
        "We can remove the old calendar invite after Friday.",
        "The screenshots are in the folder named final review.",
        "Please check the address before the courier leaves.",
        "The short summary is enough for the status email.",
        "I updated the ticket with the latest reproduction steps.",
        "The conference link works from my laptop now.",
        "Please keep the first paragraph and trim the rest.",
        "The receipt is attached for accounting records.",
    ]
}

fn body_reads_naturally(body: &str) -> bool {
    let words = body.split_whitespace().count();
    let lowercase = body.chars().filter(|c| c.is_ascii_lowercase()).count();
    let letters = body.chars().filter(|c| c.is_ascii_alphabetic()).count();
    words >= 12
        && !body.contains("DPC0::")
        && !body.contains("DPC1::")
        && !body.contains("http://")
        && !body.contains("https://")
        && letters > 0
        && lowercase * 100 / letters >= 80
}

#[test]
fn task_4507_osl_mail_body_uses_the_chat_pointer_reader() {
    let scope = scope();
    let detection_key = derive_detection_key(&CONVERSATION_KEY).expect("detector derives");
    let (base_url, server) = upload_server(20);
    let client = CipherStoreClient::new(base_url).expect("local cipher-store client builds");

    let mut found = 0usize;
    let mut natural = 0usize;
    for index in 0..20 {
        let wire = format!(
            "DPC0::{}",
            B64.encode(format!("task-4507 encrypted email payload {index}"))
        );
        let sent = prose_token_send_with_client(
            &client,
            &scope,
            &detection_key,
            send_keys(),
            &wire,
            TTL_1H,
        )
        .expect("OSL send helper prepares a prose pointer");
        let email_body = prose_token_mail_body_from_cover(&sent.cover_text);
        let recovered = prose_token_recover_pointer(&scope, &detection_key, &email_body)
            .expect("mail body uses the shared text pointer reader");
        if recovered.as_ref().map(|pointer| pointer.blob_id.as_str()) == Some(sent.blob_id.as_str())
        {
            found += 1;
        }
        if body_reads_naturally(&email_body) {
            natural += 1;
        }
    }

    let ordinary_none = ordinary_emails()
        .iter()
        .filter(|body| {
            prose_token_recover_pointer(&scope, &detection_key, body)
                .expect("ordinary mail read is local")
                .is_none()
        })
        .count();

    let accepted_uploads = server.join().expect("upload server exits");
    println!("TASK4507_OSL_EMAIL_POINTERS_FOUND={found}");
    println!("TASK4507_ORDINARY_EMAILS_WITH_NO_POINTER={ordinary_none}");
    println!("TASK4507_NATURAL_EMAIL_BODIES={natural}");
    println!("TASK4507_LOCAL_UPLOADS_ACCEPTED={accepted_uploads}");

    assert_eq!(found, 20);
    assert_eq!(ordinary_none, 20);
    assert_eq!(natural, 20);
    assert_eq!(accepted_uploads, 20);
}

fn upload_server(expected_uploads: usize) -> (String, thread::JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback cipher store");
    let address = listener.local_addr().expect("loopback address");
    let server = thread::spawn(move || {
        let mut accepted = 0usize;
        for index in 0..expected_uploads {
            let (mut stream, _) = listener.accept().expect("accept upload");
            let request = read_request(&mut stream);
            assert!(request.starts_with("POST /v1/blob "));
            assert!(request.contains("\r\nx-osl-fetch-token: "));
            let id_hex = format!("{:016x}", index + 1);
            let body = format!(r#"{{"id":"{id_hex}","expires_at":1}}"#);
            let response = format!(
                "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write upload response");
            accepted += 1;
        }
        accepted
    });
    (format!("http://{address}"), server)
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut raw = Vec::new();
    loop {
        let mut chunk = [0u8; 1024];
        let count = stream.read(&mut chunk).expect("read request");
        assert_ne!(count, 0, "connection closed before request completed");
        raw.extend_from_slice(&chunk[..count]);
        let Some(header_end) = raw.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&raw[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().expect("content length"))
            })
            .unwrap_or(0);
        let request_end = header_end + 4 + content_length;
        if raw.len() >= request_end {
            raw.truncate(request_end);
            return String::from_utf8(raw).expect("request is utf8");
        }
    }
}
