use ipc::metered_bytes::{
    MeteredByteClass, MeteredByteRecord, MonthlyAllowanceStore, VoiceAccountingError,
    VoiceCounterSource, VoiceInterfaceCounters,
};

const MONTH: &str = "2026-08";
const SENT_BYTES: u64 = 318_000;
const RECEIVED_BYTES: u64 = 636_000;

#[test]
fn task_4708_voice_is_persisted_sent_plus_received_in_the_one_byte_allowance() {
    let directory = tempfile::tempdir().expect("temporary allowance directory");
    let store = MonthlyAllowanceStore::open(directory.path()).expect("open allowance store");
    store
        .record(&MeteredByteRecord::new(
            MONTH,
            8_192,
            MeteredByteClass::BackgroundConnection,
            "cover-tick-1",
        ))
        .expect("record background contribution");
    let voice = store
        .record_voice_call(
            MONTH,
            "livekit-room-2026-08-11/speaker-A",
            VoiceInterfaceCounters {
                sent_bytes: 10,
                received_bytes: 20,
            },
            VoiceInterfaceCounters {
                sent_bytes: 10 + SENT_BYTES,
                received_bytes: 20 + RECEIVED_BYTES,
            },
            VoiceCounterSource::ReleaseClientMediaInterface,
        )
        .expect("real interface deltas record a Voice row");
    assert_eq!(voice.byte_count, SENT_BYTES + RECEIVED_BYTES);

    let rows_before_restart = store.itemised_rows(MONTH).expect("itemised rows");
    let total_before_restart = store.total(MONTH).expect("total");
    drop(store);
    let reopened = MonthlyAllowanceStore::open(directory.path()).expect("restart reopens ledger");
    let rows_after_restart = reopened
        .itemised_rows(MONTH)
        .expect("itemised rows survive restart");
    let total_after_restart = reopened.total(MONTH).expect("total survives restart");
    let itemised_sum: u64 = rows_after_restart.iter().map(|(_, bytes)| bytes).sum();
    let voice_row = rows_after_restart
        .iter()
        .find(|(class, _)| *class == MeteredByteClass::Voice)
        .expect("Voice row always exists");

    println!("TASK4708_INTERFACE sent_bytes={SENT_BYTES} received_bytes={RECEIVED_BYTES} sent_plus_received={}", SENT_BYTES + RECEIVED_BYTES);
    println!("TASK4708_RESTART rows={} total_before_restart={total_before_restart} total_after_restart={total_after_restart} itemised_sum={itemised_sum}", rows_after_restart.len());
    for (class, bytes) in &rows_after_restart {
        println!("TASK4708_ROW class={} bytes={bytes}", class.name());
    }

    assert_eq!(rows_before_restart, rows_after_restart);
    assert_eq!(voice_row.1, SENT_BYTES + RECEIVED_BYTES);
    assert_eq!(total_before_restart, total_after_restart);
    assert_eq!(total_after_restart, itemised_sum);
    assert_eq!(rows_after_restart.len(), 6);
    assert!(rows_after_restart
        .iter()
        .all(|(class, _)| !class.name().contains("minute")));
}

#[test]
fn task_4708_missing_interface_bytes_and_synthetic_inputs_fail_closed() {
    let directory = tempfile::tempdir().expect("temporary allowance directory");
    let store = MonthlyAllowanceStore::open(directory.path()).expect("open allowance store");
    let before = VoiceInterfaceCounters {
        sent_bytes: 100,
        received_bytes: 200,
    };
    let absent_sent = store
        .record_voice_call(
            MONTH,
            "absent-sent",
            before,
            VoiceInterfaceCounters {
                sent_bytes: 100,
                received_bytes: 201,
            },
            VoiceCounterSource::ReleaseClientMediaInterface,
        )
        .unwrap_err();
    let absent_received = store
        .record_voice_call(
            MONTH,
            "absent-received",
            before,
            VoiceInterfaceCounters {
                sent_bytes: 101,
                received_bytes: 200,
            },
            VoiceCounterSource::ReleaseClientMediaInterface,
        )
        .unwrap_err();
    let synthetic = store
        .record_voice_call(
            MONTH,
            "synthetic",
            before,
            VoiceInterfaceCounters {
                sent_bytes: 101,
                received_bytes: 201,
            },
            VoiceCounterSource::Synthetic,
        )
        .unwrap_err();
    println!("TASK4708_RED absent_sent={absent_sent} absent_received={absent_received} synthetic={synthetic}");
    assert_eq!(absent_sent, VoiceAccountingError::MissingSentBytes);
    assert_eq!(absent_received, VoiceAccountingError::MissingReceivedBytes);
    assert_eq!(synthetic, VoiceAccountingError::SyntheticCounters);
}
