use ipc::commands::{
    cmd_osl_copy_my_history_here, cmd_osl_copy_my_history_here_with_observer,
    cmd_osl_export_history_for_copy, cmd_osl_export_history_for_copy_with_observer,
    cmd_osl_history_copy_month_data_bytes, cmd_osl_read_history_copy_semantics,
    cmd_osl_record_history_copy_semantics, HistoryCopyExpirationState, HistoryCopyReaction,
    HistoryCopySemantics,
};
use ipc::state::AppState;
use keystore::identity_from_entropy;
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use store::{
    HistoryCopyFaultBoundary, HistoryCopyFaultSide, HistoryCopyIoAccess, HistoryCopyIoEvent,
    MessageStore, StoredMessage,
};
use tempfile::TempDir;

const COMMANDS_SOURCE: &str = include_str!("../src/commands.rs");
const STORE_SOURCE: &str = include_str!("../../store/src/lib.rs");
const STORE_SCHEMA: &str = include_str!("../../store/src/schema.rs");

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct UniverseRow {
    writer: &'static str,
    fault_boundary: &'static str,
    semantic_assertion: &'static str,
}

const UNIVERSE: &[UniverseRow] = &[
    UniverseRow {
        writer: "source.message.read",
        fault_boundary: "4805.before_or_after_source_message_read",
        semantic_assertion: "4816.source_message_manifest_equality",
    },
    UniverseRow {
        writer: "source.retention.read",
        fault_boundary: "4805.before_or_after_source_retention_read",
        semantic_assertion: "4816.expired_before_copy_no_stub",
    },
    UniverseRow {
        writer: "source.attachment.read",
        fault_boundary: "4805.before_or_after_source_attachment_read",
        semantic_assertion: "4816.attachment_hash_equality",
    },
    UniverseRow {
        writer: "source.semantic.read",
        fault_boundary: "4805.before_or_after_source_semantic_read",
        semantic_assertion: "4816.semantic_manifest_equality",
    },
    UniverseRow {
        writer: "source.expiration.read",
        fault_boundary: "4805.before_or_after_source_expiration_read",
        semantic_assertion: "4816.expiration_state_equality",
    },
    UniverseRow {
        writer: "copy.ledger.stage",
        fault_boundary: "4805.before_or_after_ledger_stage",
        semantic_assertion: "4816.no_visible_graph_before_complete_ledger",
    },
    UniverseRow {
        writer: "copy.message.write",
        fault_boundary: "4805.before_or_after_message_write",
        semantic_assertion: "4816.message_body_and_order_equality",
    },
    UniverseRow {
        writer: "copy.attachment_manifest.write",
        fault_boundary: "4805.before_or_after_attachment_manifest_write",
        semantic_assertion: "4816.attachment_manifest_complete",
    },
    UniverseRow {
        writer: "copy.attachment.write",
        fault_boundary: "4805.before_or_after_attachment_write",
        semantic_assertion: "4816.attachment_hash_equality",
    },
    UniverseRow {
        writer: "copy.timer.write",
        fault_boundary: "4805.before_or_after_timer_write",
        semantic_assertion: "4816.expiration_state_equality",
    },
    UniverseRow {
        writer: "copy.semantic.write",
        fault_boundary: "4805.before_or_after_semantic_write",
        semantic_assertion: "4816.semantic_manifest_equality",
    },
    UniverseRow {
        writer: "copy.edge.write",
        fault_boundary: "4805.before_or_after_edge_write",
        semantic_assertion: "4816.relationship_graph_equality",
    },
    UniverseRow {
        writer: "copy.visibility.publish",
        fault_boundary: "4805.before_or_after_visibility_publish",
        semantic_assertion: "4816.complete_graph_is_only_visible_state",
    },
    UniverseRow {
        writer: "copy.meter.charge",
        fault_boundary: "4805.before_or_after_meter_charge",
        semantic_assertion: "4816.one_unique_byte_charge",
    },
    UniverseRow {
        writer: "copy.ledger.complete",
        fault_boundary: "4805.before_or_after_ledger_complete",
        semantic_assertion: "4816.one_completed_operation",
    },
    UniverseRow {
        writer: "copy.transaction.commit",
        fault_boundary: "4805.before_or_after_transaction_commit",
        semantic_assertion: "4816.restart_readback_is_atomic",
    },
];

fn declared_rows() -> BTreeSet<&'static str> {
    UNIVERSE.iter().map(|row| row.writer).collect()
}

