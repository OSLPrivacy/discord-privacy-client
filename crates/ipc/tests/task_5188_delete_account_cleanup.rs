use ipc::usage_counters::{
    AccountDataWriter, PurgedStoppedAccount, ServiceDataCounts, ServiceDataKind, UsageCounterStore,
    LOST_KEY_NAME_CLAIM_REFUSAL, SHIPPING_ACCOUNT_DATA_WRITERS, STOPPED_ACCOUNT_RETENTION_DAYS,
};
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};
use std::fs;

const DATABASE_FILE: &str = "person_usage.sqlite";
const STOPPED_AT: i64 = 50_000 * 86_400;
const MINUTE: i64 = 60;
const CURRENT: &str = "task-5188-current";
const OTHER: &str = "task-5188-other";
const LOST_KEY_NAME: &str = "task_5188_lost_key_name";

fn total(counts: ServiceDataCounts) -> u64 {
    counts.messages
        + counts.files
        + counts.keys
        + counts.settings
        + counts.sessions
        + counts.account_records
}

fn inventory_from_shipping_writers(fault: &str) -> Vec<AccountDataWriter> {
    let mut inventory = SHIPPING_ACCOUNT_DATA_WRITERS.to_vec();
    if fault == "empty-inventory" {
        inventory.clear();
    } else if fault == "omitted-writer" {
        inventory.retain(|writer| writer.location != "relay_held_undelivered_ciphertexts");
    }
    assert!(
        !inventory.is_empty(),
        "empty inventory location=writer_inventory count=0"
    );
    inventory
}

fn independently_discovered_locations(conn: &Connection) -> Vec<String> {
    let mut statement = conn
        .prepare(
            "SELECT name FROM sqlite_master
              WHERE type = 'table'
                AND (name = 'stored_files'
                     OR name = 'relay_held_undelivered_ciphertexts'
                     OR name LIKE 'service_%')
              ORDER BY name",
        )
        .expect("prepare independent schema inventory");
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("scan independent schema inventory")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("materialize independent schema inventory")
}

fn reconcile_inventory(
    inventory: &[AccountDataWriter],
    discovered: &[String],
) -> HashMap<String, ServiceDataKind> {
    let classified: HashMap<_, _> = inventory
        .iter()
        .map(|writer| (writer.location.to_owned(), writer.kind))
        .collect();
    for location in discovered {
        assert!(
            classified.contains_key(location),
            "omitted writer location={location} count=1"
        );
    }
    for writer in inventory {
        assert!(
            discovered
                .iter()
                .any(|location| location == writer.location),
            "writer location={} absent_from_schema count=0",
            writer.location
        );
    }
    assert_eq!(
        classified.len(),
        discovered.len(),
        "writer/schema location count divergence writers={} discovered={}",
        classified.len(),
        discovered.len()
    );
    let classes: HashSet<_> = inventory.iter().map(|writer| writer.kind).collect();
    let ruled: HashSet<_> = ServiceDataKind::ALL.into_iter().collect();
    assert_eq!(
        classes,
        ruled,
        "ruled account-data classes count={} expected=6",
        classes.len()
    );
    classified
}

fn semantic_seed_ids(location: &str) -> &'static [&'static str] {
    match location {
        "service_messages" => &["stored-ciphertext"],
        "relay_held_undelivered_ciphertexts" => &["relay-undelivered-ciphertext"],
        "stored_files" => &["encrypted-file", "encrypted-cover"],
        "service_keys" => &["server-public-key", "relay-session-key"],
        "service_settings" => &["account-settings"],
        "service_sessions" => &["device-session", "device-token"],
        "service_account_records" => &["account-row", "live-name-binding"],
        other => panic!("unclassified location={other} count=1"),
    }
}

fn seed_every_location(store: &mut UsageCounterStore, discovered: &[String], account: &str) {
    for location in discovered {
        for data_id in semantic_seed_ids(location) {
            store
                .store_account_data_at_location(account, location, data_id)
                .unwrap_or_else(|error| {
                    panic!("seed failure location={location} count=0 exception={error}")
                });
        }
    }
}

fn location_counts(
    conn: &Connection,
    discovered: &[String],
    account: &str,
) -> HashMap<String, u64> {
    discovered
        .iter()
        .map(|location| {
            let sql = format!("SELECT COUNT(*) FROM {location} WHERE person_id = ?1");
            let count = conn
                .query_row(&sql, params![account], |row| row.get::<_, u64>(0))
                .unwrap_or_else(|error| {
                    panic!("scan failure location={location} exception={error}")
                });
            (location.clone(), count)
        })
        .collect()
}

fn assert_every_location_seeded(counts: &HashMap<String, u64>) {
    for (location, count) in counts {
        assert!(
            *count >= 1,
            "empty discovered location={location} count={count}"
        );
    }
}

