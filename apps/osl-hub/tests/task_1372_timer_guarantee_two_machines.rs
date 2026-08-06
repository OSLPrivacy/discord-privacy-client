#![cfg(feature = "core")]

use message_lifecycle::AcceptedPart;
use osl_privacy_hub::message_expiry::{
    absolute_release, note_delivered_at_path, prune_at_path, verdict_at_path, ExpiryVerdict,
};
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;
use uuid::Uuid;

const KEY: [u8; 32] = [0x72; 32];
const SENT_AT: i64 = 1_810_000_000;
const EXPIRES_AT: i64 = SENT_AT + 3_600;

struct TestMachine {
    name: &'static str,
    scope_key: String,
    message_id: String,
    ledger_path: std::path::PathBuf,
    store: MessageStore,
}

#[test]
fn task_1372_timer_guarantee_on_two_machines() {
    let root = TempDir::new().expect("temp two-machine root");
    let random_mark = Uuid::new_v4().to_string();
    let marked_message = format!("TASK1372-MARKED-TIMER-MESSAGE-{random_mark}");
    let message_id = format!("task-1372-{random_mark}");

    let machine_a = put_marked_timer_message(
        root.path(),
        "OSL Machine A",
        "machine:a",
        &message_id,
        &marked_message,
    );
    let machine_b = put_marked_timer_message(
        root.path(),
        "OSL Machine B",
        "machine:b",
        &message_id,
        &marked_message,
    );
    let machines = [machine_a, machine_b];

    let machine_a_read = read_exact_message(&machines[0]);
    let machine_b_read = read_exact_message(&machines[1]);
    let machine_a_count = exact_mark_count(&machines[0], &marked_message);
    let machine_b_count = exact_mark_count(&machines[1], &marked_message);

    assert_eq!(machine_a_read, marked_message);
    assert_eq!(machine_b_read, marked_message);
    assert_eq!(machine_a_count, 1);
    assert_eq!(machine_b_count, 1);

    println!("TASK1372_MARKED_MESSAGE={marked_message}");
    println!("TASK1372_MACHINE_A_FIRST_READ={machine_a_read}");
    println!("TASK1372_MACHINE_B_FIRST_READ={machine_b_read}");
    println!("TASK1372_MACHINE_A_FIRST_COUNT={machine_a_count}");
    println!("TASK1372_MACHINE_B_FIRST_COUNT={machine_b_count}");

    let expired = expire_once(&machines, EXPIRES_AT);

    let machine_a_after_count = exact_mark_count(&machines[0], &marked_message);
    let machine_b_after_count = exact_mark_count(&machines[1], &marked_message);
    let machine_a_absent = machines[0]
        .store
        .get(&machines[0].message_id)
        .expect("read machine A after expiry")
        .is_none();
    let machine_b_absent = machines[1]
        .store
        .get(&machines[1].message_id)
        .expect("read machine B after expiry")
        .is_none();

    assert_eq!(expired, 2);
    assert_eq!(machine_a_after_count, 0);
    assert_eq!(machine_b_after_count, 0);
    assert!(machine_a_absent);
    assert!(machine_b_absent);

    println!("TASK1372_EXPIRY_RUN_COUNT=1");
    println!("TASK1372_EXPIRED_TOTAL={expired}");
    println!("TASK1372_MACHINE_A_AFTER_COUNT={machine_a_after_count}");
    println!("TASK1372_MACHINE_B_AFTER_COUNT={machine_b_after_count}");
    println!("TASK1372_MACHINE_A_MARKED_MESSAGE_ABSENT={machine_a_absent}");
    println!("TASK1372_MACHINE_B_MARKED_MESSAGE_ABSENT={machine_b_absent}");
}

fn put_marked_timer_message(
    root: &std::path::Path,
    name: &'static str,
    scope_key: &str,
    message_id: &str,
    marked_message: &str,
) -> TestMachine {
    let machine_root = root.join(name.replace(' ', "-").to_ascii_lowercase());
    let store_root = machine_root.join("message-store");
    let ledger_root = machine_root.join("expiry-ledger");
    std::fs::create_dir_all(&ledger_root).expect("create machine ledger root");
    let ledger_path = ledger_root.join("message_open_clock.json");
    let store = MessageStore::open(&store_root, &KEY).expect("open machine store");

    store
        .put(&StoredMessage {
            discord_message_id: message_id.to_owned(),
            channel_id: format!("{scope_key}:channel"),
            sender_discord_id: "1372-sender".to_owned(),
            sender_osl_user_id: "1372-osl-sender".to_owned(),
            plaintext: marked_message.to_owned(),
            decrypted_at: SENT_AT,
            burned: false,
        })
        .expect("put marked timer message in machine store");

    note_delivered_at_path(
        &ledger_path,
        &KEY,
        scope_key,
        message_id,
        absolute_release(SENT_AT, 3_600).expect("one-hour timed release"),
        vec![AcceptedPart {
            index: 0,
            sealed_bytes: 512,
            digest: [0x72; 32],
        }],
        [0x72; 32],
        Some(message_id.to_owned()),
        SENT_AT,
    )
    .expect("record message expiry ledger");

    assert_eq!(
        verdict_at_path(&ledger_path, &KEY, scope_key, message_id, SENT_AT + 1),
        ExpiryVerdict::Readable {
            effective_expires_at: EXPIRES_AT
        },
        "{name} timer message must be readable before expiry"
    );

    TestMachine {
        name,
        scope_key: scope_key.to_owned(),
        message_id: message_id.to_owned(),
        ledger_path,
        store,
    }
}

fn read_exact_message(machine: &TestMachine) -> String {
    machine
        .store
        .get(&machine.message_id)
        .unwrap_or_else(|error| panic!("{} read failed: {error}", machine.name))
        .unwrap_or_else(|| panic!("{} marked message is absent before expiry", machine.name))
        .plaintext
}

fn exact_mark_count(machine: &TestMachine, marked_message: &str) -> usize {
    machine
        .store
        .get(&machine.message_id)
        .unwrap_or_else(|error| panic!("{} count read failed: {error}", machine.name))
        .filter(|message| message.plaintext == marked_message)
        .map(|_| 1)
        .unwrap_or(0)
}

fn expire_once(machines: &[TestMachine; 2], now: i64) -> usize {
    let mut expired = 0usize;
    for machine in machines {
        let prune = prune_at_path(&machine.ledger_path, &KEY, now)
            .unwrap_or_else(|error| panic!("{} prune failed: {error}", machine.name));
        assert_eq!(
            prune.shred_cache_ids,
            vec![machine.message_id.clone()],
            "{} expiry must name exactly its marked cache row",
            machine.name
        );

        let shredded = machine
            .store
            .shred_expired_messages(&prune.shred_cache_ids)
            .unwrap_or_else(|error| panic!("{} shred failed: {error}", machine.name));
        assert_eq!(
            shredded, 1,
            "{} expiry must shred one cache row",
            machine.name
        );
        assert_eq!(
            verdict_at_path(
                &machine.ledger_path,
                &KEY,
                &machine.scope_key,
                &machine.message_id,
                now
            ),
            ExpiryVerdict::Expired,
            "{} expiry ledger must no longer allow reads",
            machine.name
        );
        expired += prune.expired;
    }
    expired
}
