#![cfg(feature = "core")]

use std::time::Duration;

use message_lifecycle::AcceptedPart;
use osl_privacy_hub::message_expiry::{
    absolute_release, note_delivered_at_path, prune_at_path, resolve_expiry_time, verdict_at_path,
    ExpiryTimeDecision, ExpiryTimeInput, ExpiryTimeSource,
};
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;
use uuid::Uuid;

const KEY: [u8; 32] = [0x78; 32];
const ISSUED_AT: i64 = 1_830_000_000;
const DUE_AT: i64 = ISSUED_AT + 3_600;
const SKEW_SECONDS: i64 = 300;

#[derive(Clone, Copy)]
enum Connectivity {
    Online,
    Offline,
}

impl Connectivity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Offline => "offline",
        }
    }

    fn expected_source(self) -> ExpiryTimeSource {
        match self {
            Self::Online => ExpiryTimeSource::TrustedService,
            Self::Offline => ExpiryTimeSource::LastTrustedServicePlusElapsed,
        }
    }
}

#[derive(Clone, Copy)]
enum Skew {
    ServiceAhead,
    ServiceBehind,
}

impl Skew {
    fn as_str(self) -> &'static str {
        match self {
            Self::ServiceAhead => "service-ahead",
            Self::ServiceBehind => "service-behind",
        }
    }

    fn machine_wall_for(self, trusted_now: i64) -> i64 {
        match self {
            Self::ServiceAhead => trusted_now - SKEW_SECONDS,
            Self::ServiceBehind => trusted_now + SKEW_SECONDS,
        }
    }
}

struct CaseReport {
    label: String,
    before_count: usize,
    after_count: usize,
    selected_before: i64,
    selected_after: i64,
    service_minus_machine_seconds: i64,
    wall_time_winner_count: usize,
    expired_first_pass: usize,
    expired_second_pass: usize,
    shredded_first_pass: usize,
    shredded_second_pass: usize,
    source: ExpiryTimeSource,
    wall_before: i64,
    wall_changed: i64,
    wall_after: i64,
}

#[test]
fn task_3781_disagree_machine_and_service_clocks() {
    let root = TempDir::new().expect("temp task 3781 root");
    let mut reports = Vec::new();

    for connectivity in [Connectivity::Online, Connectivity::Offline] {
        for skew in [Skew::ServiceAhead, Skew::ServiceBehind] {
            reports.push(run_case(root.path(), connectivity, skew));
        }
    }

    let before_total: usize = reports.iter().map(|report| report.before_count).sum();
    let after_total: usize = reports.iter().map(|report| report.after_count).sum();
    let wall_time_winner_total: usize = reports
        .iter()
        .map(|report| report.wall_time_winner_count)
        .sum();
    let expires_once_total = reports
        .iter()
        .filter(|report| report.expired_first_pass == 1 && report.expired_second_pass == 0)
        .count();

    assert_eq!(reports.len(), 4);
    assert_eq!(before_total, 4);
    assert_eq!(after_total, 0);
    assert_eq!(wall_time_winner_total, 0);
    assert_eq!(expires_once_total, 4);

    for report in &reports {
        assert_eq!(report.before_count, 1, "{} before count", report.label);
        assert_eq!(report.after_count, 0, "{} after count", report.label);
        assert_eq!(
            report.selected_before,
            DUE_AT - 1,
            "{} selected before",
            report.label
        );
        assert_eq!(
            report.selected_after, DUE_AT,
            "{} selected after",
            report.label
        );
        assert_eq!(
            report.service_minus_machine_seconds.abs(),
            SKEW_SECONDS,
            "{} five-minute skew",
            report.label
        );
        assert_eq!(
            report.wall_time_winner_count, 0,
            "{} wall winner",
            report.label
        );
        assert_eq!(
            report.expired_first_pass, 1,
            "{} first expiry",
            report.label
        );
        assert_eq!(
            report.expired_second_pass, 0,
            "{} second expiry",
            report.label
        );
        assert_eq!(
            report.shredded_first_pass, 1,
            "{} first shred",
            report.label
        );
        assert_eq!(
            report.shredded_second_pass, 0,
            "{} second shred",
            report.label
        );
        assert_ne!(
            report.wall_before, report.wall_changed,
            "{} machine wall time changed",
            report.label
        );
    }

    for report in &reports {
        println!(
            "TASK3781_CASE={} SOURCE={:?} SERVICE_MINUS_MACHINE_SECONDS={} WALL_BEFORE={} WALL_CHANGED={} WALL_AFTER={} SELECTED_BEFORE={} SELECTED_AFTER={} BEFORE_MARK_COUNT={} AFTER_MARK_COUNT={} WALL_TIME_WINNER_COUNT={} EXPIRED_FIRST_PASS={} EXPIRED_SECOND_PASS={} SHREDDED_FIRST_PASS={} SHREDDED_SECOND_PASS={}",
            report.label,
            report.source,
            report.service_minus_machine_seconds,
            report.wall_before,
            report.wall_changed,
            report.wall_after,
            report.selected_before,
            report.selected_after,
            report.before_count,
            report.after_count,
            report.wall_time_winner_count,
            report.expired_first_pass,
            report.expired_second_pass,
            report.shredded_first_pass,
            report.shredded_second_pass
        );
    }
    println!("TASK3781_CASE_COUNT={}", reports.len());
    println!("TASK3781_BEFORE_MARK_TOTAL={before_total}");
    println!("TASK3781_AFTER_MARK_TOTAL={after_total}");
    println!("TASK3781_WALL_TIME_WINNER_TOTAL={wall_time_winner_total}");
    println!("TASK3781_EXPIRES_ONCE_TOTAL={expires_once_total}");
    println!("TASK3781_ONLINE_SOURCE=TrustedService");
    println!("TASK3781_OFFLINE_SOURCE=LastTrustedServicePlusElapsed");
}

