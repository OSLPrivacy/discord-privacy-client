//! TASK 3224: filling the receiver's disk during a streamed attachment
//! download must never turn a prefix (even one accompanied by its key) into a
//! stored file.
//!
//! The real ENOSPC case runs in a subprocess with its own unprivileged mount
//! namespace. That process mounts a deliberately small tmpfs, completes the
//! control download, then fills the same filesystem exactly halfway through a
//! second download. The production staging-file creator, streaming client,
//! partial-file guard, hash check, and decryptor are all on the measured path.

#![cfg(target_os = "linux")]

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crypto::aead::Key;
use ipc::cipher_store_client::{CipherStoreClient, CipherStoreError};
use osl_privacy_hub::attachment_partial_guard::AttachmentPartialGuard;
use osl_privacy_hub::peer_attachment_io;
use sha2::{Digest, Sha256};

const CHILD_ENV: &str = "TASK3224_CHILD";
const ROOT_ENV: &str = "TASK3224_ROOT";
const MUTANT_ENV: &str = "TASK3224_KEEP_HALF_WITH_KEY";
const OBJECT_ID: &str = "32243224322432243224322432243224";
const FETCH_TOKEN: [u8; 16] = [0x24; 16];
const ATTACHMENT_KEY: [u8; 32] = [0x42; 32];
const PLAINTEXT_BYTES: usize = 4 * 1024 * 1024;

fn payload() -> Vec<u8> {
    (0..PLAINTEXT_BYTES)
        .map(|index| ((index * 37 + 24) % 251) as u8)
        .collect()
}

fn sealed_fixture() -> (Vec<u8>, Vec<u8>) {
    let fixture = tempfile::tempdir().expect("create source fixture root");
    let plaintext = payload();
    let source_path = fixture.path().join("source.png");
    fs::write(&source_path, &plaintext).expect("write source fixture");
    let mut source = File::open(source_path).expect("open source fixture");
    let staged = peer_attachment_io::encrypt_file(
        fixture.path(),
        &mut source,
        "fixture.png",
        "image/png",
        Key::from_bytes(ATTACHMENT_KEY),
        b"task-3224-full-disk".to_vec(),
        0,
    )
    .expect("encrypt source fixture");
    let sealed = fs::read(staged.path()).expect("read sealed fixture");
    (plaintext, sealed)
}

fn read_request(stream: &mut TcpStream) {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 2048];
    while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
        let read = stream.read(&mut chunk).expect("read download request");
        assert_ne!(read, 0, "download request ended before its headers");
        request.extend_from_slice(&chunk[..read]);
    }
    let request = String::from_utf8(request).expect("request headers are UTF-8");
    assert!(
        request.starts_with(&format!("GET /v1/attachment/{OBJECT_ID} HTTP/1.1\r\n")),
        "download uses the attachment endpoint: {request}"
    );
    assert!(request.lines().any(|line| {
        line.eq_ignore_ascii_case("x-osl-fetch-token: 24242424242424242424242424242424")
    }));
}

fn write_headers(stream: &mut TcpStream, content_length: usize) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {content_length}\r\nConnection: close\r\n\r\n"
    )
    .expect("write response headers");
}

fn start_good_server(bytes: Vec<u8>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind good download server");
    let address = listener.local_addr().expect("good download address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept good download");
        read_request(&mut stream);
        write_headers(&mut stream, bytes.len());
        stream.write_all(&bytes).expect("stream complete download");
    });
    (format!("http://{address}"), server)
}

fn start_paused_server(
    bytes: Vec<u8>,
    resume: Receiver<()>,
) -> (String, thread::JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind full-disk server");
    let address = listener.local_addr().expect("full-disk server address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept full-disk download");
        read_request(&mut stream);
        write_headers(&mut stream, bytes.len());
        let halfway = bytes.len() / 2;
        stream
            .write_all(&bytes[..halfway])
            .expect("stream first half before disk fill");
        stream.flush().expect("flush first half before disk fill");
        resume.recv().expect("disk filler resumes the server");
        // Once the receiver reports ENOSPC it closes the socket. The relay may
        // therefore observe BrokenPipe while trying to supply the second half.
        let _ = stream.write_all(&bytes[halfway..]);
        halfway
    });
    (format!("http://{address}"), server)
}

struct HalfwayTriggerWriter {
    file: File,
    written: usize,
    halfway: usize,
    trigger: Option<Sender<()>>,
}

