use std::path::{Path, PathBuf};

use ipc::allowed_places::{AllowedPlaceQuery, AllowedPlaceRecord};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::{self, AddFriendDisposition, HubSecurityState};
use tempfile::TempDir;

const TEST_FILE_KEY: [u8; 32] = [0x83; 32];

struct ActiveAccount {
    dir: PathBuf,
    previous_active: Option<PathBuf>,
    previous_key: Option<[u8; 32]>,
    _temp: TempDir,
}

impl ActiveAccount {
    fn new() -> Self {
        let temp = TempDir::new().expect("create task 0183 account");
        let dir = temp.path().to_path_buf();
        let previous_active = keystore::active_account_dir();
        let previous_key = ipc::main_password::get_file_storage_key();
        keystore::set_active_account_dir(Some(dir.clone()));
        ipc::main_password::set_file_storage_key(Some(TEST_FILE_KEY));
        Self {
            dir,
            previous_active,
            previous_key,
            _temp: temp,
        }
    }
}

impl Drop for ActiveAccount {
    fn drop(&mut self) {
        keystore::set_active_account_dir(self.previous_active.clone());
        ipc::main_password::set_file_storage_key(self.previous_key);
    }
}

fn install_identity(core: &HubCoreState, user_id: &str) {
    core.osl
        .install_identity(keystore::generate_identity(user_id.to_owned()));
}

fn request_sink_count(dir: &Path) -> usize {
    let path = dir.join("pending_friend_requests.json");
    let Ok(raw) = std::fs::read(path) else {
        return 0;
    };
    let plain = ipc::main_password::maybe_decrypt(&raw).expect("request sink decrypts");
    let records: serde_json::Value = serde_json::from_slice(&plain).expect("request sink is JSON");
    records.as_array().map(Vec::len).unwrap_or(0)
}

fn notice_sink_count(dir: &Path) -> usize {
    osl_privacy_hub::osl_chat_queue::osl_chat_send_queue(dir)
        .expect("notice sink opens")
        .pending()
        .expect("notice sink reads")
        .len()
}

#[test]
fn one_party_whitelist_stays_silent_after_identity_and_restart() {
    let account = ActiveAccount::new();
    let owner = HubCoreState::default();
    install_identity(&owner, "task-0183-owner");
    let security = HubSecurityState::default();

    let invite = security::create_one_use_invite_link(&security, "TASK0183 non OSL person".into())
        .expect("create local one-use invite for a non-OSL person");
    assert_eq!(invite.recipient_label, "TASK0183 non OSL person");
    assert_eq!(invite.use_limit, 1);
    let non_osl_invite_count = security::list_one_use_invite_links(&security)
        .expect("list one-use invites")
        .len();

    let record = AllowedPlaceRecord {
        app: "discord".to_owned(),
        account: "task-0183-local-account".to_owned(),
        kind: "direct_message".to_owned(),
        stable_id: "discord:task-0183-local-account:direct_message:task-0183-non-osl".to_owned(),
    };
    let original_query = AllowedPlaceQuery::from(record.clone());
    security::add_allowed_place_record(&security, record).expect("whitelist non-OSL place");
    let allowed_before_restart =
        security::query_allowed_place_allowed(&security, original_query.clone())
            .expect("original allowed-place query runs before restart");
    assert!(allowed_before_restart);

    let request_sink_before_identity = request_sink_count(&account.dir);
    let notice_sink_before_identity = notice_sink_count(&account.dir);
    assert_eq!(request_sink_before_identity, 0);
    assert_eq!(notice_sink_before_identity, 0);

    let friend_source = HubCoreState::default();
    install_identity(&friend_source, "task-0183-friend-osl-identity");
    let friend_code =
        security::export_friend_code(&friend_source).expect("new OSL identity exports invite");
    let adopted = security::add_friend_code(
        &owner,
        &security,
        friend_code.friend_code,
        Some("TASK0183 non OSL person".into()),
    )
    .expect("give the same person an OSL identity");
    assert_eq!(adopted.disposition, AddFriendDisposition::Added);

    let request_sink_after_identity = request_sink_count(&account.dir);
    let notice_sink_after_identity = notice_sink_count(&account.dir);
    assert_eq!(request_sink_after_identity, 0);
    assert_eq!(notice_sink_after_identity, 0);

    let restarted_copy_a = HubSecurityState::default();
    let restarted_copy_b = HubSecurityState::default();
    let copy_a_allowed =
        security::query_allowed_place_allowed(&restarted_copy_a, original_query.clone())
            .expect("copy A reloads original allowed-place query after restart");
    let copy_b_allowed = security::query_allowed_place_allowed(&restarted_copy_b, original_query)
        .expect("copy B reloads original allowed-place query after restart");
    assert!(copy_a_allowed);
    assert!(copy_b_allowed);

    println!(
        "TASK0183 one_party_whitelist_silent non_osl_invites={} identity_disposition={:?} request_sink_before_identity={} notice_sink_before_identity={} request_sink_after_identity={} notice_sink_after_identity={} restarted_copies=2 copy_a_allowed={} copy_b_allowed={} original_allowed_after_restart={}",
        non_osl_invite_count,
        adopted.disposition,
        request_sink_before_identity,
        notice_sink_before_identity,
        request_sink_after_identity,
        notice_sink_after_identity,
        copy_a_allowed,
        copy_b_allowed,
        copy_a_allowed && copy_b_allowed
    );
}
