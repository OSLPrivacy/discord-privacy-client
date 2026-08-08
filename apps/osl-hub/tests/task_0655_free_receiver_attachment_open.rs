//! TASK 0655: received attachment opening is authorized by the authenticated
//! recipient share, while the same Free account remains unable to upload a
//! file one byte beyond its 25 MiB sender ceiling.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::Arc;
use std::thread;

use ipc::cipher_store_client::{
    CipherStoreClient, ProChunkedUploadFile, ProChunkedUploadPiece, ProChunkedUploadReport,
    ATTACHMENT_MULTIPART_PART_BYTES,
};
use osl_privacy_hub::attachment_limits::{
    check_attachment_size, AttachmentAccountTier, FREE_MAX_ATTACHMENT_BYTES,
};
use osl_privacy_hub::osl_chat_attachment_download_permission::{
    download_pro_attachment_for_recipient, grant_recipient_download_from_authenticated_notice,
    grant_recipient_download_from_pro_send, RecipientAttachmentDownloadRequest,
};
use sha2::{Digest, Sha256};

const FILE_ID: &str = "06550655065506550655065506550655";
const RECIPIENT_ID: &str = "free-recipient-0655";
const FETCH_TOKEN: [u8; 16] = [0x65; 16];
const UPLOAD_CHILD_ENV: &str = "OSL_TASK_0655_FREE_UPLOAD_CHILD";

fn function_body<'a>(source: &'a str, start: &str, next: &str) -> &'a str {
    let start = source
        .find(start)
        .expect("production function remains present");
    let tail = &source[start..];
    let end = tail
        .find(next)
        .expect("next production function remains present");
    &tail[..end]
}

fn completed_pro_send(byte_length: u64) -> ProChunkedUploadReport {
    let mut remaining = byte_length;
    let mut finished_pieces = Vec::new();
    let mut piece_number = 1_u32;
    while remaining != 0 {
        let size_bytes = remaining.min(ATTACHMENT_MULTIPART_PART_BYTES);
        finished_pieces.push(ProChunkedUploadPiece {
            upload_id: FILE_ID.to_owned(),
            piece_number,
            size_bytes,
        });
        remaining -= size_bytes;
        piece_number += 1;
    }
    ProChunkedUploadReport {
        completed_file: ProChunkedUploadFile {
            file_id: FILE_ID.to_owned(),
            total_size_bytes: byte_length,
            piece_count: finished_pieces.len() as u32,
            expires_at: 1_900_000_655,
        },
        finished_pieces,
    }
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 2048];
    loop {
        let read = stream.read(&mut buffer).expect("read receiver request");
        assert_ne!(read, 0, "receiver request ended before headers");
        request.extend_from_slice(&buffer[..read]);
        if request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            return String::from_utf8(request).expect("receiver request headers are UTF-8");
        }
    }
}

#[test]
fn task_0655_free_upload_over_25_mib_child() {
    if std::env::var_os(UPLOAD_CHILD_ENV).is_none() {
        return;
    }
    let upload_bytes = FREE_MAX_ATTACHMENT_BYTES + 1;
    let result = check_attachment_size(upload_bytes, AttachmentAccountTier::Free);
    println!(
        "TASK0655_FREE_UPLOAD_BYTES={upload_bytes} TASK0655_FREE_UPLOAD_ALLOWED={}",
        result.is_ok()
    );
    if result.is_err() {
        println!("TASK0655_FREE_UPLOAD_EXIT_CODE=1");
        std::process::exit(1);
    }
    std::process::exit(0);
}

