#![cfg(feature = "core")]

use osl_privacy_hub::broker::{
    activate_owned_osl_chat_context, burn_active_osl_chat_both_sides, load_osl_chat_history,
    HubBrokerState, OSL_CHAT_BOTH_SIDES_ALREADY_GONE,
};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::{
    add_friend_code, export_friend_code, manual_peer_binding, set_manual_peer_scope_permission,
    set_scope_security, verify_friend_safety_number, HubSecurityState,
};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use store::MessageStore;

const TEST_MAIN_PASSWORD: &str = "task-3642-osl-chat-burn-password";
const ALICE_MARK_1: &str = "TASK3642 Alice exact mark 1";
const ALICE_MARK_2: &str = "TASK3642 Alice exact mark 2";
const BOB_MARK_1: &str = "TASK3642 Bob exact mark 1";
const BOB_MARK_2: &str = "TASK3642 Bob exact mark 2";
const MARKS: [&str; 4] = [ALICE_MARK_1, ALICE_MARK_2, BOB_MARK_1, BOB_MARK_2];

fn expected_marks() -> Vec<String> {
    MARKS.into_iter().map(str::to_owned).collect()
}

fn fixture_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct TestStorage {
    root: PathBuf,
}

impl TestStorage {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3642-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).expect("create isolated OSL test root");
        keystore::set_base_dir_override(Some(root.clone()));
        ipc::main_password::set_file_storage_key(None);
        ipc::main_password::set_main_password(&root, TEST_MAIN_PASSWORD)
            .expect("set isolated OSL main password");
        Self { root }
    }

    fn account(&self, name: &str) -> PathBuf {
        let dir = self.root.join(name);
        std::fs::create_dir(&dir).expect("create isolated OSL account dir");
        dir
    }

    fn activate(dir: &Path) {
        keystore::set_active_account_dir(Some(dir.to_owned()));
    }
}

impl Drop for TestStorage {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn core(identity: keystore::Identity) -> HubCoreState {
    let core = HubCoreState::default();
    *core.osl.identity.lock().unwrap() = Some(identity);
    core
}

struct Peer {
    dir: PathBuf,
    identity_id: String,
    core: HubCoreState,
    security: HubSecurityState,
    broker: HubBrokerState,
    friend_code: String,
    scope_key: Mutex<Option<String>>,
}

impl Peer {
    fn new(storage: &TestStorage, name: &str) -> Self {
        let dir = storage.account(name);
        let identity = keystore::generate_identity(format!("osl-{name}-task-3642"));
        let identity_id = identity.user_id.clone();
        let core = core(identity);
        TestStorage::activate(&dir);
        let exported = export_friend_code(&core).expect("export friend code");
        Self {
            dir,
            identity_id,
            core,
            security: HubSecurityState::default(),
            broker: HubBrokerState::default(),
            friend_code: exported.friend_code,
            scope_key: Mutex::new(None),
        }
    }

    fn activate(&self) {
        TestStorage::activate(&self.dir);
    }

    fn open_osl_chat_context_to(&self, other_code: &str) {
        self.activate();
        let friend = add_friend_code(
            &self.core,
            &self.security,
            other_code.to_owned(),
            Some("task 3642 peer".to_owned()),
        )
        .expect("add OSL Chat friend code");
        verify_friend_safety_number(
            &self.core,
            &self.security,
            friend.person_id.clone(),
            friend.safety_number.clone(),
        )
        .expect("verify OSL Chat safety number");
        let binding =
            manual_peer_binding(&self.core, friend.person_id).expect("manual peer binding");
        let activated = activate_owned_osl_chat_context(&self.broker, &self.identity_id, binding)
            .expect("activate OSL Chat context");
        set_manual_peer_scope_permission(
            &self.core,
            &self.security,
            "osl-chat",
            "osl-main",
            activated.person_id,
            activated.scope.clone(),
            true,
        )
        .expect("approve OSL Chat scope");
        set_scope_security(&self.security, activated.scope.clone(), 3600, true)
            .expect("enable decrypted display for OSL Chat");
        let scope: ipc::scope::Scope = activated.scope.try_into().expect("valid OSL Chat scope");
        *self.scope_key.lock().unwrap() = Some(scope.storage_key());
    }

    fn open_history_store(&self, label: &str) {
        self.activate();
        let identity = self
            .core
            .osl
            .identity
            .lock()
            .unwrap()
            .clone()
            .expect("identity is loaded");
        let path = self.dir.join(format!("message-store-{label}"));
        std::fs::create_dir_all(&path).expect("create message store directory");
        let store = MessageStore::open(&path, identity.x25519_secret.as_bytes())
            .expect("open OSL Chat history store");
        *self.core.osl.message_store.lock().unwrap() = Some(store);
    }

    fn scope_key(&self) -> String {
        self.scope_key
            .lock()
            .unwrap()
            .clone()
            .expect("OSL Chat scope was opened")
    }

    fn put_self_mark(&self, message_id: &str, plaintext: &str) {
        self.activate();
        ipc::commands::cmd_osl_persist_outbound(
            &self.core.osl,
            self.scope_key(),
            message_id.to_owned(),
            plaintext.to_owned(),
        )
        .expect("persist self-authored OSL Chat mark");
    }

