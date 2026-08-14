//! TASK 3223: an attachment is published only after every byte is present.
//!
//! The ignored helpers are re-executed as separate processes. The parent cuts
//! their local network flag, kills them after the named phase is entered, and
//! starts a fresh recovery process. This exercises process-death cleanup rather
//! than returning an ordinary Rust error through the same stack frame.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

use crypto::aead::Key;
use ipc::cipher_store_client::{CipherStoreClient, TTL_7D};
use osl_privacy_hub::peer_attachment_io;

const CHILD_ENV: &str = "TASK3223_CHILD_MODE";
const ROOT_ENV: &str = "TASK3223_ROOT";
const PHASE_ENV: &str = "TASK3223_PHASE";
const MUTANT_ENV: &str = "TASK3223_PUBLISH_INCOMPLETE";
const INCOMPLETE_PUBLISHED_FILE_RESULT: &str = "INCOMPLETE-PUBLISHED-FILE-3223-A";
const OBJECT_ID: &str = "0123456789abcdef0123456789abcdef";
const PAYLOAD: &[u8] = b"TASK-3223-COMPLETE-READABLE-FILE\n\
0123456789abcdef0123456789abcdef\n\
the publication barrier must never expose a prefix";
const PHASES: [&str; 4] = ["preparing", "uploading", "recording", "downloading"];

fn phase_root() -> PathBuf {
    PathBuf::from(std::env::var(ROOT_ENV).expect("TASK3223_ROOT is set for child"))
}

fn phase_name() -> String {
    std::env::var(PHASE_ENV).expect("TASK3223_PHASE is set for child")
}

fn marker(root: &Path, phase: &str) -> PathBuf {
    root.join(format!("entered-{phase}"))
}

fn write_marker(root: &Path, phase: &str) {
    fs::write(marker(root, phase), phase).expect("write phase marker");
}

fn source_file(root: &Path, bytes: &[u8]) -> File {
    let path = root.join("source.png");
    fs::write(&path, bytes).expect("write plaintext source fixture");
    File::open(path).expect("open plaintext source fixture")
}

fn sealed_fixture(root: &Path) -> PathBuf {
    let mut source = source_file(root, PAYLOAD);
    peer_attachment_io::encrypt_file(
        root,
        &mut source,
        "fixture.png",
        "image/png",
        Key::from_bytes([0x32; 32]),
        b"task-3223".to_vec(),
        0,
    )
    .expect("prepare sealed fixture")
    .path()
    .to_path_buf()
}

fn request(stream: &mut TcpStream) -> (String, Vec<u8>) {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let read = stream.read(&mut chunk).expect("read fixture request");
        assert_ne!(read, 0, "request ended before headers");
        bytes.extend_from_slice(&chunk[..read]);
        let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&bytes[..end]).into_owned();
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length").then(|| {
                    value
                        .trim()
                        .parse::<usize>()
                        .expect("numeric content length")
                })
            })
            .unwrap_or(0);
        while bytes.len() < end + 4 + content_length {
            let read = stream.read(&mut chunk).expect("read fixture request body");
            assert_ne!(read, 0, "request body ended early");
            bytes.extend_from_slice(&chunk[..read]);
        }
        return (headers, bytes[end + 4..end + 4 + content_length].to_vec());
    }
}

fn request_path(headers: &str) -> &str {
    headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .expect("request path")
}

fn respond(stream: &mut TcpStream, content_type: &str, body: &[u8]) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .expect("write fixture headers");
    stream.write_all(body).expect("write fixture body");
}

