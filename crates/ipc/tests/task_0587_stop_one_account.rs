use ipc::usage_counters::UsageCounterStore;

const DAY: i64 = 20_000 * 86_400;
const STOPPED_ACCOUNT: &str = "task-0587-stopped-account";
const OTHER_ACCOUNT: &str = "task-0587-other-account";
const KEPT_FILE: &[u8] = b"task-0587-stopped-account-kept-file";
const OTHER_FILE: &[u8] = b"task-0587-second-account-exact-file";
const OTHER_FINGERPRINT: &str = "5aa070645f7342a8b632f93d208e32f8db80080f820cf1c97f08ceba2d66d275";

#[test]
fn task_0587_stops_only_one_account_without_deleting_its_stored_file() {
    let dir = tempfile::tempdir().expect("temporary usage directory");
    let mut store = UsageCounterStore::open(dir.path()).expect("open usage store");

    let kept_fingerprint = store
        .upload_file(STOPPED_ACCOUNT, "kept-file", KEPT_FILE)
        .expect("seed stopped account's stored file");
    assert_eq!(
        kept_fingerprint,
        "97a72001cb454da7dcc653d90d5ceaf2379eb2197b834a481be8435ba33be308"
    );
    let bytes_before = store
        .read_at(STOPPED_ACCOUNT, DAY)
        .expect("read bytes before stop")
        .stored_bytes;
    assert_eq!(bytes_before, KEPT_FILE.len() as u64);

    let stopped = store
        .stop_account(STOPPED_ACCOUNT)
        .expect("stop exactly one account");
    assert_eq!(stopped.stored_bytes, bytes_before);
    drop(store);
    let mut store = UsageCounterStore::open(dir.path()).expect("reopen durable usage store");

    let upload_refusal = store
        .upload_file(STOPPED_ACCOUNT, "refused-upload", b"must not be stored")
        .expect_err("stopped upload must be refused")
        .to_string();
    let download_refusal = store
        .download_file(STOPPED_ACCOUNT, "kept-file", DAY)
        .expect_err("stopped download must be refused")
        .to_string();
    let send_refusal = store
        .send_message(STOPPED_ACCOUNT, 9, DAY)
        .expect_err("stopped send must be refused")
        .to_string();

    assert_eq!(
        upload_refusal,
        "upload refused: account task-0587-stopped-account is stopped"
    );
    assert_eq!(
        download_refusal,
        "download refused: account task-0587-stopped-account is stopped"
    );
    assert_eq!(
        send_refusal,
        "send refused: account task-0587-stopped-account is stopped"
    );

    let bytes_after = store
        .read_at(STOPPED_ACCOUNT, DAY)
        .expect("read bytes after refused operations")
        .stored_bytes;
    assert_eq!(bytes_after, bytes_before);

    let other_fingerprint = store
        .upload_file(OTHER_ACCOUNT, "exact-file", OTHER_FILE)
        .expect("second account upload remains available");
    assert_eq!(other_fingerprint, OTHER_FINGERPRINT);
    assert_eq!(
        store
            .read_at(OTHER_ACCOUNT, DAY)
            .expect("read second account stored bytes")
            .stored_bytes,
        OTHER_FILE.len() as u64
    );

    println!(
        "TASK0587 upload_refusal=\"{upload_refusal}\" download_refusal=\"{download_refusal}\" send_refusal=\"{send_refusal}\" stored_bytes_before={bytes_before} stored_bytes_after={bytes_after} second_account_upload=worked second_account_stored_bytes={} second_account_fingerprint={other_fingerprint}",
        OTHER_FILE.len()
    );
}