impl Write for HalfwayTriggerWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let allowed = if self.written < self.halfway {
            bytes.len().min(self.halfway - self.written)
        } else {
            bytes.len()
        };
        let written = self.file.write(&bytes[..allowed])?;
        self.written += written;
        if self.written == self.halfway {
            if let Some(trigger) = self.trigger.take() {
                trigger
                    .send(())
                    .map_err(|_| io::Error::other("disk filler stopped"))?;
            }
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

fn fill_disk_after_halfway(
    root: PathBuf,
    start: Receiver<()>,
    resume: Sender<()>,
) -> thread::JoinHandle<(bool, u64)> {
    thread::spawn(move || {
        start.recv().expect("download reaches halfway");
        let filler_path = root.join("disk-filler.bin");
        let mut filler = File::create(&filler_path).expect("create disk filler");
        let block = [0x5a_u8; 64 * 1024];
        let mut written = 0_u64;
        let saw_enospc = loop {
            match filler.write(&block) {
                Ok(0) => break false,
                Ok(count) => written += count as u64,
                Err(error) if error.raw_os_error() == Some(28) => break true,
                Err(error) => panic!("disk fill failed before ENOSPC: {error}"),
            }
        };
        resume.send(()).expect("resume paused download server");
        (saw_enospc, written)
    })
}

fn atomic_publish(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("stored-file parent"))
        .expect("create stored-file directory");
    let temporary = path.with_extension("part");
    let mut output = File::create(&temporary).expect("create stored-file partial");
    output.write_all(bytes).expect("write complete stored file");
    output.sync_all().expect("sync complete stored file");
    drop(output);
    fs::rename(&temporary, path).expect("publish complete stored file atomically");
}

fn regular_file_count(path: &Path) -> usize {
    fs::read_dir(path)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .count()
}

fn staging_part_count(root: &Path) -> usize {
    regular_file_count(&root.join("peer-attachment-staging"))
}

fn run_good(root: &Path, plaintext: &[u8], sealed: &[u8]) -> usize {
    let (base_url, server) = start_good_server(sealed.to_vec());
    let client = CipherStoreClient::new(base_url).expect("build good download client");
    let (download_path, mut download) =
        peer_attachment_io::create_download_file(root).expect("create good download staging");
    let mut partial = AttachmentPartialGuard::new(
        root,
        download_path,
        peer_attachment_io::remove_staging_path_in_root,
    );
    let fetched = client
        .fetch_attachment_to_writer(OBJECT_ID, &FETCH_TOKEN, &mut download)
        .expect("good download has room to finish");
    assert_eq!(fetched, sealed.len() as u64);
    download.sync_all().expect("sync good download");
    drop(download);
    let (digest, size) =
        peer_attachment_io::sha256_file(partial.path()).expect("hash good download");
    let expected_digest: [u8; 32] = Sha256::digest(sealed).into();
    assert_eq!((digest, size), (expected_digest, sealed.len() as u64));
    let mut encrypted = File::open(partial.path()).expect("open complete sealed download");
    let opened = peer_attachment_io::decrypt_file_to_memory(
        &mut encrypted,
        "fixture.png",
        "image/png",
        Key::from_bytes(ATTACHMENT_KEY),
    )
    .expect("complete download decrypts with its unlock key");
    assert_eq!(&*opened, plaintext);
    drop(encrypted);
    atomic_publish(&root.join("stored/fixture.png"), &opened);
    partial.discard().expect("discard good sealed staging");
    server.join().expect("good server exits");
    assert_eq!(staging_part_count(root), 0);
    regular_file_count(&root.join("stored"))
}

