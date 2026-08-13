use osl_privacy_hub::enclave_key_lifecycle::{
    relay_open, AccountDevices, Ciphertext, DeviceClient, EnclaveKeyError, EnclaveKeyService,
    MembershipOperation, Relay, SIGNED_REMOVAL_PROGRESS_THRESHOLD,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Barrier};

const MARKED_MESSAGES: usize = 50;
const PATHS: [&str; 8] = [
    "mailbox",
    "history",
    "reconnect",
    "restore",
    "attachment",
    "cache",
    "retry",
    "alternate-endpoint",
];
const REMOVED_PATHS: [&str; 5] = ["mailbox", "cache", "retry", "alternate-endpoint", "restart"];

fn set(items: impl IntoIterator<Item = String>) -> BTreeSet<String> {
    items.into_iter().collect()
}

fn initial_roster(size: usize) -> AccountDevices {
    assert!(
        size >= 4,
        "need owner, A, L and at least one independent client"
    );
    let mut roster = BTreeMap::new();
    roster.insert("O".to_owned(), set(["O1".to_owned()]));
    roster.insert(
        "A".to_owned(),
        set(["A1".to_owned(), "A2".to_owned(), "A3".to_owned()]),
    );
    roster.insert("L".to_owned(), set(["L1".to_owned()]));
    for index in 1..=(size - 3) {
        roster.insert(format!("M{index:04}"), set([format!("M{index:04}D1")]));
    }
    assert_eq!(roster.len(), size);
    roster
}

fn clients_for(roster: &AccountDevices) -> BTreeMap<String, DeviceClient> {
    roster
        .iter()
        .flat_map(|(account, devices)| {
            devices.iter().map(move |device| {
                let client = DeviceClient::new(account, device);
                (format!("{account}/{device}"), client)
            })
        })
        .collect()
}

fn must_refuse<T: std::fmt::Debug>(
    result: Result<T, EnclaveKeyError>,
    account: &str,
    device: &str,
    epoch: u64,
    path: &str,
) {
    let error = result.expect_err(&format!(
        "account={account} device={device} epoch={epoch} path={path} must refuse"
    ));
    assert_eq!(
        error.to_string(),
        format!("refused account={account} device={device} epoch={epoch} path={path}")
    );
}

fn marked(
    service: &EnclaveKeyService,
    epoch: u64,
    prefix: &str,
    attachment: bool,
) -> Vec<(Ciphertext, Vec<u8>)> {
    (0..MARKED_MESSAGES)
        .map(|index| {
            let id = format!("{prefix}-{index:02}");
            let plaintext = format!("{id}: independently-marked-enclave-content").into_bytes();
            let ciphertext = service
                .seal(epoch, id, attachment, &plaintext)
                .expect("seal marked message");
            (ciphertext, plaintext)
        })
        .collect()
}

fn all_permutations() -> Vec<[usize; 3]> {
    vec![
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ]
}

fn malicious_mode(size: usize) -> Option<String> {
    let mode = std::env::var("TASK4882_MUTATION")
        .ok()
        .filter(|mode| !mode.is_empty())?;
    let target = std::env::var("TASK4882_MUTATION_SIZE")
        .ok()
        .and_then(|value| value.parse::<usize>().ok());
    target.map_or(Some(mode.clone()), |target| {
        (target == size).then_some(mode)
    })
}