fn start_good_relay(root: &Path) -> (String, thread::JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind good relay");
    let address = listener.local_addr().expect("good relay address");
    let relay = root.join("relay");
    fs::create_dir_all(&relay).expect("create relay directory");
    let server = thread::spawn(move || {
        let (mut upload, _) = listener.accept().expect("accept upload");
        let (headers, body) = request(&mut upload);
        assert_eq!(request_path(&headers), "/v1/attachment");
        let temporary = relay.join("object.part");
        let complete = relay.join("object.complete");
        let mut file = File::create(&temporary).expect("create relay partial");
        file.write_all(&body).expect("write full relay object");
        file.sync_all().expect("sync full relay object");
        fs::rename(&temporary, &complete).expect("publish full relay object");
        respond(
            &mut upload,
            "application/json",
            format!(r#"{{"id":"{OBJECT_ID}","expires_at":1900003223}}"#).as_bytes(),
        );

        let (mut download, _) = listener.accept().expect("accept download");
        let (headers, body) = request(&mut download);
        assert!(body.is_empty());
        assert_eq!(
            request_path(&headers),
            format!("/v1/attachment/{OBJECT_ID}")
        );
        let stored = fs::read(&complete).expect("read published relay object");
        respond(&mut download, "application/octet-stream", &stored);
        usize::from(complete.is_file())
    });
    (format!("http://{address}"), server)
}

fn atomic_publish(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("published parent")).expect("create published dir");
    let temporary = path.with_extension("part");
    let mut output = File::create(&temporary).expect("create output partial");
    output.write_all(bytes).expect("write complete output");
    output.sync_all().expect("sync complete output");
    fs::rename(temporary, path).expect("publish complete output");
}

fn run_good(root: &Path) -> (usize, usize) {
    fs::create_dir_all(root).expect("create good root");
    let sealed = sealed_fixture(root);
    let expected_digest = peer_attachment_io::sha256_file(&sealed).expect("hash sealed fixture");
    let (base_url, relay) = start_good_relay(root);
    let client = CipherStoreClient::new(base_url).expect("build cipher-store client");
    let upload = client
        .upload_attachment_file(
            File::open(&sealed).expect("open sealed upload"),
            TTL_7D,
            &[0x23; 16],
        )
        .expect("upload complete attachment");
    assert_eq!(upload.id_hex, OBJECT_ID);

    let records = root.join("records");
    fs::create_dir_all(&records).expect("create records directory");
    atomic_publish(
        &records.join("attachment.json"),
        format!(
            r#"{{"object_id":"{OBJECT_ID}","size":{}}}"#,
            expected_digest.1
        )
        .as_bytes(),
    );

    let (download_path, mut download) =
        peer_attachment_io::create_download_file(root).expect("create download partial");
    let fetched = client
        .fetch_attachment_to_writer(OBJECT_ID, &[0x23; 16], &mut download)
        .expect("download complete attachment");
    download.sync_all().expect("sync downloaded attachment");
    drop(download);
    assert_eq!(fetched, expected_digest.1);
    assert_eq!(
        peer_attachment_io::sha256_file(&download_path).unwrap(),
        expected_digest
    );
    let mut sealed_download = File::open(&download_path).expect("open downloaded ciphertext");
    let plaintext = peer_attachment_io::decrypt_file_to_memory(
        &mut sealed_download,
        "fixture.png",
        "image/png",
        Key::from_bytes([0x32; 32]),
    )
    .expect("decrypt complete download");
    assert_eq!(&*plaintext, PAYLOAD);
    atomic_publish(&root.join("published/fixture.png"), &plaintext);
    peer_attachment_io::remove_staging_path_in_root(root, &download_path)
        .expect("remove downloaded partial");
    peer_attachment_io::remove_staging_path_in_root(root, &sealed).expect("remove prepared upload");
    let relay_complete = relay.join().expect("good relay exits cleanly");
    (complete_output_count(root), relay_complete)
}

fn child_preparing(root: &Path) -> ! {
    let path = root.join("large-source.png");
    File::create(&path)
        .expect("create sparse preparation fixture")
        .set_len(256 * 1024 * 1024)
        .expect("size sparse preparation fixture");
    let monitor_root = root.to_path_buf();
    thread::spawn(move || loop {
        let staging = monitor_root.join("peer-attachment-staging");
        if fs::read_dir(staging).ok().is_some_and(|entries| {
            entries.filter_map(Result::ok).any(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|value| value == "part")
            })
        }) {
            write_marker(&monitor_root, "preparing");
            return;
        }
        thread::sleep(Duration::from_millis(2));
    });
    let mut source = File::open(path).expect("open sparse preparation fixture");
    let _ = peer_attachment_io::encrypt_file(
        root,
        &mut source,
        "large.png",
        "image/png",
        Key::from_bytes([0x32; 32]),
        b"task-3223-preparing".to_vec(),
        0,
    );
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

