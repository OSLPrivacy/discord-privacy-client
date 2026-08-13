use ipc::app_preferences::{load_app_preferences, AppPreferences};
use ipc::commands::{
    cmd_osl_answer_discovery_ping, cmd_osl_publish_discovery_card, cmd_osl_query_discovery_card,
    cmd_osl_read_discovery_replies_switch, cmd_osl_resume_discovery_off_transition,
    cmd_osl_save_discovery_setting, cmd_osl_set_discovery_replies_switch,
};
use ipc::state::AppState;
use keystore::client::DiscoveryCardResponse;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const TARGET_ACCOUNT: &str = "task-4764-TARGET-account";
const PROTECTED_ACCOUNT: &str = "task-4764-PROTECTED-account";
const CURRENT_WEEK: &str = "2026-W33";
const PRIOR_WEEK: &str = "2026-W32";
const CHILD_ENV: &str = "OSL_TASK_4764_OS_RESTART_CHILD";
const CHILD_SERVER_ENV: &str = "OSL_TASK_4764_SERVER";
const CHILD_PREFS_ENV: &str = "OSL_TASK_4764_PREFS_DIR";

#[derive(Clone, Debug)]
struct StoredCard {
    owner: String,
    card: DiscoveryCardResponse,
}

#[derive(Default)]
struct ServiceData {
    enabled: HashMap<String, bool>,
    cards: HashMap<(String, String), StoredCard>,
    enables: usize,
    publishes: usize,
    take_backs: usize,
    reads: usize,
    last_removed: usize,
}

#[derive(Default)]
struct GateState {
    armed: bool,
    entered: bool,
    released: bool,
}

#[derive(Default)]
struct Gate {
    state: Mutex<GateState>,
    changed: Condvar,
}

impl Gate {
    fn arm(&self) {
        *self.state.lock().expect("gate lock") = GateState {
            armed: true,
            entered: false,
            released: false,
        };
    }

    fn block_if_armed(&self) {
        let mut state = self.state.lock().expect("gate lock");
        if !state.armed {
            return;
        }
        state.entered = true;
        self.changed.notify_all();
        while !state.released {
            state = self.changed.wait(state).expect("gate wait");
        }
        state.armed = false;
    }

    fn wait_until_entered(&self, name: &str) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut state = self.state.lock().expect("gate lock");
        while !state.entered {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "{name} did not reach its barrier");
            let (next, timeout) = self
                .changed
                .wait_timeout(state, remaining)
                .expect("gate timed wait");
            state = next;
            assert!(
                state.entered || !timeout.timed_out(),
                "{name} barrier timed out"
            );
        }
    }

    fn release(&self) {
        let mut state = self.state.lock().expect("gate lock");
        state.released = true;
        self.changed.notify_all();
    }
}

#[derive(Default)]
struct ServiceState {
    data: Mutex<ServiceData>,
    publish_before_commit: Gate,
    take_back_before_commit: Gate,
    take_back_after_commit: Gate,
}

impl ServiceState {
    fn seed(&self, cards: &[StoredCard]) {
        let mut data = self.data.lock().expect("service data lock");
        for stored in cards {
            data.cards.insert(
                (stored.card.drawer_name.clone(), stored.card.label.clone()),
                stored.clone(),
            );
        }
    }

    fn owner_count(&self, owner: &str) -> usize {
        self.data
            .lock()
            .expect("service data lock")
            .cards
            .values()
            .filter(|stored| stored.owner == owner)
            .count()
    }
}

struct LoopbackKeyServer {
    base_url: String,
    state: Arc<ServiceState>,
    stop: Arc<Mutex<bool>>,
    server: Option<JoinHandle<()>>,
}

