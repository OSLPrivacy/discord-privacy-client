//! TASK 0654: a completed Pro send grants its recipient a tier-independent
//! direct download permission, and revocation is a named local refusal.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread;

use ipc::cipher_store_client::{
    CipherStoreClient, ProChunkedUploadFile, ProChunkedUploadPiece, ProChunkedUploadReport,
};
use osl_privacy_hub::attachment_limits::AttachmentAccountTier;
use osl_privacy_hub::osl_chat_attachment_download_permission::{
    download_pro_attachment_for_recipient, grant_recipient_download_from_pro_send,
    RecipientAttachmentDownloadRequest, SHARE_REVOKED_REFUSAL_NAME,
};

const FILE_ID: &str = "06540654065406540654065406540654";
const RECIPIENT_ID: &str = "free-recipient-0654";
const FETCH_TOKEN: [u8; 16] = [0x65; 16];
const FILE_BYTES: &[u8] = b"TASK0654-PRO-SENDER-SEALED-FILE-BYTES";

fn read_request(stream: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 2048];
    loop {
        let read = stream.read(&mut buffer).expect("read direct request");
        assert_ne!(read, 0, "request ended before headers");
        request.extend_from_slice(&buffer[..read]);
        if request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            return String::from_utf8(request).expect("request headers are UTF-8");
        }
    }
}

fn completed_pro_send() -> ProChunkedUploadReport {
    ProChunkedUploadReport {
        finished_pieces: vec![ProChunkedUploadPiece {
            upload_id: FILE_ID.to_owned(),
            piece_number: 1,
            size_bytes: FILE_BYTES.len() as u64,
        }],
        completed_file: ProChunkedUploadFile {
            file_id: FILE_ID.to_owned(),
            total_size_bytes: FILE_BYTES.len() as u64,
            piece_count: 1,
            expires_at: 1_900_000_654,
        },
    }
}

#[test]
fn task_0654_free_recipient_downloads_exact_pro_file_and_revoked_share_is_named() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind direct download fixture");
    let address = listener.local_addr().expect("fixture address");
    let request_count = Arc::new(AtomicUsize::new(0));
    let observed_count = Arc::clone(&request_count);
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept direct download");
        let request = read_request(&mut stream);
        let mut lines = request.lines();
        let expected_request_line = format!("GET /v1/attachment/{FILE_ID} HTTP/1.1");
        assert_eq!(lines.next(), Some(expected_request_line.as_str()));
        assert!(request.lines().any(|line| {
            line.eq_ignore_ascii_case("x-osl-fetch-token: 65656565656565656565656565656565")
        }));
        observed_count.fetch_add(1, Ordering::SeqCst);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            FILE_BYTES.len()
        )
        .expect("write response headers");
        stream.write_all(FILE_BYTES).expect("write exact fixture");
    });

    let client =
        CipherStoreClient::new(&format!("http://{address}")).expect("loopback cipher-store client");
    let mut permission =
        grant_recipient_download_from_pro_send(&completed_pro_send(), RECIPIENT_ID, FETCH_TOKEN)
            .expect("completed Pro send grants its recipient");
    let request = RecipientAttachmentDownloadRequest {
        recipient_osl_user_id: RECIPIENT_ID.to_owned(),
        account_tier: AttachmentAccountTier::Free,
    };
    let mut downloaded = Vec::new();
    let receipt =
        download_pro_attachment_for_recipient(&permission, &request, &client, &mut downloaded)
            .expect("Free recipient is authorized for Pro-sent file");
    server.join().expect("direct download fixture completes");

    assert_eq!(receipt.recipient_account_tier, AttachmentAccountTier::Free);
    assert_eq!(receipt.byte_length, FILE_BYTES.len() as u64);
    assert_eq!(downloaded, FILE_BYTES);
    assert_eq!(request_count.load(Ordering::SeqCst), 1);
    println!(
        "TASK0654 active_share authorized=true recipient_tier={} exact_byte_length={} direct_request_count={}",
        receipt.recipient_account_tier.label(),
        receipt.byte_length,
        request_count.load(Ordering::SeqCst),
    );

    assert!(permission.revoke());
    let mut revoked_output = Vec::new();
    let refusal =
        download_pro_attachment_for_recipient(&permission, &request, &client, &mut revoked_output)
            .expect_err("revoked recipient share must be refused");
    assert_eq!(refusal.name(), SHARE_REVOKED_REFUSAL_NAME);
    assert!(revoked_output.is_empty());
    assert_eq!(
        request_count.load(Ordering::SeqCst),
        1,
        "revoked share must fail before a network request"
    );
    println!(
        "TASK0654 revoked_share authorized=false refusal_name={} direct_request_count={}",
        refusal.name(),
        request_count.load(Ordering::SeqCst),
    );
}
