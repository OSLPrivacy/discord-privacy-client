use ipc::{
    automatic_stop::{AutomaticStopError, AutomaticStopStore, SERVICE_PAUSED_MESSAGE},
    cost_alarm::MoneyCeiling,
    usage_counters::UsageCounterStore,
};

const WHEN: i64 = 20_586 * 86_400;
const CEILING: MoneyCeiling = MoneyCeiling {
    ceiling_micros: 100,
    stored_byte_micros: 1,
    transferred_byte_micros: 1,
};
const MARKED_WORD: &[u8] = b"TASK0586-MARKED-WORD";

#[test]
fn task_0586_money_ceiling_pauses_new_transfers_but_keeps_stored_file_readable() {
    let dir = tempfile::tempdir().expect("temporary counter directory");
    let counters = UsageCounterStore::open(dir.path()).expect("open usage counters");
    let mut store = AutomaticStopStore::new(counters, CEILING);
    // Existing retained files account for exactly 100% before either newly
    // named transfer. The marked file is the one that must survive the stop.
    store
        .import_retained_file("alice", "already-stored.txt", MARKED_WORD.to_vec())
        .expect("import marked retained file");
    store
        .import_retained_file("fixture", "ceiling-fill", vec![0; 80])
        .expect("fill ceiling fixture");

    let upload = store
        .upload("bob", "new-upload.txt", b"new upload".to_vec(), WHEN)
        .unwrap_err();
    assert!(matches!(upload, AutomaticStopError::ServicePaused));
    assert_eq!(upload.to_string(), SERVICE_PAUSED_MESSAGE);
    println!(
        "TASK0586 percent=100 upload_name=new-upload.txt refusal={}",
        upload
    );

    let download = store
        .download("alice", "new-download.txt", WHEN)
        .unwrap_err();
    assert!(matches!(download, AutomaticStopError::ServicePaused));
    assert_eq!(download.to_string(), SERVICE_PAUSED_MESSAGE);
    println!(
        "TASK0586 percent=100 download_name=new-download.txt refusal={}",
        download
    );

    let retained = store
        .read_stored("alice", "already-stored.txt")
        .expect("retained file remains readable");
    assert_eq!(retained, MARKED_WORD);
    println!(
        "TASK0586 retained_name=already-stored.txt exact_word={}",
        String::from_utf8_lossy(retained)
    );

    // Lower the fixture below the ceiling (without deleting the marked file)
    // and both transfer directions resume.
    store
        .remove_stored("fixture", "ceiling-fill")
        .expect("lower cost fixture");
    store
        .upload("bob", "new-upload.txt", b"new upload".to_vec(), WHEN)
        .expect("upload below ceiling");
    let resumed = store
        .download("bob", "new-upload.txt", WHEN)
        .expect("download below ceiling");
    assert_eq!(resumed, b"new upload");
    println!("TASK0586 lowered_percent=20 upload_name=new-upload.txt download_name=new-upload.txt resumed_bytes={}", resumed.len());
}