impl LoopbackKeyServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind task 4764 key server");
        listener
            .set_nonblocking(true)
            .expect("make task 4764 listener nonblocking");
        let address = listener.local_addr().expect("key server address");
        let state = Arc::new(ServiceState::default());
        let stop = Arc::new(Mutex::new(false));
        let server_state = Arc::clone(&state);
        let server_stop = Arc::clone(&stop);
        let server = thread::spawn(move || {
            let mut workers = Vec::new();
            loop {
                if *server_stop.lock().expect("stop lock") {
                    break;
                }
                match listener.accept() {
                    Ok((stream, _)) => {
                        let request_state = Arc::clone(&server_state);
                        workers.push(thread::spawn(move || {
                            handle_request(stream, &request_state)
                        }));
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("accept task 4764 request: {error}"),
                }
            }
            for worker in workers {
                worker.join().expect("task 4764 request worker exits");
            }
        });
        Self {
            base_url: format!("http://{address}"),
            state,
            stop,
            server: Some(server),
        }
    }

    fn client(&self) -> keystore::KeyServerClient {
        keystore::KeyServerClient::new(&self.base_url).expect("task 4764 client")
    }
}

impl Drop for LoopbackKeyServer {
    fn drop(&mut self) {
        *self.stop.lock().expect("stop lock") = true;
        if let Some(server) = self.server.take() {
            server.join().expect("task 4764 server exits");
        }
    }
}

fn request_body(mut stream: &TcpStream) -> (String, String) {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set request timeout");
    let mut request = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let read = stream.read(&mut chunk).expect("read task 4764 request");
        assert_ne!(read, 0, "request ended before complete body");
        request.extend_from_slice(&chunk[..read]);
        let Some(headers_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..headers_end]);
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
        if request.len() < headers_end + 4 + content_length {
            continue;
        }
        let request_line = headers.lines().next().expect("request line").to_owned();
        let body =
            String::from_utf8(request[headers_end + 4..headers_end + 4 + content_length].to_vec())
                .expect("request body UTF-8");
        return (request_line, body);
    }
}

fn write_json(mut stream: &TcpStream, status: u16, body: Value) {
    let text = body.to_string();
    let reason = match status {
        200 => "OK",
        201 => "Created",
        404 => "Not Found",
        409 => "Conflict",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    )
    .expect("write task 4764 response");
}

fn string_field(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("missing string field {field}"))
        .to_owned()
}

fn card_json(card: &DiscoveryCardResponse) -> Value {
    json!({
        "drawer_name": card.drawer_name,
        "label": card.label,
        "sealed_note": card.sealed_note,
        "discovery_epoch": card.discovery_epoch,
    })
}

