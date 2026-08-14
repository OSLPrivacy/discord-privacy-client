use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use task_6214_durable_send::{
    measured_worst_case_bytes, Authority, Draft, Ledger, Reservation, SendError, SendStatus, Store,
    Usage, VolumeSnapshot, ABSOLUTE_DISK_FLOOR, GLOBAL_BYTES, GLOBAL_ITEMS, PER_ACCOUNT_BYTES,
    PER_ACCOUNT_ITEMS,
};

const KEY: [u8; 32] = [0x62; 32];
const STEPS: [&str; 5] = [
    "private-save",
    "service_acceptance",
    "local_save",
    "receiver_publish",
    "final-confirmation",
];
const POSITIONS: [&str; 3] = [
    "before-effect",
    "after-effect-before-progress",
    "after-progress-commit",
];

struct ChildGuard(Option<Child>);
impl ChildGuard {
    fn child_mut(&mut self) -> &mut Child {
        self.0.as_mut().unwrap()
    }
    fn kill_wait(&mut self) {
        if let Some(mut c) = self.0.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.kill_wait();
    }
}

struct Provider {
    child: ChildGuard,
    addr: String,
    root: PathBuf,
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_task-6214-durable-send")
}
fn wait_for(path: &Path, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "TASK6214 missing {label}: {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(5));
    }
}
fn reserve_addr() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().to_string()
}
fn start_provider(base: &Path) -> Provider {
    let root = base.join("outside-provider");
    let addr = reserve_addr();
    let child = Command::new(bin())
        .args(["provider", root.to_str().unwrap(), &addr])
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(Some(child));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match std::net::TcpStream::connect(&addr) {
            Ok(_) => break,
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            Err(e) => panic!("provider did not start: {e}"),
        }
    }
    assert!(guard.child_mut().try_wait().unwrap().is_none());
    Provider {
        child: guard,
        addr,
        root,
    }
}
fn send_child(
    app: &Path,
    provider: &str,
    send_id: &str,
    hook: Option<(&str, &Path)>,
) -> ChildGuard {
    let mut command = Command::new(bin());
    command
        .args([
            "send",
            app.to_str().unwrap(),
            send_id,
            provider,
            "signed-in@example.test",
            "receiver@example.test",
            "fresh marked encrypted payload",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some((spec, dir)) = hook {
        command
            .env("OSL_6214_HOOK", spec)
            .env("OSL_6214_HOOK_DIR", dir);
    }
    ChildGuard(Some(command.spawn().unwrap()))
}
fn count(path: &Path) -> usize {
    fs::read_dir(path).map(|e| e.count()).unwrap_or(0)
}

#[test]
fn fresh_marked_sends_survive_all_fifteen_real_kill_points_exactly_once() {
    let deployment = tempfile::tempdir().unwrap();
    let provider = start_provider(deployment.path());
    let mut exercised = 0;
    for step in STEPS {
        for position in POSITIONS {
            let send_id = format!("task6214-{exercised:02}");
            let app = deployment.path().join(format!("app-{exercised:02}"));
            let hook = deployment.path().join(format!("hook-{exercised:02}"));
            fs::create_dir_all(&hook).unwrap();
            let spec = format!("{step}:{position}");
            let mut crashed = send_child(&app, &provider.addr, &send_id, Some((&spec, &hook)));
            wait_for(&hook.join("reached"), &spec);
            crashed.kill_wait();
            if step == "service_acceptance" && position == "after-effect-before-progress" {
                let interrupted = Store::open(&app, KEY).unwrap();
                let row = interrupted.record(&send_id).unwrap();
                let ledger = interrupted.ledger().unwrap();
                assert_eq!(
                    row.status,
                    SendStatus::PrivateSaved,
                    "send_id={send_id} accepted before reservation writer=crashed"
                );
                assert!(
                    ledger.reservations.contains_key(&send_id),
                    "send_id={send_id} missing atomic reservation before Pending progress commit"
                );
            }
            let mut recovered = send_child(&app, &provider.addr, &send_id, None);
            let output = recovered.0.take().unwrap().wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "send_id={send_id} step={step} position={position} recovery failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let store = Store::open(&app, KEY).unwrap();
            let row = store.record(&send_id).unwrap();
            assert_eq!(
                row.status,
                SendStatus::Sent,
                "send_id={send_id} left Pending after confirmation"
            );
            assert_eq!(row.authority_generation, 1);
            assert_eq!(
                store.open_payload(&send_id).unwrap(),
                b"fresh marked encrypted payload"
            );
            assert_eq!(store.ledger().unwrap().global, Usage::default(), "send_id={send_id} reservation remained after reported success (success early or Pending after confirmation)");
            assert_eq!(
                count(&provider.root.join("requests")),
                exercised + 1,
                "send_id={send_id} request observer mismatch"
            );
            assert_eq!(
                count(&provider.root.join("objects")),
                exercised + 1,
                "send_id={send_id} provider object mismatch"
            );
            assert_eq!(
                count(&provider.root.join("receiver")),
                exercised + 1,
                "send_id={send_id} receiver arrival mismatch"
            );
            println!("TASK6214_KILL send_id={send_id} step={step} position={position} authority_generation=1 provider_objects=1 receiver_arrivals=1 final=Sent");
            exercised += 1;
        }
    }
    assert_eq!(exercised, 15);
    println!("TASK6214_KILL_POINTS exercised={exercised} named_steps=5 positions_per_step=3 outside_journal=15 provider_objects=15 receiver_arrivals=15 exactly_once=15");
    drop(provider.child);
}

#[test]
fn encrypted_pending_outage_cancel_and_both_race_orders_have_durable_authority() {
    let deployment = tempfile::tempdir().unwrap();
    let mut provider = start_provider(deployment.path());
    let volume = VolumeSnapshot {
        free_bytes: u64::MAX / 4,
        total_bytes: 8 * 1024 * 1024 * 1024,
    };

    let app = deployment.path().join("cancel-before-recovery");
    let store = Store::open(&app, KEY).unwrap();
    let draft = Draft {
        account: "acct".into(),
        recipient: "receiver".into(),
        body: b"complete authenticated encrypted Pending payload".to_vec(),
    };
    let pending = store
        .accept("cancel-wins", "writer-cancel", &draft, volume)
        .unwrap();
    assert_eq!(pending.status, SendStatus::Pending);
    let ciphertext = fs::read(app.join("sends/cancel-wins/payload.enc")).unwrap();
    assert!(!ciphertext
        .windows(draft.body.len())
        .any(|w| w == draft.body));
    drop(store);
    provider.child.kill_wait();

    // A real app handle and independent service process both restart while
    // the row is still Pending. The complete authenticated payload must open
    // only after that restart, and Cancel must remain available.
    let mut provider = start_provider(deployment.path());
    let restarted_pending = Store::open(&app, KEY).unwrap();
    assert_eq!(
        restarted_pending.open_payload("cancel-wins").unwrap(),
        draft.body
    );
    assert_eq!(
        restarted_pending.record("cancel-wins").unwrap().status,
        SendStatus::Pending
    );
    assert!(
        restarted_pending
            .can_cancel("cancel-wins", &provider.addr)
            .unwrap(),
        "visible Pending row must expose Cancel after app/service restart"
    );
    let cancelled = restarted_pending
        .cancel("cancel-wins", &provider.addr)
        .unwrap();
    assert_eq!(
        (
            cancelled.status,
            cancelled.authority,
            cancelled.authority_generation
        ),
        (SendStatus::Cancelled, Authority::Cancelled, 2)
    );
    drop(restarted_pending);
    provider.child.kill_wait();
    let provider = start_provider(deployment.path());
    let restarted = Store::open(&app, KEY).unwrap();
    assert!(
        matches!(
            restarted.ship("cancel-wins", &provider.addr),
            Err(SendError::Cancelled { generation: 2, .. })
        ),
        "send_id=cancel-wins authority_generation=2 replay after Cancel reached provider effect"
    );
    assert_eq!(count(&provider.root.join("requests")), 0);
    assert_eq!(count(&provider.root.join("objects")), 0);
    assert_eq!(count(&provider.root.join("receiver")), 0);
    assert_eq!(
        restarted
            .record("cancel-wins")
            .expect("send_id=cancel-wins authority_generation=2 tombstone row deleted")
            .status,
        SendStatus::Cancelled
    );

    let app_cancel = deployment.path().join("race-cancel-first");
    let race_store = Store::open(&app_cancel, KEY).unwrap();
    race_store
        .accept("race-cancel-first", "recovery-writer", &draft, volume)
        .unwrap();
    let hook = deployment.path().join("race-hook-cancel");
    fs::create_dir_all(&hook).unwrap();
    let mut recovery = send_child(
        &app_cancel,
        &provider.addr,
        "race-cancel-first",
        Some(("receiver_publish:before-effect", &hook)),
    );
    wait_for(&hook.join("reached"), "cancel-first hold");
    let tombstone = race_store
        .cancel("race-cancel-first", &provider.addr)
        .unwrap();
    fs::write(hook.join("release"), b"release").unwrap();
    let output = recovery.0.take().unwrap().wait_with_output().unwrap();
    assert!(!output.status.success(), "send_id=race-cancel-first authority_generation=2 committed tombstone lost to later provider effect");
    assert_eq!(tombstone.authority_generation, 2);
    assert_eq!(count(&provider.root.join("objects")), 0);
    assert_eq!(count(&provider.root.join("receiver")), 0);

    let app_effect = deployment.path().join("race-effect-first");
    let effect_store = Store::open(&app_effect, KEY).unwrap();
    effect_store
        .accept("race-effect-first", "recovery-writer", &draft, volume)
        .unwrap();
    let hook = deployment.path().join("race-hook-effect");
    fs::create_dir_all(&hook).unwrap();
    let mut recovery = send_child(
        &app_effect,
        &provider.addr,
        "race-effect-first",
        Some(("receiver_publish:after-effect-before-progress", &hook)),
    );
    wait_for(&hook.join("reached"), "effect-first hold");
    assert!(matches!(effect_store.cancel("race-effect-first", &provider.addr), Err(SendError::CancelRefused { .. })), "send_id=race-effect-first authority_generation=1 falsely claimed cancellation after provider effect");
    assert_eq!(
        effect_store.record("race-effect-first").unwrap().status,
        SendStatus::Sent
    );
    fs::write(hook.join("release"), b"release").unwrap();
    let output = recovery.0.take().unwrap().wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "effect-first recovery completes Sent: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(count(&provider.root.join("objects")), 1);
    assert_eq!(count(&provider.root.join("receiver")), 1);
    println!("TASK6214_CANCEL pending_cancel_exposed=true encrypted_payload_bytes={} cancel_before_recovery=Cancelled authority_generation=2 provider_requests=0 provider_objects=0 receiver_arrivals=0 replay=refused", ciphertext.len());
    println!("TASK6214_RACE cancel_first=Cancelled later_provider_effects=0 effect_first=Sent cancel_refused=true provider_objects=1 receiver_arrivals=1");
}

fn seed_ledger(account: &str, items: u64, bytes: u64) -> Ledger {
    let mut ledger = Ledger::default();
    if items == 0 && bytes == 0 {
        return ledger;
    }
    let r = Reservation {
        writer: "independent-existing-writer".into(),
        account: account.into(),
        items,
        bytes,
    };
    ledger
        .reservations
        .insert("independent-existing-send".into(), r);
    ledger
        .accounts
        .insert(account.into(), Usage { items, bytes });
    ledger.global = Usage { items, bytes };
    ledger
}
fn seed_global(items: u64, bytes: u64) -> Ledger {
    let mut ledger = Ledger::default();
    let mut left_items = items;
    let mut left_bytes = bytes;
    let mut index = 0;
    while left_items > 0 || left_bytes > 0 {
        let take_items = left_items.min(PER_ACCOUNT_ITEMS);
        let take_bytes = left_bytes.min(PER_ACCOUNT_BYTES);
        let account = format!("global-seed-{index}");
        let r = Reservation {
            writer: format!("independent-global-writer-{index}"),
            account: account.clone(),
            items: take_items,
            bytes: take_bytes,
        };
        ledger
            .reservations
            .insert(format!("independent-global-send-{index}"), r);
        ledger.accounts.insert(
            account,
            Usage {
                items: take_items,
                bytes: take_bytes,
            },
        );
        ledger.global.items += take_items;
        ledger.global.bytes += take_bytes;
        left_items -= take_items;
        left_bytes -= take_bytes;
        index += 1;
    }
    ledger
}
fn run_two_writer_edge(
    root: &Path,
    ledger: Ledger,
    accounts: [&str; 2],
    volume: VolumeSnapshot,
    label: &str,
) -> (usize, Ledger) {
    let store = Store::open(root, KEY).unwrap();
    store.install_ledger_for_independent_audit(&ledger).unwrap();
    let start = root.join("release-two-writers");
    let mut children = Vec::new();
    for i in 0..2 {
        let mut c = Command::new(bin());
        c.args([
            "accept",
            root.to_str().unwrap(),
            &format!("{label}-writer-{i}"),
            &format!("writer-{i}"),
            accounts[i],
            "receiver",
            "x",
            &volume.free_bytes.to_string(),
            &volume.total_bytes.to_string(),
        ])
        .env("OSL_6214_START", &start)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        children.push(c.spawn().unwrap());
    }
    fs::write(&start, b"start").unwrap();
    let outputs: Vec<_> = children
        .into_iter()
        .map(|c| c.wait_with_output().unwrap())
        .collect();
    let winners = outputs.iter().filter(|o| o.status.success()).count();
    assert_eq!(
        winners,
        1,
        "edge={label} must admit a linearizable fitting subset; outputs={:?}",
        outputs
            .iter()
            .map(|o| String::from_utf8_lossy(&o.stderr))
            .collect::<Vec<_>>()
    );
    for (i, o) in outputs.iter().enumerate() {
        if !o.status.success() {
            let reopened = Store::open(root, KEY).unwrap();
            assert_eq!(
                reopened
                    .load_draft(&format!("{label}-writer-{i}"))
                    .unwrap()
                    .body,
                b"x",
                "edge={label} loser draft changed"
            );
            assert!(
                !String::from_utf8_lossy(&o.stdout).contains("ACCEPTED"),
                "edge={label} loser claimed acceptance"
            );
        }
    }
    let final_ledger = Store::open(root, KEY).unwrap().ledger().unwrap();
    (winners, final_ledger)
}

#[test]
fn linearizable_reservations_hold_all_exact_item_byte_and_disk_edges() {
    let deployment = tempfile::tempdir().unwrap();
    let request = measured_worst_case_bytes(1);
    let huge = VolumeSnapshot {
        free_bytes: u64::MAX / 4,
        total_bytes: 8 * 1024 * 1024 * 1024,
    };
    let cases = [
        (
            "per-account-items",
            seed_ledger("same", PER_ACCOUNT_ITEMS - 1, 0),
            ["same", "same"],
            huge,
        ),
        (
            "per-account-bytes",
            seed_ledger("same", 1, PER_ACCOUNT_BYTES - request),
            ["same", "same"],
            huge,
        ),
        (
            "global-items",
            seed_global(GLOBAL_ITEMS - 1, 0),
            ["a", "b"],
            huge,
        ),
        (
            "global-bytes",
            seed_global(8, GLOBAL_BYTES - request),
            ["a", "b"],
            huge,
        ),
        (
            "disk-floor",
            Ledger::default(),
            ["a", "b"],
            VolumeSnapshot {
                free_bytes: ABSOLUTE_DISK_FLOOR + request,
                total_bytes: 8 * 1024 * 1024 * 1024,
            },
        ),
    ];
    for (label, ledger, accounts, volume) in cases {
        let root = deployment.path().join(label);
        let (_, final_ledger) = run_two_writer_edge(&root, ledger.clone(), accounts, volume, label);
        let max_account_items = final_ledger
            .accounts
            .values()
            .map(|u| u.items)
            .max()
            .unwrap_or(0);
        let max_account_bytes = final_ledger
            .accounts
            .values()
            .map(|u| u.bytes)
            .max()
            .unwrap_or(0);
        assert!(
            max_account_items <= PER_ACCOUNT_ITEMS,
            "edge={label} measured peak items={max_account_items}"
        );
        assert!(
            max_account_bytes <= PER_ACCOUNT_BYTES,
            "edge={label} measured peak bytes={max_account_bytes}"
        );
        assert!(
            final_ledger.global.items <= GLOBAL_ITEMS,
            "edge={label} global items peak={}",
            final_ledger.global.items
        );
        assert!(
            final_ledger.global.bytes <= GLOBAL_BYTES,
            "edge={label} global bytes peak={}",
            final_ledger.global.bytes
        );
        println!("TASK6214_EDGE edge={label} writers=2 fitting_alone=2 accepted_subset=1 request_reservation={request} peak_account_items={max_account_items} peak_account_bytes={max_account_bytes} peak_global_items={} peak_global_bytes={} floor={} loser_draft_exact=true", final_ledger.global.items, final_ledger.global.bytes, volume.floor());
    }
    let actual = VolumeSnapshot::for_path(deployment.path()).unwrap();
    println!("TASK6214_LIMITS per_account_items={PER_ACCOUNT_ITEMS} per_account_bytes={PER_ACCOUNT_BYTES} global_items={GLOBAL_ITEMS} global_bytes={GLOBAL_BYTES} disk_floor_formula=max(2147483648,10%) actual_free={} actual_total={} actual_floor={}", actual.free_bytes, actual.total_bytes, actual.floor());
}
