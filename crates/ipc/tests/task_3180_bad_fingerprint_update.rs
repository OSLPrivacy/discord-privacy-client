use ipc::commands::{
    cmd_osl_install_site_update, SiteUpdateInstallRequest, SiteUpdateInstallResult,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

fn sha256_fingerprint(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    format!("sha256:{hex}")
}

fn installed_identity(install_path: &Path, version_path: &Path) -> (String, String) {
    (
        std::fs::read_to_string(version_path)
            .expect("read installed version")
            .trim()
            .to_owned(),
        sha256_fingerprint(&std::fs::read(install_path).expect("read installed bytes")),
    )
}

fn count_downloaded_files(staging_dir: &Path) -> usize {
    std::fs::read_dir(staging_dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter(|entry| {
            entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
        })
        .count()
}

fn spawn_update_site(
    routes: HashMap<String, Vec<u8>>,
    requests: usize,
) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind update fixture");
    let base = format!("http://{}", listener.local_addr().expect("local addr"));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_for_thread = Arc::clone(&seen);
    thread::spawn(move || {
        for _ in 0..requests {
            let (mut stream, _) = listener.accept().expect("accept update request");
            let mut buffer = [0u8; 2048];
            let read = stream.read(&mut buffer).expect("read request");
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/")
                .to_owned();
            seen_for_thread
                .lock()
                .expect("seen lock")
                .push(path.clone());
            let (status, reason, body) = match routes.get(&path) {
                Some(body) => (200, "OK", body.clone()),
                None => (404, "Not Found", b"not found".to_vec()),
            };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\n\r\n",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response headers");
            stream.write_all(&body).expect("write response body");
        }
    });
    (base, seen)
}

#[test]
fn task_3180_bad_fingerprint_update_is_refused_and_matching_update_installs() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let install_path = temp.path().join("installed").join("OSL Privacy.exe");
    let version_path = temp.path().join("installed").join("VERSION");
    let staging_dir = temp.path().join("downloads");
    std::fs::create_dir_all(install_path.parent().expect("install parent"))
        .expect("create installed dir");
    std::fs::create_dir_all(&staging_dir).expect("create staging dir");

    let old_bytes = b"installed OSL version 0.1.0".to_vec();
    std::fs::write(&install_path, &old_bytes).expect("seed installed bytes");
    std::fs::write(&version_path, b"0.1.0").expect("seed installed version");

    let good_bytes = b"installed OSL version 0.2.0 with verified bytes".to_vec();
    let mut bad_bytes = good_bytes.clone();
    bad_bytes[0] ^= 0x01;
    let expected_good_fingerprint = sha256_fingerprint(&good_bytes);
    let routes = HashMap::from([
        ("/bad-update.bin".to_owned(), bad_bytes),
        ("/good-update.bin".to_owned(), good_bytes),
    ]);
    let (base, seen) = spawn_update_site(routes, 2);

    let before_bad = installed_identity(&install_path, &version_path);
    assert_eq!(count_downloaded_files(&staging_dir), 0);
    let bad_result = cmd_osl_install_site_update(SiteUpdateInstallRequest {
        version: "0.2.0".to_owned(),
        download_address: format!("{base}/bad-update.bin"),
        expected_fingerprint: expected_good_fingerprint.clone(),
        install_path: install_path.clone(),
        installed_version_path: version_path.clone(),
        staging_dir: staging_dir.clone(),
    });
    let after_bad = installed_identity(&install_path, &version_path);
    assert_eq!(before_bad, after_bad);
    let SiteUpdateInstallResult::Refused {
        message: bad_message,
        version: bad_version,
        fingerprint: bad_fingerprint,
        downloaded_file_count_during: bad_downloaded_during,
        downloaded_file_count_after: bad_downloaded_after,
    } = bad_result
    else {
        panic!("bad update should be refused");
    };
    assert!(bad_message.contains("fingerprint mismatch"));
    assert_eq!(bad_version, before_bad.0);
    assert_eq!(bad_fingerprint, before_bad.1);
    assert_eq!(bad_downloaded_during, 1);
    assert_eq!(bad_downloaded_after, 0);
    assert_eq!(count_downloaded_files(&staging_dir), 0);

    let good_result = cmd_osl_install_site_update(SiteUpdateInstallRequest {
        version: "0.2.0".to_owned(),
        download_address: format!("{base}/good-update.bin"),
        expected_fingerprint: expected_good_fingerprint.clone(),
        install_path: install_path.clone(),
        installed_version_path: version_path.clone(),
        staging_dir: staging_dir.clone(),
    });
    let after_good = installed_identity(&install_path, &version_path);
    let SiteUpdateInstallResult::Installed {
        version: good_version,
        fingerprint: good_fingerprint,
        downloaded_file_count_during: good_downloaded_during,
        downloaded_file_count_after: good_downloaded_after,
    } = good_result
    else {
        panic!("matching update should install");
    };
    assert_eq!(good_version, "0.2.0");
    assert_eq!(good_fingerprint, expected_good_fingerprint);
    assert_eq!(after_good, (good_version.clone(), good_fingerprint.clone()));
    assert_eq!(good_downloaded_during, 1);
    assert_eq!(good_downloaded_after, 0);
    assert_eq!(count_downloaded_files(&staging_dir), 0);
    assert_eq!(
        seen.lock().expect("seen lock").as_slice(),
        ["/bad-update.bin", "/good-update.bin"]
    );

    println!(
        "TASK3180_BAD_BEFORE version={} fingerprint={}",
        before_bad.0, before_bad.1
    );
    println!(
        "TASK3180_BAD_RESULT status=refused message=\"{}\" downloaded_file_count_during={} downloaded_file_count_after={}",
        bad_message, bad_downloaded_during, bad_downloaded_after
    );
    println!(
        "TASK3180_BAD_AFTER version={} fingerprint={}",
        after_bad.0, after_bad.1
    );
    println!(
        "TASK3180_GOOD_RESULT status=installed version={} fingerprint={} downloaded_file_count_during={} downloaded_file_count_after={}",
        good_version, good_fingerprint, good_downloaded_during, good_downloaded_after
    );
    println!(
        "TASK3180_GOOD_AFTER version={} fingerprint={}",
        after_good.0, after_good.1
    );
    println!(
        "TASK3180_REQUESTS={}",
        seen.lock().expect("seen lock").join(",")
    );
}
