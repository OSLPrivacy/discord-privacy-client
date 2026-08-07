use ipc::commands::cmd_osl_persist_outbound;
use ipc::state::AppState;
use ipc::wire_rn::{
    accept_and_persist_with_sealer, initiate_and_persist_with_sealer, receive_rn_with_sealer,
    select_wire_version, send_rn_for_state, RnError, RnPolicy, RnSessionStore, SelectedVersion,
    RN_CONTEXT_DISCORD_MANUAL, RN_SESSION_DIR,
};
use keystore::client::{PeerCapabilities, RN_CAP_WIRE_RN};
use keystore::sealer::MemorySealer;
use osl_ratchet_next::test_support::{fresh_bundle, seeded_rng};
use osl_ratchet_next::{SessionParams, XSecret};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use store::MessageStore;

const AGREEMENT_NAME: &str = "ONYX-0435";
const MESSAGE_ID: &str = "MSG-0435";
const WEAK_MESSAGE_ID: &str = "MSG-0435-WEAK";
const CHANNEL_ID: &str = "task-0435-dm";
const STORE_KEY: &[u8; 32] = &[0x43; 32];

#[test]
fn task_0435_restart_keeps_no_downgrade_state() {
    let root = tempfile::tempdir().expect("tempdir");
    let alice_profile = root.path().join("alice-profile");
    let bob_profile = root.path().join("bob-profile");
    fs::create_dir_all(&alice_profile).expect("alice profile");
    fs::create_dir_all(&bob_profile).expect("bob profile");

    let sealer = MemorySealer::new();
    let mut rng = seeded_rng(435);
    let (bob_prekeys, bob_bundle) = fresh_bundle(&mut rng);
    let bob_peer = *bob_bundle.identity.as_bytes();
    let bob_ek = bob_bundle.pq_prekey.to_bytes();

    let alice_identity = keystore::generate_identity(AGREEMENT_NAME.to_string());
    let alice_secret = XSecret::from_bytes(*alice_identity.x25519_secret.as_bytes());
    let alice_peer = *alice_identity.x25519_public.as_bytes();

    let alice_initial_store = RnSessionStore::for_config_dir(&alice_profile).expect("alice rn");
    let bob_initial_store = RnSessionStore::for_config_dir(&bob_profile).expect("bob rn");
    initiate_and_persist_with_sealer(
        &alice_initial_store,
        &sealer,
        &alice_secret,
        &alice_peer,
        &bob_bundle,
        PeerCapabilities::Verified(RN_CAP_WIRE_RN),
        &bob_ek,
        RN_CONTEXT_DISCORD_MANUAL,
        SessionParams::default(),
    )
    .expect("seed agreed RN session");
    let bootstrap = send_rn_for_state(
        &wire_enabled_state(alice_identity.clone(), None),
        &alice_initial_store,
        &sealer,
        &bob_peer,
        ipc::wire_v2::MSG_TYPE_CONTENT,
        b"TASK0435 bootstrap agreement",
    )
    .expect("bootstrap send");
    accept_and_persist_with_sealer(
        &bob_initial_store,
        &sealer,
        &bob_prekeys,
        bob_prekeys.identity.public().as_bytes(),
        &bob_ek,
        &bootstrap,
        RN_CONTEXT_DISCORD_MANUAL,
        SessionParams::default(),
    )
    .expect("bob accepts agreement");

    let agreement_readable = alice_initial_store
        .load_pin(&bob_peer)
        .expect("alice pin")
        .is_pinned_to_rn()
        && bob_initial_store
            .load_pin(&alice_peer)
            .expect("bob pin")
            .is_pinned_to_rn();
    assert!(agreement_readable, "{AGREEMENT_NAME} must be readable");
    println!("TASK0435_READABLE_AGREEMENT name={AGREEMENT_NAME} readable={agreement_readable}");

    drop(alice_initial_store);
    drop(bob_initial_store);

    let alice_store = RnSessionStore::for_config_dir(&alice_profile).expect("alice restart rn");
    let bob_store = RnSessionStore::for_config_dir(&bob_profile).expect("bob restart rn");
    let alice_messages =
        MessageStore::open(&alice_profile.join("store"), STORE_KEY).expect("alice message store");
    let before_count = sent_count(&alice_messages);
    assert_eq!(before_count, 0);
    println!("TASK0435_SENT_COUNT_BEFORE={before_count}");

    let alice_state = wire_enabled_state(alice_identity, Some(alice_messages));
    let strong_selected = select_wire_version(
        &alice_store
            .load_pin(&bob_peer)
            .expect("alice restarted pin"),
        PeerCapabilities::Verified(RN_CAP_WIRE_RN),
        RnPolicy::Opportunistic,
    )
    .expect("strong selection");
    assert_eq!(strong_selected, SelectedVersion::Rn);

    let strong_wire = send_rn_for_state(
        &alice_state,
        &alice_store,
        &sealer,
        &bob_peer,
        ipc::wire_v2::MSG_TYPE_CONTENT,
        b"TASK0435 strong send",
    )
    .expect("strong RN send");
    let opened = receive_rn_with_sealer(&bob_store, &sealer, &alice_peer, &strong_wire)
        .expect("bob receives strong send");
    assert_eq!(opened.plaintext, b"TASK0435 strong send");
    cmd_osl_persist_outbound(
        &alice_state,
        CHANNEL_ID.to_string(),
        MESSAGE_ID.to_string(),
        String::from_utf8(opened.plaintext).expect("plaintext utf8"),
        None,
    )
    .expect("persist sent outbound");

    let after_strong_count = sent_count_from_state(&alice_state);
    assert_eq!(after_strong_count, 1);
    let agreement_fingerprint_after_strong = profile_fingerprint(&alice_profile, &bob_profile);
    let msg_fingerprint_after_strong = message_fingerprint_from_state(&alice_state);
    println!(
        "TASK0435_STRONG_SEND message_id={MESSAGE_ID} strength=Verified({RN_CAP_WIRE_RN}) selected={strong_selected:?} sent_count={after_strong_count}"
    );
    println!(
        "TASK0435_FINGERPRINT_AFTER_STRONG agreement={AGREEMENT_NAME}:{} message={MESSAGE_ID}:{}",
        agreement_fingerprint_after_strong, msg_fingerprint_after_strong
    );

    let weaker_result = select_wire_version(
        &alice_store
            .load_pin(&bob_peer)
            .expect("pin still readable before downgrade retry"),
        PeerCapabilities::Absent,
        RnPolicy::Opportunistic,
    );
    assert!(
        matches!(weaker_result, Err(RnError::PinnedToRn)),
        "weaker capability must be refused as a downgrade: {weaker_result:?}"
    );
    if weaker_result.is_ok() {
        let _ = send_rn_for_state(
            &alice_state,
            &alice_store,
            &sealer,
            &bob_peer,
            ipc::wire_v2::MSG_TYPE_CONTENT,
            b"TASK0435 weaker send must not happen",
        );
        let _ = cmd_osl_persist_outbound(
            &alice_state,
            CHANNEL_ID.to_string(),
            WEAK_MESSAGE_ID.to_string(),
            "TASK0435 weaker send must not persist".to_string(),
            None,
        );
    }

    let after_weaker_count = sent_count_from_state(&alice_state);
    let agreement_fingerprint_after_weaker = profile_fingerprint(&alice_profile, &bob_profile);
    let msg_fingerprint_after_weaker = message_fingerprint_from_state(&alice_state);
    assert_eq!(after_weaker_count, 1);
    assert_eq!(
        agreement_fingerprint_after_weaker,
        agreement_fingerprint_after_strong
    );
    assert_eq!(msg_fingerprint_after_weaker, msg_fingerprint_after_strong);
    println!(
        "TASK0435_WEAKER_SEND message_id={WEAK_MESSAGE_ID} strength=Absent refused=\"{}\" sent_count={after_weaker_count}",
        weaker_result.expect_err("weaker send refused")
    );
    println!(
        "TASK0435_FINGERPRINT_AFTER_WEAKER agreement={AGREEMENT_NAME}:{} message={MESSAGE_ID}:{}",
        agreement_fingerprint_after_weaker, msg_fingerprint_after_weaker
    );
}