fn run_full_disk(root: &Path, sealed: &[u8]) -> (usize, bool, usize, usize) {
    fs::create_dir_all(root.join("stored")).expect("create full-disk stored directory");
    let (resume_tx, resume_rx) = mpsc::channel();
    let (base_url, server) = start_paused_server(sealed.to_vec(), resume_rx);
    let (fill_tx, fill_rx) = mpsc::channel();
    let filler = fill_disk_after_halfway(root.to_path_buf(), fill_rx, resume_tx);
    let client = CipherStoreClient::new(base_url).expect("build full-disk download client");
    let (download_path, download) =
        peer_attachment_io::create_download_file(root).expect("create full-disk staging");
    let partial = AttachmentPartialGuard::new(
        root,
        download_path,
        peer_attachment_io::remove_staging_path_in_root,
    );
    let mut download = HalfwayTriggerWriter {
        file: download,
        written: 0,
        halfway: sealed.len() / 2,
        trigger: Some(fill_tx),
    };
    let result = client.fetch_attachment_to_writer(OBJECT_ID, &FETCH_TOKEN, &mut download);
    let received_before_enospc = download.written;
    drop(download);
    let relay_half = server.join().expect("full-disk server exits");
    let (saw_enospc, filler_bytes) = filler.join().expect("disk filler exits");
    assert!(
        saw_enospc,
        "disk filler must reach the real ENOSPC boundary"
    );
    let download_enospc = matches!(
        &result,
        Err(CipherStoreError::Io(error)) if error.raw_os_error() == Some(28)
    );
    assert!(
        download_enospc,
        "streaming download must report ENOSPC, got {result:?}"
    );
    // A filesystem may have a final partially allocated block available to
    // this inode even after the filler cannot allocate its next block.
    assert!(received_before_enospc >= relay_half);
    assert!(received_before_enospc < sealed.len());

    if std::env::var_os(MUTANT_ENV).is_some() {
        // Deliberately reproduce the unsafe outcome in the throwaway tmpfs:
        // retain the exact half ciphertext and place its real key beside it.
        fs::remove_file(root.join("disk-filler.bin")).expect("free space for mutant key");
        let half = root.join("stored/fixture.png.half");
        fs::rename(partial.path(), &half).expect("mutant retains half download");
        File::options()
            .write(true)
            .open(&half)
            .expect("open mutant half")
            .set_len(relay_half as u64)
            .expect("retain exactly half the download");
        let key = root.join("stored/fixture.png.unlock-key");
        fs::write(&key, ATTACHMENT_KEY).expect("mutant retains unlock key");
        drop(partial);
        let half_bytes = fs::metadata(&half).expect("half metadata").len() as usize;
        let key_bytes = fs::metadata(&key).expect("key metadata").len() as usize;
        println!(
            "TASK3224 MUTANT recoverable_half_file_with_unlock_key half_bytes={half_bytes} unlock_key_bytes={key_bytes}"
        );
        assert!(half.is_file() && key.is_file());
        panic!("recoverable_half_file_with_unlock_key");
    }

    // This is the production failure path: dropping the guard removes the
    // staging prefix even while the filesystem remains completely full.
    drop(partial);
    fs::remove_file(root.join("disk-filler.bin")).expect("remove disk filler");
    let stored_files = regular_file_count(&root.join("stored"));
    let leftover_parts = staging_part_count(root);
    (
        stored_files,
        saw_enospc,
        leftover_parts,
        filler_bytes as usize,
    )
}

fn mount_private_disk(root: &Path) {
    let status = Command::new("mount")
        .args(["-t", "tmpfs", "-o", "size=20m,nr_inodes=1024", "tmpfs"])
        .arg(root)
        .status()
        .expect("start private tmpfs mount");
    assert!(status.success(), "private tmpfs mount failed: {status}");
}

#[test]
fn task_3224_attack_40_full_disk_during_download_stores_no_file() {
    if std::env::var_os(CHILD_ENV).is_some() {
        return;
    }
    let mountpoint = tempfile::tempdir().expect("create private-disk mountpoint");
    let status = Command::new("unshare")
        .args(["-U", "-r", "-m"])
        .arg(std::env::current_exe().expect("current test executable"))
        .args([
            "--ignored",
            "--exact",
            "task_3224_private_disk_child",
            "--nocapture",
        ])
        .env(CHILD_ENV, "1")
        .env(ROOT_ENV, mountpoint.path())
        .status()
        .expect("run private-disk child");
    assert!(status.success(), "private-disk child failed: {status}");
}

#[test]
#[ignore = "subprocess helper requiring its own mount namespace"]
fn task_3224_private_disk_child() {
    let root = PathBuf::from(std::env::var(ROOT_ENV).expect("TASK3224_ROOT is set"));
    mount_private_disk(&root);
    let (plaintext, sealed) = sealed_fixture();

    let good_files = run_good(&root.join("good"), &plaintext, &sealed);
    assert_eq!(
        good_files, 1,
        "good download stores exactly one complete file"
    );
    println!(
        "TASK3224 good_download room_to_spare=true complete_files={good_files} complete_bytes={}",
        plaintext.len()
    );

    let (full_files, saw_enospc, leftover_parts, filler_bytes) =
        run_full_disk(&root.join("full-disk"), &sealed);
    assert_eq!(full_files, 0, "full-disk run must store zero files");
    assert_eq!(leftover_parts, 0, "full-disk run must remove its half file");
    println!(
        "TASK3224 full_disk_download enospc={saw_enospc} received_bytes={} filler_bytes={filler_bytes} complete_files={full_files} leftover_parts={leftover_parts} unlock_key_files=0",
        sealed.len() / 2
    );

    let status = Command::new("umount")
        .arg(&root)
        .status()
        .expect("unmount private tmpfs");
    assert!(status.success(), "private tmpfs unmount failed: {status}");
}
