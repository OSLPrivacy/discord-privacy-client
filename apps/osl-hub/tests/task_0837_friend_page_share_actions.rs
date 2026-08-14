use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::{
    add_friend_code, cancel_friend_page_share_data, export_friend_code,
    read_friend_page_share_data, remove_friend_from_friend_page, remove_friend_page_share_data,
    save_friend_page_share_data, set_friend_page_new_account_rule, FriendAccountReachAccount,
    FriendPageShareAccountChoice, FriendPageShareConversation, FriendPageShareData,
    HubSecurityState,
};

const FILE_KEY: [u8; 32] = [0x83; 32];

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
            "osl-task-0837-actions-{}-{nonce}",
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct FriendRecord {
    present: bool,
    share: FriendPageShareData,
}

fn add_fixture_friend(
    core: &HubCoreState,
    security: &HubSecurityState,
    entropy: u8,
    name: &str,
) -> String {
    let remote = HubCoreState::default();
    *remote.osl.identity.lock().unwrap() = Some(keystore::identity_from_entropy(
        [entropy; 16],
        name.to_owned(),
    ));
    let code = export_friend_code(&remote).unwrap();
    add_friend_code(core, security, code.friend_code, Some(name.to_owned()))
        .unwrap()
        .person_id
}

fn record(
    core: &HubCoreState,
    security: &HubSecurityState,
    person_id: &str,
    accounts: &[FriendAccountReachAccount],
    conversations: &[FriendPageShareConversation],
) -> FriendRecord {
    let present = osl_privacy_hub::security::list_people(core)
        .unwrap()
        .iter()
        .any(|person| person.person_id == person_id);
    let share = read_friend_page_share_data(
        security,
        person_id.to_owned(),
        accounts.to_vec(),
        conversations.to_vec(),
    )
    .unwrap();
    FriendRecord { present, share }
}

fn differences(before: &FriendRecord, after: &FriendRecord) -> usize {
    usize::from(before.present != after.present)
        + usize::from(before.share.person_id != after.share.person_id)
        + usize::from(before.share.accounts != after.share.accounts)
        + usize::from(before.share.conversations != after.share.conversations)
        + usize::from(before.share.new_account_rule != after.share.new_account_rule)
        + usize::from(before.share.actions != after.share.actions)
}

fn account_choices(
    accounts: &[FriendAccountReachAccount],
    checks: [bool; 2],
) -> Vec<FriendPageShareAccountChoice> {
    accounts
        .iter()
        .zip(checks)
        .map(|(account, checked)| FriendPageShareAccountChoice {
            service_id: account.service_id.clone(),
            account_id: account.account_id.clone(),
            account_label: account.account_label.clone(),
            checked,
        })
        .collect()
}

