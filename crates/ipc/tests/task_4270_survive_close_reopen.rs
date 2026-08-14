use ipc::allowed_places::{list_allowed_place_records, AllowedPlaceRecord};
use ipc::friend_service_name::{
    load_friend_service_name_file_state, save_friend_service_name_file_state,
    FriendServiceNameFileState,
};
use keystore::{set_active_account_dir, set_base_dir_override};
use std::path::Path;
use std::sync::Mutex;

static KEY_LOCK: Mutex<()> = Mutex::new(());

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        set_active_account_dir(None);
        set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
    }
}

fn use_temp_config_dir(dir: &Path) -> ConfigDirGuard {
    set_active_account_dir(None);
    set_base_dir_override(Some(dir.to_path_buf()));
    ipc::main_password::set_file_storage_key(None);
    ConfigDirGuard
}

const SERVICES: [(&str, &str, &str); 3] = [
    ("x", "friend-4270-x", "friend_x_handle"),
    ("instagram", "friend-4270-instagram", "friend_instagram_handle"),
    ("messenger", "friend-4270-messenger", "friend_messenger_name"),
];

fn allowed_place_for(service_id: &str) -> AllowedPlaceRecord {
    let account = format!("{service_id}-account");
    let peer = format!("{service_id}-place");
    AllowedPlaceRecord::from_parts(
        service_id.to_owned(),
        account.clone(),
        "direct_message".to_owned(),
        format!("{service_id}:{account}:direct_message:{peer}"),
    )
}

#[test]
fn the_three_survive_closing_and_reopening_osl() {
    let _serial = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());

    // --- Session 1: add a friend and an allowed place on each of the three ---
    {
        let mut friend_state = FriendServiceNameFileState::new();
        for (service_id, friend_id, service_name) in SERVICES {
            friend_state
                .bind(friend_id, service_id, service_name)
                .expect("bind friend to known service");
            ipc::allowed_places::add_allowed_place_record(dir.path(), allowed_place_for(service_id))
                .expect("add allowed place");
        }
        save_friend_service_name_file_state(dir.path(), &friend_state)
            .expect("save friend service name state");
        // friend_state and its open handles are dropped here, out of scope.
    }
    let places_before = list_allowed_place_records(dir.path()).expect("list allowed places before close");
    let friends_before = load_friend_service_name_file_state(dir.path())
        .expect("load friend service name state before close");
    let count_before = places_before.len() + friends_before.count();
    println!("TASK4270 count_before={count_before}");
    assert_eq!(count_before, 6, "3 friends + 3 allowed places before close");

    // --- Close OSL: nothing but the on-disk files remains; drop everything above ---
    drop(places_before);
    drop(friends_before);

    // --- Reopen OSL: load fresh state from disk only ---
    let friends_after = load_friend_service_name_file_state(dir.path())
        .expect("load friend service name state after reopen");
    let places_after = list_allowed_place_records(dir.path()).expect("list allowed places after reopen");
    let count_after = friends_after.count() + places_after.len();
    println!("TASK4270 count_after={count_after}");
    let count_lost = count_before as i64 - count_after as i64;
    println!("TASK4270 count_lost={count_lost}");
    assert_eq!(count_lost, 0, "no record may be lost across close/reopen");

    for (service_id, friend_id, service_name) in SERVICES {
        let friend_record = friends_after
            .read(friend_id, service_id)
            .unwrap_or_else(|| panic!("friend {friend_id} bound to {service_id} must read back"));
        assert_eq!(friend_record.service_id, service_id);
        assert_eq!(friend_record.service_name, service_name);
        println!(
            "TASK4270 friend={} service_id={} service_name={}",
            friend_record.friend_id, friend_record.service_id, friend_record.service_name
        );

        let expected_place = allowed_place_for(service_id);
        let place_record = places_after
            .iter()
            .find(|record| record.stable_id == expected_place.stable_id)
            .unwrap_or_else(|| panic!("allowed place for {service_id} must read back"));
        assert_eq!(place_record.app, service_id);
        println!(
            "TASK4270 allowed_place service_id={} stable_id={}",
            place_record.app, place_record.stable_id
        );
    }

    assert_eq!(friends_after.count(), 3, "all 3 friend bindings read back");
    assert_eq!(places_after.len(), 3, "all 3 allowed places read back");
}