fn run_lifecycle(size: usize, require_milestone: bool) -> String {
    let initial = initial_roster(size);
    let service = EnclaveKeyService::new(
        format!("task-4882-enclave-{size}"),
        format!("task-4882-authority-secret-{size}"),
        initial.clone(),
        size as u64,
    )
    .expect("create signed enclave key authority");
    service.validate_history().expect("signed genesis");
    let pre_epoch = size as u64;
    let mut pre_clients = clients_for(&initial);
    let mut pre_relay = Relay::default();
    for client in pre_clients.values_mut() {
        pre_relay
            .deliver(&service, pre_epoch, client, "mailbox")
            .expect("current pre-join recipient");
    }
    let old_messages = marked(&service, pre_epoch, "prejoin", false);
    assert_eq!(old_messages.len(), MARKED_MESSAGES);

    // All three requests begin from the same barrier. The authority mutex is
    // the canonical serializer; no caller can choose an epoch or parent.
    let barrier = Arc::new(Barrier::new(4));
    let join_service = service.clone();
    let leave_service = service.clone();
    let remove_service = service.clone();
    let join_barrier = barrier.clone();
    let leave_barrier = barrier.clone();
    let remove_barrier = barrier.clone();
    let join = std::thread::spawn(move || {
        join_barrier.wait();
        join_service.apply(MembershipOperation::Join {
            account: "J".to_owned(),
            devices: set(["J1".to_owned()]),
        })
    });
    let leave = std::thread::spawn(move || {
        leave_barrier.wait();
        leave_service.apply(MembershipOperation::Leave {
            account: "L".to_owned(),
        })
    });
    let remove = std::thread::spawn(move || {
        remove_barrier.wait();
        remove_service.apply(MembershipOperation::RemoveDevice {
            account: "A".to_owned(),
            device: "A2".to_owned(),
        })
    });
    barrier.wait();
    let concurrent = vec![
        join.join().expect("join thread"),
        leave.join().expect("leave thread"),
        remove.join().expect("remove thread"),
    ];
    assert!(
        concurrent.iter().all(Result::is_ok),
        "all 3 operations overlap and commit"
    );
    let history = service.records();
    let changed = &history[1..];
    assert_eq!(changed.len(), 3, "exactly three concurrent operations");
    assert_eq!(
        changed
            .iter()
            .map(|record| record.epoch)
            .collect::<Vec<_>>(),
        vec![pre_epoch + 1, pre_epoch + 2, pre_epoch + 3]
    );
    service
        .validate_history()
        .expect("canonical parent-linked epoch chain");
    let final_epoch = pre_epoch + 3;
    let final_record = service.current();
    assert_eq!(final_record.epoch, final_epoch);
    assert!(final_record.accounts.contains("J") && !final_record.accounts.contains("L"));
    assert_eq!(
        final_record.device_rosters.get("A").unwrap(),
        &set(["A1".to_owned(), "A3".to_owned()])
    );
    assert!(!final_record.recipients.contains("A/A2"));

    // Every pre-join path is a denial for J even after a restart/restore-like
    // fresh client is made. Its first key must be later than every old cipher.
    let applicant = DeviceClient::new("J", "J1");
    for (ciphertext, _) in &old_messages {
        for path in PATHS {
            must_refuse(applicant.open(ciphertext, path), "J", "J1", pre_epoch, path);
        }
    }
    assert_eq!(
        applicant.key_bytes(pre_epoch),
        0,
        "J receives no pre-join key bytes"
    );

    let post_text = marked(&service, final_epoch, "postjoin-text", false);
    let post_attachment = marked(&service, final_epoch, "postjoin-attachment", true);
    let expected_final = final_record.recipients.clone();
    let expected_accounts = final_record.accounts.clone();
    let expected_devices = final_record.device_rosters.clone();
    let mut total_deliveries = 0usize;
    for (order_index, order) in all_permutations().into_iter().enumerate() {
        let mut clients = clients_for(&expected_devices);
        let mut relay = Relay::default();
        for operation_index in order {
            let update = &changed[operation_index];
            for client in clients.values_mut() {
                // A device sees a record only when it was a committed recipient
                // at that epoch. This also tests reordering of legitimate updates.
                if update
                    .recipients
                    .contains(&format!("{}/{}", client.account, client.device))
                {
                    relay
                        .deliver(&service, update.epoch, client, "mailbox")
                        .expect("legitimate recipient delivery");
                }
            }
        }
        let offline = expected_final
            .iter()
            .next_back()
            .expect("final recipient")
            .clone();
        let offline_client = clients.get_mut(&offline).expect("offline client");
        relay
            .deliver(&service, final_epoch, offline_client, "reconnect")
            .expect("offline current device reconnects");
        for (principal, client) in &clients {
            assert_eq!(
                client.observed_accounts, expected_accounts,
                "order={order_index} principal={principal} account roster"
            );
            assert_eq!(
                client.observed_devices, expected_devices,
                "order={order_index} principal={principal} device roster"
            );
            assert_eq!(
                client.observed_key_hash, final_record.key_hash,
                "order={order_index} principal={principal} key hash"
            );
            assert_eq!(
                client.key_bytes(final_epoch),
                64,
                "order={order_index} principal={principal} winning key bytes"
            );
        }
        assert_eq!(
            clients.keys().cloned().collect::<BTreeSet<_>>(),
            expected_final,
            "order={order_index} exact current recipient inventory"
        );
        for principal in ["A/A1", "A/A3", "J/J1"] {
            let client = clients.get(principal).expect("required current recipient");
            for (ciphertext, plaintext) in &post_text {
                assert_eq!(
                    client.open(ciphertext, "mailbox").unwrap(),
                    *plaintext,
                    "order={order_index} principal={principal} text"
                );
            }
        }
        total_deliveries += relay.deliveries;
    }

    // L and A2 retain all material from the pre-removal epoch, but never get a
    // later grant or plaintext through any relay/client surface.
    for (account, device) in [("L", "L1"), ("A", "A2")] {
        let hostile = pre_clients
            .get(&format!("{account}/{device}"))
            .expect("pre-removal material");
        assert_eq!(
            hostile.key_bytes(pre_epoch),
            64,
            "account={account} device={device} retained pre-removal material"
        );
        assert_eq!(
            hostile.key_bytes(final_epoch),
            0,
            "account={account} device={device} winning key absent"
        );
        for path in REMOVED_PATHS {
            must_refuse(
                service.grant(final_epoch, account, device, path),
                account,
                device,
                final_epoch,
                path,
            );
            for (ciphertext, _) in &post_text {
                must_refuse(
                    hostile.open(ciphertext, path),
                    account,
                    device,
                    final_epoch,
                    path,
                );
            }
        }
    }

    // Replays, reordered updates and equal-epoch forks are never an external
    // write path. Each error names the attacked epoch.
    for record in [changed[0].clone(), changed[2].clone(), final_record.clone()] {
        let error = service.accept_external_epoch(&record).unwrap_err();
        assert!(error
            .to_string()
            .contains(&format!("epoch={}", record.epoch)));
    }
    assert_eq!(
        Relay::consumer_inventory().len(),
        8,
        "generated recipient/cache consumer inventory"
    );

    if let Some(mode) = malicious_mode(size) {
        let mut relay = Relay::default();
        let winning = service
            .grant(final_epoch, "J", "J1", "mailbox")
            .expect("winning grant only for hostile red proof");
        let sink = match mode.as_str() {
            "neutral-cache" => {
                relay.hostile_retain_raw("process-session-state", winning);
                "process-session-state"
            }
            "operator-backup" => {
                relay.hostile_retain_raw("operator-export-backup", winning);
                "operator-export-backup"
            }
            "derived-material" => {
                relay.hostile_retain_derived("hkdf-secret-and-epoch-context", &winning);
                "hkdf-secret-and-epoch-context"
            }
            "a2-cache" => {
                let mut a2 = pre_clients.remove("A/A2").expect("hostile A2");
                a2.hostile_cache_for_red_proof(final_epoch, winning);
                assert_eq!(
                    a2.key_bytes(final_epoch),
                    0,
                    "MUTATION size={size} principal=A device=A2 epoch={final_epoch} path=key-cache"
                );
                "a2-key-cache"
            }
            "l-cache" => {
                let mut l1 = pre_clients.remove("L/L1").expect("hostile L");
                l1.hostile_cache_for_red_proof(final_epoch, winning);
                assert_eq!(
                    l1.key_bytes(final_epoch),
                    0,
                    "MUTATION size={size} principal=L device=L1 epoch={final_epoch} path=key-cache"
                );
                "l1-key-cache"
            }
            other => panic!("unknown TASK4882_MUTATION={other}"),
        };
        if mode == "a2-cache" || mode == "l-cache" {
            return format!("unreachable mutation size={size} sink={sink}");
        }
        let retained = relay.hostile_key(sink).expect("retained bytes");
        let recovered_text = relay_open(&post_text[0].0, retained);
        let recovered_attachment = relay_open(&post_attachment[0].0, retained);
        assert_eq!(
            recovered_text, post_text[0].1,
            "production relay retained copy must decrypt live text"
        );
        assert_eq!(
            recovered_attachment, post_attachment[0].1,
            "production relay retained copy must decrypt live attachment"
        );
        let audit = relay
            .audit_no_key_material(final_epoch)
            .expect_err("MUTATION relay retention must make this task red");
        panic!("MUTATION size={size} principal=relay epoch={final_epoch} sink={sink} recovered_content_ids={},{} audit={audit}", post_text[0].0.content_id, post_attachment[0].0.content_id);
    }

    if require_milestone {
        assert!(matches!(size, 10 | 50 | 200));
    }
    format!("size={size} epochs={}-{} old_keys_J=0 old_opens_J=0 post_opens_J=50 operations=3 delivery_orders=6 final_recipients={} final_removed=L/L1,A/A2 removed_key_bytes=0 removed_opens=0 allowances={} consumers=8", pre_epoch + 1, final_epoch, expected_final.len(), total_deliveries)
}

