#![cfg(feature = "core")]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use osl_privacy_hub::osl_chat_content_name::{
    OldOslChatContentIndex, OpenedOldOslChatContent, NOT_SAVED_ON_THIS_DEVICE,
};
use store::{MessageStore, StoredMessage};

const OLD_TEXT_ITEM: &str = "task3647-old-marked-message";
const OLD_TEXT_MESSAGE_ID: &str = "task3647-old-marked-message-row";
const OLD_TEXT: &str = "TASK3647 OLD MARKED MESSAGE exact local text";
const DOWNLOADED_FILE_ITEM: &str = "task3647-downloaded-file";
const DOWNLOADED_FILE_BYTES: &[u8] = b"TASK3647\0DOWNLOADED\xffEXACT-BYTES\r\n";
const REMOTE_TEXT_ITEM: &str = "task3647-remote-text";
const REMOTE_FILE_ITEM: &str = "task3647-remote-file";
const STORE_KEY: &[u8; 32] = &[0x47; 32];

struct OnlineExistenceService {
    address: SocketAddr,
    calls: Arc<AtomicUsize>,
    server: thread::JoinHandle<()>,
}

impl OnlineExistenceService {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind whole OSL service fixture");
        let address = listener.local_addr().expect("read service address");
        let calls = Arc::new(AtomicUsize::new(0));
        let server_calls = Arc::clone(&calls);
        let server = thread::spawn(move || {
            for expected_item in [REMOTE_TEXT_ITEM, REMOTE_FILE_ITEM] {
                let (mut stream, _) = listener.accept().expect("accept online existence proof");
                let request = read_request(&mut stream);
                assert!(
                    request.starts_with(&format!("GET /content/{expected_item} HTTP/1.1\r\n")),
                    "wrong remote-only item request: {request}"
                );
                server_calls.fetch_add(1, Ordering::SeqCst);
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nexists",
                    )
                    .expect("write online existence response");
            }
            // Dropping the only listener takes the whole fixture service
            // offline after both remote-only objects have been proved.
        });
        Self {
            address,
            calls,
            server,
        }
    }

    fn prove_exists(&self, item_id: &str) {
        let mut stream = TcpStream::connect(self.address).expect("service is online");
        write!(
            stream,
            "GET /content/{item_id} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            self.address
        )
        .expect("request remote-only existence");
        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .expect("read existence response");
        assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));
        assert!(response.ends_with(b"exists"));
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn take_offline(self) -> (SocketAddr, Arc<AtomicUsize>) {
        let address = self.address;
        let calls = self.calls;
        self.server.join().expect("whole OSL service exits");
        assert!(
            TcpStream::connect(address).is_err(),
            "whole OSL service must refuse connections after shutdown"
        );
        (address, calls)
    }
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        let read = stream.read(&mut buffer).expect("read service request");
        assert_ne!(read, 0, "request ended before headers");
        request.extend_from_slice(&buffer[..read]);
        if request.windows(4).any(|part| part == b"\r\n\r\n") {
            return String::from_utf8(request).expect("HTTP request is UTF-8");
        }
    }
}

fn assert_text(opened: OpenedOldOslChatContent) {
    assert_eq!(
        opened,
        OpenedOldOslChatContent::Text(OLD_TEXT.to_owned()),
        "old marked message text changed"
    );
}

fn assert_file(opened: OpenedOldOslChatContent) {
    assert_eq!(
        opened,
        OpenedOldOslChatContent::File(DOWNLOADED_FILE_BYTES.to_vec()),
        "downloaded file bytes changed"
    );
}

