//! TASK 0656: an exact 1 GiB sparse fixture is uploaded by the Pro path,
//! rebuilt by a bounded sparse storage fixture, and fetched directly by a Free
//! recipient. The read-back hash must detect a one-byte source mutation.

#![cfg(feature = "core")]

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;

use ipc::cipher_store_client::{
    CipherStoreClient, ATTACHMENT_MULTIPART_MAX_PARTS, ATTACHMENT_MULTIPART_PART_BYTES,
    MAX_SEALED_ATTACHMENT_BYTES, TTL_7D,
};
use osl_privacy_hub::attachment_limits::AttachmentAccountTier;
use osl_privacy_hub::osl_chat_attachment_download_permission::{
    download_pro_attachment_for_recipient, grant_recipient_download_from_pro_send,
    RecipientAttachmentDownloadRequest, StoredReceiverPermission, StoredRecipientAttachment,
};
use sha2::{Digest, Sha256};

const FILE_ID: &str = "06560656065606560656065606560656";
const RECIPIENT_ID: &str = "free-recipient-0656";
const FETCH_TOKEN: [u8; 16] = [0x56; 16];
const EXACT_FILE_BYTES: u64 = 1024 * 1024 * 1024;
const EXPIRES_AT: i64 = 1_900_000_656;
const MUTATION_ENV: &str = "TASK0656_MUTATE_FIXTURE";
const MUTATION_OFFSET: u64 = ATTACHMENT_MULTIPART_PART_BYTES * 91 + 656;
const IO_BUFFER_BYTES: usize = 256 * 1024;
const REQUEST_COUNT: usize = 1 + ATTACHMENT_MULTIPART_MAX_PARTS as usize + 1 + 1;

struct RequestHead {
    method: String,
    path: String,
    headers: String,
    initial_body: Vec<u8>,
}

struct StorageObservation {
    uploaded_bytes: u64,
    uploaded_sha256: String,
    uploaded_piece_count: u32,
}

struct SparseStorageFixture {
    base_url: String,
    server: thread::JoinHandle<StorageObservation>,
}

impl SparseStorageFixture {
    fn start(stored_path: PathBuf) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind sparse storage fixture");
        let address = listener.local_addr().expect("sparse storage address");
        let server = thread::spawn(move || {
            let mut stored = OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .open(&stored_path)
                .expect("create rebuilt sparse storage file");
            stored
                .set_len(EXACT_FILE_BYTES)
                .expect("size rebuilt sparse storage file");
            let mut upload_hasher = Sha256::new();
            let mut uploaded_bytes = 0_u64;
            let mut uploaded_piece_count = 0_u32;

            for _ in 0..REQUEST_COUNT {
                let (mut stream, _) = listener.accept().expect("accept storage request");
                let request = read_request_head(&mut stream);
                if request.method == "POST" && request.path == "/v1/attachment/session" {
                    assert_eq!(
                        header(&request.headers, "x-osl-size-bytes"),
                        Some(EXACT_FILE_BYTES.to_string())
                    );
                    assert_eq!(
                        header(&request.headers, "x-osl-fetch-token"),
                        Some("56565656565656565656565656565656".to_owned())
                    );
                    respond_json(
                        &mut stream,
                        &format!(
                            r#"{{"id":"{FILE_ID}","expires_at":{EXPIRES_AT},"size_bytes":{EXACT_FILE_BYTES},"max_part_bytes":{ATTACHMENT_MULTIPART_PART_BYTES},"max_parts":{ATTACHMENT_MULTIPART_MAX_PARTS}}}"#
                        ),
                    );
                    continue;
                }

                let part_prefix = format!("/v1/attachment/{FILE_ID}/part/");
                if request.method == "PUT" && request.path.starts_with(&part_prefix) {
                    let part_number = request.path[part_prefix.len()..]
                        .parse::<u32>()
                        .expect("numeric multipart piece");
                    assert_eq!(part_number, uploaded_piece_count + 1);
                    let content_length = header(&request.headers, "content-length")
                        .expect("multipart content length")
                        .parse::<u64>()
                        .expect("numeric multipart content length");
                    assert_eq!(content_length, ATTACHMENT_MULTIPART_PART_BYTES);
                    let part_offset = u64::from(part_number - 1) * ATTACHMENT_MULTIPART_PART_BYTES;
                    consume_sparse_part(
                        &mut stream,
                        request.initial_body,
                        content_length,
                        part_offset,
                        &mut stored,
                        &mut upload_hasher,
                    );
                    uploaded_bytes += content_length;
                    uploaded_piece_count += 1;
                    respond_json(
                        &mut stream,
                        &format!(
                            r#"{{"part_number":{part_number},"size_bytes":{content_length}}}"#
                        ),
                    );
                    continue;
                }

                if request.method == "POST"
                    && request.path == format!("/v1/attachment/{FILE_ID}/complete")
                {
                    assert_eq!(uploaded_bytes, EXACT_FILE_BYTES);
                    assert_eq!(uploaded_piece_count, ATTACHMENT_MULTIPART_MAX_PARTS);
                    stored.sync_all().expect("sync rebuilt sparse storage file");
                    respond_json(
                        &mut stream,
                        &format!(
                            r#"{{"id":"{FILE_ID}","expires_at":{EXPIRES_AT},"size_bytes":{EXACT_FILE_BYTES}}}"#
                        ),
                    );
                    continue;
                }

                assert_eq!(request.method, "GET");
                assert_eq!(request.path, format!("/v1/attachment/{FILE_ID}"));
                assert_eq!(
                    header(&request.headers, "x-osl-fetch-token"),
                    Some("56565656565656565656565656565656".to_owned())
                );
                stream_file_response(&mut stream, &mut stored);
            }

            StorageObservation {
                uploaded_bytes,
                uploaded_sha256: format!("{:x}", upload_hasher.finalize()),
                uploaded_piece_count,
            }
        });
        Self {
            base_url: format!("http://{address}"),
            server,
        }
    }

    fn join(self) -> StorageObservation {
        self.server
            .join()
            .expect("sparse storage fixture completes")
    }
}

