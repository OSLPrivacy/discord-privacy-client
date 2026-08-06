#![cfg(feature = "core")]

use osl_privacy_hub::broker::{self, HubBrokerState};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::ManualPeerBinding;

const DIRECT_COMMAND: &str = "add_osl_chat_reaction";
const MESSAGE_ID: &str = "peer-00001111222233334444555566667777";
const THUMBS_UP: &str = "\u{1f44d}";
const HEART: &str = "\u{2764}\u{fe0f}";
const TEST_MAIN_PASSWORD: &str = "aB3!z9";

struct IsolatedAccount {
    _dir: tempfile::TempDir,
}

impl IsolatedAccount {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temporary account directory");
        keystore::set_base_dir_override(Some(dir.path().to_owned()));
        keystore::set_active_account_dir(Some(dir.path().to_owned()));
        ipc::main_password::set_file_storage_key(None);
        ipc::main_password::set_main_password(dir.path(), TEST_MAIN_PASSWORD)
            .expect("set isolated main password");
        Self { _dir: dir }
    }
}

impl Drop for IsolatedAccount {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn binding_for(identity: &keystore::Identity, person_id: &str) -> ManualPeerBinding {
    ManualPeerBinding {
        person_id: person_id.to_owned(),
        peer_osl_user_id: identity.user_id.clone(),
        peer_x25519_public: *identity.x25519_public.as_bytes(),
        peer_mlkem768_public: identity.mlkem_public_bytes,
    }
}

fn activate_osl_chat_as(
    core: &HubCoreState,
    broker: &HubBrokerState,
    owner: &keystore::Identity,
    peer: &keystore::Identity,
    peer_person_id: &str,
) {
    *core.osl.identity.lock().expect("identity state") = Some(owner.clone());
    broker::activate_owned_osl_chat_context(
        broker,
        &owner.user_id,
        binding_for(peer, peer_person_id),
    )
    .expect("activate OSL Chat context");
}

fn assert_shipping_command_registered_and_granted() {
    let main = include_str!("../src/main.rs");
    let handlers = include_str!("../src/hub_command_surface.rs");
    let permissions = include_str!("../permissions/hub.toml");
    let capability = include_str!("../capabilities/hub.json");
    let permission = "allow-add-osl-chat-reaction";

    assert!(main.contains(&format!("fn {DIRECT_COMMAND}(")));
    assert!(handlers.contains(DIRECT_COMMAND));
    assert!(permissions.contains(&format!("commands.allow = [\"{DIRECT_COMMAND}\"]")));
    assert!(capability.contains(&format!("\"{permission}\"")));
}

#[test]
fn direct_command_adds_reaction_and_repeat_does_not_duplicate() {
    assert_shipping_command_registered_and_granted();
    let _account = IsolatedAccount::new();
    let broker = HubBrokerState::default();
    let core = HubCoreState::default();
    let alice = keystore::generate_identity("task-1364-alice".to_owned());
    let bob = keystore::generate_identity("task-1364-bob".to_owned());

    activate_osl_chat_as(&core, &broker, &alice, &bob, "task-1364-person-bob");
    let first =
        broker::add_osl_chat_reaction(&core, &broker, MESSAGE_ID.to_owned(), THUMBS_UP.to_owned())
            .expect("first command adds a reaction");
    let repeat =
        broker::add_osl_chat_reaction(&core, &broker, MESSAGE_ID.to_owned(), THUMBS_UP.to_owned())
            .expect("repeat command is idempotent");
    let alice_second_emoji =
        broker::add_osl_chat_reaction(&core, &broker, MESSAGE_ID.to_owned(), HEART.to_owned())
            .expect("same identity may add a different emoji");

    activate_osl_chat_as(&core, &broker, &bob, &alice, "task-1364-person-alice");
    let bob_same_emoji =
        broker::add_osl_chat_reaction(&core, &broker, MESSAGE_ID.to_owned(), THUMBS_UP.to_owned())
            .expect("different identity may add the same emoji");

    println!("TASK1364_DIRECT_COMMAND={DIRECT_COMMAND}");
    println!("TASK1364_MESSAGE_ID={}", first.message_id);
    println!("TASK1364_EMOJI={}", first.emoji);
    println!("TASK1364_FIRST_IDENTITY={}", first.identity_osl_user_id);
    println!("TASK1364_FIRST_ADDED={}", first.added);
    println!("TASK1364_FIRST_REACTION_COUNT={}", first.reaction_count);
    println!("TASK1364_REPEAT_ADDED={}", repeat.added);
    println!("TASK1364_REPEAT_REACTION_COUNT={}", repeat.reaction_count);
    println!(
        "TASK1364_ALICE_SECOND_EMOJI_ADDED={} count={}",
        alice_second_emoji.added, alice_second_emoji.reaction_count
    );
    println!(
        "TASK1364_BOB_SAME_EMOJI_ADDED={} count={}",
        bob_same_emoji.added, bob_same_emoji.reaction_count
    );

    assert_eq!(first.message_id, MESSAGE_ID);
    assert_eq!(first.emoji, THUMBS_UP);
    assert_eq!(first.identity_osl_user_id, alice.user_id);
    assert!(first.added);
    assert_eq!(first.reaction_count, 1);
    assert!(!repeat.added);
    assert_eq!(repeat.reaction_count, 1);
    assert!(alice_second_emoji.added);
    assert_eq!(alice_second_emoji.reaction_count, 2);
    assert!(bob_same_emoji.added);
    assert_eq!(bob_same_emoji.reaction_count, 3);
}
