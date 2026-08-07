#![cfg(feature = "core")]

use osl_privacy_hub::models::ServiceKind;
use osl_privacy_hub::services::{
    read_messaging_risk_agreement, save_messaging_risk_agreement, MESSAGING_RISK_FACTS,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const TEST_MAIN_PASSWORD: &str = "task-3111-risk-agreement-password";

struct AccountStorage {
    root: PathBuf,
}

impl AccountStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3111-risk-agreement-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create isolated task 3111 root");
        keystore::set_base_dir_override(Some(root.clone()));
        keystore::set_active_account_dir(None);
        ipc::main_password::set_file_storage_key(None);
        ipc::main_password::set_main_password(&root, TEST_MAIN_PASSWORD)
            .expect("set isolated main password");
        Self { root }
    }

    fn account_dir(&self, label: &str) -> PathBuf {
        let dir = self.root.join(label);
        fs::create_dir(&dir).expect("create isolated account dir");
        dir
    }
}

impl Drop for AccountStorage {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn activate(dir: &Path) {
    keystore::set_active_account_dir(Some(dir.to_owned()));
}

#[test]
fn task_3111_store_messaging_risk_agreement_per_account() {
    let storage = AccountStorage::new();
    let alice_dir = storage.account_dir("alice");
    activate(&alice_dir);

    let identity = keystore::generate_identity("task-3111-alice".to_owned());
    let owner = identity.user_id.clone();
    let registry = osl_privacy_hub::services::ServiceRegistryState::load(
        alice_dir.join("service-registry.json"),
    );
    let first = registry
        .create_for_owner(&owner, ServiceKind::Discord, "Discord first".to_owned())
        .expect("create first Discord service account");
    let second = registry
        .create_for_owner(&owner, ServiceKind::Discord, "Discord second".to_owned())
        .expect("create second Discord service account");

    let before_save = ipc::main_password::now_unix_secs_pub();
    save_messaging_risk_agreement(&owner, "discord", &first.id)
        .expect("save Discord risk agreement for first account");
    let after_save = ipc::main_password::now_unix_secs_pub();

    let agreement = read_messaging_risk_agreement(&owner, "discord", &first.id)
        .expect("read first Discord risk agreement")
        .expect("first Discord account must be agreed");
    let second_agreement = read_messaging_risk_agreement(&owner, "discord", &second.id)
        .expect("read second Discord risk agreement");

    println!("TASK3111_DIRECT_SAVE_COMMAND=save_messaging_risk_agreement");
    println!("TASK3111_DIRECT_READ_COMMAND=read_messaging_risk_agreement");
    println!("TASK3111_AGREEMENT_SERVICE={}", agreement.service_id);
    println!("TASK3111_AGREEMENT_ACCOUNT={}", agreement.account_id);
    println!("TASK3111_AGREEMENT_DATE={}", agreement.agreed_at);
    println!(
        "TASK3111_AGREEMENT_WORDING_COUNT={}",
        agreement.wording.len()
    );
    for wording in &agreement.wording {
        println!("TASK3111_AGREEMENT_WORDING={wording}");
    }
    println!("TASK3111_SECOND_ACCOUNT={}", second.id);
    println!(
        "TASK3111_SECOND_ACCOUNT_AGREED={}",
        second_agreement.is_some()
    );

    assert_eq!(agreement.service_id, "discord");
    assert_eq!(agreement.account_id, first.id);
    assert!(
        agreement.agreed_at >= before_save && agreement.agreed_at <= after_save,
        "agreement date must be the save time"
    );
    assert_eq!(agreement.wording, MESSAGING_RISK_FACTS);
    assert!(
        second_agreement.is_none(),
        "agreement must not apply to a second account in the same service"
    );
}