fn assert_every_location_empty(counts: &HashMap<String, u64>) {
    for (location, count) in counts {
        assert_eq!(
            *count, 0,
            "retained discovered item location={location} count={count}"
        );
    }
}

fn payment_schema_rows(conn: &Connection) -> u64 {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master
          WHERE lower(name) LIKE '%payment%'
             OR lower(name) LIKE '%billing%'
             OR lower(name) LIKE '%invoice%'
             OR lower(COALESCE(sql, '')) LIKE '%card_number%'",
        [],
        |row| row.get(0),
    )
    .expect("scan schema for payment-data rows")
}

fn payment_store_rows(conn: &Connection) -> u64 {
    let mut statement = conn
        .prepare(
            "SELECT name FROM sqlite_master
              WHERE type = 'table'
                AND (lower(name) LIKE '%payment%'
                     OR lower(name) LIKE '%billing%'
                     OR lower(name) LIKE '%invoice%')",
        )
        .expect("prepare payment store scan");
    let names = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("scan payment store table names")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("materialize payment store table names");
    names
        .iter()
        .map(|name| {
            conn.query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |row| {
                row.get::<_, u64>(0)
            })
            .expect("count payment store rows")
        })
        .sum()
}

fn payment_log_rows(log: &str) -> u64 {
    log.lines()
        .filter(|line| {
            let line = line.to_ascii_lowercase();
            line.contains("card_number=")
                || line.contains("payment_token=")
                || line.contains("billing_address=")
                || line.contains("invoice_id=")
        })
        .count() as u64
}

fn assert_receipt_matches_scan(receipt: &PurgedStoppedAccount, scan: ServiceDataCounts) {
    assert_eq!(
        receipt.remaining,
        scan,
        "receipt divergence location=post_cleanup_storage receipt_count={} scan_count={}",
        total(receipt.remaining),
        total(scan)
    );
}

