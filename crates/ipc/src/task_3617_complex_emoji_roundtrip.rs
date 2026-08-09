use crate::commands::{cmd_osl_decrypt_message_v2, cmd_osl_encrypt_message_v2_wire};
use crate::peer_map::{legacy_entry, WhitelistEntry};
use crate::scope::{Scope, ScopeInput};
use crate::state::AppState;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use keystore::{generate_identity, Identity};
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const SENDER_DID: &str = "900000000000003617";
const RECEIVER_DID: &str = "900000000000003618";
const STORE_KEY: [u8; 32] = [0x61; 32];

/// These are intentionally complete grapheme sequences, not substitutes that
/// happen to look similar on one platform.  The read-back assertions compare
/// Rust strings exactly, so dropping a ZWJ, regional indicator, or modifier
/// makes the check fail.
const EMOJI_SEQUENCES: [(&str, &str); 4] = [
    ("single", "🙂"),
    ("family", "👨‍👩‍👧‍👦"),
    ("flags", "🇺🇸🇯🇵"),
    ("skin-tone", "👍🏽"),
];

struct MessagePath {
    label: &'static str,
    scope: Scope,
}

struct CopyStores {
    root: TempDir,
    sender_path: std::path::PathBuf,
    receiver_path: std::path::PathBuf,
}

impl CopyStores {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("two-copy emoji fixture root");
        Self {
            sender_path: root.path().join("sender").join("messages"),
            receiver_path: root.path().join("receiver").join("messages"),
            root,
        }
    }

    fn sender_open(&self) -> MessageStore {
        MessageStore::open(&self.sender_path, &STORE_KEY).expect("open sender encrypted store")
    }

    fn receiver_open(&self) -> MessageStore {
        MessageStore::open(&self.receiver_path, &STORE_KEY).expect("open receiver encrypted store")
    }
}

#[test]
fn task_3617_complex_emoji_round_trip_through_every_scope_and_both_restarts() {
    let sender_identity = generate_identity("task-3617-sender".to_owned());
    let receiver_identity = generate_identity("task-3617-receiver".to_owned());
    let sender_state = sender_state(&sender_identity, &receiver_identity);
    let receiver_state = receiver_state(&receiver_identity, &sender_identity);
    let stores = CopyStores::new();
    let paths = supported_message_paths();
    assert_eq!(paths.len(), 4, "all supported scope paths must be listed");
    assert_eq!(
        EMOJI_SEQUENCES.len(),
        4,
        "four complex emoji cases are required"
    );

    let sender_store = stores.sender_open();
    *receiver_state
        .message_store
        .lock()
        .expect("receiver message-store mutex") = Some(stores.receiver_open());

    for path in &paths {
        for (emoji_kind, expected) in EMOJI_SEQUENCES {
            let message_id = format!("task-3617-{}-{emoji_kind}", path.label);
            let encrypted = cmd_osl_encrypt_message_v2_wire(
                &sender_state,
                expected.to_owned(),
                ScopeInput::from(&path.scope),
                vec![RECEIVER_DID.to_owned()],
                SENDER_DID.to_owned(),
            )
            .unwrap_or_else(|error| panic!("{} {emoji_kind} send: {error}", path.label));
            assert!(
                encrypted.content.starts_with("DPC0::"),
                "{} {emoji_kind}",
                path.label
            );

            // Sender-key group and server paths emit an SKDM before their v5
            // content. Feed every emitted control wire through the production
            // receiver dispatcher before handing it the content wire.
            for control in encrypted.control_messages {
                cmd_osl_decrypt_message_v2(
                    &receiver_state,
                    None,
                    path.scope.storage_key(),
                    SENDER_DID.to_owned(),
                    control,
                    Some(ScopeInput::from(&path.scope)),
                    None,
                )
                .unwrap_or_else(|error| {
                    panic!("{} {emoji_kind} sender-key control: {error}", path.label)
                });
            }
            let received = cmd_osl_decrypt_message_v2(
                &receiver_state,
                Some(message_id.clone()),
                path.scope.storage_key(),
                SENDER_DID.to_owned(),
                encrypted.content,
                Some(ScopeInput::from(&path.scope)),
                None,
            )
            .unwrap_or_else(|error| panic!("{} {emoji_kind} receive: {error}", path.label));
            assert_eq!(
                received, expected,
                "{} {emoji_kind} changed before storage",
                path.label
            );

            put(
                &sender_store,
                &message_id,
                &path.scope,
                &sender_identity,
                expected,
            );
            // The production receive dispatcher above, not this test, writes
            // the receiver's durable history row.
        }
        let sender_before = exact_path_count(&sender_store, &path.scope);
        let receiver_before = exact_path_count(
            receiver_state
                .message_store
                .lock()
                .expect("receiver message-store mutex")
                .as_ref()
                .expect("receiver store installed"),
            &path.scope,
        );
        assert_eq!(sender_before, 4, "{} sender before restart", path.label);
        assert_eq!(receiver_before, 4, "{} receiver before restart", path.label);
        println!(
            "TASK3617_BEFORE_RESTART path={} sender_marked_messages={sender_before} receiver_marked_messages={receiver_before}",
            path.label
        );
    }

    // Drop both live handles: reopening them below is the test's two-process
    // restart boundary, not a second read from an already-open SQLite handle.
    drop(sender_store);
    drop(sender_state);
    drop(receiver_state);

    let restarted_sender = stores.sender_open();
    let restarted_receiver = stores.receiver_open();
    for path in &paths {
        let sender_after = exact_path_count(&restarted_sender, &path.scope);
        let received_after = read_exact_path(&restarted_receiver, &path.scope);
        assert_eq!(sender_after, 4, "{} sender after restart", path.label);
        assert_eq!(
            received_after, EMOJI_SEQUENCES,
            "{} receiver after restart",
            path.label
        );
        println!(
            "TASK3617_AFTER_RESTART path={} sender_marked_messages={sender_after} receiver_marked_messages={} receiver_sequences={received_after:?}",
            path.label,
            received_after.len(),
        );
    }

    // Keep the TempDir owned until every reopened store read completes.
    assert!(stores.root.path().exists());
    println!(
        "TASK3617_SUMMARY paths={} messages_per_path={} total_marked_messages_before_restart={} receiver_exact_sequences_after_restart={}",
        paths.len(),
        EMOJI_SEQUENCES.len(),
        paths.len() * EMOJI_SEQUENCES.len(),
        paths.len() * EMOJI_SEQUENCES.len(),
    );
}