#[test]
fn task_4882_proves_backward_secrecy_and_current_device_convergence() {
    let generated_larger = SIGNED_REMOVAL_PROGRESS_THRESHOLD + (MARKED_MESSAGES + 3);
    assert!(generated_larger > SIGNED_REMOVAL_PROGRESS_THRESHOLD);
    if let Ok(target) = std::env::var("TASK4882_MUTATION_SIZE") {
        let size = target
            .parse::<usize>()
            .expect("TASK4882_MUTATION_SIZE is numeric");
        assert!(
            matches!(size, SIGNED_REMOVAL_PROGRESS_THRESHOLD) || size == generated_larger,
            "red proof only permits N or its generated larger population"
        );
        let _ = run_lifecycle(size, false);
        panic!("requested mutation size={size} did not make the proof red");
    }
    let milestones = [10usize, 50, 200]
        .into_iter()
        .map(|size| run_lifecycle(size, true))
        .collect::<Vec<_>>();
    let signed_threshold = run_lifecycle(SIGNED_REMOVAL_PROGRESS_THRESHOLD, false);
    let larger = run_lifecycle(generated_larger, false);
    println!("TASK4882 PASS build=osl-hub-{} signed_removal_threshold={} generated_larger={} marked_pre=50 marked_post=50 milestone=[{}] threshold=[{}] larger=[{}] relay_key_holders=0", env!("CARGO_PKG_VERSION"), SIGNED_REMOVAL_PROGRESS_THRESHOLD, generated_larger, milestones.join("; "), signed_threshold, larger);
}