#[test]
fn task_5188_finishes_delete_account_at_the_ruled_boundary() {
    let fault = std::env::var("TASK5188_FAULT").unwrap_or_default();
    let dir = tempfile::tempdir().expect("temporary TASK 5188 store");
    let mut store = UsageCounterStore::open(dir.path()).expect("open TASK 5188 store");
    let audit = Connection::open(dir.path().join(DATABASE_FILE))
        .expect("open independent TASK 5188 storage scan");

    let inventory = inventory_from_shipping_writers(&fault);
    let discovered = independently_discovered_locations(&audit);
    let classified = reconcile_inventory(&inventory, &discovered);
    assert_eq!(classified.len(), 7, "writer-derived location count");

    seed_every_location(&mut store, &discovered, CURRENT);
    seed_every_location(&mut store, &discovered, OTHER);
    store
        .store_lost_key_name_tombstone(LOST_KEY_NAME, STOPPED_AT - 1)
        .expect("seed separate permanent lost-key name tombstone");
    store
        .stop_account_at(CURRENT, STOPPED_AT)
        .expect("stop account at deterministic boundary");

    let before_locations = location_counts(&audit, &discovered, CURRENT);
    let other_before_locations = location_counts(&audit, &discovered, OTHER);
    assert_every_location_seeded(&before_locations);
    assert_every_location_seeded(&other_before_locations);
    let before = store
        .service_data_counts(CURRENT)
        .expect("scan before cleanup");
    let other_before = store
        .service_data_counts(OTHER)
        .expect("scan other before cleanup");
    let relay_before = before_locations["relay_held_undelivered_ciphertexts"];
    assert_eq!(before.messages, 2, "messages must include relay-held row");
    assert_eq!(relay_before, 1, "relay-held undelivered row before cleanup");

    let schema_payment_before = payment_schema_rows(&audit);
    let store_payment_before = payment_store_rows(&audit);
    let log_before = format!(
        "event=account_stopped account={CURRENT} inventory_total={} tombstone_count=1",
        total(before)
    );
    let log_payment_before = payment_log_rows(&log_before);
    assert_eq!(
        schema_payment_before, 0,
        "payment schema rows before cleanup"
    );
    assert_eq!(store_payment_before, 0, "payment store rows before cleanup");
    assert_eq!(log_payment_before, 0, "payment log rows before cleanup");

    let deadline = STOPPED_AT + STOPPED_ACCOUNT_RETENTION_DAYS * 86_400;
    let early_receipts = store
        .purge_due_stopped_accounts_at(deadline - MINUTE)
        .expect("run cleanup one minute before seven days");
    assert!(
        early_receipts.is_empty(),
        "early cleanup receipt count must be 0"
    );
    let one_minute_before = store
        .service_data_counts(CURRENT)
        .expect("scan one minute before seven days");
    assert_eq!(
        total(one_minute_before),
        total(before),
        "seven-days-minus-one-minute inventory changed before={} after={}",
        total(before),
        total(one_minute_before)
    );

    let mut receipts = store
        .purge_due_stopped_accounts_at(deadline + MINUTE)
        .expect("run cleanup one minute after seven days");
    assert_eq!(receipts.len(), 1, "cleanup receipt account count");
    let mut receipt = receipts.remove(0);
    assert_eq!(receipt.person_id, CURRENT, "cleanup receipt account");
    assert_eq!(
        receipt.deleted, before,
        "cleanup receipt deleted inventory mismatch"
    );

    if fault == "retained-item" {
        audit
            .execute("PRAGMA foreign_keys = OFF", [])
            .expect("disable audit foreign keys for retained-item fault");
        audit
            .execute(
                "INSERT INTO service_settings (person_id, data_id) VALUES (?1, 'retained-fault')",
                params![CURRENT],
            )
            .expect("inject retained discovered item fault");
    } else if fault == "receipt-divergence" {
        receipt.remaining.account_records = 1;
    } else if fault == "deleted-tombstone" {
        audit
            .execute("DELETE FROM lost_key_name_tombstones", [])
            .expect("inject deleted tombstone fault");
    }

    let after_locations = location_counts(&audit, &discovered, CURRENT);
    assert_every_location_empty(&after_locations);
    let after = store
        .service_data_counts(CURRENT)
        .expect("post-cleanup storage scan");
    let other_after = store
        .service_data_counts(OTHER)
        .expect("post-cleanup other-account scan");
    let relay_after = after_locations["relay_held_undelivered_ciphertexts"];
    assert_eq!(total(after), 0, "seven-days-plus-one-minute total");
    assert_eq!(relay_after, 0, "relay-held undelivered row after cleanup");
    assert_eq!(
        total(other_after),
        total(other_before),
        "other account inventory changed before={} after={}",
        total(other_before),
        total(other_after)
    );
    assert_eq!(
        location_counts(&audit, &discovered, OTHER),
        other_before_locations,
        "other account location scan changed"
    );
    assert_receipt_matches_scan(&receipt, after);

    let tombstones = store
        .lost_key_name_tombstone_count()
        .expect("scan permanent lost-key tombstone after cleanup");
    assert_eq!(
        tombstones, 1,
        "deleted tombstone location=lost_key_name_tombstones count={tombstones}"
    );
    assert_eq!(
        receipt.lost_key_name_tombstones, 1,
        "receipt divergence location=lost_key_name_tombstones receipt_count={} scan_count=1",
        receipt.lost_key_name_tombstones
    );
    let reclaim = store
        .claim_public_name(LOST_KEY_NAME)
        .expect_err("permanent lost-key name reclaim must be refused")
        .to_string();
    assert_eq!(reclaim, LOST_KEY_NAME_CLAIM_REFUSAL, "ruling 3120 wording");

    let schema_payment_after = payment_schema_rows(&audit);
    let store_payment_after = payment_store_rows(&audit);
    let cleanup_log = format!(
        "event=account_cleanup account={} deleted={} remaining={} tombstones={}",
        receipt.person_id,
        total(receipt.deleted),
        total(receipt.remaining),
        receipt.lost_key_name_tombstones
    );
    let log_path = dir.path().join("osl-account-cleanup.log");
    fs::write(&log_path, &cleanup_log).expect("write cleanup receipt log");
    let log_payment_after =
        payment_log_rows(&fs::read_to_string(&log_path).expect("read cleanup receipt log"));
    assert_eq!(schema_payment_after, 0, "payment schema rows after cleanup");
    assert_eq!(store_payment_after, 0, "payment store rows after cleanup");
    assert_eq!(log_payment_after, 0, "payment log rows after cleanup");

    println!(
        "TASK5188 PASS inventory_locations={} ruled_classes=6 unclassified_locations=0 seeded_locations={} exact_total_before={} seven_days_minus_one_minute={} seven_days_plus_one_minute={} messages_before={} relay_held_before={} relay_held_after={} tombstones_after={} reclaim_status=refused reclaim_wording={:?} other_before={} other_after={} receipt_remaining={} post_scan={} payment_schema_before={} payment_store_before={} payment_log_before={} payment_schema_after={} payment_store_after={} payment_log_after={}",
        inventory.len(),
        before_locations.values().filter(|count| **count >= 1).count(),
        total(before),
        total(one_minute_before),
        total(after),
        before.messages,
        relay_before,
        relay_after,
        tombstones,
        reclaim,
        total(other_before),
        total(other_after),
        total(receipt.remaining),
        total(after),
        schema_payment_before,
        store_payment_before,
        log_payment_before,
        schema_payment_after,
        store_payment_after,
        log_payment_after,
    );
}