fn child_uploading(root: &Path) -> ! {
    let sealed = sealed_fixture(root);
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind interrupted relay");
    let address = listener.local_addr().expect("interrupted relay address");
    let server_root = root.to_path_buf();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept interrupted upload");
        let mut header = Vec::new();
        let mut byte = [0_u8; 1];
        while !header.ends_with(b"\r\n\r\n") {
            stream
                .read_exact(&mut byte)
                .expect("read interrupted headers");
            header.push(byte[0]);
        }
        let relay = server_root.join("relay");
        fs::create_dir_all(&relay).expect("create interrupted relay");
        let mut partial = File::create(relay.join("object.part")).expect("relay partial");
        let mut body_prefix = [0_u8; 32];
        stream
            .read_exact(&mut body_prefix)
            .expect("read upload prefix");
        partial
            .write_all(&body_prefix)
            .expect("write upload prefix");
        partial.sync_all().expect("sync upload prefix");
        write_marker(&server_root, "uploading");
        while server_root.join("network.available").exists() {
            thread::sleep(Duration::from_millis(10));
        }
        drop(stream);
    });
    fs::write(root.join("network.available"), b"online").expect("mark network online");
    let client = CipherStoreClient::new(format!("http://{address}"))
        .expect("build interrupted cipher-store client");
    let _ = client.upload_attachment_file(
        File::open(sealed).expect("open interrupted upload"),
        TTL_7D,
        &[0x23; 16],
    );
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

fn child_recording(root: &Path) -> ! {
    let sealed = sealed_fixture(root);
    let relay = root.join("relay");
    fs::create_dir_all(&relay).expect("create recording relay");
    fs::copy(sealed, relay.join("object.complete")).expect("store complete remote object");
    let records = root.join("records");
    fs::create_dir_all(&records).expect("create recording directory");
    let mut partial = File::create(records.join("attachment.json.part")).expect("record partial");
    partial
        .write_all(br#"{"object_id":"0123"#)
        .expect("write record prefix");
    partial.sync_all().expect("sync record prefix");
    write_marker(root, "recording");
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

fn child_downloading(root: &Path) -> ! {
    let sealed = sealed_fixture(root);
    let relay = root.join("relay");
    fs::create_dir_all(&relay).expect("create download relay");
    fs::copy(&sealed, relay.join("object.complete")).expect("store complete remote object");
    let sealed_bytes = fs::read(sealed).expect("read remote ciphertext");
    let (download_path, mut download) =
        peer_attachment_io::create_download_file(root).expect("create download partial");
    download
        .write_all(&sealed_bytes[..sealed_bytes.len() / 2])
        .expect("write download prefix");
    download.sync_all().expect("sync download prefix");
    assert!(download_path.exists());
    write_marker(root, "downloading");
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

fn remove_if_present(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("remove {}: {error}", path.display()),
    }
}

fn recovery_child(root: &Path, phase: &str) {
    if std::env::var(MUTANT_ENV).ok().as_deref() == Some(phase) {
        let staging = root.join("peer-attachment-staging");
        if let Some(incomplete) = fs::read_dir(&staging).ok().and_then(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .find(|path| path.extension().is_some_and(|value| value == "part"))
        }) {
            fs::create_dir_all(root.join("published")).expect("create mutant output dir");
            fs::copy(incomplete, root.join("published/incomplete.bin"))
                .expect("mutant publishes incomplete file");
        }
    }
    peer_attachment_io::scavenge_staging_on_startup(root).expect("restart scavenges staging");
    remove_if_present(&root.join("relay/object.part"));
    remove_if_present(&root.join("records/attachment.json.part"));
    remove_if_present(&root.join("network.available"));
}

fn spawn_helper(test: &str, root: &Path, phase: &str) -> Child {
    let mut command = Command::new(std::env::current_exe().expect("current test executable"));
    command
        .arg("--ignored")
        .arg("--exact")
        .arg(test)
        .arg("--nocapture")
        .env(CHILD_ENV, "1")
        .env(ROOT_ENV, root)
        .env(PHASE_ENV, phase);
    if let Ok(mutant) = std::env::var(MUTANT_ENV) {
        command.env(MUTANT_ENV, mutant);
    }
    command.spawn().expect("spawn task 3223 helper")
}

fn wait_for_marker(child: &mut Child, root: &Path, phase: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if marker(root, phase).is_file() {
            return;
        }
        if let Some(status) = child.try_wait().expect("poll interruption child") {
            panic!("{phase} child exited before phase marker: {status}");
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!("{phase} child did not enter phase before deadline");
}

fn complete_output_count(root: &Path) -> usize {
    let published = root.join("published");
    let Ok(entries) = fs::read_dir(published) else {
        return 0;
    };
    let mut count = 0;
    for entry in entries {
        let path = entry.expect("read published entry").path();
        if path.is_file() {
            let bytes = fs::read(&path).expect("read published file");
            assert!(
                bytes == PAYLOAD,
                "{INCOMPLETE_PUBLISHED_FILE_RESULT} is the result that should have been refused: incomplete_published_file path={} published_bytes={} expected_bytes={}",
                path.display(),
                bytes.len(),
                PAYLOAD.len()
            );
            count += 1;
        }
    }
    count
}

fn count_parts(root: &Path) -> usize {
    fn visit(path: &Path, total: &mut usize) {
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                visit(&path, total);
            } else if path.extension().is_some_and(|value| value == "part") {
                *total += 1;
            }
        }
    }
    let mut total = 0;
    visit(root, &mut total);
    total
}

