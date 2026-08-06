use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::commands::{
    cmd_osl_decrypt_message_v2, cmd_osl_decrypt_message_with_id, cmd_osl_load_channel_history,
    encrypt_osl_phase4_to_pubkeys,
};
use ipc::peer_map::legacy_entry;
use ipc::state::AppState;
use ipc::wire_v2::{self, RecipientV3};
use keystore::{generate_identity, Identity};
use store::MessageStore;

const OLD_DID: &str = "900000000000003614";
const NEW_DID: &str = "900000000000003615";
const CHANNEL_ID: &str = "task-3614-private-dm";
const OLD_TO_NEW_ID: &str = "task-3614-old-to-new";
const NEW_TO_OLD_ID: &str = "task-3614-new-to-old";
const OLD_TO_NEW_TEXT: &str = "task 3614 old copy private message";
const NEW_TO_OLD_TEXT: &str = "task 3614 new copy private message";
const LEGACY_VERSION_REFUSAL: &str =
    "OSL: unsupported wire version 0x03 (this client only decodes 0x01)";

#[derive(Clone)]
struct CarrierMessage {
    id: &'static str,
    content: String,
}

fn state_for(identity: &Identity, store_key: [u8; 32]) -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp message store dir");
    let state = AppState::new();
    state.install_identity(identity.clone());
    let store = MessageStore::open(dir.path(), &store_key).expect("message store opens");
    *state.message_store.lock().expect("message store mutex") = Some(store);
    (state, dir)
}

fn pin_peer(state: &AppState, discord_id: &str, identity: &Identity) {
    let mut entry = legacy_entry(identity.user_id.clone());
    entry.discord_id = Some(discord_id.to_owned());
    entry.pubkey = Some(STANDARD.encode(identity.x25519_public.as_bytes()));
    entry.ik_mlkem768_pub = Some(STANDARD.encode(identity.mlkem_public_bytes));

    state
        .peer_map
        .lock()
        .expect("peer map mutex")
        .insert(discord_id.to_owned(), entry);
    state
        .sender_pubkey_cache
        .insert(identity.user_id.clone(), identity.x25519_public);
}

fn recipient_v3(identity: &Identity) -> RecipientV3 {
    RecipientV3 {
        x25519_pub: identity.x25519_public,
        mlkem_pub: identity.mlkem_encapsulation_key(),
    }
}

fn matching_marked_covers(messages: &[CarrierMessage], id: &str) -> usize {
    messages
        .iter()
        .filter(|message| message.id == id && message.content.starts_with("DPC0::"))
        .count()
}

fn matching_history_rows(state: &AppState, plaintext: &str) -> usize {
    cmd_osl_load_channel_history(state, CHANNEL_ID.to_owned(), Some(20))
        .expect("history loads")
        .into_iter()
        .filter(|message| message.plaintext == plaintext)
        .count()
}

fn assert_exact_or_version_refusal(
    side: &str,
    result: Result<String, String>,
    expected_plaintext: &str,
) -> String {
    match result {
        Ok(plaintext) => {
            assert_eq!(
                plaintext, expected_plaintext,
                "{side} must read the exact private message"
            );
            format!("read_exact:{plaintext}")
        }
        Err(refusal) => {
            assert_eq!(
                refusal, LEGACY_VERSION_REFUSAL,
                "{side} may only pass with the exact version refusal"
            );
            format!("version_refusal:{refusal}")
        }
    }
}

#[test]
fn task_3614_old_and_new_copies_exchange_marked_covers() {
    let old_identity = generate_identity("task-3614-old-osl".to_owned());
    let new_identity = generate_identity("task-3614-new-osl".to_owned());

    let (old_state, _old_dir) = state_for(&old_identity, [0x36; 32]);
    let (new_state, _new_dir) = state_for(&new_identity, [0x14; 32]);
    pin_peer(&old_state, NEW_DID, &new_identity);
    pin_peer(&new_state, OLD_DID, &old_identity);

    let mut old_inbox = Vec::<CarrierMessage>::new();
    let mut new_inbox = Vec::<CarrierMessage>::new();

    let old_before = matching_marked_covers(&old_inbox, NEW_TO_OLD_ID);
    let new_before = matching_marked_covers(&new_inbox, OLD_TO_NEW_ID);
    println!("TASK3614 old_side_matching_messages_before={old_before}");
    println!("TASK3614 new_side_matching_messages_before={new_before}");
    assert_eq!(old_before, 0);
    assert_eq!(new_before, 0);

    let old_cover = encrypt_osl_phase4_to_pubkeys(
        &old_identity.x25519_secret,
        &[new_identity.x25519_public],
        OLD_TO_NEW_TEXT,
    )
    .expect("old copy emits a marked legacy cover");
    let new_cover = wire_v2::encrypt_v3(
        &new_identity.x25519_secret,
        &new_identity.x25519_public,
        &[recipient_v3(&old_identity)],
        wire_v2::MSG_TYPE_CONTENT,
        NEW_TO_OLD_TEXT.as_bytes(),
    )
    .expect("new copy emits a marked v3 cover");
    assert!(old_cover.starts_with("DPC0::"));
    assert!(new_cover.starts_with("DPC0::"));

    new_inbox.push(CarrierMessage {
        id: OLD_TO_NEW_ID,
        content: old_cover.clone(),
    });
    old_inbox.push(CarrierMessage {
        id: NEW_TO_OLD_ID,
        content: new_cover.clone(),
    });

    let old_after = matching_marked_covers(&old_inbox, NEW_TO_OLD_ID);
    let new_after = matching_marked_covers(&new_inbox, OLD_TO_NEW_ID);
    println!("TASK3614 old_side_matching_messages_after={old_after}");
    println!("TASK3614 new_side_matching_messages_after={new_after}");
    assert_eq!(old_after, 1);
    assert_eq!(new_after, 1);

    let new_read = assert_exact_or_version_refusal(
        "new side receiving old cover",
        cmd_osl_decrypt_message_v2(
            &new_state,
            Some(OLD_TO_NEW_ID.to_owned()),
            CHANNEL_ID.to_owned(),
            OLD_DID.to_owned(),
            old_cover,
            None,
            None,
        ),
        OLD_TO_NEW_TEXT,
    );
    println!("TASK3614 new_side_received_message={new_read}");
    assert_eq!(matching_history_rows(&new_state, OLD_TO_NEW_TEXT), 1);
    println!("TASK3614 new_side_matching_private_messages_after=1");

    let old_read = assert_exact_or_version_refusal(
        "old side receiving new cover",
        cmd_osl_decrypt_message_with_id(
            &old_state,
            Some(NEW_TO_OLD_ID.to_owned()),
            CHANNEL_ID.to_owned(),
            NEW_DID.to_owned(),
            new_cover,
        ),
        NEW_TO_OLD_TEXT,
    );
    println!("TASK3614 old_side_received_message={old_read}");

    let bad_refusal = "OSL: not a recipient of this message";
    assert_ne!(bad_refusal, LEGACY_VERSION_REFUSAL);
    assert!(
        !matches!(Err::<String, String>(bad_refusal.to_owned()), Err(ref e) if e == LEGACY_VERSION_REFUSAL),
        "no non-version refusal wording may pass the 3614 gate"
    );
    println!("TASK3614 non_version_refusal_wording_passes=false");
}