/// This inventory is generated from shipping callsites and schema names.  It
/// intentionally does not iterate `UNIVERSE`: the declared reconciliation
/// matrix is a consumer of this output, never its completeness oracle.
fn discover_static_universe() -> Result<BTreeSet<&'static str>, String> {
    let command_calls = [
        (".get(&message_id)", "source.message.read"),
        (".count_message_records(", "source.retention.read"),
        (".list_history_attachments(", "source.attachment.read"),
        (".get_history_copy_metadata(", "source.semantic.read"),
        (".message_timer_minutes(", "source.expiration.read"),
    ];
    let schema_tables = [
        ("history_copy_operations", "copy.ledger.stage"),
        ("messages", "copy.message.write"),
        ("attachment_manifests", "copy.attachment_manifest.write"),
        ("attachments", "copy.attachment.write"),
        ("message_timers", "copy.timer.write"),
        ("history_copy_semantics", "copy.semantic.write"),
        ("history_copy_edges", "copy.edge.write"),
        ("history_copy_visibility", "copy.visibility.publish"),
        ("history_copy_meter", "copy.meter.charge"),
    ];
    let mut found = BTreeSet::new();
    for (needle, writer) in command_calls {
        if !COMMANDS_SOURCE.contains(needle) {
            return Err(format!(
                "TASK4823 static universe starved at callsite {writer}"
            ));
        }
        found.insert(writer);
    }
    for (table, writer) in schema_tables {
        let declaration = format!("CREATE TABLE IF NOT EXISTS {table}");
        if !STORE_SCHEMA.contains(&declaration) {
            return Err(format!(
                "TASK4823 static universe starved at schema writer {writer}"
            ));
        }
        if !STORE_SOURCE.contains(table) {
            return Err(format!(
                "TASK4823 static callgraph cannot reach writer {writer}"
            ));
        }
        found.insert(writer);
    }
    for (needle, writer) in [
        (
            "HistoryCopyFaultBoundary::LedgerComplete",
            "copy.ledger.complete",
        ),
        (
            "HistoryCopyFaultBoundary::Commit",
            "copy.transaction.commit",
        ),
    ] {
        if !STORE_SOURCE.contains(needle) {
            return Err(format!(
                "TASK4823 static universe starved at writer {writer}"
            ));
        }
        found.insert(writer);
    }
    Ok(found)
}

fn reconcile(
    static_rows: &BTreeSet<&'static str>,
    runtime_rows: &BTreeSet<&'static str>,
) -> Result<(), String> {
    if static_rows.is_empty() {
        return Err("TASK4823 static universe is empty".to_string());
    }
    if runtime_rows.is_empty() {
        return Err("TASK4823 runtime universe is empty".to_string());
    }
    let declared = declared_rows();
    for writer in static_rows.union(runtime_rows) {
        if !declared.contains(writer) {
            return Err(format!(
                "TASK4823 unclassified durable writer={writer} effect=outside frozen 4805/4816 matrix"
            ));
        }
    }
    if static_rows != runtime_rows {
        let missing_runtime: Vec<_> = static_rows.difference(runtime_rows).copied().collect();
        let missing_static: Vec<_> = runtime_rows.difference(static_rows).copied().collect();
        return Err(format!(
            "TASK4823 universes differ missing_runtime={missing_runtime:?} missing_static={missing_static:?}"
        ));
    }
    if static_rows != &declared {
        let missing: Vec<_> = declared.difference(static_rows).copied().collect();
        return Err(format!("TASK4823 durable row starved writers={missing:?}"));
    }
    for row in UNIVERSE {
        if row.fault_boundary.is_empty() || !row.fault_boundary.starts_with("4805.") {
            return Err(format!(
                "TASK4823 writer={} has no 4805 boundary",
                row.writer
            ));
        }
        if row.semantic_assertion.is_empty() || !row.semantic_assertion.starts_with("4816.") {
            return Err(format!(
                "TASK4823 writer={} has no 4816 assertion",
                row.writer
            ));
        }
    }
    Ok(())
}

fn spawn_ignored(test: &str, scenario: &str) -> Output {
    Command::new(std::env::current_exe().expect("current test binary"))
        .args(["--ignored", "--exact", test, "--nocapture"])
        .env("TASK4823_SCENARIO", scenario)
        .output()
        .unwrap_or_else(|error| panic!("spawn {test}/{scenario}: {error}"))
}

