use ipc::allowed_places::AllowedPlaceRecord;
use ipc::auto_whitelist_rules::{AutoWhitelistAppKind, AutoWhitelistChoice, SignalWhitelistKind};
use ipc::commands::{
    cmd_osl_create_group_conversation, cmd_osl_list_all_whitelists, cmd_osl_membership_get,
    cmd_osl_query_allowed_place, cmd_osl_query_auto_whitelist_rule, cmd_osl_save_allowed_place,
    cmd_osl_save_auto_whitelist_rule, cmd_osl_set_whitelist,
};
use ipc::scope::ScopeInput;
use ipc::state_reload::reload_encrypted_state_after_unlock;
use ipc::AppState;
use std::sync::Mutex;
use tempfile::tempdir;

static PROCESS_GLOBALS: Mutex<()> = Mutex::new(());

struct ProcessGlobalReset;

impl Drop for ProcessGlobalReset {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

#[test]
fn task_0182_whitelist_data_survives_full_restart() {
    let _serial = PROCESS_GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    let _reset = ProcessGlobalReset;

    let base = tempdir().unwrap();
    let account_dir = base.path().join("task-0182-account");
    std::fs::create_dir_all(&account_dir).unwrap();
    keystore::set_base_dir_override(Some(base.path().to_path_buf()));
    keystore::set_active_account_dir(Some(account_dir.clone()));
    ipc::main_password::set_file_storage_key(Some([0x18; 32]));

    let state = AppState::new();

    let place = AllowedPlaceRecord::signal(
        "task0182-signal-account",
        SignalWhitelistKind::GroupChat,
        "task0182-family-room",
    );
    let saved_place = cmd_osl_save_allowed_place(&state, place).unwrap();

    let saved_rule = cmd_osl_save_auto_whitelist_rule(
        &state,
        AutoWhitelistAppKind::Chat,
        AutoWhitelistChoice::OnlyIfAFriend,
    )
    .unwrap();

    let group = cmd_osl_create_group_conversation(
        &state,
        "Task 0182 Family".to_owned(),
        vec![
            "task0182_member_a".to_owned(),
            "task0182_member_b".to_owned(),
            "task0182_member_c".to_owned(),
        ],
    )
    .unwrap();
    let saved_group_members = group.member_ids.clone();
    let relation_peer = "task0182_member_b".to_owned();
    cmd_osl_set_whitelist(
        &state,
        relation_peer.clone(),
        ScopeInput::from(&ipc::scope::Scope::gc(group.group_id.clone())),
        false,
    )
    .unwrap();
    let saved_relation = cmd_osl_list_all_whitelists(&state)
        .unwrap()
        .into_iter()
        .find(|row| row.peer_discord_id == relation_peer && row.scope_id == group.group_id)
        .expect("saved relation row");

    drop(state);

    let restart = AppState::new();
    let report = reload_encrypted_state_after_unlock(&restart, &account_dir).unwrap();
    assert!(report.allowed_places_loaded);
    assert!(report.auto_whitelist_rules_loaded);
    assert!(report.scope_membership_loaded);
    assert!(report.peer_map_loaded);
    assert!(report.whitelist_loaded);
    assert!(report.errors.is_empty());

    let restart_place = cmd_osl_query_allowed_place(&restart, saved_place.stable_id.clone())
        .unwrap()
        .expect("saved place after restart");
    assert_eq!(restart_place.app, saved_place.app);
    assert_eq!(restart_place.account, saved_place.account);
    assert_eq!(restart_place.kind, saved_place.kind);
    assert_eq!(restart_place.stable_id, saved_place.stable_id);

    let restart_members = cmd_osl_membership_get(&restart, group.group_id.clone()).unwrap();
    assert_eq!(restart_members, saved_group_members);

    let restart_rule =
        cmd_osl_query_auto_whitelist_rule(&restart, AutoWhitelistAppKind::Chat).unwrap();
    assert_eq!(restart_rule.saved_choice, Some(saved_rule.choice));

    let restart_relation = cmd_osl_list_all_whitelists(&restart)
        .unwrap()
        .into_iter()
        .find(|row| row.peer_discord_id == relation_peer && row.scope_id == group.group_id)
        .expect("saved relation row after restart");
    assert_eq!(restart_relation.scope_kind, saved_relation.scope_kind);
    assert_eq!(
        restart_relation.peer_discord_id,
        saved_relation.peer_discord_id
    );
    assert_eq!(restart_relation.scope_id, saved_relation.scope_id);
    assert_eq!(restart_relation.channel_id, saved_relation.channel_id);
    assert_eq!(
        restart_relation.encrypt_toggle,
        saved_relation.encrypt_toggle
    );

    println!(
        "TASK0182_PLACE saved_stable_id={} restart_stable_id={} app={} account={} kind={}",
        saved_place.stable_id,
        restart_place.stable_id,
        restart_place.app,
        restart_place.account,
        restart_place.kind
    );
    println!(
        "TASK0182_GROUP_MEMBER saved_members={} restart_members={}",
        saved_group_members.join(","),
        restart_members.join(",")
    );
    println!(
        "TASK0182_RULE app_kind=chat saved_choice={} restart_choice={}",
        saved_rule.choice.label(),
        restart_rule.saved_choice.unwrap().label()
    );
    println!(
        "TASK0182_TWO_WAY_RELATION saved_peer={} restart_peer={} saved_scope={} restart_scope={} kind={} encrypt_toggle={}",
        saved_relation.peer_discord_id,
        restart_relation.peer_discord_id,
        saved_relation.scope_id,
        restart_relation.scope_id,
        restart_relation.scope_kind,
        restart_relation.encrypt_toggle
    );
}
