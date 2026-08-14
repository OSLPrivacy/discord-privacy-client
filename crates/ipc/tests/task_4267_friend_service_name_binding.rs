use ipc::friend_service_name::{
    friends_with_unknown_service, load_friend_service_name_file_state,
    save_friend_service_name_file_state, FriendServiceKind, FriendServiceNameFileState,
};
use ipc::main_password::set_file_storage_key;
use std::sync::Mutex;

// The file-storage key is process global.
static KEY_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn friends_are_bound_to_their_x_instagram_and_messenger_names_and_read_back() {
    let _guard = KEY_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    set_file_storage_key(Some([0x67u8; 32]));

    let dir = tempfile::tempdir().expect("tempdir");

    let cases = [
        ("friend-4267-x", "x", "friend_x_handle", FriendServiceKind::X),
        (
            "friend-4267-instagram",
            "instagram",
            "friend_instagram_handle",
            FriendServiceKind::Instagram,
        ),
        (
            "friend-4267-messenger",
            "messenger",
            "friend_messenger_name",
            FriendServiceKind::Messenger,
        ),
    ];

    let mut saving_state = FriendServiceNameFileState::new();
    for (friend_id, service_id, service_name, _) in cases {
        saving_state
            .bind(friend_id, service_id, service_name)
            .unwrap_or_else(|error| panic!("bind {friend_id} to {service_id}: {error}"));
    }
    save_friend_service_name_file_state(dir.path(), &saving_state).expect("save friend service names");

    // Read back through a freshly loaded copy of the on-disk state, not the
    // in-memory struct just written, so this proves the round trip through
    // the encrypted file rather than just the in-process bind().
    let loaded_state =
        load_friend_service_name_file_state(dir.path()).expect("load friend service names");

    // Check every friend that came back has a known service *before* trying
    // to read each one back by (friend, service) key, so a friend that came
    // back with no service is named here rather than reported as merely
    // "missing".
    let unknown = friends_with_unknown_service(loaded_state.records());
    println!("TASK4267 unknown_service_count={}", unknown.len());
    assert!(
        unknown.is_empty(),
        "friends read back with no known service: {unknown:?}"
    );

    let mut saved_and_read_back = 0usize;
    for (friend_id, service_id, expected_name, expected_kind) in cases {
        let read = loaded_state
            .read(friend_id, service_id)
            .unwrap_or_else(|| panic!("{friend_id} was not read back with service {service_id}"));
        println!(
            "TASK4267 friend={} service_id={} service_name={}",
            read.friend_id, read.service_id, read.service_name
        );
        assert_eq!(read.friend_id, friend_id);
        assert_eq!(read.service_name, expected_name);
        assert_eq!(read.service().expect("known service"), expected_kind);
        saved_and_read_back += 1;
    }
    println!("TASK4267 friends_saved_and_read_back={saved_and_read_back}");
    assert_eq!(saved_and_read_back, 3);

    let mut refusal_state = loaded_state.clone();
    let refused = refusal_state.bind("friend-4267-made-up", "friendster", "someone");
    let error = refused.expect_err("made up service must be refused");
    println!("TASK4267 refused_made_up_service_error={error}");
    assert!(error.contains("friendster"));

    set_file_storage_key(None);
}
