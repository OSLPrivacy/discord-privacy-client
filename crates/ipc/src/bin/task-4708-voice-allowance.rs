//! Release-call allowance reconciliation used by TASK 4708.
//!
//! Reads the three independent release-client media-interface observations
//! emitted by the qualified LiveKit call. It persists each person's actual
//! sent-plus-received bytes, closes the store, and reopens it before reporting.

use ipc::metered_bytes::{
    MeteredByteClass, MonthlyAllowanceStore, VoiceCounterSource, VoiceInterfaceCounters,
};
use serde_json::Value;
use std::env;
use std::fs;
use std::path::PathBuf;

fn usage() -> ! {
    eprintln!("usage: task-4708-voice-allowance <4701-artifact-dir> <ledger-dir> <YYYY-MM>");
    std::process::exit(2);
}

fn counter(path: &PathBuf) -> Result<(String, VoiceInterfaceCounters), String> {
    let text =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let event = text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|value| value.get("event").and_then(Value::as_str) == Some("interface_counters"))
        .ok_or_else(|| format!("missing interface counters in {}", path.display()))?;
    if event.get("counter_source").and_then(Value::as_str) != Some("release-client-media-interface")
    {
        return Err(format!("synthetic counters in {}", path.display()));
    }
    let identity = event
        .get("identity")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing counter identity in {}", path.display()))?
        .to_owned();
    let sent_bytes = event
        .get("sent_bytes")
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("missing sent bytes in {}", path.display()))?;
    let received_bytes = event
        .get("received_bytes")
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("missing received bytes in {}", path.display()))?;
    Ok((
        identity,
        VoiceInterfaceCounters {
            sent_bytes,
            received_bytes,
        },
    ))
}

fn main() {
    let mut arguments = env::args_os().skip(1);
    let artifact_dir = arguments
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| usage());
    let ledger_dir = arguments
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| usage());
    let month = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .unwrap_or_else(|| usage());
    if arguments.next().is_some() {
        usage();
    }
    fs::create_dir_all(&ledger_dir)
        .unwrap_or_else(|error| panic!("create ledger directory: {error}"));

    let store = MonthlyAllowanceStore::open(&ledger_dir).expect("open allowance ledger");
    let mut people = Vec::new();
    for speaker in ["speaker-A", "speaker-B", "speaker-C"] {
        let (identity, observed) = counter(&artifact_dir.join(format!("{speaker}.jsonl")))
            .unwrap_or_else(|error| panic!("TASK4708 {error}"));
        let record = store
            .record_voice_call(
                &month,
                format!("livekit-qualified-call/{identity}"),
                VoiceInterfaceCounters {
                    sent_bytes: 0,
                    received_bytes: 0,
                },
                observed,
                VoiceCounterSource::ReleaseClientMediaInterface,
            )
            .unwrap_or_else(|error| panic!("TASK4708 {identity}: {error}"));
        people.push(serde_json::json!({
            "identity": identity,
            "interface_sent_bytes": observed.sent_bytes,
            "interface_received_bytes": observed.received_bytes,
            "voice_row_bytes": record.byte_count,
        }));
    }
    let rows_before = store.itemised_rows(&month).expect("read itemised rows");
    let total_before = store.total(&month).expect("read total");
    drop(store);
    let reopened = MonthlyAllowanceStore::open(&ledger_dir).expect("reopen allowance ledger");
    let rows_after = reopened.itemised_rows(&month).expect("read restarted rows");
    let total_after = reopened.total(&month).expect("read restarted total");
    assert_eq!(
        rows_before, rows_after,
        "itemised rows changed after restart"
    );
    assert_eq!(total_before, total_after, "total changed after restart");
    let itemised_sum: u64 = rows_after.iter().map(|(_, bytes)| bytes).sum();
    assert_eq!(
        total_after, itemised_sum,
        "total does not reconcile to rows"
    );
    let voice_rows: Vec<_> = rows_after
        .iter()
        .filter(|(class, _)| *class == MeteredByteClass::Voice)
        .collect();
    assert_eq!(
        voice_rows.len(),
        1,
        "the Data-this-month ledger has one Voice row"
    );
    println!(
        "{}",
        serde_json::json!({
            "task": 4708,
            "month": month,
            "people": people,
            "rows": rows_after.into_iter().map(|(class, bytes)| serde_json::json!({"class": class.name(), "bytes": bytes})).collect::<Vec<_>>(),
            "total_before_restart": total_before,
            "total_after_restart": total_after,
            "itemised_sum": itemised_sum,
            "voice_minute_units": 0,
        })
    );
}
