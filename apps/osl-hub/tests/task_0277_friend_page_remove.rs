use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::{
    add_friend_code, export_friend_code, list_people, remove_friend_from_friend_page,
    HubSecurityState,
};

const FILE_KEY: [u8; 32] = [0x77; 32];

struct Harness {
    path: PathBuf,
    previous_dir: Option<PathBuf>,
    previous_key: Option<[u8; 32]>,
}

impl Harness {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "osl-task-0277-remove-friend-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        let previous_dir = keystore::active_account_dir();
        let previous_key = ipc::main_password::get_file_storage_key();
        keystore::set_active_account_dir(Some(path.clone()));
        ipc::main_password::set_file_storage_key(Some(FILE_KEY));
        Self {
            path,
            previous_dir,
            previous_key,
        }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(self.previous_key);
        keystore::set_active_account_dir(self.previous_dir.clone());
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn add_fixture_friend(core: &HubCoreState, security: &HubSecurityState, entropy: u8) -> String {
    let remote = HubCoreState::default();
    *remote.osl.identity.lock().unwrap() = Some(keystore::identity_from_entropy(
        [entropy; 16],
        format!("osl-task-0277-friend-{entropy}"),
    ));
    let invite = export_friend_code(&remote).unwrap();
    add_friend_code(core, security, invite.friend_code, None)
        .unwrap()
        .person_id
}

#[test]
fn remove_friend_control_requires_confirmation_before_removing_the_selected_friend() {
    let _harness = Harness::new();
    let core = HubCoreState::default();
    *core.osl.identity.lock().unwrap() = Some(keystore::identity_from_entropy(
        [0x70; 16],
        "osl-task-0277-owner".to_owned(),
    ));
    let security = HubSecurityState::default();
    let selected = add_fixture_friend(&core, &security, 0x71);
    let preserved = add_fixture_friend(&core, &security, 0x72);

    let before_confirmation = list_people(&core).unwrap().len();
    assert_eq!(before_confirmation, 2);
    let prompt = remove_friend_from_friend_page(&core, &security, selected.clone(), false).unwrap();
    let after_unconfirmed_press = list_people(&core).unwrap().len();
    assert!(prompt.confirmation_required);
    assert!(!prompt.removed);
    assert_eq!(after_unconfirmed_press, 2);

    let removal = remove_friend_from_friend_page(&core, &security, selected.clone(), true).unwrap();
    let after_confirmation = list_people(&core).unwrap();
    assert!(removal.removed);
    assert!(!removal.confirmation_required);
    assert_eq!(after_confirmation.len(), 1);
    assert_eq!(after_confirmation[0].person_id, preserved);

    println!(
        "TASK0277 unconfirmed_friend_count={} confirmation_required={} confirmed_friend_count={} removed_person={}",
        after_unconfirmed_press,
        prompt.confirmation_required,
        after_confirmation.len(),
        selected,
    );
}