#[test]
fn task_0655_free_recipient_opens_pro_file_and_free_upload_still_exits_1() {
    let file_length = FREE_MAX_ATTACHMENT_BYTES + 1;
    let file_bytes = Arc::new(
        (0..file_length)
            .map(|index| ((index.wrapping_mul(131) + 65) & 0xff) as u8)
            .collect::<Vec<_>>(),
    );
    let expected_fingerprint = Sha256::digest(file_bytes.as_slice());

    let pro_report = completed_pro_send(file_length);
    let report_permission =
        grant_recipient_download_from_pro_send(&pro_report, RECIPIENT_ID, FETCH_TOKEN)
            .expect("completed Pro upload grants its named recipient");
    assert_eq!(report_permission.expected_byte_length(), file_length);
    assert_eq!(report_permission.file_id(), FILE_ID);

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind receiver fixture");
    let address = listener.local_addr().expect("receiver fixture address");
    let served_bytes = Arc::clone(&file_bytes);
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept receiver download");
        let request = read_request(&mut stream);
        assert!(request.starts_with(&format!("GET /v1/attachment/{FILE_ID} HTTP/1.1\r\n")));
        assert!(request.lines().any(|line| {
            line.eq_ignore_ascii_case("x-osl-fetch-token: 65656565656565656565656565656565")
        }));
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            served_bytes.len()
        )
        .expect("write receiver response headers");
        stream
            .write_all(served_bytes.as_slice())
            .expect("write exact Pro attachment bytes");
    });

    // This is the constructor used by the native receive path after the broker
    // authenticates the sender, conversation, recipient, object, and token.
    let permission = grant_recipient_download_from_authenticated_notice(
        RECIPIENT_ID,
        FILE_ID,
        file_length,
        FETCH_TOKEN,
    )
    .expect("authenticated notice restores recipient permission");
    let request = RecipientAttachmentDownloadRequest {
        recipient_osl_user_id: RECIPIENT_ID.to_owned(),
        account_tier: AttachmentAccountTier::Free,
    };
    let client =
        CipherStoreClient::new(format!("http://{address}")).expect("receiver fixture client");
    let mut opened = Vec::new();
    let receipt =
        download_pro_attachment_for_recipient(&permission, &request, &client, &mut opened)
            .expect("Free recipient opens the Pro attachment through recipient permission");
    server.join().expect("receiver fixture completes");

    let opened_fingerprint = Sha256::digest(&opened);
    assert_eq!(receipt.recipient_account_tier, AttachmentAccountTier::Free);
    assert_eq!(receipt.byte_length, file_length);
    assert_eq!(opened_fingerprint, expected_fingerprint);
    println!(
        "TASK0655_FREE_RECIPIENT_OPENED=true TASK0655_RECIPIENT_TIER={} TASK0655_PRO_FILE_BYTES={} TASK0655_FINGERPRINT_MATCH={}",
        receipt.recipient_account_tier.label(),
        receipt.byte_length,
        opened_fingerprint == expected_fingerprint
    );

    let child = Command::new(std::env::current_exe().expect("test executable path"))
        .env(UPLOAD_CHILD_ENV, "1")
        .arg("--exact")
        .arg("task_0655_free_upload_over_25_mib_child")
        .arg("--nocapture")
        .output()
        .expect("Free upload command child runs");
    let stdout = String::from_utf8_lossy(&child.stdout);
    let stderr = String::from_utf8_lossy(&child.stderr);
    print!("{stdout}");
    eprint!("{stderr}");
    assert_eq!(child.status.code(), Some(1), "Free oversize upload exits 1");
    assert!(stdout.contains("TASK0655_FREE_UPLOAD_ALLOWED=false"));
    assert!(stdout.contains("TASK0655_FREE_UPLOAD_EXIT_CODE=1"));
    println!(
        "TASK0655_OBSERVED_FREE_UPLOAD_EXIT_CODE={}",
        child.status.code().unwrap_or(-1)
    );
}

#[test]
fn task_0655_production_receive_commands_are_wired_to_recipient_permission() {
    let main = include_str!("../src/main.rs");
    let native = include_str!("../src/native_attachment_transport.rs");

    let select_chat = function_body(
        main,
        "async fn select_osl_chat_attachment(",
        "async fn list_osl_chat_attachments(",
    );
    let list_chat = function_body(
        main,
        "async fn list_osl_chat_attachments(",
        "async fn open_osl_chat_attachment(",
    );
    let open_chat = function_body(
        main,
        "async fn open_osl_chat_attachment(",
        "async fn select_native_discord_overlay_attachment(",
    );
    assert!(select_chat.contains("require_active_pro_entitlement"));
    assert!(!list_chat.contains("require_active_pro_entitlement"));
    assert!(!open_chat.contains("require_active_pro_entitlement"));

    let open_inner = function_body(
        native,
        "fn open_pending_inner(",
        "const MAX_ATTACHMENT_TTL_SECONDS",
    );
    assert!(open_inner.contains("grant_recipient_download_from_authenticated_notice("));
    assert!(open_inner.contains("download_pro_attachment_for_recipient("));
    assert!(open_inner.contains("authorize_current_recipient(core, &permission)"));
    assert!(!open_inner.contains("require_active_pro(core)"));
    println!(
        "TASK0655_PRODUCTION_RECEIVE_PERMISSION_WIRED=true TASK0655_SEND_PRO_GATE_PRESENT=true TASK0655_RECEIVE_PRO_GATE_COUNT=0"
    );
}
