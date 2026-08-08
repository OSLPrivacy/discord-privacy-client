use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::security::{
    read_friend_page_share_data, save_friend_page_share_data, FriendAccountReachAccount,
    FriendPageShareAccountChoice, FriendPageShareConversation, FriendPageShareData,
    HubSecurityState,
};

const CHILD_ENV: &str = "OSL_TASK_0839_BROKEN_COPY";
const FRIEND_A: &str = "hub-person-task-0838-a";
const FRIEND_B: &str = "hub-person-task-0838-b";
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
            "osl-task-0839-friend-share-copy-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("create isolated test-copy directory");
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

fn accounts() -> Vec<FriendAccountReachAccount> {
    vec![
        FriendAccountReachAccount {
            service_id: "discord".to_owned(),
            account_id: "account-0839-discord".to_owned(),
            account_label: "TASK0839 Discord".to_owned(),
        },
        FriendAccountReachAccount {
            service_id: "telegram".to_owned(),
            account_id: "account-0839-telegram".to_owned(),
            account_label: "TASK0839 Telegram".to_owned(),
        },
    ]
}

fn conversations() -> Vec<FriendPageShareConversation> {
    vec![
        FriendPageShareConversation {
            storage_key: "dm:conversation-0839-one".to_owned(),
            conversation_label: "TASK0839 One".to_owned(),
            checked: false,
        },
        FriendPageShareConversation {
            storage_key: "gc:conversation-0839-two".to_owned(),
            conversation_label: "TASK0839 Two".to_owned(),
            checked: false,
        },
    ]
}

fn save_choices(
    security: &HubSecurityState,
    friend: &str,
    account_catalog: &[FriendAccountReachAccount],
    conversation_catalog: &[FriendPageShareConversation],
    account_checks: [bool; 2],
    conversation_checks: [bool; 2],
    new_account_rule: &str,
) {
    let account_choices = account_catalog
        .iter()
        .zip(account_checks)
        .map(|(account, checked)| FriendPageShareAccountChoice {
            service_id: account.service_id.clone(),
            account_id: account.account_id.clone(),
            account_label: account.account_label.clone(),
            checked,
        })
        .collect();
    let conversation_choices = conversation_catalog
        .iter()
        .zip(conversation_checks)
        .map(|(conversation, checked)| FriendPageShareConversation {
            checked,
            ..conversation.clone()
        })
        .collect();
    save_friend_page_share_data(
        security,
        friend.to_owned(),
        account_choices,
        conversation_choices,
        new_account_rule.to_owned(),
    )
    .expect("save friend-page choices in isolated test copy");
}

fn choices_differ(left: &FriendPageShareData, right: &FriendPageShareData) -> bool {
    left.accounts != right.accounts
        || left.conversations != right.conversations
        || left.new_account_rule != right.new_account_rule
}

fn read(
    security: &HubSecurityState,
    friend: &str,
    account_catalog: &[FriendAccountReachAccount],
    conversation_catalog: &[FriendPageShareConversation],
) -> FriendPageShareData {
    read_friend_page_share_data(
        security,
        friend.to_owned(),
        account_catalog.to_vec(),
        conversation_catalog.to_vec(),
    )
    .expect("read friend-page choices from isolated test copy")
}

#[test]
fn broken_copy_exits_1_and_names_both_friend_records() {
    if std::env::var_os(CHILD_ENV).is_some() {
        let harness = Harness::new();
        let security = HubSecurityState::default();
        let account_catalog = accounts();
        let conversation_catalog = conversations();

        save_choices(
            &security,
            FRIEND_A,
            &account_catalog,
            &conversation_catalog,
            [true, false],
            [true, false],
            "on",
        );
        save_choices(
            &security,
            FRIEND_B,
            &account_catalog,
            &conversation_catalog,
            [false, true],
            [false, true],
            "off",
        );
        let control_a = read(&security, FRIEND_A, &account_catalog, &conversation_catalog);
        let control_b = read(&security, FRIEND_B, &account_catalog, &conversation_catalog);
        let control_separate = choices_differ(&control_a, &control_b);
        println!(
            "TASK0839_CONTROL friend_a={} friend_b={} separate={control_separate}",
            control_a.person_id, control_b.person_id
        );
        if !control_separate {
            drop(harness);
            std::process::exit(2);
        }

        // The only break: save Friend A's exact choices under Friend B's id in
        // this isolated test copy.
        save_choices(
            &security,
            FRIEND_B,
            &account_catalog,
            &conversation_catalog,
            [true, false],
            [true, false],
            "on",
        );
        let broken_a = read(&security, FRIEND_A, &account_catalog, &conversation_catalog);
        let broken_b = read(&security, FRIEND_B, &account_catalog, &conversation_catalog);
        let broken_separate = choices_differ(&broken_a, &broken_b);
        eprintln!(
            "TASK0839_SEPARATION_PROOF friend_a={} friend_b={} separate={} saved_a_onto_b=true",
            broken_a.person_id, broken_b.person_id, broken_separate
        );
        drop(harness);
        std::process::exit(if broken_separate { 0 } else { 1 });
    }

    let output = Command::new(std::env::current_exe().expect("resolve focused test binary"))
        .env(CHILD_ENV, "1")
        .args([
            "--exact",
            "broken_copy_exits_1_and_names_both_friend_records",
            "--nocapture",
            "--test-threads=1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run the separation proof against the broken test copy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    print!("{stdout}");
    eprint!("{stderr}");
    let proof_exit = output.status.code().unwrap_or(-1);
    println!("TASK0839_PROOF_EXIT={proof_exit}");

    assert_eq!(proof_exit, 1, "broken separation proof must exit 1");
    assert!(stdout.contains("TASK0839_CONTROL") && stdout.contains("separate=true"));
    assert!(stderr.contains("TASK0839_SEPARATION_PROOF"));
    assert!(stderr.contains(FRIEND_A), "proof output must name Friend A");
    assert!(stderr.contains(FRIEND_B), "proof output must name Friend B");
    assert!(stderr.contains("separate=false"));
    assert!(stderr.contains("saved_a_onto_b=true"));
}
