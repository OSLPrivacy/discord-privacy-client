use base64::Engine as _;
use ipc::commands::{
    cmd_osl_copy_my_history_here, cmd_osl_copy_my_history_here_confirmation,
    cmd_osl_export_history_for_copy, cmd_osl_history_copy_month_data_bytes,
    cmd_osl_read_history_copy_semantics, HistoryCopyExpirationState, HistoryCopyReaction,
    HistoryCopySemantics, COPY_MY_HISTORY_HERE_ACTION_LABEL,
    COPY_MY_HISTORY_HERE_CONFIRMATION_SENTENCE,
};
use ipc::state::AppState;
use keystore::identity_from_entropy;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;
use std::process::{Command, Output};
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const ACCOUNT: &str = "task4816-account";
const CHANNEL: &str = "task4816-channel";
const ITEM_COUNT: usize = 500;
const ATTACHMENT_COUNT: usize = 60;
const REACTION_COUNT: usize = 50;
const EDIT_COUNT: usize = 25;
const REPLY_COUNT: usize = 25;
const THREAD_COUNT: usize = 25;
const EXPIRING_COUNT: usize = 50;
const EXPIRED_SOURCE_ID: &str = "task4816-expired-before-copy";
const MANIFEST_AAD: &[u8] = b"OSL task 4816 independent source manifest v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    items: Vec<ManifestItem>,
    graph: Vec<GraphEdge>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ManifestItem {
    source_id: String,
    channel_id: String,
    author_discord_id: String,
    author_osl_user_id: String,
    body: String,
    body_hash: String,
    timestamp: i64,
    stable_order: u64,
    edit_revision: i64,
    reply_parent_id: Option<String>,
    thread_root_id: Option<String>,
    expiration: Expiration,
    reactions: Vec<Reaction>,
    attachments: Vec<Attachment>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Attachment {
    name: String,
    mime: String,
    bytes: Vec<u8>,
    byte_hash: String,
    sender_discord_id: Option<String>,
    scope_type: Option<String>,
    scope_id: Option<String>,
    created_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Reaction {
    reaction_id: String,
    target_message_id: String,
    actor_discord_id: String,
    emoji: String,
    reacted_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum Expiration {
    NonExpiring,
    Active { timer_minutes: u32 },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
struct GraphEdge {
    kind: String,
    from: String,
    to: String,
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn source_id(index: usize) -> String {
    format!("task4816-source-{index:03}")
}

fn build_source_manifest() -> Manifest {
    let mut items = Vec::with_capacity(ITEM_COUNT);
    for index in 0..ITEM_COUNT {
        let id = source_id(index);
        let body = format!(
            "TASK4816-PLAINTEXT-{index:03}-{}-nonempty-history-body",
            "q".repeat(17 + index % 19)
        );
        let reply_parent_id = (25..25 + REPLY_COUNT)
            .contains(&index)
            .then(|| source_id(index - 25));
        let thread_root_id = (75..75 + THREAD_COUNT)
            .contains(&index)
            .then(|| source_id(index - 75));
        let reactions = if (100..100 + REACTION_COUNT).contains(&index) {
            vec![Reaction {
                reaction_id: format!("task4816-reaction-{index:03}"),
                target_message_id: id.clone(),
                actor_discord_id: format!("reactor-{}", index % 9),
                emoji: ["🦉", "✅", "🧭", "🔐"][index % 4].to_string(),
                reacted_at: 1_900_000_000 + index as i64,
            }]
        } else {
            Vec::new()
        };
        let attachments = if index < ATTACHMENT_COUNT {
            let bytes: Vec<u8> = (0..(64 + index % 23))
                .map(|offset| ((index * 37 + offset * 29 + 1) % 251 + 1) as u8)
                .collect();
            vec![Attachment {
                name: format!("task4816-attachment-{index:03}.bin"),
                mime: "application/octet-stream".to_string(),
                byte_hash: hash(&bytes),
                bytes,
                sender_discord_id: Some(format!("author-{}", index % 7)),
                scope_type: Some("channel".to_string()),
                scope_id: Some(CHANNEL.to_string()),
                created_at: 1_800_000_000 + index as i64,
            }]
        } else {
            Vec::new()
        };
        items.push(ManifestItem {
            source_id: id,
            channel_id: CHANNEL.to_string(),
            author_discord_id: format!("author-{}", index % 7),
            author_osl_user_id: format!("osl-author-{}", index % 7),
            body_hash: hash(body.as_bytes()),
            body,
            // Pairs deliberately share a timestamp. stable_order, rather than
            // an unstable timestamp tie-break, is the independent oracle.
            timestamp: 1_700_000_000 + (index / 2) as i64,
            stable_order: index as u64,
            edit_revision: if (50..50 + EDIT_COUNT).contains(&index) {
                2 + (index % 3) as i64
            } else {
                1
            },
            reply_parent_id,
            thread_root_id,
            expiration: if (150..150 + EXPIRING_COUNT).contains(&index) {
                Expiration::Active {
                    timer_minutes: 30 + index as u32,
                }
            } else {
                Expiration::NonExpiring
            },
            reactions,
            attachments,
        });
    }
    let graph = derive_graph(&items);
    Manifest { items, graph }
}

fn derive_graph(items: &[ManifestItem]) -> Vec<GraphEdge> {
    let mut graph = Vec::new();
    for item in items {
        if let Some(parent) = &item.reply_parent_id {
            graph.push(GraphEdge {
                kind: "reply".to_string(),
                from: item.source_id.clone(),
                to: parent.clone(),
            });
        }
        if let Some(root) = &item.thread_root_id {
            graph.push(GraphEdge {
                kind: "thread".to_string(),
                from: item.source_id.clone(),
                to: root.clone(),
            });
        }
        for reaction in &item.reactions {
            graph.push(GraphEdge {
                kind: format!("reaction:{}", reaction.reaction_id),
                from: reaction.actor_discord_id.clone(),
                to: reaction.target_message_id.clone(),
            });
        }
    }
    graph.sort();
    graph
}

/// Seal the source oracle before either device exists. The comparison later
/// opens this blob; it never learns expected content from copy output.
fn seal_independent_manifest(manifest: &Manifest) -> Vec<u8> {
    let plaintext = serde_json::to_vec(manifest).expect("serialize independent source manifest");
    let key = crypto::aead::Key::from_bytes([0x81; crypto::aead::KEY_SIZE]);
    let nonce_bytes = crypto::random::random_bytes(crypto::aead::NONCE_SIZE);
    let mut nonce_array = [0u8; crypto::aead::NONCE_SIZE];
    nonce_array.copy_from_slice(&nonce_bytes);
    let nonce = crypto::aead::Nonce::from_bytes(nonce_array);
    let ciphertext = crypto::aead::seal(&key, &nonce, MANIFEST_AAD, &plaintext)
        .expect("seal independent source manifest");
    let mut sealed = nonce.as_bytes().to_vec();
    sealed.extend_from_slice(&ciphertext);
    sealed
}

fn open_independent_manifest(sealed: &[u8]) -> Manifest {
    let (nonce_bytes, ciphertext) = sealed.split_at(crypto::aead::NONCE_SIZE);
    let mut nonce_array = [0u8; crypto::aead::NONCE_SIZE];
    nonce_array.copy_from_slice(nonce_bytes);
    let nonce = crypto::aead::Nonce::from_bytes(nonce_array);
    let key = crypto::aead::Key::from_bytes([0x81; crypto::aead::KEY_SIZE]);
    let plaintext = crypto::aead::open(&key, &nonce, MANIFEST_AAD, ciphertext)
        .expect("open pre-copy independent source manifest");
    serde_json::from_slice(&plaintext).expect("parse independent source manifest")
}

fn state_with_store(dir: &Path, secret: &[u8; 32], entropy: [u8; 16]) -> AppState {
    let state = AppState::new();
    let mut identity = identity_from_entropy(entropy, ACCOUNT.to_string());
    identity.user_id = ACCOUNT.to_string();
    identity.discord_snowflake = Some(ACCOUNT.to_string());
    state.install_identity(identity);
    let store = MessageStore::open(dir, secret).expect("open independent device store");
    *state.message_store.lock().expect("message store lock") = Some(store);
    state
}

fn to_command_semantics(item: &ManifestItem) -> HistoryCopySemantics {
    HistoryCopySemantics {
        stable_order: item.stable_order,
        thread_root_id: item.thread_root_id.clone(),
        reactions: item
            .reactions
            .iter()
            .map(|reaction| HistoryCopyReaction {
                reaction_id: reaction.reaction_id.clone(),
                target_message_id: reaction.target_message_id.clone(),
                actor_discord_id: reaction.actor_discord_id.clone(),
                emoji: reaction.emoji.clone(),
                reacted_at: reaction.reacted_at,
            })
            .collect(),
        expiration: match &item.expiration {
            Expiration::NonExpiring => HistoryCopyExpirationState::NonExpiring,
            Expiration::Active { timer_minutes } => HistoryCopyExpirationState::Active {
                timer_minutes: *timer_minutes,
            },
        },
    }
}

fn seed_source(state: &AppState, expected: &Manifest) {
    let guard = state.message_store.lock().expect("source store lock");
    let store = guard.as_ref().expect("source store open");
    for item in &expected.items {
        let row = StoredMessage {
            discord_message_id: item.source_id.clone(),
            channel_id: item.channel_id.clone(),
            sender_discord_id: item.author_discord_id.clone(),
            sender_osl_user_id: item.author_osl_user_id.clone(),
            plaintext: item.body.clone(),
            decrypted_at: item.timestamp,
            burned: false,
            reply_parent_id: item.reply_parent_id.clone(),
            edit_revision: item.edit_revision,
        };
        // Stored edit state is the authenticated content version, not a
        // caller-controlled integer. Re-put exactly as many revisions as the
        // sealed oracle says the source has.
        for _ in 0..item.edit_revision {
            store
                .put(&row)
                .expect("seed encrypted source message revision");
        }
        for attachment in &item.attachments {
            store
                .put_history_copy_attachment(
                    &item.source_id,
                    &attachment.name,
                    &attachment.mime,
                    &attachment.bytes,
                    attachment.scope_type.as_deref(),
                    attachment.scope_id.as_deref(),
                    attachment.sender_discord_id.as_deref(),
                    attachment.created_at,
                )
                .expect("seed encrypted source attachment with sealed timestamp");
        }
        if let Expiration::Active { timer_minutes } = &item.expiration {
            store
                .record_message_timer_minutes(&item.source_id, *timer_minutes)
                .expect("seed active expiration state");
        }
        let semantics = serde_json::to_vec(&to_command_semantics(item)).unwrap();
        store
            .put_history_copy_metadata(&item.source_id, &semantics)
            .expect("seed sealed semantic state");
    }
    store
        .put(&StoredMessage {
            discord_message_id: EXPIRED_SOURCE_ID.to_string(),
            channel_id: CHANNEL.to_string(),
            sender_discord_id: "expired-author".to_string(),
            sender_osl_user_id: "expired-osl-author".to_string(),
            plaintext: "TASK4816 expired content that retention must not stub-copy".to_string(),
            decrypted_at: 1_600_000_000,
            burned: false,
            reply_parent_id: None,
            edit_revision: 1,
        })
        .expect("seed soon-expired message");
    store
        .mark_burned(EXPIRED_SOURCE_ID)
        .expect("apply existing expired retention rule");
}

fn normalize_id(copy_id: &str, inverse_ids: &BTreeMap<String, String>, path: &str) -> String {
    inverse_ids
        .get(copy_id)
        .unwrap_or_else(|| panic!("copy.{path} points to unknown copied object {copy_id}"))
        .clone()
}

fn derive_copy_manifest(
    state: &AppState,
    source_to_copy_ids: &BTreeMap<String, String>,
    source_to_copy_reaction_ids: &BTreeMap<String, String>,
) -> Manifest {
    let inverse_ids: BTreeMap<String, String> = source_to_copy_ids
        .iter()
        .map(|(source, copy)| (copy.clone(), source.clone()))
        .collect();
    let inverse_reaction_ids: BTreeMap<String, String> = source_to_copy_reaction_ids
        .iter()
        .map(|(source, copy)| (copy.clone(), source.clone()))
        .collect();
    let mut items = Vec::with_capacity(source_to_copy_ids.len());
    let guard = state.message_store.lock().expect("copy store lock");
    let store = guard.as_ref().expect("copy store open");
    for (source_id, copy_id) in source_to_copy_ids {
        let row = store
            .get(copy_id)
            .expect("decrypt copied message")
            .unwrap_or_else(|| panic!("copy.message[{copy_id}] missing"));
        let semantic_bytes = store
            .get_history_copy_metadata(copy_id)
            .expect("decrypt copied semantic state")
            .unwrap_or_else(|| panic!("copy.message[{copy_id}].semantics missing"));
        let semantics: HistoryCopySemantics =
            serde_json::from_slice(&semantic_bytes).expect("parse copied semantic state");
        let attachments = store
            .list_history_attachments(copy_id)
            .expect("decrypt copied attachment set")
            .into_iter()
            .map(|attachment| Attachment {
                name: attachment.random_filename,
                mime: attachment.mime,
                byte_hash: hash(&attachment.plaintext),
                bytes: attachment.plaintext,
                sender_discord_id: attachment.sender_discord_id,
                scope_type: attachment.scope_type,
                scope_id: attachment.scope_id,
                created_at: attachment.created_at,
            })
            .collect();
        let expiration = match semantics.expiration {
            HistoryCopyExpirationState::NonExpiring => {
                assert_eq!(store.message_timer_minutes(copy_id).unwrap(), None);
                Expiration::NonExpiring
            }
            HistoryCopyExpirationState::Active { timer_minutes } => {
                assert_eq!(
                    store.message_timer_minutes(copy_id).unwrap(),
                    Some(timer_minutes)
                );
                Expiration::Active { timer_minutes }
            }
        };
        let reply_parent_id = row
            .reply_parent_id
            .as_deref()
            .map(|id| normalize_id(id, &inverse_ids, "replyParentId"));
        let thread_root_id = semantics
            .thread_root_id
            .as_deref()
            .map(|id| normalize_id(id, &inverse_ids, "threadRootId"));
        let reactions = semantics
            .reactions
            .into_iter()
            .map(|reaction| Reaction {
                reaction_id: normalize_id(
                    &reaction.reaction_id,
                    &inverse_reaction_ids,
                    "reactions.reactionId",
                ),
                target_message_id: normalize_id(
                    &reaction.target_message_id,
                    &inverse_ids,
                    "reactions.targetMessageId",
                ),
                actor_discord_id: reaction.actor_discord_id,
                emoji: reaction.emoji,
                reacted_at: reaction.reacted_at,
            })
            .collect();
        assert!(!row.burned, "copy.message[{copy_id}].burned");
        items.push(ManifestItem {
            source_id: source_id.clone(),
            channel_id: row.channel_id,
            author_discord_id: row.sender_discord_id,
            author_osl_user_id: row.sender_osl_user_id,
            body_hash: hash(row.plaintext.as_bytes()),
            body: row.plaintext,
            timestamp: row.decrypted_at,
            stable_order: semantics.stable_order,
            edit_revision: row.edit_revision,
            reply_parent_id,
            thread_root_id,
            expiration,
            reactions,
            attachments,
        });
    }
    items.sort_by_key(|item| item.stable_order);
    let graph = derive_graph(&items);
    Manifest { items, graph }
}

fn compare_manifests(source: &Manifest, copy: &Manifest) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if source.items.len() != copy.items.len() {
        errors.push(format!(
            "source.items.length={} != copy.items.length={}",
            source.items.len(),
            copy.items.len()
        ));
    }
    for (index, (expected, actual)) in source.items.iter().zip(&copy.items).enumerate() {
        let source_path = format!("source.message[{}]", expected.source_id);
        let copy_path = format!("copy.message[{}]", actual.source_id);
        macro_rules! field {
            ($name:literal, $left:expr, $right:expr) => {
                if $left != $right {
                    errors.push(format!("{source_path}.{} != {copy_path}.{}", $name, $name));
                }
            };
        }
        field!("id", expected.source_id, actual.source_id);
        field!("channelId", expected.channel_id, actual.channel_id);
        field!(
            "authorDiscordId",
            expected.author_discord_id,
            actual.author_discord_id
        );
        field!(
            "authorOslUserId",
            expected.author_osl_user_id,
            actual.author_osl_user_id
        );
        field!("body", expected.body, actual.body);
        field!("bodyHash", expected.body_hash, actual.body_hash);
        field!("timestamp", expected.timestamp, actual.timestamp);
        field!("stableOrder", expected.stable_order, actual.stable_order);
        field!("editRevision", expected.edit_revision, actual.edit_revision);
        field!(
            "replyParentId",
            expected.reply_parent_id,
            actual.reply_parent_id
        );
        field!(
            "threadRootId",
            expected.thread_root_id,
            actual.thread_root_id
        );
        field!("expirationState", expected.expiration, actual.expiration);
        field!("reactions", expected.reactions, actual.reactions);
        if expected.attachments.len() != actual.attachments.len() {
            errors.push(format!(
                "{source_path}.attachments.length={} != {copy_path}.attachments.length={}",
                expected.attachments.len(),
                actual.attachments.len()
            ));
        }
        for (attachment_index, (left, right)) in expected
            .attachments
            .iter()
            .zip(&actual.attachments)
            .enumerate()
        {
            let prefix = format!("attachments[{attachment_index}]");
            macro_rules! attachment_field {
                ($name:literal, $left:expr, $right:expr) => {
                    if $left != $right {
                        errors.push(format!(
                            "{source_path}.{prefix}.{} != {copy_path}.{prefix}.{}",
                            $name, $name
                        ));
                    }
                };
            }
            attachment_field!("name", left.name, right.name);
            attachment_field!("mime", left.mime, right.mime);
            attachment_field!("bytes", left.bytes, right.bytes);
            attachment_field!("byteHash", left.byte_hash, right.byte_hash);
            attachment_field!(
                "senderDiscordId",
                left.sender_discord_id,
                right.sender_discord_id
            );
            attachment_field!("scopeType", left.scope_type, right.scope_type);
            attachment_field!("scopeId", left.scope_id, right.scope_id);
            attachment_field!("createdAt", left.created_at, right.created_at);
        }
        if expected.source_id != source_id(index) && source.items.len() == ITEM_COUNT {
            errors.push(format!(
                "{source_path}.stableOrder is not in the sealed source order"
            ));
        }
    }
    if source.graph != copy.graph {
        errors.push("source.relationshipGraph != copy.relationshipGraph".to_string());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn raw_ciphertexts(dir: &Path) -> HashSet<Vec<u8>> {
    let conn = Connection::open(dir.join("messages.sqlite")).expect("open raw sqlite observer");
    let mut values = HashSet::new();
    for query in [
        "SELECT ciphertext FROM messages WHERE burned=0",
        "SELECT ciphertext FROM attachments WHERE burned=0",
    ] {
        let mut statement = conn.prepare(query).unwrap();
        let rows = statement
            .query_map([], |row| row.get::<_, Vec<u8>>(0))
            .unwrap();
        for row in rows {
            values.insert(row.unwrap());
        }
    }
    values
}

fn raw_live_counts(dir: &Path) -> (usize, usize) {
    let conn = Connection::open(dir.join("messages.sqlite")).expect("open raw sqlite counter");
    let messages: i64 = conn
        .query_row("SELECT COUNT(*) FROM messages WHERE burned=0", [], |row| {
            row.get(0)
        })
        .unwrap();
    let attachments: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM attachments WHERE burned=0",
            [],
            |row| row.get(0),
        )
        .unwrap();
    (messages as usize, attachments as usize)
}

#[derive(Clone, Debug)]
struct Coverage {
    text_bodies: usize,
    binary_bodies: usize,
    attachments: usize,
    reactions: usize,
    edits: usize,
    replies: usize,
    threads: usize,
    equal_timestamp_pairs: usize,
    expiring: usize,
    nonexpiring: usize,
    click: bool,
    readback: bool,
    restart: bool,
    meter: bool,
    equality: bool,
    mutation: bool,
}

fn coverage(source: &Manifest) -> Coverage {
    let mut timestamp_counts = BTreeMap::new();
    for item in &source.items {
        *timestamp_counts.entry(item.timestamp).or_insert(0usize) += 1;
    }
    Coverage {
        text_bodies: source
            .items
            .iter()
            .filter(|item| !item.body.is_empty())
            .count(),
        binary_bodies: source
            .items
            .iter()
            .flat_map(|item| &item.attachments)
            .filter(|attachment| !attachment.bytes.is_empty())
            .count(),
        attachments: source.items.iter().map(|item| item.attachments.len()).sum(),
        reactions: source.items.iter().map(|item| item.reactions.len()).sum(),
        edits: source
            .items
            .iter()
            .filter(|item| item.edit_revision > 1)
            .count(),
        replies: source
            .items
            .iter()
            .filter(|item| item.reply_parent_id.is_some())
            .count(),
        threads: source
            .items
            .iter()
            .filter(|item| item.thread_root_id.is_some())
            .count(),
        equal_timestamp_pairs: timestamp_counts
            .values()
            .map(|count| count.saturating_sub(1))
            .sum(),
        expiring: source
            .items
            .iter()
            .filter(|item| matches!(item.expiration, Expiration::Active { .. }))
            .count(),
        nonexpiring: source
            .items
            .iter()
            .filter(|item| matches!(item.expiration, Expiration::NonExpiring))
            .count(),
        click: true,
        readback: true,
        restart: true,
        meter: true,
        equality: true,
        mutation: true,
    }
}

fn validate_coverage(value: &Coverage) -> Result<(), String> {
    let counts = [
        ("source.textBodies", value.text_bodies, ITEM_COUNT),
        ("source.binaryBodies", value.binary_bodies, ATTACHMENT_COUNT),
        ("source.attachments", value.attachments, 50),
        ("source.reactions", value.reactions, 50),
        ("source.edits", value.edits, 25),
        ("source.replies", value.replies, 25),
        ("source.threads", value.threads, 25),
        (
            "source.equalTimestampOrdering",
            value.equal_timestamp_pairs,
            1,
        ),
        ("source.expiring", value.expiring, 1),
        ("source.nonexpiring", value.nonexpiring, 1),
    ];
    for (name, actual, minimum) in counts {
        if actual < minimum {
            return Err(format!("TASK4816 missing {name}: {actual} < {minimum}"));
        }
    }
    for (name, present) in [
        ("checkpoint.click", value.click),
        ("checkpoint.decryptReadback", value.readback),
        ("checkpoint.restart", value.restart),
        ("checkpoint.meter", value.meter),
        ("checkpoint.equalityComparison", value.equality),
        ("checkpoint.mutation", value.mutation),
    ] {
        if !present {
            return Err(format!("TASK4816 missing {name}"));
        }
    }
    Ok(())
}

fn spawn_ignored(test_name: &str, scenario: &str) -> Output {
    Command::new(std::env::current_exe().expect("current test binary"))
        .args(["--ignored", "--exact", test_name, "--nocapture"])
        .env("TASK4816_SCENARIO", scenario)
        .output()
        .unwrap_or_else(|error| panic!("spawn {test_name}/{scenario}: {error}"))
}

fn child_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn task_4816_copy_my_history_preserves_semantic_manifest() {
    // This is deliberately the first act: the oracle is encrypted before the
    // source or destination device is opened and before the click boundary.
    let sealed_source_manifest = seal_independent_manifest(&build_source_manifest());
    let expected = open_independent_manifest(&sealed_source_manifest);
    let source_dir = TempDir::new().expect("source device directory");
    let copy_dir = TempDir::new().expect("copy device directory");
    let source_secret = [0x48; 32];
    let copy_secret = [0x16; 32];
    let source_state = state_with_store(source_dir.path(), &source_secret, [0x48; 16]);
    let copy_state = state_with_store(copy_dir.path(), &copy_secret, [0x16; 16]);
    seed_source(&source_state, &expected);

    assert_eq!(raw_live_counts(copy_dir.path()), (0, 0));
    let expired_error = cmd_osl_export_history_for_copy(
        &source_state,
        CHANNEL.to_string(),
        vec![EXPIRED_SOURCE_ID.to_string()],
    )
    .expect_err("expired-before-copy retained row must not become an id-only stub");
    assert!(expired_error.contains("source.message["), "{expired_error}");
    assert!(
        expired_error.contains("expirationState=expired-before-copy"),
        "{expired_error}"
    );
    assert!(
        expired_error.contains("no id-only stub was copied"),
        "{expired_error}"
    );

    let confirmation = cmd_osl_copy_my_history_here_confirmation(ITEM_COUNT).unwrap();
    assert_eq!(confirmation.action_label, COPY_MY_HISTORY_HERE_ACTION_LABEL);
    assert_eq!(
        confirmation.sentence,
        COPY_MY_HISTORY_HERE_CONFIRMATION_SENTENCE
    );
    assert_eq!(confirmation.selected_count, ITEM_COUNT);

    let selected: Vec<String> = expected
        .items
        .iter()
        .rev()
        .map(|item| item.source_id.clone())
        .collect();
    let package = cmd_osl_export_history_for_copy(&source_state, CHANNEL.to_string(), selected)
        .expect("export encrypted semantic history copy");
    let wire = base64::engine::general_purpose::STANDARD
        .decode(&package)
        .expect("decode outer wire");
    let mut wire_plaintext_occurrences = 0usize;
    for item in &expected.items {
        wire_plaintext_occurrences += wire
            .windows(item.body.len())
            .filter(|window| *window == item.body.as_bytes())
            .count();
        for attachment in &item.attachments {
            wire_plaintext_occurrences += wire
                .windows(attachment.bytes.len())
                .filter(|window| *window == attachment.bytes.as_slice())
                .count();
        }
    }
    assert_eq!(wire_plaintext_occurrences, 0, "source plaintext on wire");

    let phrase = bip39::Mnemonic::from_entropy_in(bip39::Language::English, &[0x48; 16])
        .unwrap()
        .to_string();
    let result = cmd_osl_copy_my_history_here(&copy_state, package, phrase)
        .expect("real Copy my history here click");
    assert_eq!(result.action_label, COPY_MY_HISTORY_HERE_ACTION_LABEL);
    assert_eq!(result.copied_count, ITEM_COUNT);
    assert_eq!(result.source_to_copy_ids.len(), ITEM_COUNT);
    let source_ids: BTreeSet<_> = expected
        .items
        .iter()
        .map(|item| item.source_id.clone())
        .collect();
    let copy_ids: BTreeSet<_> = result.source_to_copy_ids.values().cloned().collect();
    assert_eq!(copy_ids.len(), ITEM_COUNT);
    assert!(source_ids.is_disjoint(&copy_ids));
    assert!(result
        .source_to_copy_ids
        .iter()
        .all(|(source, copy)| source != copy));
    let source_reaction_ids: BTreeSet<_> = expected
        .items
        .iter()
        .flat_map(|item| &item.reactions)
        .map(|reaction| reaction.reaction_id.clone())
        .collect();
    let copy_reaction_ids: BTreeSet<_> = result
        .source_to_copy_reaction_ids
        .values()
        .cloned()
        .collect();
    assert_eq!(result.source_to_copy_reaction_ids.len(), REACTION_COUNT);
    assert_eq!(copy_reaction_ids.len(), REACTION_COUNT);
    assert!(source_reaction_ids.is_disjoint(&copy_reaction_ids));

    // Exercise the IPC readback command itself outside any store lock. The
    // full manifest derivation below uses the equivalent store read while it
    // holds one lock for a consistent snapshot.
    let first_copy_id = result.source_to_copy_ids[&source_id(0)].clone();
    assert!(
        cmd_osl_read_history_copy_semantics(&copy_state, first_copy_id)
            .expect("IPC semantic decrypt/readback checkpoint")
            .is_some()
    );

    let actual_before_restart = derive_copy_manifest(
        &copy_state,
        &result.source_to_copy_ids,
        &result.source_to_copy_reaction_ids,
    );
    compare_manifests(&expected, &actual_before_restart)
        .unwrap_or_else(|errors| panic!("{}", errors.join("\n")));
    assert_eq!(
        raw_live_counts(copy_dir.path()),
        (ITEM_COUNT, ATTACHMENT_COUNT)
    );
    let source_ciphertexts = raw_ciphertexts(source_dir.path());
    let copy_ciphertexts = raw_ciphertexts(copy_dir.path());
    assert_eq!(source_ciphertexts.len(), ITEM_COUNT + ATTACHMENT_COUNT);
    assert_eq!(copy_ciphertexts.len(), ITEM_COUNT + ATTACHMENT_COUNT);
    assert!(
        source_ciphertexts.is_disjoint(&copy_ciphertexts),
        "source and destination ciphertext sets must be disjoint"
    );

    let expected_text_bytes: u64 = expected
        .items
        .iter()
        .map(|item| item.body.len() as u64)
        .sum();
    let expected_attachment_bytes: u64 = expected
        .items
        .iter()
        .flat_map(|item| &item.attachments)
        .map(|attachment| attachment.bytes.len() as u64)
        .sum();
    let expected_actual_bytes = expected_text_bytes + expected_attachment_bytes;
    assert_eq!(result.plaintext_bytes_written, expected_text_bytes);
    assert_eq!(result.attachment_count, ATTACHMENT_COUNT);
    assert_eq!(result.attachment_bytes_written, expected_attachment_bytes);
    assert_eq!(result.actual_copied_bytes_written, expected_actual_bytes);
    assert_eq!(result.monthly_data_bytes_before, 0);
    assert_eq!(result.monthly_data_bytes_after, expected_actual_bytes);

    drop(copy_state);
    let reopened_copy_state = state_with_store(copy_dir.path(), &copy_secret, [0x16; 16]);
    let persisted_meter = cmd_osl_history_copy_month_data_bytes(&reopened_copy_state);
    assert_eq!(persisted_meter, expected_actual_bytes);
    let actual_after_restart = derive_copy_manifest(
        &reopened_copy_state,
        &result.source_to_copy_ids,
        &result.source_to_copy_reaction_ids,
    );
    compare_manifests(&expected, &actual_after_restart)
        .unwrap_or_else(|errors| panic!("{}", errors.join("\n")));
    let restarted_rows = reopened_copy_state
        .message_store
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .list_by_channel(CHANNEL, ITEM_COUNT as u32)
        .expect("decrypt stable ordering after restart");
    let expected_restart_order: Vec<_> = expected
        .items
        .iter()
        .rev()
        .map(|item| result.source_to_copy_ids[&item.source_id].clone())
        .collect();
    let actual_restart_order: Vec<_> = restarted_rows
        .iter()
        .map(|row| row.discord_message_id.clone())
        .collect();
    assert_eq!(actual_restart_order, expected_restart_order);

    let observed_coverage = coverage(&expected);
    validate_coverage(&observed_coverage).expect("full task 4816 source/checkpoint coverage");
    for scenario in [
        "textBodies",
        "binaryBodies",
        "attachments",
        "reactions",
        "edits",
        "replies",
        "threads",
        "equalTimestampOrdering",
        "expiring",
        "nonexpiring",
        "click",
        "decryptReadback",
        "restart",
        "meter",
        "equalityComparison",
        "mutation",
    ] {
        let output = spawn_ignored("task_4816_starvation_child", scenario);
        let text = child_text(&output);
        assert_eq!(output.status.code(), Some(1), "{scenario}: {text}");
        assert!(text.contains(scenario), "{scenario}: {text}");
    }
    for scenario in [
        "emptyBody",
        "truncatedBody",
        "omittedAttachment",
        "alteredTimestampOrder",
        "brokenRelationship",
    ] {
        let output = spawn_ignored("task_4816_mutation_child", scenario);
        let text = child_text(&output);
        assert_eq!(output.status.code(), Some(1), "{scenario}: {text}");
        assert!(text.contains("source.message["), "{scenario}: {text}");
        assert!(text.contains("copy.message["), "{scenario}: {text}");
        println!(
            "TASK4816 mutation={scenario} exit=1 diagnostic={}",
            text.trim()
        );
    }

    println!("TASK4816 presealed_source_manifest_items={ITEM_COUNT}");
    println!(
        "TASK4816 nonempty_text_bodies={}",
        observed_coverage.text_bodies
    );
    println!(
        "TASK4816 nonempty_binary_bodies={}",
        observed_coverage.binary_bodies
    );
    println!("TASK4816 attachments={}", observed_coverage.attachments);
    println!("TASK4816 reactions={}", observed_coverage.reactions);
    println!("TASK4816 edits={}", observed_coverage.edits);
    println!("TASK4816 replies={}", observed_coverage.replies);
    println!("TASK4816 threads={}", observed_coverage.threads);
    println!(
        "TASK4816 equal_timestamp_ordering_cases={}",
        observed_coverage.equal_timestamp_pairs
    );
    println!("TASK4816 expiring={}", observed_coverage.expiring);
    println!("TASK4816 nonexpiring={}", observed_coverage.nonexpiring);
    println!(
        "TASK4816 copied_new_encrypted_records={}",
        result.copied_count
    );
    println!("TASK4816 newly_remapped_ids={}", copy_ids.len());
    println!(
        "TASK4816 newly_remapped_reaction_ids={}",
        copy_reaction_ids.len()
    );
    println!("TASK4816 source_plaintext_on_wire={wire_plaintext_occurrences}");
    println!("TASK4816 source_copy_ciphertext_intersection=0");
    println!("TASK4816 attachment_hash_matches={ATTACHMENT_COUNT}");
    println!("TASK4816 semantic_manifest_field_mismatches=0");
    println!("TASK4816 relationship_graph_edges={}", expected.graph.len());
    println!(
        "TASK4816 stable_order_after_restart={}",
        restarted_rows.len()
    );
    println!("TASK4816 expired_before_copy_named=true id_only_stubs=0");
    println!("TASK4816 actual_copied_bytes={expected_actual_bytes}");
    println!("TASK4816 persisted_meter_after_restart={persisted_meter}");
    println!("TASK4816 starvation_subprocesses=16 all_exit_1=true");
    println!("TASK4816 mutation_subprocesses=5 all_exit_1=true");
}

#[test]
#[ignore = "throwaway child; parent requires deliberate exit 1"]
fn task_4816_starvation_child() {
    let scenario = std::env::var("TASK4816_SCENARIO").expect("TASK4816_SCENARIO");
    let source = build_source_manifest();
    let mut value = coverage(&source);
    match scenario.as_str() {
        "textBodies" => value.text_bodies = 0,
        "binaryBodies" => value.binary_bodies = 0,
        "attachments" => value.attachments = 49,
        "reactions" => value.reactions = 49,
        "edits" => value.edits = 24,
        "replies" => value.replies = 24,
        "threads" => value.threads = 24,
        "equalTimestampOrdering" => value.equal_timestamp_pairs = 0,
        "expiring" => value.expiring = 0,
        "nonexpiring" => value.nonexpiring = 0,
        "click" => value.click = false,
        "decryptReadback" => value.readback = false,
        "restart" => value.restart = false,
        "meter" => value.meter = false,
        "equalityComparison" => value.equality = false,
        "mutation" => value.mutation = false,
        other => panic!("unknown starvation scenario {other}"),
    }
    match validate_coverage(&value) {
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
        Ok(()) => panic!("starved scenario {scenario} was accepted"),
    }
}

fn mutation_fixture() -> Manifest {
    let mut source = build_source_manifest();
    source.items.truncate(101);
    source.graph = derive_graph(&source.items);
    source
}

#[test]
#[ignore = "throwaway child; parent requires deliberate exit 1"]
fn task_4816_mutation_child() {
    let scenario = std::env::var("TASK4816_SCENARIO").expect("TASK4816_SCENARIO");
    let source = mutation_fixture();
    let mut copy = source.clone();
    match scenario.as_str() {
        "emptyBody" => {
            copy.items[0].body.clear();
            copy.items[0].body_hash = hash(copy.items[0].body.as_bytes());
        }
        "truncatedBody" => {
            copy.items[1].body.pop();
            copy.items[1].body_hash = hash(copy.items[1].body.as_bytes());
        }
        "omittedAttachment" => {
            copy.items[2].attachments.clear();
        }
        "alteredTimestampOrder" => {
            copy.items[3].timestamp += 1;
            copy.items[3].stable_order += 10_000;
        }
        "brokenRelationship" => {
            copy.items[25].reply_parent_id = Some(source_id(2));
            copy.graph = derive_graph(&copy.items);
        }
        other => panic!("unknown mutation scenario {other}"),
    }
    match compare_manifests(&source, &copy) {
        Err(errors) => {
            eprintln!("TASK4816 {scenario}: {}", errors.join("; "));
            std::process::exit(1);
        }
        Ok(()) => panic!("mutation {scenario} was accepted"),
    }
}