#[test]
fn task_3647_opens_saved_content_and_refuses_remote_only_content_offline() {
    let temp = tempfile::tempdir().expect("task 3647 local device root");
    let store = MessageStore::open(&temp.path().join("messages"), STORE_KEY)
        .expect("open local encrypted message store");
    store
        .put(&StoredMessage {
            discord_message_id: OLD_TEXT_MESSAGE_ID.to_owned(),
            channel_id: "task3647-old-chat".to_owned(),
            sender_discord_id: "task3647-old-sender".to_owned(),
            sender_osl_user_id: "task3647-old-osl-sender".to_owned(),
            plaintext: OLD_TEXT.to_owned(),
            decrypted_at: 1_700_003_647,
            reply_parent_id: None,
            edit_revision: 1,
            burned: false,
        })
        .expect("persist old marked message locally");
    let downloaded_path = temp.path().join("downloads").join("task3647.bin");
    std::fs::create_dir_all(downloaded_path.parent().expect("download parent"))
        .expect("create downloads directory");
    std::fs::write(&downloaded_path, DOWNLOADED_FILE_BYTES)
        .expect("persist exact downloaded file bytes");

    let mut index = OldOslChatContentIndex::default();
    index.record_saved_text(OLD_TEXT_ITEM, OLD_TEXT_MESSAGE_ID);
    index.record_downloaded_file(DOWNLOADED_FILE_ITEM, &downloaded_path);
    index.record_remote_only(REMOTE_TEXT_ITEM);
    index.record_remote_only(REMOTE_FILE_ITEM);

    let service = OnlineExistenceService::start();
    let service_calls_before_local_opens = service.calls();
    let mut local_success_count = 0;
    assert_text(
        index
            .open_saved(OLD_TEXT_ITEM, &store)
            .expect("old marked message opens locally while online"),
    );
    local_success_count += 1;
    assert_file(
        index
            .open_saved(DOWNLOADED_FILE_ITEM, &store)
            .expect("downloaded file opens locally while online"),
    );
    local_success_count += 1;
    assert_eq!(local_success_count, 2);
    assert_eq!(
        service.calls(),
        service_calls_before_local_opens,
        "saved opens changed service-call count"
    );

    service.prove_exists(REMOTE_TEXT_ITEM);
    service.prove_exists(REMOTE_FILE_ITEM);
    let service_calls_after_existence_proof = service.calls();
    assert_eq!(service_calls_after_existence_proof, 2);

    let (_offline_address, service_calls) = service.take_offline();
    let service_calls_before_offline_opens = service_calls.load(Ordering::SeqCst);

    assert_text(
        index
            .open_saved(OLD_TEXT_ITEM, &store)
            .expect("old marked message opens from local store offline"),
    );
    local_success_count += 1;
    assert_file(
        index
            .open_saved(DOWNLOADED_FILE_ITEM, &store)
            .expect("downloaded file opens from local disk offline"),
    );
    local_success_count += 1;

    let mut remote_success_count = 0;
    let remote_refusals = [REMOTE_TEXT_ITEM, REMOTE_FILE_ITEM].map(|item_id| {
        match index.open_saved(item_id, &store) {
            Ok(_) => {
                remote_success_count += 1;
                panic!("remote-only item opened while offline: {item_id}");
            }
            Err(refusal) => refusal,
        }
    });

    let service_calls_after_offline_opens = service_calls.load(Ordering::SeqCst);
    assert_eq!(local_success_count, 4);
    assert_eq!(remote_success_count, 0);
    assert_eq!(
        remote_refusals,
        [NOT_SAVED_ON_THIS_DEVICE, NOT_SAVED_ON_THIS_DEVICE]
    );
    assert_eq!(
        service_calls_after_offline_opens, service_calls_before_offline_opens,
        "offline opens must not call the service"
    );

    println!("TASK3647_REMOTE_ONLY_ONLINE_EXISTENCE_COUNT=2");
    println!("TASK3647_LOCAL_SUCCESS_COUNT_BEFORE_DISCONNECTION=2");
    println!("TASK3647_SERVICE_OFFLINE=true");
    println!("TASK3647_LOCAL_SUCCESS_COUNT_OFFLINE_CUMULATIVE={local_success_count}");
    println!("TASK3647_SAME_TEXT_OFFLINE=true TEXT={OLD_TEXT:?}");
    println!(
        "TASK3647_SAME_BYTES_OFFLINE=true BYTES={:?}",
        DOWNLOADED_FILE_BYTES
    );
    println!(
        "TASK3647_SERVICE_CALLS_BEFORE_OFFLINE_OPENS={service_calls_before_offline_opens} TASK3647_SERVICE_CALLS_AFTER_OFFLINE_OPENS={service_calls_after_offline_opens}"
    );
    println!("TASK3647_REMOTE_SUCCESS_COUNT={remote_success_count}");
    println!("TASK3647_REMOTE_TEXT_REFUSAL={:?}", remote_refusals[0]);
    println!("TASK3647_REMOTE_FILE_REFUSAL={:?}", remote_refusals[1]);
}
