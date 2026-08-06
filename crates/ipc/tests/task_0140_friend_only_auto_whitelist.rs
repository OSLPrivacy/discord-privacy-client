use ipc::allowed_places::AllowedPlaceRecord;
use ipc::commands::{
    cmd_osl_new_place, cmd_osl_save_auto_whitelist_rule, cmd_osl_search_allowed_places,
    cmd_osl_set_friend_ids,
};
use ipc::state::AppState;
use std::path::Path;

const ACCOUNT_LABEL: &str = "PEAR";
const FRIEND_LABEL: &str = "SABLE-0140";

fn friend_results(dir: &Path) -> Vec<ipc::commands::AllowedPlaceSearchResultDto> {
    cmd_osl_search_allowed_places(dir.to_path_buf(), FRIEND_LABEL.to_string())
        .expect("search allowed places")
}

fn stored_record(dir: &Path) -> (String, String, String) {
    let mut results = friend_results(dir);
    let record = results.pop().expect("read SABLE-0140 allowed place");
    (record.account, record.person_name, record.stable_id)
}

#[test]
fn task_0140_friend_only_auto_whitelist_adds_friend_and_refuses_non_friend_copy() {
    let state = AppState::new();
    let dir = tempfile::tempdir().expect("tempdir");
    cmd_osl_save_auto_whitelist_rule(
        &state,
        "discord".to_string(),
        "only if a friend".to_string(),
        None,
    )
    .expect("save friend-only auto-whitelist rule");

    let place = AllowedPlaceRecord::discord_direct_message(ACCOUNT_LABEL, FRIEND_LABEL);
    let fingerprint_before = place.stable_id.clone();
    let before_results = friend_results(dir.path());
    let count_before = before_results.len();

    cmd_osl_set_friend_ids(&state, vec![FRIEND_LABEL.to_string()]).expect("seed friend");
    let friend_add = cmd_osl_new_place(&state, place.clone(), Some(dir.path().to_path_buf()))
        .expect("friend place is auto-added");
    let count_after_friend = friend_results(dir.path()).len();
    let (stored_account, stored_person, stored_fingerprint) = stored_record(dir.path());

    cmd_osl_set_friend_ids(&state, Vec::new()).expect("relationship changed to non-friend");
    let non_friend_copy = place.clone();
    let non_friend_fingerprint = non_friend_copy.stable_id.clone();
    let non_friend_error =
        cmd_osl_new_place(&state, non_friend_copy, Some(dir.path().to_path_buf()))
            .expect_err("non-friend copy must be refused");
    let count_after_non_friend = friend_results(dir.path()).len();

    println!(
        "TASK_0140_FRIEND_ONLY_AUTO_WHITELIST before_count={} before_readable_sable_results={} account_label={} friend_add_name={} friend_add_status={} count_after_friend={} non_friend_copy_only_changed_field=relationship non_friend_error=\"{}\" count_after_non_friend={} fingerprint_before={} stored_fingerprint={} non_friend_fingerprint={} stored_account={} stored_person={}",
        count_before,
        before_results.len(),
        ACCOUNT_LABEL,
        friend_add.place.person_name,
        friend_add.status,
        count_after_friend,
        non_friend_error,
        count_after_non_friend,
        fingerprint_before,
        stored_fingerprint,
        non_friend_fingerprint,
        stored_account,
        stored_person
    );

    assert_eq!(count_before, 0);
    assert_eq!(before_results.len(), 0);
    assert_eq!(friend_add.place.person_name, FRIEND_LABEL);
    assert_eq!(friend_add.place.account, ACCOUNT_LABEL);
    assert_eq!(friend_add.status, "allowed");
    assert_eq!(count_after_friend, 1);
    assert!(non_friend_error.contains("not a friend"));
    assert_eq!(count_after_non_friend, 1);
    assert_eq!(stored_account, ACCOUNT_LABEL);
    assert_eq!(stored_person, FRIEND_LABEL);
    assert_eq!(stored_fingerprint, fingerprint_before);
    assert_eq!(non_friend_fingerprint, fingerprint_before);
}