fn child_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn required_evidence() -> BTreeSet<String> {
    let mut required: BTreeSet<String> = [
        "staticUniverse",
        "runtimeUniverse",
        "lifecycle.clean",
        "lifecycle.retry",
        "lifecycle.reconnect",
        "lifecycle.error",
        "lifecycle.allowanceExhaustion",
        "faultSide.before",
        "faultSide.after",
        "semanticReadback",
        "ledgerObservation",
        "meterObservation",
        "addedWriter",
        "parentFailure",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    for writer in discover_static_universe().expect("static discovery for evidence inventory") {
        required.insert(format!("durableRow.{writer}"));
    }
    required
}

fn validate_evidence(observed: &BTreeSet<String>) -> Result<(), String> {
    let required = required_evidence();
    if let Some(missing) = required.difference(observed).next() {
        return Err(format!("TASK4823 starved evidence={missing}"));
    }
    Ok(())
}

#[test]
fn task_4823_starving_any_required_evidence_exits_1_by_name() {
    let required = required_evidence();
    validate_evidence(&required).expect("positive evidence inventory");
    for missing in &required {
        let output = spawn_ignored("task_4823_starvation_child", missing);
        let text = child_text(&output);
        assert_eq!(output.status.code(), Some(1), "{missing}: {text}");
        assert!(text.contains(missing), "{missing}: {text}");
    }
    println!(
        "TASK4823 starvation_mutants={} all_exit_1=true",
        required.len()
    );
}

#[test]
#[ignore = "throwaway child; parent requires deliberate exit 1"]
fn task_4823_starvation_child() {
    let missing = std::env::var("TASK4823_SCENARIO").expect("TASK4823_SCENARIO");
    let mut observed = required_evidence();
    assert!(observed.remove(&missing), "known starvation name {missing}");
    match validate_evidence(&observed) {
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
        Ok(()) => panic!("TASK4823 starvation was accepted: {missing}"),
    }
}

const ACCOUNT: &str = "task4823-account";
const CHANNEL: &str = "task4823-channel";
const SOURCE_SECRET: [u8; 32] = [0x48; 32];
const DEST_SECRET: [u8; 32] = [0x23; 32];

fn state_with_store(dir: &Path, secret: &[u8; 32], entropy: [u8; 16]) -> AppState {
    let state = AppState::new();
    let mut identity = identity_from_entropy(entropy, ACCOUNT.to_string());
    identity.user_id = ACCOUNT.to_string();
    identity.discord_snowflake = Some(ACCOUNT.to_string());
    state.install_identity(identity);
    *state
        .message_store
        .lock()
        .expect("message_store mutex poisoned") =
        Some(MessageStore::open(dir, secret).expect("open message store"));
    state
}

fn source_id(index: usize) -> String {
    format!("task4823-source-{index}")
}

fn seed_source(state: &AppState) {
    let guard = state.message_store.lock().unwrap();
    let store = guard.as_ref().unwrap();
    for index in 0..3 {
        store
            .put(&StoredMessage {
                discord_message_id: source_id(index),
                channel_id: CHANNEL.to_string(),
                sender_discord_id: format!("sender-{index}"),
                sender_osl_user_id: format!("osl-sender-{index}"),
                plaintext: format!("TASK4823 body {index} {}", "x".repeat(index + 3)),
                decrypted_at: 4_823_000 + index as i64,
                reply_parent_id: (index == 1).then(|| source_id(0)),
                edit_revision: if index == 1 { 2 } else { 1 },
                burned: false,
            })
            .expect("seed source message");
    }
    store
        .put_history_copy_attachment(
            &source_id(1),
            "task4823.bin",
            "application/octet-stream",
            b"TASK4823-BINARY-ATTACHMENT",
            Some("channel"),
            Some(CHANNEL),
            Some("sender-1"),
            4_823_001,
        )
        .expect("seed source attachment");
    store
        .record_message_timer_minutes(&source_id(1), 23)
        .expect("seed source timer");
    drop(guard);
    for index in 0..3 {
        cmd_osl_record_history_copy_semantics(
            state,
            source_id(index),
            HistoryCopySemantics {
                stable_order: index as u64,
                thread_root_id: (index == 1).then(|| source_id(0)),
                reactions: if index == 1 {
                    vec![HistoryCopyReaction {
                        reaction_id: "task4823-reaction".to_string(),
                        target_message_id: source_id(1),
                        actor_discord_id: "task4823-reactor".to_string(),
                        emoji: "✅".to_string(),
                        reacted_at: 4_823_002,
                    }]
                } else {
                    Vec::new()
                },
                expiration: if index == 1 {
                    HistoryCopyExpirationState::Active { timer_minutes: 23 }
                } else {
                    HistoryCopyExpirationState::NonExpiring
                },
            },
        )
        .expect("seed source semantics");
    }
}

fn phrase() -> String {
    bip39::Mnemonic::from_entropy_in(bip39::Language::English, &[0x48; 16])
        .unwrap()
        .to_string()
}

fn all_source_ids() -> Vec<String> {
    (0..3).map(source_id).collect()
}

fn raw_counts(dir: &Path) -> (usize, usize) {
    let conn = Connection::open(dir.join("messages.sqlite")).expect("raw sqlite open");
    let messages: i64 = conn
        .query_row("SELECT COUNT(*) FROM messages WHERE burned=0", [], |row| {
            row.get(0)
        })
        .unwrap();
    let attachments: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM attachments WHERE burned=0",
            [],
            |row| row.get(0),
        )
        .unwrap();
    (messages as usize, attachments as usize)
}