fn handle_request(stream: TcpStream, state: &ServiceState) {
    let (request_line, body) = request_body(&stream);
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default();
    let path = request_parts.next().unwrap_or_default();
    assert_eq!(method, "POST", "all task 4764 release routes are POST");
    let value: Value = serde_json::from_str(&body).expect("task 4764 request JSON");
    match path {
        "/v1/discovery-cards/enable-replies" => {
            let account = string_field(&value, "account_id");
            let mut data = state.data.lock().expect("service data lock");
            data.enabled.insert(account, true);
            data.enables += 1;
            write_json(&stream, 200, json!({ "enabled": true, "removed": 0 }));
        }
        "/v1/discovery-cards/publish" => {
            state.publish_before_commit.block_if_armed();
            let account = string_field(&value, "account_id");
            let app = string_field(&value, "app_id");
            let handle = string_field(&value, "account_handle");
            let sealed_note = string_field(&value, "sealed_note");
            let mut data = state.data.lock().expect("service data lock");
            data.publishes += 1;
            if !data.enabled.get(&account).copied().unwrap_or(true) {
                write_json(
                    &stream,
                    409,
                    json!({ "error": "discovery replies are off", "removed": 0, "wrote": 0 }),
                );
                return;
            }
            let drawer_name = match app.as_str() {
                "discord" => "d01".to_owned(),
                "signal" => "d02".to_owned(),
                _ => "d99".to_owned(),
            };
            let card = DiscoveryCardResponse {
                drawer_name,
                label: format!("{handle}-current-live"),
                sealed_note,
                discovery_epoch: CURRENT_WEEK.to_owned(),
            };
            data.cards.insert(
                (card.drawer_name.clone(), card.label.clone()),
                StoredCard {
                    owner: account,
                    card: card.clone(),
                },
            );
            write_json(
                &stream,
                201,
                json!({ "removed": 0, "wrote": 1, "card": card_json(&card) }),
            );
        }
        "/v1/discovery-cards/take-back" => {
            state.take_back_before_commit.block_if_armed();
            let account = string_field(&value, "account_id");
            let removed = {
                let mut data = state.data.lock().expect("service data lock");
                data.take_backs += 1;
                data.enabled.insert(account.clone(), false);
                let owned: Vec<(String, String)> = data
                    .cards
                    .iter()
                    .filter_map(|(key, stored)| (stored.owner == account).then(|| key.clone()))
                    .collect();
                for key in &owned {
                    data.cards.remove(key);
                }
                data.last_removed = owned.len();
                owned.len()
            };
            state.take_back_after_commit.block_if_armed();
            write_json(
                &stream,
                200,
                json!({ "enabled": false, "removed": removed }),
            );
        }
        "/v1/discovery-cards/read" => {
            let drawer = string_field(&value, "drawer_name");
            let label = string_field(&value, "label");
            let card = {
                let mut data = state.data.lock().expect("service data lock");
                data.reads += 1;
                data.cards
                    .get(&(drawer, label))
                    .map(|stored| stored.card.clone())
            };
            match card {
                Some(card) => write_json(&stream, 200, card_json(&card)),
                None => write_json(&stream, 404, json!({ "error": "not found" })),
            }
        }
        other => panic!("unexpected task 4764 route {other}"),
    }
}

fn retained_cards(owner: &str, kind: &str) -> Vec<StoredCard> {
    [("d01", "discord"), ("d02", "signal")]
        .into_iter()
        .flat_map(|(drawer, app)| {
            [("current", CURRENT_WEEK), ("prior", PRIOR_WEEK)]
                .into_iter()
                .map(move |(week_name, epoch)| StoredCard {
                    owner: owner.to_owned(),
                    card: DiscoveryCardResponse {
                        drawer_name: drawer.to_owned(),
                        label: format!("{kind}-{app}-{week_name}-independent-label"),
                        sealed_note: format!("{kind}:{app}:{week_name}:BYTE-EXACT"),
                        discovery_epoch: epoch.to_owned(),
                    },
                })
        })
        .collect()
}

fn racing_target_card() -> StoredCard {
    StoredCard {
        owner: TARGET_ACCOUNT.to_owned(),
        card: DiscoveryCardResponse {
            drawer_name: "d01".to_owned(),
            label: "TARGET-racing-publish-current-live".to_owned(),
            sealed_note: "TARGET:RACING:BYTE-EXACT".to_owned(),
            discovery_epoch: CURRENT_WEEK.to_owned(),
        },
    }
}

fn all_withdrawn_target_cards() -> Vec<StoredCard> {
    let mut cards = retained_cards(TARGET_ACCOUNT, "TARGET");
    cards.push(racing_target_card());
    cards
}

fn configured_state(
    identity: keystore::Identity,
    client: keystore::KeyServerClient,
    prefs: AppPreferences,
) -> Arc<AppState> {
    let state = Arc::new(AppState::new());
    state.install_identity(identity);
    *state.keyserver_slot() = Some(client);
    *state.app_preferences.lock().expect("app preferences lock") = prefs;
    state
}

