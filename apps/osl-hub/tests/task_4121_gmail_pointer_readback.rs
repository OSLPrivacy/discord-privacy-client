#![cfg(feature = "core")]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use ipc::cipher_store_client::{CipherStoreClient, TTL_1H};
use ipc::prose_token::{derive_detection_key, prose_token_send_with_client, ProseTokenSendKeys};
use ipc::scope::{ScopeInput, ScopeKind};
use osl_privacy_hub::shipping_mailbox_pointer_reader::{
    read_shipping_mailbox_pointers, ShippingMailboxText,
};

const COVER_COUNT: usize = 20;
const ORDINARY_COUNT: usize = 20;
const SENDER_GMAIL_ACCOUNT: &str = "task-4121-sender@gmail.invalid";
const READER_GMAIL_ACCOUNT: &str = "task-4121-reader@gmail.invalid";
const MESSAGE_KEY: [u8; 32] = [0x41; 32];
const SEND_KEY: [u8; 32] = [0x42; 32];
const CONVERSATION_KEY: [u8; 32] = [0x43; 32];

fn scope() -> ScopeInput {
    ScopeInput {
        kind: ScopeKind::Dm,
        id: "task-4121-gmail-shipping-thread".to_owned(),
        server_id: None,
        channel_id: None,
    }
}

fn keys() -> ProseTokenSendKeys<'static> {
    ProseTokenSendKeys {
        message_key: &MESSAGE_KEY,
        send_key: &SEND_KEY,
        conversation_key: &CONVERSATION_KEY,
    }
}

fn read_request(stream: &mut TcpStream) {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let count = stream.read(&mut chunk).expect("read cipher-store upload");
        assert_ne!(count, 0, "upload ended before HTTP headers");
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            return;
        }
    }
}

fn upload_server() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local cipher-store");
    let address = listener.local_addr().expect("local cipher-store address");
    let server = thread::spawn(move || {
        for ordinal in 0..COVER_COUNT {
            let (mut stream, _) = listener.accept().expect("accept cipher-store upload");
            read_request(&mut stream);
            let body = format!(r#"{{"id":"{ordinal:016x}","expires_at":1}}"#);
            write!(
                stream,
                "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .expect("write cipher-store response");
        }
    });
    (format!("http://{address}"), server)
}

fn gmail_reply_rendering(cover: &str) -> String {
    let midpoint = cover.len() / 2;
    let split = cover[..midpoint]
        .rfind(char::is_whitespace)
        .unwrap_or(midpoint);
    format!(
        "Thanks for the note.\r\n\r\nOn Tue, Aug 11, 2026 at 10:00 AM {SENDER_GMAIL_ACCOUNT} wrote:\r\n> {}\r\n> {}\r\n> --\r\n> {SENDER_GMAIL_ACCOUNT}\r\n",
        &cover[..split],
        cover[split..].trim_start(),
    )
}

fn mailbox_seen_by_reader() -> (Vec<ShippingMailboxText>, Vec<String>) {
    let (url, server) = upload_server();
    let client = CipherStoreClient::new(url).expect("cipher-store client");
    let scope = scope();
    let detector = derive_detection_key(&CONVERSATION_KEY).expect("detector derives");
    let mut expected_ids = Vec::with_capacity(COVER_COUNT);
    let mut rows = Vec::with_capacity(COVER_COUNT + ORDINARY_COUNT);
    for ordinal in 0..COVER_COUNT {
        let wire = format!("DPC0::{}", B64.encode(format!("ciphertext-{ordinal}")));
        let sent = prose_token_send_with_client(&client, &scope, &detector, keys(), &wire, TTL_1H)
            .expect("shipping cover is minted");
        expected_ids.push(sent.blob_id);
        rows.push(ShippingMailboxText {
            server_id: format!("gmail-server-cover-{ordinal:02}"),
            text: gmail_reply_rendering(&sent.cover_text),
        });
    }
    server.join().expect("all twenty shipping uploads complete");
    for ordinal in 0..ORDINARY_COUNT {
        rows.push(ShippingMailboxText {
            server_id: format!("gmail-server-ordinary-{ordinal:02}"),
            text: format!(
                "Ordinary Gmail mail {ordinal}: agenda, lunch, and a signature.\n-- \n{SENDER_GMAIL_ACCOUNT}"
            ),
        });
    }
    (rows, expected_ids)
}

fn coverage_error(actual: usize) -> Result<(), String> {
    if actual == COVER_COUNT {
        Ok(())
    } else {
        Err(format!(
            "TASK4121_COVER_POINTER_COUNT expected={COVER_COUNT} actual={actual}"
        ))
    }
}

#[test]
fn task_4121_gmail_rendered_text_pointer_readback() {
    let (mut rows, expected_ids) = mailbox_seen_by_reader();
    let fault = std::env::var("TASK4121_FAULT").ok();
    let actual = match fault.as_deref() {
        Some("starve-email-20") => {
            rows.remove(COVER_COUNT - 1);
            read_shipping_mailbox_pointers(
                &rows,
                &scope(),
                &derive_detection_key(&CONVERSATION_KEY).expect("detector derives"),
            )
            .expect("shipping pointer reader runs")
        }
        Some("omit-shipping-pointer-reader") => Vec::new(),
        None => read_shipping_mailbox_pointers(
            &rows,
            &scope(),
            &derive_detection_key(&CONVERSATION_KEY).expect("detector derives"),
        )
        .expect("shipping pointer reader runs"),
        Some(other) => panic!("unknown TASK4121_FAULT={other}"),
    };

    coverage_error(actual.len()).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        actual
            .iter()
            .map(|pointer| &pointer.blob_id)
            .collect::<Vec<_>>(),
        expected_ids.iter().collect::<Vec<_>>(),
        "every Gmail server row must preserve its exact shipping pointer"
    );
    assert_eq!(actual.len(), COVER_COUNT);
    assert_eq!(rows.len() - COVER_COUNT, ORDINARY_COUNT);
    assert!(actual
        .iter()
        .all(|pointer| pointer.server_id.starts_with("gmail-server-cover-")));

    println!("TASK4121_SENDER_ACCOUNT={SENDER_GMAIL_ACCOUNT}");
    println!("TASK4121_READER_ACCOUNT={READER_GMAIL_ACCOUNT}");
    println!("TASK4121_COVER_MESSAGES_SENT={COVER_COUNT}");
    println!("TASK4121_GMAIL_ROWS_SEEN={}", rows.len());
    println!(
        "TASK4121_COVER_POINTER_COUNT expected={COVER_COUNT} actual={}",
        actual.len()
    );
    println!("TASK4121_ORDINARY_MESSAGES={ORDINARY_COUNT} pointers=0");
    println!("TASK4121_FIXTURE_MAILBOXES=0");
    for pointer in actual {
        println!(
            "TASK4121_POINTER server_id={} blob_id={}",
            pointer.server_id, pointer.blob_id
        );
    }
}