#[test]
fn each_direct_action_changes_one_selected_field_and_zero_other_friend_fields() {
    let _harness = Harness::new();
    let core = HubCoreState::default();
    *core.osl.identity.lock().unwrap() = Some(keystore::identity_from_entropy(
        [0x81; 16],
        "osl-task-0837-owner".to_owned(),
    ));
    let security = HubSecurityState::default();
    let selected = add_fixture_friend(&core, &security, 0x82, "osl-task-0837-selected");
    let other = add_fixture_friend(&core, &security, 0x84, "osl-task-0837-other");
    let accounts = vec![
        FriendAccountReachAccount {
            service_id: "discord".to_owned(),
            account_id: "account-0837-discord".to_owned(),
            account_label: "TASK0837 Discord".to_owned(),
        },
        FriendAccountReachAccount {
            service_id: "telegram".to_owned(),
            account_id: "account-0837-telegram".to_owned(),
            account_label: "TASK0837 Telegram".to_owned(),
        },
    ];
    let conversations = vec![
        FriendPageShareConversation {
            storage_key: "dm:conversation-0837-one".to_owned(),
            conversation_label: "TASK0837 One".to_owned(),
            checked: false,
        },
        FriendPageShareConversation {
            storage_key: "gc:conversation-0837-two".to_owned(),
            conversation_label: "TASK0837 Two".to_owned(),
            checked: false,
        },
    ];

    save_friend_page_share_data(
        &security,
        other.clone(),
        account_choices(&accounts, [false, true]),
        vec![
            conversations[0].clone(),
            FriendPageShareConversation {
                checked: true,
                ..conversations[1].clone()
            },
        ],
        "on".to_owned(),
    )
    .unwrap();
    let other_baseline = record(&core, &security, &other, &accounts, &conversations);

    let save_before = record(&core, &security, &selected, &accounts, &conversations);
    save_friend_page_share_data(
        &security,
        selected.clone(),
        account_choices(&accounts, [true, false]),
        conversations.clone(),
        "off".to_owned(),
    )
    .unwrap();
    let save_after = record(&core, &security, &selected, &accounts, &conversations);
    let save_changed = differences(&save_before, &save_after);
    let save_other = differences(
        &other_baseline,
        &record(&core, &security, &other, &accounts, &conversations),
    );
    assert_eq!((save_changed, save_other), (1, 0));
    println!(
        "TASK0837 action=save selected_friend={selected} selected_changed_fields={save_changed} other_friend={other} other_full_record_differences={save_other}"
    );

    let mut cancel_draft = save_after.clone();
    cancel_draft.share.conversations[0].checked = true;
    let cancel_after = FriendRecord {
        present: true,
        share: cancel_friend_page_share_data(
            &security,
            selected.clone(),
            accounts.clone(),
            conversations.clone(),
        )
        .unwrap(),
    };
    let cancel_changed = differences(&cancel_draft, &cancel_after);
    let cancel_other = differences(
        &other_baseline,
        &record(&core, &security, &other, &accounts, &conversations),
    );
    assert_eq!((cancel_changed, cancel_other), (1, 0));
    assert_eq!(cancel_after, save_after);
    println!(
        "TASK0837 action=cancel selected_friend={selected} selected_changed_fields={cancel_changed} other_friend={other} other_full_record_differences={cancel_other}"
    );

    let rule_before = record(&core, &security, &selected, &accounts, &conversations);
    set_friend_page_new_account_rule(
        &security,
        selected.clone(),
        accounts.clone(),
        conversations.clone(),
        "on".to_owned(),
    )
    .unwrap();
    let rule_after = record(&core, &security, &selected, &accounts, &conversations);
    let rule_changed = differences(&rule_before, &rule_after);
    let rule_other = differences(
        &other_baseline,
        &record(&core, &security, &other, &accounts, &conversations),
    );
    assert_eq!((rule_changed, rule_other), (1, 0));
    println!(
        "TASK0837 action=new_account_rule selected_friend={selected} selected_changed_fields={rule_changed} other_friend={other} other_full_record_differences={rule_other}"
    );

    remove_friend_page_share_data(
        &security,
        selected.clone(),
        accounts.clone(),
        conversations.clone(),
    )
    .unwrap();
    let remove_before = record(&core, &security, &selected, &accounts, &conversations);
    let confirmation =
        remove_friend_from_friend_page(&core, &security, selected.clone(), false).unwrap();
    assert!(confirmation.confirmation_required);
    assert!(!confirmation.removed);
    assert_eq!(
        record(&core, &security, &selected, &accounts, &conversations),
        remove_before
    );
    let removed = remove_friend_from_friend_page(&core, &security, selected.clone(), true).unwrap();
    assert!(removed.removed);
    let remove_after = record(&core, &security, &selected, &accounts, &conversations);
    let remove_changed = differences(&remove_before, &remove_after);
    let remove_other = differences(
        &other_baseline,
        &record(&core, &security, &other, &accounts, &conversations),
    );
    assert_eq!((remove_changed, remove_other), (1, 0));
    println!(
        "TASK0837 action=remove_confirmation selected_friend={selected} selected_changed_fields={remove_changed} other_friend={other} other_full_record_differences={remove_other} confirmation_required_before_remove={} removed={}",
        confirmation.confirmation_required,
        removed.removed
    );
    println!(
        "TASK0837 finish actions=4 selected_changed_fields=1,1,1,1 other_full_record_differences=0,0,0,0"
    );
}