fn run_case(root: &std::path::Path, connectivity: Connectivity, skew: Skew) -> CaseReport {
    let label = format!("{}-{}", connectivity.as_str(), skew.as_str());
    let case_root = root.join(label.as_str());
    std::fs::create_dir_all(&case_root).expect("create case root");
    let ledger_path = case_root.join("message_open_clock.json");
    let store = MessageStore::open(&case_root.join("store"), &KEY).expect("open message store");
    let message_id = format!("task-3781-{}", Uuid::new_v4());
    let scope_key = format!("dm:task3781:{}", connectivity.as_str());
    let marked_text = format!("TASK3781-MARK-{}-{}", label, Uuid::new_v4());

    store
        .put(&StoredMessage {
            discord_message_id: message_id.clone(),
            channel_id: "task3781-channel".to_owned(),
            sender_discord_id: "task3781-sender".to_owned(),
            sender_osl_user_id: "task3781-osl-sender".to_owned(),
            plaintext: marked_text.clone(),
            decrypted_at: ISSUED_AT,
            burned: false,
        })
        .expect("store marked message");

    note_delivered_at_path(
        &ledger_path,
        &KEY,
        &scope_key,
        &message_id,
        absolute_release(ISSUED_AT, ipc::cipher_store_client::TTL_1H).expect("one-hour release"),
        vec![AcceptedPart {
            index: 0,
            sealed_bytes: 512,
            digest: [0x78; 32],
        }],
        [0x79; 32],
        Some(message_id.clone()),
        ISSUED_AT,
    )
    .expect("record expiry ledger");

    let before_wall = skew.machine_wall_for(DUE_AT - 1);
    let before = resolve(connectivity, before_wall, DUE_AT - 1);
    assert_eq!(before.source, connectivity.expected_source());
    assert!(verdict_at_path(
        &ledger_path,
        &KEY,
        &scope_key,
        &message_id,
        before.now_unix_secs
    )
    .is_readable());
    let before_count = mark_count(&store, &message_id, &marked_text);

    let changed_wall = if before_wall < DUE_AT {
        DUE_AT + 900
    } else {
        DUE_AT - 900
    };
    let changed = resolve(connectivity, changed_wall, DUE_AT - 1);
    assert_eq!(changed.now_unix_secs, DUE_AT - 1);
    assert_eq!(changed.source, connectivity.expected_source());
    assert!(verdict_at_path(
        &ledger_path,
        &KEY,
        &scope_key,
        &message_id,
        changed.now_unix_secs
    )
    .is_readable());

    let after_wall = skew.machine_wall_for(DUE_AT);
    let after = resolve(connectivity, after_wall, DUE_AT);
    assert_eq!(after.source, connectivity.expected_source());
    let first_prune = prune_at_path(&ledger_path, &KEY, after.now_unix_secs).expect("first prune");
    let shredded_first_pass = store
        .shred_expired_messages(&first_prune.shred_cache_ids)
        .expect("first shred");
    let after_count = mark_count(&store, &message_id, &marked_text);
    let second_prune =
        prune_at_path(&ledger_path, &KEY, after.now_unix_secs + 30).expect("second prune");
    let shredded_second_pass = store
        .shred_expired_messages(&second_prune.shred_cache_ids)
        .expect("second shred");

    CaseReport {
        label,
        before_count,
        after_count,
        selected_before: before.now_unix_secs,
        selected_after: after.now_unix_secs,
        service_minus_machine_seconds: (DUE_AT - 1) - before_wall,
        wall_time_winner_count: [before, changed, after]
            .into_iter()
            .filter(|decision| decision.wall_time_won())
            .count(),
        expired_first_pass: first_prune.expired,
        expired_second_pass: second_prune.expired,
        shredded_first_pass,
        shredded_second_pass,
        source: before.source,
        wall_before: before_wall,
        wall_changed: changed_wall,
        wall_after: after_wall,
    }
}

fn resolve(
    connectivity: Connectivity,
    machine_wall_unix_secs: i64,
    trusted_now: i64,
) -> ExpiryTimeDecision {
    let input = match connectivity {
        Connectivity::Online => ExpiryTimeInput {
            machine_wall_unix_secs,
            trusted_service_unix_secs: Some(trusted_now),
            last_trusted_service_unix_secs: None,
            elapsed_since_last_trusted: None,
        },
        Connectivity::Offline => ExpiryTimeInput {
            machine_wall_unix_secs,
            trusted_service_unix_secs: None,
            last_trusted_service_unix_secs: Some(ISSUED_AT),
            elapsed_since_last_trusted: Some(Duration::from_secs(
                trusted_now.saturating_sub(ISSUED_AT) as u64,
            )),
        },
    };
    resolve_expiry_time(input).expect("trusted expiry time")
}

fn mark_count(store: &MessageStore, message_id: &str, marked_text: &str) -> usize {
    store
        .get(message_id)
        .expect("read message")
        .filter(|message| message.plaintext == marked_text)
        .map(|_| 1)
        .unwrap_or(0)
}