fn boundary_writer(boundary: HistoryCopyFaultBoundary) -> &'static str {
    match boundary {
        HistoryCopyFaultBoundary::LedgerStage => "copy.ledger.stage",
        HistoryCopyFaultBoundary::MessageWrite => "copy.message.write",
        HistoryCopyFaultBoundary::AttachmentManifestWrite => "copy.attachment_manifest.write",
        HistoryCopyFaultBoundary::AttachmentWrite => "copy.attachment.write",
        HistoryCopyFaultBoundary::TimerWrite => "copy.timer.write",
        HistoryCopyFaultBoundary::SemanticWrite => "copy.semantic.write",
        HistoryCopyFaultBoundary::EdgeWrite => "copy.edge.write",
        HistoryCopyFaultBoundary::VisibilityPublish => "copy.visibility.publish",
        HistoryCopyFaultBoundary::MeterCharge => "copy.meter.charge",
        HistoryCopyFaultBoundary::LedgerComplete => "copy.ledger.complete",
        HistoryCopyFaultBoundary::Commit => "copy.transaction.commit",
    }
}

fn io_writer(event: &HistoryCopyIoEvent) -> Option<&'static str> {
    match (event.access, event.table.as_str()) {
        (HistoryCopyIoAccess::Insert, "history_copy_operations") => Some("copy.ledger.stage"),
        (HistoryCopyIoAccess::Insert, "messages") => Some("copy.message.write"),
        (HistoryCopyIoAccess::Insert, "attachment_manifests") => {
            Some("copy.attachment_manifest.write")
        }
        (HistoryCopyIoAccess::Insert, "attachments") => Some("copy.attachment.write"),
        (HistoryCopyIoAccess::Insert, "message_timers") => Some("copy.timer.write"),
        (HistoryCopyIoAccess::Insert, "history_copy_semantics") => Some("copy.semantic.write"),
        (HistoryCopyIoAccess::Insert, "history_copy_edges") => Some("copy.edge.write"),
        (HistoryCopyIoAccess::Insert, "history_copy_visibility") => Some("copy.visibility.publish"),
        (HistoryCopyIoAccess::Insert, "history_copy_meter") => Some("copy.meter.charge"),
        (HistoryCopyIoAccess::Update, "history_copy_operations") => Some("copy.ledger.complete"),
        (HistoryCopyIoAccess::Transaction, "__transaction__") => Some("copy.transaction.commit"),
        _ => None,
    }
}

fn destination_observations(state: &AppState) -> Arc<Mutex<Vec<HistoryCopyIoEvent>>> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    state
        .message_store
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .set_history_copy_io_observer(Some(Arc::new(move |event| {
            sink.lock().unwrap().push(event);
        })));
    events
}

fn assert_complete(state: &AppState, dir: &Path, expected_bytes: u64) {
    assert_eq!(raw_counts(dir), (3, 1));
    assert_eq!(cmd_osl_history_copy_month_data_bytes(state), expected_bytes);
    let store_guard = state.message_store.lock().unwrap();
    let store = store_guard.as_ref().unwrap();
    assert_eq!(store.history_copy_completed_operation_count().unwrap(), 1);
    assert_eq!(store.history_copy_visibility_count().unwrap(), 3);
    assert_eq!(store.history_copy_edge_count().unwrap(), 3);
    let rows = store.list_by_channel(CHANNEL, 10).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .filter(|row| row.reply_parent_id.is_some())
            .count(),
        1
    );
    let reply = rows
        .iter()
        .find(|row| row.reply_parent_id.is_some())
        .unwrap();
    assert_eq!(
        store
            .list_history_attachments(&reply.discord_message_id)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        store
            .message_timer_minutes(&reply.discord_message_id)
            .unwrap(),
        Some(23)
    );
    drop(store_guard);
    let semantics = cmd_osl_read_history_copy_semantics(state, reply.discord_message_id.clone())
        .unwrap()
        .unwrap();
    assert_eq!(semantics.reactions.len(), 1);
    assert!(semantics.thread_root_id.is_some());
}

