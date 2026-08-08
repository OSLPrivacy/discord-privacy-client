use ipc::scope::{ScopeInput, ScopeKind};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::{
    add_friend_code, apply_scoped_trust_grant, export_friend_code, list_people,
    manual_peer_binding, manual_peer_scope_id, set_friend_scope_permission, set_friend_scope_reach,
    set_manual_peer_scope_permission, verify_friend_safety_number, AddFriendDisposition,
    HubSecurityState, ScopedTrustConsent, ScopedTrustGrant,
};

const TEST_FILE_KEY: [u8; 32] = [0x53; 32];

struct AccountHarness {
    _dir: tempfile::TempDir,
    previous_dir: Option<std::path::PathBuf>,
    previous_key: Option<[u8; 32]>,
}

impl AccountHarness {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("task 5034 temp account");
        let previous_dir = keystore::active_account_dir();
        let previous_key = ipc::main_password::get_file_storage_key();
        keystore::set_active_account_dir(Some(dir.path().to_path_buf()));
        ipc::main_password::set_file_storage_key(Some(TEST_FILE_KEY));
        Self {
            _dir: dir,
            previous_dir,
            previous_key,
        }
    }
}

impl Drop for AccountHarness {
    fn drop(&mut self) {
        keystore::set_active_account_dir(self.previous_dir.clone());
        ipc::main_password::set_file_storage_key(self.previous_key);
    }
}

fn manual_scope(account_id: &str, person_id: &str) -> ScopeInput {
    ScopeInput {
        kind: ScopeKind::Dm,
        id: manual_peer_scope_id("osl-chat", account_id, person_id).unwrap(),
        server_id: None,
        channel_id: None,
    }
}

#[test]
fn task_5034_key_changed_refuses_every_whitelist_write_until_reverified() {
    let _harness = AccountHarness::new();
    let core = HubCoreState::default();
    let security = HubSecurityState::default();
    *core.osl.identity.lock().unwrap() = Some(keystore::generate_native_identity());

    let friend = keystore::generate_native_identity();
    let friend_core = HubCoreState::default();
    *friend_core.osl.identity.lock().unwrap() = Some(friend.clone());
    let invite = export_friend_code(&friend_core).expect("friend invite exports");
    let added = add_friend_code(
        &core,
        &security,
        invite.friend_code,
        Some("Rose".to_owned()),
    )
    .expect("fixture friend imports");
    verify_friend_safety_number(
        &core,
        &security,
        added.person_id.clone(),
        added.safety_number,
    )
    .expect("fixture friend verifies");

    let base_scope = manual_scope("account-base", &added.person_id);
    set_manual_peer_scope_permission(
        &core,
        &security,
        "osl-chat",
        "account-base",
        added.person_id.clone(),
        base_scope,
        true,
    )
    .expect("verified fixture establishes one base whitelist row");
    let stable_binding = manual_peer_binding(&core, added.person_id.clone()).unwrap();
    let grant_scope = manual_scope("account-grant", &added.person_id);
    let stable_grant = ScopedTrustGrant::for_manual_peer(
        &stable_binding,
        "osl-chat",
        "account-grant",
        grant_scope.clone(),
        ScopedTrustConsent::ExplicitUserAction,
    )
    .unwrap();

    let replacement = keystore::generate_native_identity();
    let mut changed_friend = friend;
    changed_friend.x25519_public = replacement.x25519_public;
    changed_friend.mlkem_public_bytes = replacement.mlkem_public_bytes;
    changed_friend.ratchet_initial_pub = replacement.ratchet_initial_pub;
    *friend_core.osl.identity.lock().unwrap() = Some(changed_friend);
    let changed_invite = export_friend_code(&friend_core).expect("changed invite exports");
    let staged = add_friend_code(&core, &security, changed_invite.friend_code, None)
        .expect("signed transport-key change stages");
    assert_eq!(
        staged.disposition,
        AddFriendDisposition::KeyChangeRequiresVerification
    );

    let before_peers = core.osl.peer_map.lock().unwrap().clone();
    let before_whitelist = core.osl.whitelist_state.lock().unwrap().clone();
    let before_count = list_people(&core).unwrap()[0].whitelist_count;
    assert_eq!(before_count, 1);
    let expected_refusal = "Resolve this friend's pending key change before enabling encryption";

    let refusals = [
        set_friend_scope_permission(
            &core,
            &security,
            added.person_id.clone(),
            manual_scope("account-generic", &added.person_id),
            true,
        )
        .unwrap_err(),
        set_manual_peer_scope_permission(
            &core,
            &security,
            "osl-chat",
            "account-manual",
            added.person_id.clone(),
            manual_scope("account-manual", &added.person_id),
            true,
        )
        .unwrap_err(),
        apply_scoped_trust_grant(&security, &stable_binding, &stable_grant).unwrap_err(),
        set_friend_scope_reach(
            &core,
            &security,
            "osl-chat",
            "account-base",
            added.person_id.clone(),
            true,
        )
        .unwrap_err(),
    ];
    assert!(refusals.iter().all(|error| error == expected_refusal));
    assert_eq!(*core.osl.peer_map.lock().unwrap(), before_peers);
    assert_eq!(*core.osl.whitelist_state.lock().unwrap(), before_whitelist);
    assert_eq!(list_people(&core).unwrap()[0].whitelist_count, before_count);

    verify_friend_safety_number(
        &core,
        &security,
        added.person_id.clone(),
        staged.safety_number,
    )
    .expect("changed key verifies");
    let verified_binding = manual_peer_binding(&core, added.person_id.clone()).unwrap();
    let verified_grant = ScopedTrustGrant::for_manual_peer(
        &verified_binding,
        "osl-chat",
        "account-grant",
        grant_scope,
        ScopedTrustConsent::ExplicitUserAction,
    )
    .unwrap();
    let released = [
        set_friend_scope_permission(
            &core,
            &security,
            added.person_id.clone(),
            manual_scope("account-generic", &added.person_id),
            true,
        ),
        set_manual_peer_scope_permission(
            &core,
            &security,
            "osl-chat",
            "account-manual",
            added.person_id.clone(),
            manual_scope("account-manual", &added.person_id),
            true,
        ),
        apply_scoped_trust_grant(&security, &verified_binding, &verified_grant),
        set_friend_scope_reach(
            &core,
            &security,
            "osl-chat",
            "account-base",
            added.person_id,
            true,
        )
        .map(|_| ()),
    ];
    assert!(released.iter().all(Result::is_ok));
    assert_eq!(list_people(&core).unwrap()[0].whitelist_count, 3);

    println!(
        "TASK5034_NATIVE direct_writes_refused={} whitelist_writes_during_block=0 whitelist_rows_during_block={} direct_writes_after_verify={} whitelist_rows_after_verify=3 refusal=\"{}\"",
        refusals.len(),
        before_count,
        released.len(),
        expected_refusal
    );
}
