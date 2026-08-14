use osl_privacy_hub::osl_chat_alert_words::{AlertWordStore, PRIVACY_STATEMENT};
use std::io::{ErrorKind, Read};
use std::net::TcpListener;
use std::time::{Duration, Instant};

struct FileKeyGuard(Option<[u8; 32]>);

impl FileKeyGuard {
    fn install(key: [u8; 32]) -> Self {
        let previous = ipc::main_password::get_file_storage_key();
        ipc::main_password::set_file_storage_key(Some(key));
        Self(previous)
    }
}

impl Drop for FileKeyGuard {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(self.0);
    }
}

#[test]
fn task_5030_alert_words_are_exact_durable_and_device_local() {
    let _file_key = FileKeyGuard::install([0x50; 32]);
    let temporary = tempfile::tempdir().expect("create local settings directory");
    let settings_path = temporary.path().join("alert-words.json");
    let chat_id = "chat/../../still-a-map-key";
    let saved = vec![
        "Orchid".to_owned(),
        "lantern".to_owned(),
        "Cobalt_7".to_owned(),
    ];

    let mut store = AlertWordStore::open(&settings_path).expect("open empty local store");
    store
        .save_words(chat_id, saved.clone())
        .expect("save three alert words");
    let sealed_settings = std::fs::read(&settings_path).expect("read sealed local settings");
    assert!(ipc::main_password::has_enc_magic(&sealed_settings));
    assert!(saved.iter().all(|word| !sealed_settings
        .windows(word.len())
        .any(|window| { window.eq_ignore_ascii_case(word.as_bytes()) })));

    let fixtures = [
        "The ORCHID is flowering.",
        "Bring a lantern tonight",
        "The garden is quiet.",
        "Cobalt paint is blue.",
        "We arrived before noon.",
        "A paper lamp glowed.",
        "No special token here.",
    ];
    let fixture_results: Vec<bool> = fixtures
        .iter()
        .map(|text| {
            store
                .matches_decrypted_text(chat_id, text)
                .expect("match already-decrypted fixture")
        })
        .collect();
    let fixture_alerts = fixture_results.iter().filter(|&&matched| matched).count();
    let fixture_nonalerts = fixture_results.len() - fixture_alerts;
    assert_eq!(fixture_alerts, 2);
    assert_eq!(fixture_nonalerts, 5);

    let substring_alerts = usize::from(
        store
            .matches_decrypted_text(chat_id, "orchidaceous lanternfish precobalt_7x")
            .expect("match substring-only fixture"),
    );
    assert_eq!(substring_alerts, 0);

    drop(store);
    let restarted = AlertWordStore::open(&settings_path).expect("reopen store after restart");
    assert_eq!(restarted.words_for_chat(chat_id).unwrap(), saved.as_slice());

    // The capture socket is live during matching. The matcher receives neither
    // its address nor a relay handle, so no connection and no bytes can result.
    let relay = TcpListener::bind("127.0.0.1:0").expect("bind loopback capture relay");
    relay.set_nonblocking(true).unwrap();
    let capture_started = Instant::now();
    for text in fixtures {
        let _ = restarted
            .matches_decrypted_text(chat_id, text)
            .expect("match while relay capture is active");
    }

    let mut relay_bytes = Vec::new();
    while capture_started.elapsed() < Duration::from_millis(50) {
        match relay.accept() {
            Ok((mut stream, _)) => {
                stream
                    .set_read_timeout(Some(Duration::from_millis(10)))
                    .unwrap();
                match stream.read_to_end(&mut relay_bytes) {
                    Ok(_) => {}
                    Err(error)
                        if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                    Err(error) => panic!("capture relay read failed: {error}"),
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => std::thread::yield_now(),
            Err(error) => panic!("capture relay accept failed: {error}"),
        }
    }
    let word_list_bytes: usize = saved
        .iter()
        .map(|word| {
            if relay_bytes
                .windows(word.len())
                .any(|window| window.eq_ignore_ascii_case(word.as_bytes()))
            {
                word.len()
            } else {
                0
            }
        })
        .sum();
    assert_eq!(relay_bytes.len(), 0);
    assert_eq!(word_list_bytes, 0);

    assert_eq!(
        PRIVACY_STATEMENT,
        "Matching runs on this device against already-decrypted text. The list never leaves the machine."
    );

    println!("TASK_5030_FIXTURE_ALERTS={fixture_alerts}");
    println!("TASK_5030_FIXTURE_NONALERTS={fixture_nonalerts}");
    println!("TASK_5030_SUBSTRING_ALERTS={substring_alerts}");
    println!("TASK_5030_RESTART_WORDS={}", saved.len());
    println!("TASK_5030_RESTART_WORDS_EXACT={}", saved.join("|"));
    println!("TASK_5030_RELAY_TOTAL_BYTES={}", relay_bytes.len());
    println!("TASK_5030_RELAY_WORD_LIST_BYTES={word_list_bytes}");
    println!("TASK_5030_PRIVACY_STATEMENT={PRIVACY_STATEMENT}");
}