fn read_request_head(stream: &mut TcpStream) -> RequestHead {
    let mut request = Vec::with_capacity(4096);
    let mut buffer = [0_u8; 4096];
    loop {
        let read = stream.read(&mut buffer).expect("read storage request head");
        assert_ne!(read, 0, "storage request ended before headers");
        request.extend_from_slice(&buffer[..read]);
        let Some(headers_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8(request[..headers_end].to_vec())
            .expect("storage request headers are UTF-8");
        let mut request_line = headers
            .lines()
            .next()
            .expect("request line")
            .split_whitespace();
        return RequestHead {
            method: request_line.next().expect("request method").to_owned(),
            path: request_line.next().expect("request path").to_owned(),
            headers,
            initial_body: request[headers_end + 4..].to_vec(),
        };
    }
}

fn header(headers: &str, wanted: &str) -> Option<String> {
    headers.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case(wanted)
            .then(|| value.trim().to_owned())
    })
}

fn consume_sparse_part(
    stream: &mut TcpStream,
    initial: Vec<u8>,
    content_length: u64,
    part_offset: u64,
    stored: &mut File,
    hasher: &mut Sha256,
) {
    assert!(initial.len() as u64 <= content_length);
    let mut consumed = 0_u64;
    store_sparse_block(stored, part_offset, &initial);
    hasher.update(&initial);
    consumed += initial.len() as u64;
    let mut buffer = vec![0_u8; IO_BUFFER_BYTES];
    while consumed < content_length {
        let wanted = usize::try_from((content_length - consumed).min(buffer.len() as u64))
            .expect("bounded read length");
        stream
            .read_exact(&mut buffer[..wanted])
            .expect("read multipart body");
        store_sparse_block(stored, part_offset + consumed, &buffer[..wanted]);
        hasher.update(&buffer[..wanted]);
        consumed += wanted as u64;
    }
}

fn store_sparse_block(stored: &mut File, offset: u64, bytes: &[u8]) {
    if bytes.iter().any(|byte| *byte != 0) {
        stored
            .seek(SeekFrom::Start(offset))
            .expect("seek rebuilt sparse storage file");
        stored
            .write_all(bytes)
            .expect("write nonzero sparse storage block");
    }
}

fn respond_json(stream: &mut TcpStream, body: &str) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("write JSON fixture response");
}

fn stream_file_response(stream: &mut TcpStream, stored: &mut File) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {EXACT_FILE_BYTES}\r\nConnection: close\r\n\r\n"
    )
    .expect("write file response headers");
    stored
        .seek(SeekFrom::Start(0))
        .expect("rewind rebuilt sparse file");
    let mut remaining = EXACT_FILE_BYTES;
    let mut buffer = vec![0_u8; IO_BUFFER_BYTES];
    while remaining != 0 {
        let wanted =
            usize::try_from(remaining.min(buffer.len() as u64)).expect("bounded response read");
        stored
            .read_exact(&mut buffer[..wanted])
            .expect("read rebuilt sparse file");
        stream
            .write_all(&buffer[..wanted])
            .expect("stream rebuilt sparse file");
        remaining -= wanted as u64;
    }
}

fn create_sparse_fixture(path: &Path) {
    let mut fixture = File::create(path).expect("create sender sparse fixture");
    fixture
        .set_len(EXACT_FILE_BYTES)
        .expect("set exact sparse fixture length");
    for (offset, marker) in [
        (0, b"TASK0656-PRO-SPARSE-BEGIN".as_slice()),
        (
            ATTACHMENT_MULTIPART_PART_BYTES * 64 + 656,
            b"TASK0656-SPARSE-MIDDLE".as_slice(),
        ),
        (EXACT_FILE_BYTES - 24, b"TASK0656-PRO-SPARSE-END".as_slice()),
    ] {
        fixture
            .seek(SeekFrom::Start(offset))
            .expect("seek sender sparse fixture marker");
        fixture
            .write_all(marker)
            .expect("write sender sparse fixture marker");
    }
    fixture.sync_all().expect("sync sender sparse fixture");
}