#[test]
fn task_3223_attack_40_interrupted_upload_never_publishes_a_partial_file() {
    if std::env::var_os(CHILD_ENV).is_some() {
        return;
    }
    let workspace = tempfile::tempdir().expect("task 3223 workspace");
    let good_root = workspace.path().join("good");
    let (good_files, relay_files) = run_good(&good_root);
    assert_eq!(good_files, 1, "good run stores exactly one complete file");
    assert_eq!(
        relay_files, 1,
        "good relay stores exactly one complete object"
    );
    println!("TASK3223 good_run complete_files={good_files} relay_complete_files={relay_files}");

    for phase in PHASES {
        let root = workspace.path().join(phase);
        fs::create_dir_all(&root).expect("create interrupted run root");
        let mut child = spawn_helper("task_3223_interruption_child", &root, phase);
        wait_for_marker(&mut child, &root, phase);
        remove_if_present(&root.join("network.available"));
        child.kill().expect("kill interrupted app process");
        let killed = child.wait().expect("reap interrupted app process");
        assert!(!killed.success(), "interrupted app must be killed");

        let status = spawn_helper("task_3223_recovery_child", &root, phase)
            .wait()
            .expect("wait for restarted app process");
        assert!(
            status.success(),
            "restart cleanup failed during {phase}: {status}"
        );
        let complete_files = complete_output_count(&root);
        let leftover_parts = count_parts(&root);
        assert!(
            complete_files <= 1,
            "{phase} stored more than one complete file"
        );
        assert_eq!(
            leftover_parts, 0,
            "{phase} restart left upload/download parts"
        );
        println!(
            "TASK3223 interrupted_phase={phase} network_cut=true app_killed=true app_restarted=true complete_files={complete_files} leftover_parts={leftover_parts}"
        );
    }
}

#[test]
#[ignore = "subprocess helper"]
fn task_3223_interruption_child() {
    let root = phase_root();
    match phase_name().as_str() {
        "preparing" => child_preparing(&root),
        "uploading" => child_uploading(&root),
        "recording" => child_recording(&root),
        "downloading" => child_downloading(&root),
        other => panic!("unknown TASK 3223 phase {other}"),
    }
}

#[test]
#[ignore = "subprocess helper"]
fn task_3223_recovery_child() {
    let root = phase_root();
    recovery_child(&root, &phase_name());
}