fn read_all(
    state: &AppState,
    expected: &[StoredCard],
    should_exist: bool,
    transition: &str,
) -> usize {
    let mut matches = 0;
    for (index, stored) in expected.iter().enumerate() {
        let found = cmd_osl_query_discovery_card(
            state,
            stored.card.drawer_name.clone(),
            stored.card.label.clone(),
        )
        .unwrap_or_else(|error| {
            panic!(
                "query failed card={} drawer={} transition={transition}: {error}",
                stored.card.label, stored.card.drawer_name
            )
        });
        if should_exist {
            let found = found.unwrap_or_else(|| {
                panic!(
                    "missing card={} drawer={} transition={transition}",
                    stored.card.label, stored.card.drawer_name
                )
            });
            assert_eq!(
                found, stored.card,
                "changed card={} drawer={} transition={transition}",
                stored.card.label, stored.card.drawer_name
            );
            matches += 1;
            println!(
                "TASK4764 protected_card={index} card={} drawer={} transition={transition} bytes={} queryable=1",
                stored.card.label, stored.card.drawer_name, found.sealed_note
            );
        } else {
            assert!(
                found.is_none(),
                "retained TARGET card={} drawer={} transition={transition}",
                stored.card.label,
                stored.card.drawer_name
            );
            println!(
                "TASK4764 target_card={index} card={} drawer={} transition={transition} matches=0",
                stored.card.label, stored.card.drawer_name
            );
        }
    }
    matches
}

fn child_os_restart_check() {
    let server = std::env::var(CHILD_SERVER_ENV).expect("child server URL");
    let prefs_dir = PathBuf::from(std::env::var(CHILD_PREFS_ENV).expect("child preferences dir"));
    let prefs = load_app_preferences(&prefs_dir.join("app_preferences.json"));
    let identity = keystore::generate_identity(TARGET_ACCOUNT.to_owned());
    let client = keystore::KeyServerClient::new(server).expect("child key server client");
    let state = configured_state(identity, client, prefs);
    let target = all_withdrawn_target_cards();

    let displayed = cmd_osl_read_discovery_replies_switch(&state).expect("child read switch");
    let publish = cmd_osl_publish_discovery_card(
        &state,
        "discord".to_owned(),
        "child-hostile".to_owned(),
        "TARGET:CHILD:HOSTILE".to_owned(),
    )
    .expect("child hostile publish is locally refused");
    let reply = cmd_osl_answer_discovery_ping(&state).expect("child ping");
    let matches = read_all(&state, &target, false, "os-restart");
    assert_eq!(displayed, "off");
    assert_eq!(publish.cards_written, 0);
    assert_eq!(reply.len(), 0);
    assert_eq!(matches, 0);
    println!(
        "TASK4764 os_restart displayed={displayed} hostile_publish={} target_matches={matches} ping_reply_bytes={}",
        publish.cards_written,
        reply.len()
    );
}

fn restore_pending_file(path: &Path, pending_bytes: &[u8]) {
    std::fs::remove_dir(path).expect("remove persistence-failure directory");
    std::fs::write(path, pending_bytes).expect("restore durable pending preference bytes");
}

