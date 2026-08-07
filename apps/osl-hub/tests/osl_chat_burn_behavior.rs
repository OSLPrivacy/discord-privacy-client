use ipc::scope::Scope;
use osl_privacy_hub::broker::{
    activate_owned_osl_chat_context, burn_osl_chat_history, filter_osl_chat_history_visibility,
    OslChatBurnChoice,
};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::ManualPeerBinding;
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;
use uuid::Uuid;

fn seed_row(
    store: &MessageStore,
    channel_id: &str,
    message_id: &str,
    sender_osl_user_id: &str,
    plaintext: &str,
    seq: i64,
) {
    store
        .put(&StoredMessage {
            discord_message_id: message_id.to_owned(),
            channel_id: channel_id.to_owned(),
            sender_discord_id: sender_osl_user_id.to_owned(),
            sender_osl_user_id: sender_osl_user_id.to_owned(),
            plaintext: plaintext.to_owned(),
            decrypted_at: 1_800_000_000 + seq,
            burned: false,
        })
        .expect("seed chat history row");
}

fn choice_label(choice: OslChatBurnChoice) -> &'static str {
    match choice {
        OslChatBurnChoice::YourSide => "Your Side",
        OslChatBurnChoice::TheirSide => "Their Side",
        OslChatBurnChoice::BothSides => "Both Sides",
    }
}

fn choice_slug(choice: OslChatBurnChoice) -> &'static str {
    match choice {
        OslChatBurnChoice::YourSide => "your-side",
        OslChatBurnChoice::TheirSide => "their-side",
        OslChatBurnChoice::BothSides => "both-sides",
    }
}

fn expected_counts(choice: OslChatBurnChoice) -> (usize, usize, usize, usize) {
    match choice {
        OslChatBurnChoice::YourSide => (2, 0, 3, 2),
        OslChatBurnChoice::TheirSide => (0, 2, 3, 2),
        OslChatBurnChoice::BothSides => (2, 2, 1, 4),
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
fn task1352_osl_chat_burn_choices_remove_only_their_stated_records() {
    for choice in [
        OslChatBurnChoice::YourSide,
        OslChatBurnChoice::TheirSide,
        OslChatBurnChoice::BothSides,
    ] {
        let copy = choice_label(choice);
        let slug = choice_slug(choice);
        let alice = keystore::generate_identity(format!("task1352-{slug}-alice"));
        let bob = keystore::generate_identity(format!("task1352-{slug}-bob"));
        let other_osl_user_id = format!("OSLUSER-task1352-other-{}", Uuid::new_v4());
        let marked = format!("TASK1352 {copy} marked {}", Uuid::new_v4());
        let temp = TempDir::new().expect("temp history dir");
        let store = MessageStore::open(temp.path(), alice.x25519_secret.as_bytes())
            .expect("open sealed message store");
        let core = HubCoreState::default();
        *core.osl.identity.lock().expect("identity lock") = Some(alice.clone());
        *core.osl.message_store.lock().expect("store lock") = Some(store);
        let broker = osl_privacy_hub::broker::HubBrokerState::default();
        let activated = activate_owned_osl_chat_context(
            &broker,
            &alice.user_id,
            ManualPeerBinding {
                person_id: format!("task1352-{slug}-bob-person"),
                peer_osl_user_id: bob.user_id.clone(),
                peer_x25519_public: *bob.x25519_public.as_bytes(),
                peer_mlkem768_public: bob.mlkem_public_bytes,
            },
        )
        .expect("activate OSL chat context");
        let scope = Scope::try_from(activated.scope.clone()).expect("valid chat scope");
        let channel_id = scope.storage_key();
        let store_guard = core.osl.message_store.lock().expect("store lock");
        let store = store_guard.as_ref().expect("store installed");
        let your_marked = if matches!(
            choice,
            OslChatBurnChoice::YourSide | OslChatBurnChoice::BothSides
        ) {
            marked.as_str()
        } else {
            "your unmarked one"
        };
        let their_marked = if matches!(
            choice,
            OslChatBurnChoice::TheirSide | OslChatBurnChoice::BothSides
        ) {
            marked.as_str()
        } else {
            "their unmarked one"
        };
        seed_row(
            store,
            &channel_id,
            "task1352-self-1",
            &alice.user_id,
            your_marked,
            1,
        );
        seed_row(
            store,
            &channel_id,
            "task1352-self-2",
            &alice.user_id,
            "your second",
            2,
        );
        seed_row(
            store,
            &channel_id,
            "task1352-peer-1",
            &bob.user_id,
            their_marked,
            3,
        );
        seed_row(
            store,
            &channel_id,
            "task1352-peer-2",
            &bob.user_id,
            "their second",
            4,
        );
        seed_row(
            store,
            &channel_id,
            "task1352-other-1",
            &other_osl_user_id,
            "other unmarked one",
            5,
        );

        let before = store.list_by_channel(&channel_id, 10).expect("read before");
        assert_eq!(before.len(), 5);
        assert!(
            before.iter().any(|row| row.plaintext == marked),
            "{copy} must first read its random marked message"
        );
        println!("TASK1352 before copy={copy} count=5 marked=\"{marked}\" present=true");
        let hidden = filter_osl_chat_history_visibility(
            before
                .clone()
                .into_iter()
                .map(ipc::commands::StoredMessageDto::from)
                .collect(),
            &alice.user_id,
            true,
        );
        let store_count_after_hide = store
            .count_live_by_channel(&channel_id, None)
            .expect("count after hiding");
        println!(
            "TASK1352 hide_others copy={copy} visible_count={} store_count_after_hide={store_count_after_hide}",
            hidden.len()
        );
        assert_eq!(hidden.len(), 2);
        assert_eq!(store_count_after_hide, 5);
        drop(store_guard);

        let result = burn_osl_chat_history(&core, &broker, choice, false).expect("burn choice");

        let store_guard = core.osl.message_store.lock().expect("store lock");
        let store = store_guard.as_ref().expect("store installed");
        let after = store.list_by_channel(&channel_id, 10).expect("read after");
        let after_count = after.len();
        let (your_destroyed, their_destroyed, expected_after, expected_rows) =
            expected_counts(choice);
        println!(
            "TASK1352 after copy={copy} count={after_count} rows_destroyed={} your_rows_destroyed={} their_rows_destroyed={} others_rows_destroyed={} local_cleanup_complete={}",
            result.rows_destroyed,
            result.your_rows_destroyed,
            result.their_rows_destroyed,
            result.others_rows_destroyed,
            result.local_cleanup_complete
        );
        assert_eq!(result.messages_before, 5);
        assert_eq!(result.messages_after, expected_after);
        assert_eq!(after_count, expected_after);
        assert_eq!(result.rows_destroyed, expected_rows);
        assert_eq!(result.your_rows_destroyed, your_destroyed);
        assert_eq!(result.their_rows_destroyed, their_destroyed);
        assert_eq!(result.others_rows_destroyed, 0);
        assert!(result.local_cleanup_complete);
        assert!(!result.recipient_copies_deleted);
        assert!(
            after.iter().all(|row| row.plaintext != marked),
            "{copy} burn must remove the marked target row"
        );
        assert!(
            after
                .iter()
                .any(|row| row.sender_osl_user_id == other_osl_user_id),
            "{copy} burn must not remove other sender rows"
        );
    }
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