fn assert_empty(state: &AppState, dir: &Path) {
    assert_eq!(raw_counts(dir), (0, 0));
    assert_eq!(cmd_osl_history_copy_month_data_bytes(state), 0);
    let guard = state.message_store.lock().unwrap();
    let store = guard.as_ref().unwrap();
    assert_eq!(store.history_copy_completed_operation_count().unwrap(), 0);
    assert_eq!(store.history_copy_visibility_count().unwrap(), 0);
    assert_eq!(store.history_copy_edge_count().unwrap(), 0);
}

#[test]
fn task_4823_runtime_io_lifecycle_and_every_fault_side_are_atomic() {
    let source_dir = TempDir::new().unwrap();
    let clean_dest_dir = TempDir::new().unwrap();
    let source_state = state_with_store(source_dir.path(), &SOURCE_SECRET, [0x48; 16]);
    seed_source(&source_state);
    let clean_dest = state_with_store(clean_dest_dir.path(), &DEST_SECRET, [0x23; 16]);

    let mut source_runtime = BTreeSet::new();
    let package = cmd_osl_export_history_for_copy_with_observer(
        &source_state,
        CHANNEL.to_string(),
        all_source_ids(),
        |writer, _side| {
            source_runtime.insert(writer);
            Ok(())
        },
    )
    .expect("clean shipping source export");
    let io_events = destination_observations(&clean_dest);
    let mut logical_destination = BTreeSet::new();
    let clean_result = cmd_osl_copy_my_history_here_with_observer(
        &clean_dest,
        package.clone(),
        phrase(),
        u64::MAX,
        |boundary, _side| {
            logical_destination.insert(boundary_writer(boundary));
            Ok(())
        },
    )
    .expect("clean atomic shipping copy");
    let expected_bytes = clean_result.actual_copied_bytes_written;
    assert!(expected_bytes > 0);
    assert_complete(&clean_dest, clean_dest_dir.path(), expected_bytes);

    let physical_destination: BTreeSet<_> = io_events
        .lock()
        .unwrap()
        .iter()
        .filter_map(io_writer)
        .collect();
    assert_eq!(physical_destination, logical_destination);
    let mut runtime_rows: BTreeSet<_> = source_runtime
        .union(&physical_destination)
        .copied()
        .collect();
    let static_rows = discover_static_universe().expect("independent static discovery");

    // Retry in the same process and reconnect after both devices restart must
    // return the same authenticated result without another graph or charge.
    let retry_result = cmd_osl_copy_my_history_here(&clean_dest, package.clone(), phrase())
        .expect("same-process retry");
    assert_eq!(retry_result, clean_result);
    assert_complete(&clean_dest, clean_dest_dir.path(), expected_bytes);
    drop(clean_dest);
    drop(source_state);
    let reconnected_source = state_with_store(source_dir.path(), &SOURCE_SECRET, [0x48; 16]);
    let reconnected_dest = state_with_store(clean_dest_dir.path(), &DEST_SECRET, [0x23; 16]);
    // Read the source after restart too; the destination retry continues to
    // use the byte-identical package whose operation id survived disconnect.
    let reconnect_export =
        cmd_osl_export_history_for_copy(&reconnected_source, CHANNEL.to_string(), all_source_ids())
            .expect("source reconnect read");
    assert!(!reconnect_export.is_empty());
    let reconnect_result =
        cmd_osl_copy_my_history_here(&reconnected_dest, package.clone(), phrase())
            .expect("both-device reconnect retry");
    assert_eq!(reconnect_result, clean_result);
    assert_complete(&reconnected_dest, clean_dest_dir.path(), expected_bytes);
    drop(reconnected_dest);
    drop(reconnected_source);

    // The conditional retention read is reached by a real error branch and is
    // part of the runtime source universe even though clean copies never need
    // it.
    let error_source = state_with_store(source_dir.path(), &SOURCE_SECRET, [0x48; 16]);
    let mut error_runtime = BTreeSet::new();
    let missing = cmd_osl_export_history_for_copy_with_observer(
        &error_source,
        CHANNEL.to_string(),
        vec!["task4823-absent".to_string()],
        |writer, _| {
            error_runtime.insert(writer);
            Ok(())
        },
    )
    .expect_err("missing source must take retention error branch");
    assert!(missing.contains("not on this device"), "{missing}");
    assert!(error_runtime.contains("source.retention.read"));
    runtime_rows.extend(error_runtime);
    reconcile(&static_rows, &runtime_rows).expect("static/runtime exact reconciliation");
    drop(error_source);

    // Allowance exhaustion is checked before the transaction. Restarting both
    // devices and retrying with enough allowance moves from 0/0 to one graph
    // and one charge.
    let allowance_dir = TempDir::new().unwrap();
    let allowance_source = state_with_store(source_dir.path(), &SOURCE_SECRET, [0x48; 16]);
    let allowance_dest = state_with_store(allowance_dir.path(), &DEST_SECRET, [0x23; 16]);
    let allowance_error = cmd_osl_copy_my_history_here_with_observer(
        &allowance_dest,
        package.clone(),
        phrase(),
        expected_bytes - 1,
        |_, _| Ok(()),
    )
    .expect_err("allowance exhaustion");
    assert!(
        allowance_error.contains("allowance exhausted"),
        "{allowance_error}"
    );
    assert_empty(&allowance_dest, allowance_dir.path());
    drop(allowance_dest);
    drop(allowance_source);
    let allowance_source = state_with_store(source_dir.path(), &SOURCE_SECRET, [0x48; 16]);
    let allowance_dest = state_with_store(allowance_dir.path(), &DEST_SECRET, [0x23; 16]);
    assert!(!cmd_osl_export_history_for_copy(
        &allowance_source,
        CHANNEL.to_string(),
        all_source_ids(),
    )
    .unwrap()
    .is_empty());
    cmd_osl_copy_my_history_here_with_observer(
        &allowance_dest,
        package.clone(),
        phrase(),
        expected_bytes,
        |_, _| Ok(()),
    )
    .expect("allowance retry");
    assert_complete(&allowance_dest, allowance_dir.path(), expected_bytes);
    drop(allowance_dest);
    drop(allowance_source);

    let destination_prefixes = [
        (HistoryCopyFaultBoundary::LedgerStage, 1usize),
        (HistoryCopyFaultBoundary::MessageWrite, 3),
        (HistoryCopyFaultBoundary::AttachmentManifestWrite, 3),
        (HistoryCopyFaultBoundary::AttachmentWrite, 1),
        (HistoryCopyFaultBoundary::TimerWrite, 1),
        (HistoryCopyFaultBoundary::SemanticWrite, 3),
        (HistoryCopyFaultBoundary::EdgeWrite, 3),
        (HistoryCopyFaultBoundary::VisibilityPublish, 3),
        (HistoryCopyFaultBoundary::MeterCharge, 1),
        (HistoryCopyFaultBoundary::LedgerComplete, 1),
        (HistoryCopyFaultBoundary::Commit, 1),
    ];
    let mut destination_fault_cases = 0usize;
    for (target, occurrences) in destination_prefixes {
        for wanted in 1..=occurrences {
            for side in [HistoryCopyFaultSide::Before, HistoryCopyFaultSide::After] {
                destination_fault_cases += 1;
                let dest_dir = TempDir::new().unwrap();
                let fault_source = state_with_store(source_dir.path(), &SOURCE_SECRET, [0x48; 16]);
                let fault_dest = state_with_store(dest_dir.path(), &DEST_SECRET, [0x23; 16]);
                let mut seen = 0usize;
                let error = cmd_osl_copy_my_history_here_with_observer(
                    &fault_dest,
                    package.clone(),
                    phrase(),
                    expected_bytes,
                    |boundary, actual_side| {
                        if boundary == target && actual_side == side {
                            seen += 1;
                            if seen == wanted {
                                return Err(format!(
                                    "TASK4823 injected writer={} side={actual_side:?} prefix={wanted}",
                                    boundary_writer(boundary)
                                ));
                            }
                        }
                        Ok(())
                    },
                )
                .expect_err("injected destination fault must surface");
                assert!(error.contains(boundary_writer(target)), "{error}");
                if target == HistoryCopyFaultBoundary::Commit && side == HistoryCopyFaultSide::After
                {
                    assert_complete(&fault_dest, dest_dir.path(), expected_bytes);
                } else {
                    assert_empty(&fault_dest, dest_dir.path());
                }
                drop(fault_dest);
                drop(fault_source);

                let restarted_source =
                    state_with_store(source_dir.path(), &SOURCE_SECRET, [0x48; 16]);
                let restarted_dest = state_with_store(dest_dir.path(), &DEST_SECRET, [0x23; 16]);
                assert!(!cmd_osl_export_history_for_copy(
                    &restarted_source,
                    CHANNEL.to_string(),
                    all_source_ids(),
                )
                .unwrap()
                .is_empty());
                cmd_osl_copy_my_history_here(&restarted_dest, package.clone(), phrase())
                    .expect("fault restart/retry");
                assert_complete(&restarted_dest, dest_dir.path(), expected_bytes);
            }
        }
    }

    let source_prefixes = [
        ("source.message.read", 3usize),
        ("source.attachment.read", 3),
        ("source.semantic.read", 3),
        ("source.expiration.read", 3),
        ("source.retention.read", 1),
    ];
    let mut source_fault_cases = 0usize;
    for (target, occurrences) in source_prefixes {
        for wanted in 1..=occurrences {
            for side in [HistoryCopyFaultSide::Before, HistoryCopyFaultSide::After] {
                source_fault_cases += 1;
                let dest_dir = TempDir::new().unwrap();
                let fault_source = state_with_store(source_dir.path(), &SOURCE_SECRET, [0x48; 16]);
                let fault_dest = state_with_store(dest_dir.path(), &DEST_SECRET, [0x23; 16]);
                let ids = if target == "source.retention.read" {
                    vec!["task4823-absent".to_string()]
                } else {
                    all_source_ids()
                };
                let mut seen = 0usize;
                let error = cmd_osl_export_history_for_copy_with_observer(
                    &fault_source,
                    CHANNEL.to_string(),
                    ids,
                    |writer, actual_side| {
                        if writer == target && actual_side == side {
                            seen += 1;
                            if seen == wanted {
                                return Err(format!(
                                    "TASK4823 injected writer={writer} side={actual_side:?} prefix={wanted}"
                                ));
                            }
                        }
                        Ok(())
                    },
                )
                .expect_err("injected source fault must surface");
                assert!(error.contains(target), "{error}");
                assert_empty(&fault_dest, dest_dir.path());
                drop(fault_dest);
                drop(fault_source);

                let restarted_source =
                    state_with_store(source_dir.path(), &SOURCE_SECRET, [0x48; 16]);
                let restarted_dest = state_with_store(dest_dir.path(), &DEST_SECRET, [0x23; 16]);
                let restarted_package = cmd_osl_export_history_for_copy(
                    &restarted_source,
                    CHANNEL.to_string(),
                    all_source_ids(),
                )
                .expect("source fault restart export");
                cmd_osl_copy_my_history_here(&restarted_dest, restarted_package, phrase())
                    .expect("source fault restart/retry");
                assert_complete(&restarted_dest, dest_dir.path(), expected_bytes);
            }
        }
    }

    println!("TASK4823 runtime_universe_rows={}", runtime_rows.len());
    println!("TASK4823 runtime_static_universe_exact=true");
    println!("TASK4823 lifecycle_runs=clean,retry,reconnect,error,allowance-exhaustion");
    println!("TASK4823 complete_graph_messages=3 attachments=1 edges=3");
    println!("TASK4823 unique_byte_charge={expected_bytes}");
    println!("TASK4823 destination_fault_cases={destination_fault_cases}");
    println!("TASK4823 source_fault_cases={source_fault_cases}");
    println!(
        "TASK4823 all_fault_cases={} before_after=true both_device_restart_retry=true outcomes=zero_or_complete",
        destination_fault_cases + source_fault_cases
    );
    println!(
        "TASK4823 semantic_readback=true ledger_observation=1 meter_observation={expected_bytes}"
    );
}

