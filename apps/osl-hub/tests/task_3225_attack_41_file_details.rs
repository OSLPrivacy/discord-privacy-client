//! TASK 3225 / attack 41: recipient download authority is bound to every
//! security-relevant stored-file detail and fails locally if any one changes.

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
    RecipientAttachmentDownloadPermission, RecipientAttachmentDownloadRequest,
    StoredReceiverPermission, StoredRecipientAttachment,
};

const FILE_ID: &str = "32253225322532253225322532253225";
const OWNER_ID: &str = "task-3225-owner";
const RECEIVER_ID: &str = "task-3225-approved-receiver";
const UNAPPROVED_ID: &str = "task-3225-unapproved-person";
const FETCH_TOKEN: [u8; 16] = [0x32; 16];
const ORIGINAL_BYTES: &[u8] = b"TASK3225-ORIGINAL-BYTES";

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

fn completed_upload() -> ProChunkedUploadReport {
    ProChunkedUploadReport {
        finished_pieces: vec![ProChunkedUploadPiece {
            upload_id: FILE_ID.to_owned(),
            piece_number: 1,
            size_bytes: ORIGINAL_BYTES.len() as u64,
        }],
        completed_file: ProChunkedUploadFile {
            file_id: FILE_ID.to_owned(),
            total_size_bytes: ORIGINAL_BYTES.len() as u64,
            piece_count: 1,
            expires_at: 1_900_003_225,
        },
    }
}

fn original_stored_file() -> StoredRecipientAttachment {
    StoredRecipientAttachment {
        file_id: FILE_ID.to_owned(),
        file_name: "attack-41-original.png".to_owned(),
        byte_length: ORIGINAL_BYTES.len() as u64,
        kind: "image/png".to_owned(),
        owner_osl_user_id: OWNER_ID.to_owned(),
        receiver_permission: StoredReceiverPermission::Download,
    }
}

fn changed_download_refusal(
    permission: &RecipientAttachmentDownloadPermission,
    original: &StoredRecipientAttachment,
    request: &RecipientAttachmentDownloadRequest,
    client: &CipherStoreClient,
    change: impl FnOnce(&mut StoredRecipientAttachment),
) -> &'static str {
    let mut changed = original.clone();
    change(&mut changed);
    let mut output = Vec::new();
    let refusal =
        download_pro_attachment_for_recipient(permission, &changed, request, client, &mut output)
            .expect_err("changed stored-file detail must refuse the download");
    assert!(output.is_empty(), "a refused download must yield no bytes");
    refusal.name()
}

#[test]
fn task_3225_attack_41_refuses_each_changed_detail_and_every_unapproved_download() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind exact-byte fixture");
    let address = listener.local_addr().expect("fixture address");
    let direct_request_count = Arc::new(AtomicUsize::new(0));
    let observed_count = Arc::clone(&direct_request_count);
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept unchanged download only");
        let request = read_request(&mut stream);
        assert_eq!(
            request.lines().next(),
            Some(format!("GET /v1/attachment/{FILE_ID} HTTP/1.1").as_str())
        );
        assert!(request.lines().any(|line| {
            line.eq_ignore_ascii_case("x-osl-fetch-token: 32323232323232323232323232323232")
        }));
        observed_count.fetch_add(1, Ordering::SeqCst);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            ORIGINAL_BYTES.len()
        )
        .expect("write response headers");
        stream
            .write_all(ORIGINAL_BYTES)
            .expect("write exact original bytes");
    });

    let client =
        CipherStoreClient::new(&format!("http://{address}")).expect("loopback cipher-store client");
    let original = original_stored_file();
    let permission = grant_recipient_download_from_pro_send(
        &completed_upload(),
        &original,
        RECEIVER_ID,
        FETCH_TOKEN,
    )
    .expect("mint permission bound to the original stored-file details");
    let approved_request = RecipientAttachmentDownloadRequest {
        recipient_osl_user_id: RECEIVER_ID.to_owned(),
        account_tier: AttachmentAccountTier::Free,
    };

    let changed_name =
        changed_download_refusal(&permission, &original, &approved_request, &client, |file| {
            file.file_name = "attack-41-renamed.png".to_owned()
        });
    let changed_size =
        changed_download_refusal(&permission, &original, &approved_request, &client, |file| {
            file.byte_length += 1
        });
    let changed_kind =
        changed_download_refusal(&permission, &original, &approved_request, &client, |file| {
            file.kind = "application/pdf".to_owned()
        });
    let changed_owner =
        changed_download_refusal(&permission, &original, &approved_request, &client, |file| {
            file.owner_osl_user_id = "task-3225-other-owner".to_owned()
        });
    let changed_receiver_permission =
        changed_download_refusal(&permission, &original, &approved_request, &client, |file| {
            file.receiver_permission = StoredReceiverPermission::None
        });

    assert_eq!(changed_name, "file_name_changed");
    assert_eq!(changed_size, "file_size_changed");
    assert_eq!(changed_kind, "file_kind_changed");
    assert_eq!(changed_owner, "file_owner_changed");
    assert_eq!(changed_receiver_permission, "receiver_permission_changed");

    let unapproved_request = RecipientAttachmentDownloadRequest {
        recipient_osl_user_id: UNAPPROVED_ID.to_owned(),
        account_tier: AttachmentAccountTier::Pro,
    };
    let mut unapproved_output = Vec::new();
    let unapproved_result = download_pro_attachment_for_recipient(
        &permission,
        &original,
        &unapproved_request,
        &client,
        &mut unapproved_output,
    );
    let unapproved_downloadable_file_count = usize::from(unapproved_result.is_ok());
    let unapproved_refusal = unapproved_result
        .expect_err("an unapproved person must not download the file")
        .name();
    assert_eq!(unapproved_refusal, "recipient_mismatch");
    assert_eq!(unapproved_downloadable_file_count, 0);
    assert!(unapproved_output.is_empty());

    let mut unchanged_download = Vec::new();
    let unchanged_receipt = download_pro_attachment_for_recipient(
        &permission,
        &original,
        &approved_request,
        &client,
        &mut unchanged_download,
    )
    .expect("unchanged stored file must download");
    server.join().expect("exact-byte fixture completes");
    assert_eq!(unchanged_receipt.byte_length, ORIGINAL_BYTES.len() as u64);
    assert_eq!(unchanged_download.as_slice(), ORIGINAL_BYTES);
    assert_eq!(direct_request_count.load(Ordering::SeqCst), 1);

    println!(
        "TASK3225 changed_name_refusal={changed_name} changed_size_refusal={changed_size} changed_kind_refusal={changed_kind} changed_owner_refusal={changed_owner} changed_receiver_permission_refusal={changed_receiver_permission} changed_download_refusal_count=5 unchanged_download_bytes=\"{}\" unchanged_download_byte_length={} unapproved_refusal={unapproved_refusal} unapproved_downloadable_file_count={unapproved_downloadable_file_count} direct_request_count={}",
        String::from_utf8_lossy(&unchanged_download),
        unchanged_download.len(),
        direct_request_count.load(Ordering::SeqCst),
    );
}