#[test]
fn task_4764_master_off_withdraws_live_retained_cards_atomically_and_survives_restarts() {
    if std::env::var_os(CHILD_ENV).is_some() {
        child_os_restart_check();
        return;
    }

    let server = LoopbackKeyServer::start();
    let prefs_dir = tempfile::tempdir().expect("task 4764 preferences directory");
    let prefs_path = prefs_dir.path().join("app_preferences.json");
    let target_identity = keystore::generate_identity(TARGET_ACCOUNT.to_owned());
    let protected_identity = keystore::generate_identity(PROTECTED_ACCOUNT.to_owned());
    let state = configured_state(
        target_identity.clone(),
        server.client(),
        AppPreferences::default(),
    );

    cmd_osl_save_discovery_setting(
        &state,
        "anyone".to_owned(),
        Some(prefs_dir.path().to_path_buf()),
    )
    .expect("configure discovery matching");
    let enabled = cmd_osl_set_discovery_replies_switch(
        &state,
        "on".to_owned(),
        Some(prefs_dir.path().to_path_buf()),
    )
    .expect("shipping master switch starts enabled");
    server
        .client()
        .enable_discovery_replies(&protected_identity)
        .expect("enable protected account");

    let target = retained_cards(TARGET_ACCOUNT, "TARGET");
    let withdrawn_target = all_withdrawn_target_cards();
    let protected = retained_cards(PROTECTED_ACCOUNT, "PROTECTED");
    server.state.seed(&target);
    server.state.seed(&protected);
    assert_eq!(enabled, "on");
    assert_eq!(server.state.owner_count(TARGET_ACCOUNT), 4);
    assert_eq!(server.state.owner_count(PROTECTED_ACCOUNT), 4);
    assert_eq!(
        read_all(&state, &target, true, "enabled-target-baseline"),
        4
    );
    assert_eq!(
        read_all(&state, &protected, true, "enabled-protected-baseline"),
        4
    );
    println!(
        "TASK4764 shipping_client=enabled settings_entries=1 target_live=4 protected_live=4 drawers=2 retained_weeks=2 weeks={CURRENT_WEEK},{PRIOR_WEEK}"
    );

    // The racing publish owns the client transition lock while its key-server
    // transaction is barrier-starved. Off must wait, then withdraw that newly
    // committed card together with all four retained TARGET cards.
    server.state.publish_before_commit.arm();
    let racing_state = Arc::clone(&state);
    let racing_publish = thread::spawn(move || {
        cmd_osl_publish_discovery_card(
            &racing_state,
            "discord".to_owned(),
            "TARGET-racing-publish".to_owned(),
            "TARGET:RACING:BYTE-EXACT".to_owned(),
        )
    });
    server
        .state
        .publish_before_commit
        .wait_until_entered("racing publish");
    server.state.take_back_before_commit.arm();
    let off_state = Arc::clone(&state);
    let off_dir = prefs_dir.path().to_path_buf();
    let off = thread::spawn(move || {
        cmd_osl_set_discovery_replies_switch(&off_state, "off".to_owned(), Some(off_dir))
    });
    thread::sleep(Duration::from_millis(30));
    assert!(
        !off.is_finished(),
        "off overtook the barrier-starved publish"
    );
    server.state.publish_before_commit.release();
    let racing_report = racing_publish
        .join()
        .expect("racing publish thread")
        .expect("racing publish commits before off");
    assert_eq!(racing_report.cards_written, 1);
    server
        .state
        .take_back_before_commit
        .wait_until_entered("take-back before commit");

    let during = cmd_osl_read_discovery_replies_switch(&state).expect("read during take-back");
    let durable_during = load_app_preferences(&prefs_path);
    assert_eq!(during, "turning_off");
    assert!(durable_during.discovery_off_pending);
    assert_eq!(durable_during.discovery_replies.as_str(), "on");
    assert_eq!(server.state.owner_count(TARGET_ACCOUNT), 5);
    assert!(
        !off.is_finished(),
        "off displayed before withdrawal committed"
    );
    println!(
        "TASK4764 settings_entry=privacy.master-switch transition=enabled-to-off display_before_commit={during} displayed_off_before_withdrawal_commit=0 racing_publish_committed=1 target_live_before_commit=5"
    );

    server.state.take_back_before_commit.release();
    let off_result = off
        .join()
        .expect("off transition thread")
        .expect("off transition succeeds");
    assert_eq!(off_result, "off");
    assert_eq!(
        cmd_osl_read_discovery_replies_switch(&state).unwrap(),
        "off"
    );
    assert_eq!(server.state.owner_count(TARGET_ACCOUNT), 0);
    assert_eq!(server.state.owner_count(PROTECTED_ACCOUNT), 4);
    assert_eq!(server.state.data.lock().unwrap().last_removed, 5);
    let target_matches = read_all(&state, &withdrawn_target, false, "first-off");
    let protected_matches = read_all(&state, &protected, true, "first-off");
    let ping_bytes: usize = (0..3)
        .map(|_| {
            cmd_osl_answer_discovery_ping(&state)
                .expect("hostile ping")
                .len()
        })
        .sum();
    let hostile = cmd_osl_publish_discovery_card(
        &state,
        "signal".to_owned(),
        "TARGET-hostile-after-off".to_owned(),
        "TARGET:HOSTILE:BYTE-EXACT".to_owned(),
    )
    .expect("off locally refuses hostile publish");
    assert_eq!(target_matches, 0);
    assert_eq!(protected_matches, 4);
    assert_eq!(ping_bytes, 0);
    assert_eq!(hostile.cards_written, 0);
    println!(
        "TASK4764 transition=enabled-to-off removed=5 target_live=0 protected_live=4 withdrawn_target_cards_checked=5 old_drawer_target_matches={target_matches} fresh_pings=3 ping_reply_bytes={ping_bytes} hostile_publish_cards={} displayed={off_result}",
        hostile.cards_written
    );

    // A fresh AppState is the app-process restart boundary. Loaded off state
    // gates publish and reply paths without relying on the old in-memory state.
    let restarted = configured_state(
        target_identity.clone(),
        server.client(),
        load_app_preferences(&prefs_path),
    );
    let app_restart_display =
        cmd_osl_read_discovery_replies_switch(&restarted).expect("app restart read");
    let app_restart_publish = cmd_osl_publish_discovery_card(
        &restarted,
        "discord".to_owned(),
        "TARGET-app-restart".to_owned(),
        "TARGET:APP-RESTART".to_owned(),
    )
    .expect("app restart publish refusal");
    let app_restart_ping = cmd_osl_answer_discovery_ping(&restarted).expect("app restart ping");
    assert_eq!(app_restart_display, "off");
    assert_eq!(app_restart_publish.cards_written, 0);
    assert_eq!(app_restart_ping.len(), 0);
    assert_eq!(
        read_all(&restarted, &withdrawn_target, false, "app-restart"),
        0
    );
    assert_eq!(read_all(&restarted, &protected, true, "app-restart"), 4);
    println!(
        "TASK4764 app_restart displayed={app_restart_display} target_matches=0 protected_matches=4 hostile_publish={} ping_reply_bytes={} ",
        app_restart_publish.cards_written,
        app_restart_ping.len()
    );

    // Simulate the exact crash window after remote withdrawal but before the
    // final local `off` persistence. A directory at the destination makes the
    // atomic rename fail after the server has committed. Startup must observe
    // durable pending intent, refuse work, and idempotently resume to off.
    cmd_osl_set_discovery_replies_switch(
        &restarted,
        "on".to_owned(),
        Some(prefs_dir.path().to_path_buf()),
    )
    .expect("re-enable for pending-off crash fixture");
    server.state.seed(&target);
    assert_eq!(server.state.owner_count(TARGET_ACCOUNT), 4);
    server.state.take_back_after_commit.arm();
    let failing_state = Arc::clone(&restarted);
    let failing_dir = prefs_dir.path().to_path_buf();
    let failing_off = thread::spawn(move || {
        cmd_osl_set_discovery_replies_switch(&failing_state, "off".to_owned(), Some(failing_dir))
    });
    server
        .state
        .take_back_after_commit
        .wait_until_entered("take-back after commit");
    assert_eq!(server.state.owner_count(TARGET_ACCOUNT), 0);
    assert_eq!(server.state.owner_count(PROTECTED_ACCOUNT), 4);
    let pending_bytes = std::fs::read(&prefs_path).expect("read durable pending bytes");
    let pending = load_app_preferences(&prefs_path);
    assert!(pending.discovery_off_pending);
    std::fs::remove_file(&prefs_path).expect("remove preference file for failure injection");
    std::fs::create_dir(&prefs_path).expect("block final preference rename");
    server.state.take_back_after_commit.release();
    let persistence_error = failing_off
        .join()
        .expect("failing off thread")
        .expect_err("final local persistence must fail");
    assert!(persistence_error.contains("rename"), "{persistence_error}");
    assert_eq!(
        cmd_osl_read_discovery_replies_switch(&restarted).unwrap(),
        "turning_off"
    );
    restore_pending_file(&prefs_path, &pending_bytes);
    println!(
        "TASK4764 transition=withdrawal-committed-local-persist-failed remote_target_live=0 protected_live=4 display=turning_off error={persistence_error:?}"
    );

    let resumed = configured_state(
        target_identity.clone(),
        server.client(),
        load_app_preferences(&prefs_path),
    );
    assert_eq!(
        cmd_osl_read_discovery_replies_switch(&resumed).unwrap(),
        "turning_off"
    );
    assert_eq!(cmd_osl_answer_discovery_ping(&resumed).unwrap().len(), 0);
    let pending_publish = cmd_osl_publish_discovery_card(
        &resumed,
        "discord".to_owned(),
        "TARGET-pending-restart".to_owned(),
        "TARGET:PENDING:RESTART".to_owned(),
    )
    .expect("pending restart publish refusal");
    assert_eq!(pending_publish.cards_written, 0);
    let resumed_off =
        cmd_osl_resume_discovery_off_transition(&resumed, Some(prefs_dir.path().to_path_buf()))
            .expect("resume pending off after restart");
    assert_eq!(resumed_off, "off");
    assert_eq!(server.state.owner_count(TARGET_ACCOUNT), 0);
    assert_eq!(server.state.owner_count(PROTECTED_ACCOUNT), 4);
    assert_eq!(
        read_all(&resumed, &withdrawn_target, false, "pending-off-resume"),
        0
    );
    assert_eq!(
        read_all(&resumed, &protected, true, "pending-off-resume"),
        4
    );
    println!(
        "TASK4764 restart_resume transition=pending-to-off target_live=0 protected_live=4 refused_publish={} ping_reply_bytes=0 displayed={resumed_off}",
        pending_publish.cards_written
    );

    let child = Command::new(std::env::current_exe().expect("current integration-test binary"))
        .arg("--exact")
        .arg("task_4764_master_off_withdraws_live_retained_cards_atomically_and_survives_restarts")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env(CHILD_ENV, "1")
        .env(CHILD_SERVER_ENV, &server.base_url)
        .env(CHILD_PREFS_ENV, prefs_dir.path())
        .output()
        .expect("spawn task 4764 OS-restart child");
    assert!(
        child.status.success(),
        "OS-restart child failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    let child_stdout = String::from_utf8(child.stdout).expect("child stdout UTF-8");
    assert!(child_stdout.contains("TASK4764 os_restart displayed=off"));
    print!("{child_stdout}");

    // Same-build enabled control: only an explicit enable reaches the enable
    // endpoint, then one publish and one ping answer succeed.
    let enable_control = cmd_osl_set_discovery_replies_switch(
        &resumed,
        "on".to_owned(),
        Some(prefs_dir.path().to_path_buf()),
    )
    .expect("explicitly re-enable discovery");
    let control = cmd_osl_publish_discovery_card(
        &resumed,
        "signal".to_owned(),
        "TARGET-explicit-control".to_owned(),
        "TARGET:REENABLED:BYTE-EXACT".to_owned(),
    )
    .expect("publish after explicit re-enable");
    let control_reply = cmd_osl_answer_discovery_ping(&resumed).expect("answer enabled control");
    assert_eq!(enable_control, "on");
    assert_eq!(control.cards_written, 1);
    assert_eq!(control_reply, b"OSL-DISCOVERY-REPLY-v1");
    assert_eq!(server.state.owner_count(TARGET_ACCOUNT), 1);
    assert_eq!(server.state.owner_count(PROTECTED_ACCOUNT), 4);
    assert_eq!(read_all(&resumed, &protected, true, "enabled-control"), 4);

    let data = server.state.data.lock().expect("service data lock");
    println!(
        "TASK4764 enabled_control explicit_reenable={enable_control} publishes=1 answers=1 answer_bytes={} target_live=1 protected_live=4 release_routes=enable:{},publish:{},take_back:{},read:{}",
        control_reply.len(), data.enables, data.publishes, data.take_backs, data.reads
    );
}