fn supported_message_paths() -> Vec<MessagePath> {
    vec![
        MessagePath {
            label: "direct-message",
            scope: Scope::dm(RECEIVER_DID),
        },
        MessagePath {
            label: "group-chat",
            scope: Scope::gc("task-3617-group"),
        },
        MessagePath {
            label: "server-channel",
            scope: Scope::server_channel("task-3617-server", "task-3617-channel"),
        },
        MessagePath {
            label: "server-full",
            scope: Scope::server_full("task-3617-server"),
        },
    ]
}

fn sender_state(sender: &Identity, receiver: &Identity) -> AppState {
    let state = AppState::new();
    state.install_identity(sender.clone());
    state.set_rn_wire_in_enabled(false);

    let mut peer = legacy_entry(receiver.user_id.clone());
    peer.discord_id = Some(RECEIVER_DID.to_owned());
    peer.pubkey = Some(STANDARD.encode(receiver.x25519_public.as_bytes()));
    peer.ik_mlkem768_pub = Some(STANDARD.encode(receiver.mlkem_public_bytes));
    peer.outgoing_whitelists = vec![
        WhitelistEntry::Dm {
            broadened: false,
            enabled_at: None,
        },
        WhitelistEntry::Gc {
            id: "task-3617-group".to_owned(),
            user_specific: false,
        },
        WhitelistEntry::ServerChannel {
            server_id: "task-3617-server".to_owned(),
            channel_id: "task-3617-channel".to_owned(),
            user_specific: false,
        },
        WhitelistEntry::ServerFull {
            server_id: "task-3617-server".to_owned(),
            user_specific: false,
        },
    ];
    state
        .peer_map
        .lock()
        .expect("peer map mutex")
        .insert(RECEIVER_DID.to_owned(), peer);
    state
}

fn receiver_state(receiver: &Identity, sender: &Identity) -> AppState {
    let state = AppState::new();
    state.install_identity(receiver.clone());
    state.set_rn_wire_in_enabled(false);

    let mut peer = legacy_entry(sender.user_id.clone());
    peer.discord_id = Some(SENDER_DID.to_owned());
    peer.pubkey = Some(STANDARD.encode(sender.x25519_public.as_bytes()));
    peer.ik_mlkem768_pub = Some(STANDARD.encode(sender.mlkem_public_bytes));
    state
        .peer_map
        .lock()
        .expect("peer map mutex")
        .insert(SENDER_DID.to_owned(), peer);
    state
}

fn put(store: &MessageStore, message_id: &str, scope: &Scope, sender: &Identity, plaintext: &str) {
    store
        .put(&StoredMessage {
            discord_message_id: message_id.to_owned(),
            channel_id: scope.storage_key(),
            sender_discord_id: SENDER_DID.to_owned(),
            sender_osl_user_id: sender.user_id.clone(),
            plaintext: plaintext.to_owned(),
            decrypted_at: 1_800_003_617,
            burned: false,
            reply_parent_id: None,
            edit_revision: 1,
        })
        .expect("persist marked emoji message");
}

fn exact_path_count(store: &MessageStore, scope: &Scope) -> usize {
    store
        .list_by_channel(&scope.storage_key(), 16)
        .expect("list path messages")
        .into_iter()
        .filter(|row| {
            EMOJI_SEQUENCES
                .iter()
                .any(|(_, emoji)| row.plaintext == *emoji)
        })
        .count()
}

fn read_exact_path(store: &MessageStore, scope: &Scope) -> [(&'static str, &'static str); 4] {
    let rows = store
        .list_by_channel(&scope.storage_key(), 16)
        .expect("read receiver path after restart");
    assert_eq!(rows.len(), 4, "{} receiver row count", scope.storage_key());

    let mut received = [("", ""); 4];
    for (index, (kind, expected)) in EMOJI_SEQUENCES.into_iter().enumerate() {
        let message_id = format!("task-3617-{}-{kind}", path_label(scope));
        let row = rows
            .iter()
            .find(|row| row.discord_message_id == message_id)
            .unwrap_or_else(|| panic!("{} {kind} missing after restart", scope.storage_key()));
        assert_eq!(
            row.plaintext,
            expected,
            "{} {kind} changed after restart",
            scope.storage_key()
        );
        received[index] = (kind, expected);
    }
    received
}

fn path_label(scope: &Scope) -> &'static str {
    match scope.storage_key().as_str() {
        key if key.starts_with("dm:") => "direct-message",
        key if key.starts_with("gc:") => "group-chat",
        key if key.starts_with("server_channel:") => "server-channel",
        key if key.starts_with("server_full:") => "server-full",
        _ => unreachable!("test uses only supported message paths"),
    }
}