fn hash_file(path: &Path) -> String {
    let mut file = File::open(path).expect("open sparse fixture for hashing");
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; IO_BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer).expect("hash sparse fixture");
        if read == 0 {
            return format!("{:x}", hasher.finalize());
        }
        hasher.update(&buffer[..read]);
    }
}

struct HashingWriter {
    hasher: Sha256,
    bytes: u64,
}

impl HashingWriter {
    fn new() -> Self {
        Self {
            hasher: Sha256::new(),
            bytes: 0,
        }
    }

    fn finish(self) -> (u64, String) {
        (self.bytes, format!("{:x}", self.hasher.finalize()))
    }
}

impl Write for HashingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.hasher.update(bytes);
        self.bytes += bytes.len() as u64;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn task_0656_free_copy_receives_exact_one_gib_sparse_file_from_pro_copy() {
    assert_eq!(EXACT_FILE_BYTES, MAX_SEALED_ATTACHMENT_BYTES);
    assert_eq!(ATTACHMENT_MULTIPART_MAX_PARTS, 128);
    let temp = tempfile::tempdir().expect("task 0656 tempdir");
    let sender_path = temp.path().join("pro-copy-sender-fixture.bin");
    let stored_path = temp.path().join("direct-store-rebuilt.bin");
    create_sparse_fixture(&sender_path);
    let sender_fixture_hash = hash_file(&sender_path);

    if std::env::var_os(MUTATION_ENV).is_some() {
        let mut fixture = OpenOptions::new()
            .write(true)
            .open(&sender_path)
            .expect("open fixture for one-byte mutation");
        fixture
            .seek(SeekFrom::Start(MUTATION_OFFSET))
            .expect("seek one-byte mutation");
        fixture.write_all(&[1]).expect("write one-byte mutation");
        fixture.sync_all().expect("sync one-byte mutation");
        println!("TASK0656 one_byte_mutation_offset={MUTATION_OFFSET} one_byte_mutation_count=1");
    }

    let storage = SparseStorageFixture::start(stored_path);
    let client = CipherStoreClient::new(&storage.base_url).expect("direct storage client");
    let report = client
        .upload_attachment_file_pro_chunked(
            File::open(&sender_path).expect("open Pro sender fixture"),
            TTL_7D,
            &FETCH_TOKEN,
        )
        .expect("Pro copy sends exact 1 GiB sparse fixture");
    let stored_attachment = StoredRecipientAttachment {
        file_id: report.completed_file.file_id.clone(),
        file_name: "task-0656-one-gib.bin".to_owned(),
        byte_length: report.completed_file.total_size_bytes,
        kind: "application/octet-stream".to_owned(),
        owner_osl_user_id: "pro-sender-0656".to_owned(),
        receiver_permission: StoredReceiverPermission::Download,
    };
    let permission = grant_recipient_download_from_pro_send(
        &report,
        &stored_attachment,
        RECIPIENT_ID,
        FETCH_TOKEN,
    )
    .expect("completed Pro send grants the Free copy");
    let free_request = RecipientAttachmentDownloadRequest {
        recipient_osl_user_id: RECIPIENT_ID.to_owned(),
        account_tier: AttachmentAccountTier::Free,
    };
    let mut read_back = HashingWriter::new();
    let receipt = download_pro_attachment_for_recipient(
        &permission,
        &stored_attachment,
        &free_request,
        &client,
        &mut read_back,
    )
    .expect("Free copy fetches Pro-sent sparse fixture directly");
    let observation = storage.join();
    let (read_back_bytes, read_back_hash) = read_back.finish();

    assert_eq!(report.finished_pieces.len(), 128);
    assert_eq!(receipt.recipient_account_tier, AttachmentAccountTier::Free);
    assert_eq!(receipt.byte_length, EXACT_FILE_BYTES);
    assert_eq!(read_back_bytes, EXACT_FILE_BYTES);
    assert_eq!(observation.uploaded_bytes, EXACT_FILE_BYTES);
    assert_eq!(observation.uploaded_piece_count, 128);
    assert_eq!(observation.uploaded_sha256, read_back_hash);
    println!("TASK0656 sender_copy_tier=Pro receiver_copy_tier=Free");
    println!("TASK0656 sparse_fixture_bytes={EXACT_FILE_BYTES}");
    println!(
        "TASK0656 uploaded_piece_count={}",
        report.finished_pieces.len()
    );
    println!("TASK0656 direct_read_back_bytes={read_back_bytes}");
    println!("TASK0656 sender_fixture_sha256={sender_fixture_hash}");
    println!("TASK0656 uploaded_sha256={}", observation.uploaded_sha256);
    println!("TASK0656 free_read_back_sha256={read_back_hash}");
    println!(
        "TASK0656 free_read_back_hash_match={}",
        sender_fixture_hash == read_back_hash
    );
    assert_eq!(
        sender_fixture_hash, read_back_hash,
        "Free copy read-back hash must equal the Pro sender fixture hash"
    );
}
