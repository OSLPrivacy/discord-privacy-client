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

const SENDER_GMAIL_ACCOUNT: &str = "task-4122-sender@gmail.invalid";
const READER_GMAIL_ACCOUNT: &str = "task-4122-reader@gmail.invalid";
const ORIGINAL_SERVER_ID: &str = "gmail-server-original-4122";
const REPLY_SERVER_ID: &str = "gmail-server-reply-quoted-4122";
const MESSAGE_KEY: [u8; 32] = [0x51; 32];
const SEND_KEY: [u8; 32] = [0x52; 32];
const CONVERSATION_KEY: [u8; 32] = [0x53; 32];

fn scope() -> ScopeInput {
    ScopeInput {
        kind: ScopeKind::Dm,
        id: "task-4122-gmail-quoted-reply-thread".to_owned(),
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
        let (mut stream, _) = listener.accept().expect("accept cipher-store upload");
        read_request(&mut stream);
        let body = r#"{"id":"0000000000000000","expires_at":1}"#.to_owned();
        write!(
            stream,
            "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("write cipher-store response");
    });
    (format!("http://{address}"), server)
}

/// The exact Gmail rendering of `--- Original Message ---`-style quoting
/// that a genuine reply produces when the recipient replies to the cover
/// email and their client quotes the whole cover body back.
fn gmail_reply_quoting_cover(cover: &str) -> String {
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

/// Build the two Gmail rows the shipping reader would see after: (1) the
/// sender's cover email lands in the inbox, then (2) the reader replies and
/// Gmail delivers that reply back with the cover text quoted underneath.
fn mailbox_rows_after_quoted_reply() -> (Vec<ShippingMailboxText>, String) {
    let (url, server) = upload_server();
    let client = CipherStoreClient::new(url).expect("cipher-store client");
    let scope = scope();
    let detector = derive_detection_key(&CONVERSATION_KEY).expect("detector derives");
    let wire = format!("DPC0::{}", B64.encode("ciphertext-task-4122"));
    let sent = prose_token_send_with_client(&client, &scope, &detector, keys(), &wire, TTL_1H)
        .expect("shipping cover is minted");
    server.join().expect("the one shipping upload completes");

    let rows = vec![
        ShippingMailboxText {
            server_id: ORIGINAL_SERVER_ID.to_owned(),
            text: sent.cover_text.clone(),
        },
        ShippingMailboxText {
            server_id: REPLY_SERVER_ID.to_owned(),
            text: gmail_reply_quoting_cover(&sent.cover_text),
        },
    ];
    (rows, sent.blob_id)
}

#[test]
fn task_4122_quoted_reply_does_not_reopen_the_cover_pointer() {
    let (rows, expected_blob_id) = mailbox_rows_after_quoted_reply();
    let detector = derive_detection_key(&CONVERSATION_KEY).expect("detector derives");

    let opened = read_shipping_mailbox_pointers(&rows, &scope(), &detector)
        .expect("shipping pointer reader runs");

    let mut blob_id_counts = std::collections::BTreeMap::new();
    for pointer in &opened {
        *blob_id_counts.entry(pointer.blob_id.clone()).or_insert(0_usize) += 1;
    }
    if let Some((duplicated_blob_id, count)) = blob_id_counts
        .into_iter()
        .find(|(_, count)| *count > 1)
    {
        panic!(
            "TASK4122_DUPLICATE_OPEN blob_id={duplicated_blob_id} count={count}: the shipping \
             reader opened the same private message more than once from a quoted reply echo"
        );
    }

    let original_open_count = opened
        .iter()
        .filter(|pointer| pointer.server_id == ORIGINAL_SERVER_ID)
        .count();
    let reply_open_count = opened
        .iter()
        .filter(|pointer| pointer.server_id == REPLY_SERVER_ID)
        .count();

    assert_eq!(
        original_open_count, 1,
        "the original cover email must open its private message exactly once"
    );
    assert_eq!(
        reply_open_count, 0,
        "the reply's quoted copy of the cover text must never open the private message"
    );
    assert_eq!(opened.len(), 1, "exactly one pointer must open in total");
    assert_eq!(opened[0].blob_id, expected_blob_id);
    assert_eq!(opened[0].server_id, ORIGINAL_SERVER_ID);

    println!("TASK4122_SENDER_ACCOUNT={SENDER_GMAIL_ACCOUNT}");
    println!("TASK4122_READER_ACCOUNT={READER_GMAIL_ACCOUNT}");
    println!("TASK4122_ORIGINAL_SERVER_ID={ORIGINAL_SERVER_ID}");
    println!("TASK4122_REPLY_SERVER_ID={REPLY_SERVER_ID}");
    println!("TASK4122_ORIGINAL_OPEN_COUNT={original_open_count}");
    println!("TASK4122_REPLY_OPEN_COUNT={reply_open_count}");
    println!("TASK4122_BLOB_ID={expected_blob_id}");
    println!("TASK4122_FIXTURE_THREADS=0");
}