fn wire_enabled_state(
    identity: keystore::Identity,
    message_store: Option<MessageStore>,
) -> AppState {
    let state = AppState::new();
    state.install_identity(identity);
    state.set_rn_wire_in_enabled(true);
    *state.message_store.lock().expect("message store lock") = message_store;
    state
}

fn sent_count(store: &MessageStore) -> usize {
    store
        .count_message_records(&[MESSAGE_ID.to_string(), WEAK_MESSAGE_ID.to_string()])
        .expect("count sent messages")
}

fn sent_count_from_state(state: &AppState) -> usize {
    let guard = state.message_store.lock().expect("message store lock");
    sent_count(guard.as_ref().expect("message store installed"))
}

fn message_fingerprint_from_state(state: &AppState) -> String {
    let guard = state.message_store.lock().expect("message store lock");
    let msg = guard
        .as_ref()
        .expect("message store installed")
        .get(MESSAGE_ID)
        .expect("read message")
        .expect("message present");
    let canonical = serde_json::json!({
        "message_id": msg.discord_message_id,
        "channel_id": msg.channel_id,
        "sender_id": msg.sender_discord_id,
        "sender_osl_user_id": msg.sender_osl_user_id,
        "plaintext": msg.plaintext,
        "burned": msg.burned,
    });
    hex(&Sha256::digest(canonical.to_string().as_bytes()))
}

fn profile_fingerprint(alice_profile: &Path, bob_profile: &Path) -> String {
    let mut hasher = Sha256::new();
    fingerprint_dir(&mut hasher, &alice_profile.join(RN_SESSION_DIR), "alice");
    fingerprint_dir(&mut hasher, &bob_profile.join(RN_SESSION_DIR), "bob");
    hex(&hasher.finalize())
}

fn fingerprint_dir(hasher: &mut Sha256, dir: &Path, label: &str) {
    let mut entries = fs::read_dir(dir)
        .expect("read RN dir")
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) != Some("lock"))
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        let name = path.file_name().expect("file name").to_string_lossy();
        hasher.update(label.as_bytes());
        hasher.update([0]);
        hasher.update(name.as_bytes());
        hasher.update([0]);
        hasher.update(fs::read(path).expect("read RN file"));
        hasher.update([0]);
    }
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}