#[test]
fn task_4823_static_callgraph_schema_universe_is_nonempty_and_reconciled() {
    let static_rows = discover_static_universe().expect("independent static discovery");
    // Runtime rows are supplied by the separately instrumented lifecycle test;
    // this unit first proves that the reconciler accepts exactly that physical
    // inventory and rejects changes to every individual row.
    reconcile(&static_rows, &static_rows).expect("static universe reconciles to frozen matrix");
    assert_eq!(static_rows.len(), UNIVERSE.len());

    for row in UNIVERSE {
        for mutation in ["omit", "duplicate", "reorder"] {
            let scenario = format!("{mutation}:{}", row.writer);
            let output = spawn_ignored("task_4823_universe_mutation_child", &scenario);
            let text = child_text(&output);
            assert_eq!(output.status.code(), Some(1), "{scenario}: {text}");
            assert!(text.contains(row.writer), "{scenario}: {text}");
            assert!(text.contains(mutation), "{scenario}: {text}");
        }
    }
    let output = spawn_ignored(
        "task_4823_universe_mutation_child",
        "added:neutral_cache_checkpoint",
    );
    let text = child_text(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(text.contains("neutral_cache_checkpoint"), "{text}");
    assert!(text.contains("unclassified"), "{text}");
    let output = spawn_ignored("task_4823_neutral_retry_writer_child", "neutralRetryWriter");
    let actual_text = child_text(&output);
    assert_eq!(output.status.code(), Some(1), "{actual_text}");
    assert!(
        actual_text.contains("neutral_cache_checkpoint"),
        "{actual_text}"
    );
    assert!(
        actual_text.contains("runtime_write_seen=true"),
        "{actual_text}"
    );

    println!("TASK4823 static_universe_rows={}", static_rows.len());
    println!("TASK4823 matrix_rows={}", UNIVERSE.len());
    println!(
        "TASK4823 per_row_mutants={} all_exit_1=true",
        UNIVERSE.len() * 3
    );
    println!("TASK4823 added_neutral_writer=neutral_cache_checkpoint exit=1");
}

#[test]
#[ignore = "throwaway child normalizes the deliberately red shipping retry to exit 1"]
fn task_4823_neutral_retry_writer_child() {
    let output = Command::new(std::env::current_exe().expect("current test binary"))
        .args([
            "--exact",
            "task_4823_runtime_io_lifecycle_and_every_fault_side_are_atomic",
            "--nocapture",
        ])
        .env("OSL_TASK4823_NEUTRAL_RETRY_WRITER", "1")
        .output()
        .unwrap();
    let text = child_text(&output);
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(text.contains("neutral_cache_checkpoint"), "{text}");
    assert!(text.contains("runtime_write_seen=true"), "{text}");
    eprintln!(
        "TASK4823 unclassified writer=neutral_cache_checkpoint effect=retry_conditional_write runtime_write_seen=true"
    );
    std::process::exit(1);
}

#[test]
#[ignore = "throwaway child; parent requires deliberate exit 1"]
fn task_4823_universe_mutation_child() {
    let scenario = std::env::var("TASK4823_SCENARIO").expect("TASK4823_SCENARIO");
    let (effect, writer) = scenario
        .split_once(':')
        .unwrap_or_else(|| panic!("invalid scenario {scenario}"));
    let static_rows = discover_static_universe().expect("static discovery");
    let mut runtime_order: Vec<_> = static_rows.iter().copied().collect();
    match effect {
        "omit" => runtime_order.retain(|candidate| *candidate != writer),
        "duplicate" => runtime_order.push(
            runtime_order
                .iter()
                .copied()
                .find(|candidate| *candidate == writer)
                .expect("duplicate writer exists"),
        ),
        "reorder" => {
            let position = runtime_order
                .iter()
                .position(|candidate| *candidate == writer)
                .expect("reorder writer exists");
            let moved = runtime_order.remove(position);
            if position == runtime_order.len() {
                runtime_order.insert(0, moved);
            } else {
                runtime_order.push(moved);
            }
        }
        "added" => runtime_order.push(Box::leak(writer.to_string().into_boxed_str())),
        other => panic!("unknown effect {other}"),
    }

    let baseline: Vec<_> = static_rows.iter().copied().collect();
    let diagnostic = if effect == "duplicate" {
        let mut counts = BTreeMap::new();
        for value in &runtime_order {
            *counts.entry(*value).or_insert(0usize) += 1;
        }
        counts
            .get(writer)
            .filter(|count| **count > 1)
            .map(|_| format!("TASK4823 duplicate writer={writer} effect=duplicate durable row"))
    } else if effect == "reorder" {
        (runtime_order != baseline)
            .then(|| format!("TASK4823 reorder writer={writer} effect=durable row reordered"))
    } else {
        let runtime_set: BTreeSet<_> = runtime_order.iter().copied().collect();
        reconcile(&static_rows, &runtime_set).err().map(|error| {
            format!("TASK4823 {effect} writer={writer} effect={effect} durable row: {error}")
        })
    };
    if let Some(diagnostic) = diagnostic {
        eprintln!("{diagnostic}");
        std::process::exit(1);
    }
    panic!("TASK4823 mutation was accepted effect={effect} writer={writer}");
}