    fn put_peer_mark(&self, sender_osl_user_id: &str, message_id: &str, plaintext: &str) {
        self.activate();
        ipc::commands::cmd_osl_persist_inbound(
            &self.core.osl,
            self.scope_key(),
            message_id.to_owned(),
            sender_osl_user_id.to_owned(),
            plaintext.to_owned(),
        )
        .expect("persist peer-authored OSL Chat mark");
    }

    fn exact_marks(&self) -> Vec<String> {
        self.activate();
        let mut marks = load_osl_chat_history(&self.core, &self.broker)
            .expect("load OSL Chat history")
            .into_iter()
            .map(|row| row.plaintext)
            .filter(|plaintext| MARKS.contains(&plaintext.as_str()))
            .collect::<Vec<_>>();
        marks.sort();
        marks
    }

    fn burn_both_sides(&self) -> (usize, String) {
        self.activate();
        let token = self
            .broker
            .active_osl_chat_context_token()
            .expect("active OSL Chat context");
        let report = burn_active_osl_chat_both_sides(&self.core, &self.broker, &token)
            .expect("burn active OSL Chat Both Sides");
        (report.rows_destroyed, report.status.to_owned())
    }
}

#[test]
fn task3642_burn_one_conversation_from_both_people_is_exact_and_idempotent() {
    let _serial = fixture_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let storage = TestStorage::new("both-people");
    let alice = Peer::new(&storage, "alice");
    let bob = Peer::new(&storage, "bob");
    alice.open_osl_chat_context_to(&bob.friend_code);
    bob.open_osl_chat_context_to(&alice.friend_code);
    alice.open_history_store("alice-copy");
    bob.open_history_store("bob-copy");

    alice.put_self_mark("task3642-alice-copy-alice-1", ALICE_MARK_1);
    alice.put_self_mark("task3642-alice-copy-alice-2", ALICE_MARK_2);
    alice.put_peer_mark(&bob.identity_id, "task3642-alice-copy-bob-1", BOB_MARK_1);
    alice.put_peer_mark(&bob.identity_id, "task3642-alice-copy-bob-2", BOB_MARK_2);
    bob.put_peer_mark(
        &alice.identity_id,
        "task3642-bob-copy-alice-1",
        ALICE_MARK_1,
    );
    bob.put_peer_mark(
        &alice.identity_id,
        "task3642-bob-copy-alice-2",
        ALICE_MARK_2,
    );
    bob.put_self_mark("task3642-bob-copy-bob-1", BOB_MARK_1);
    bob.put_self_mark("task3642-bob-copy-bob-2", BOB_MARK_2);

    let alice_before = alice.exact_marks();
    let bob_before = bob.exact_marks();
    println!(
        "TASK3642 before copy=alice count={} marks={:?}",
        alice_before.len(),
        alice_before
    );
    println!(
        "TASK3642 before copy=bob count={} marks={:?}",
        bob_before.len(),
        bob_before
    );
    assert_eq!(alice_before, expected_marks());
    assert_eq!(bob_before, expected_marks());

    let (alice_deleted, alice_status) = alice.burn_both_sides();
    let (bob_deleted, bob_status) = bob.burn_both_sides();
    let first_pair_new_deletions = alice_deleted + bob_deleted;
    let alice_after = alice.exact_marks();
    let bob_after = bob.exact_marks();
    println!(
        "TASK3642 first_burn copy=alice rows_destroyed={} status={} after_count={}",
        alice_deleted,
        alice_status,
        alice_after.len()
    );
    println!(
        "TASK3642 first_burn copy=bob rows_destroyed={} status={} after_count={}",
        bob_deleted,
        bob_status,
        bob_after.len()
    );
    println!(
        "TASK3642 first_pair_new_deletion_count={}",
        first_pair_new_deletions
    );
    assert_eq!(alice_deleted, 4);
    assert_eq!(bob_deleted, 4);
    assert_eq!(first_pair_new_deletions, 8);
    assert!(alice_after.is_empty());
    assert!(bob_after.is_empty());

    let (alice_repeat_deleted, alice_repeat_status) = alice.burn_both_sides();
    let (bob_repeat_deleted, bob_repeat_status) = bob.burn_both_sides();
    let repeated_new_deletions = alice_repeat_deleted + bob_repeat_deleted;
    let total_new_deletions = first_pair_new_deletions + repeated_new_deletions;
    let alice_final = alice.exact_marks();
    let bob_final = bob.exact_marks();
    println!(
        "TASK3642 repeat copy=alice rows_destroyed={} status={} final_count={}",
        alice_repeat_deleted,
        alice_repeat_status,
        alice_final.len()
    );
    println!(
        "TASK3642 repeat copy=bob rows_destroyed={} status={} final_count={}",
        bob_repeat_deleted,
        bob_repeat_status,
        bob_final.len()
    );
    println!(
        "TASK3642 repeated_new_deletion_count={} total_new_deletion_count={}",
        repeated_new_deletions, total_new_deletions
    );
    assert_eq!(alice_repeat_deleted, 0);
    assert_eq!(bob_repeat_deleted, 0);
    assert_eq!(alice_repeat_status, OSL_CHAT_BOTH_SIDES_ALREADY_GONE);
    assert_eq!(bob_repeat_status, OSL_CHAT_BOTH_SIDES_ALREADY_GONE);
    assert_eq!(repeated_new_deletions, 0);
    assert_eq!(total_new_deletions, 8);
    assert!(alice_final.is_empty());
    assert!(bob_final.is_empty());
}
