//! TASK 6206 — prove every deployed OSL Chats content write is authorized.
//!
//! Gate 6120 proved the *read* half of the deployed service: a hostile client
//! could not enumerate a roster, a channel list, a restricted history, a child
//! thread or a ciphertext blob. Its "every forbidden write is refused" line
//! covered exactly one write, the self-permission raise, because at the time
//! there were no deployed content-write endpoints to reach. There are now —
//! `message.create`, `message.edit`, `message.delete`, `channel.create` and
//! `thread.create` — and this check attacks all of them.
//!
//! Nothing here is hand-picked. The inventory is read from two shipping
//! artefacts:
//!
//!   * `route-manifest.json`, written into every install directory by
//!     `osl-chats-client --generate` from `ipc::chats_route_manifest`, and
//!   * `router.json`, written into the deployed service's data directory at
//!     boot from the router table the service actually dispatches through,
//!     together with the ops its authority answers.
//!
//! The three sets — manifest, router, and what this run exercised — have to
//! match one for one, and every unknown path the deployed service is offered
//! has to be refused as an unknown route, so no deployed route can go
//! unclassified.
//!
//! What is observed is, as in 6120, two independent things:
//!
//!   * the raw bytes each installed client received off the socket, saved by
//!     that client process, and
//!   * the deployed service's own durable records (`store.json`), delivery
//!     queue (`queue.jsonl`), subscriber broadcast (`broadcast.jsonl`),
//!     decision journal (`audit.jsonl`) and rights snapshot (`rights.json`),
//!     read off the service's data directory — never served to any client.
//!
//! The store, queue and broadcast are sampled between every request, so a
//! refused write is judged on whether it reached a durable, queued or broadcast
//! byte, not merely on what it was told.
//!
//! `TASK6206_STARVE_*` knobs remove one thing at a time from the run so the
//! starvation ladder can show the check notices. They never relax the service.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;

use base64::Engine as _;
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Frozen identifiers, fixed before the service is deployed.
// ---------------------------------------------------------------------------

const ENCLAVE: &str = "enclave-6206-frozen-c410";
/// A different server. No write carrying this may land anywhere.
const OTHER_ENCLAVE: &str = "enclave-6206-other-d520";
const OPEN_CHANNEL: &str = "channel-6206-open-11a0";
const RESTRICTED_CHANNEL: &str = "channel-6206-limited-22b0";
/// A channel identifier that is not a channel of this enclave.
const FOREIGN_CHANNEL: &str = "channel-6206-foreign-33f0";
const OPEN_THREAD: &str = "thread-6206-open-41c0";
const CHILD_THREAD: &str = "thread-6206-child-42d0";

const MSG_OPEN_ADA: &str = "message-6206-open-ada-51a0";
const MSG_OPEN_BEN_KEEP: &str = "message-6206-open-ben-keep-52b0";
const MSG_OPEN_BEN_OWN: &str = "message-6206-open-ben-own-53b1";
const MSG_OPEN_BEN_DEL: &str = "message-6206-open-ben-del-54b2";
const MSG_OPEN_DEL: &str = "message-6206-open-del-55d0";
const MSG_OPENTHREAD: &str = "message-6206-openthread-56a1";
const MSG_LIMITED_CY: &str = "message-6206-limited-cy-57c0";
const MSG_LIMITED_CY_DEL: &str = "message-6206-limited-cydel-58c1";
const MSG_LIMITED_KEEP: &str = "message-6206-limited-keep-59c2";
const MSG_CHILDTHREAD: &str = "message-6206-childthread-60c3";

const NEW_MSG_OWNER: &str = "message-6206-new-owner-71a0";
const NEW_MSG_LIMITED: &str = "message-6206-new-limited-72c0";
const NEW_CHANNEL_OWNER: &str = "channel-6206-new-owner-81a0";
const NEW_CHANNEL_LIMITED: &str = "channel-6206-new-limited-82c0";
const NEW_THREAD_OWNER: &str = "thread-6206-new-owner-91a0";
const NEW_THREAD_LIMITED: &str = "thread-6206-new-limited-92c0";

const OWNER_NAME: &str = "Ada 6206";
const MEMBER_NAME: &str = "Ben 6206";
const LIMITED_NAME: &str = "Cy 6206";
const EXCLUDED_NAME: &str = "Del 6206";
const NEVER_NAME: &str = "Eve 6206";

const ROLES: [(&str, &str); 5] = [
    ("owner", OWNER_NAME),
    ("member", MEMBER_NAME),
    ("limited", LIMITED_NAME),
    ("excluded", EXCLUDED_NAME),
    ("never", NEVER_NAME),
];

const REFUSE_DIRECTORY: &str = "directory-refused";
const REFUSE_SELF_ROLE: &str = "self-role-raise-refused";
const REFUSE_MEMBERSHIP: &str = "write-membership-refused";
const REFUSE_ROLE: &str = "write-role-refused";
const REFUSE_AUTHOR: &str = "write-author-binding-refused";
const REFUSE_PARENT: &str = "write-parent-binding-refused";
const REFUSE_BAD_SIGNATURE: &str = "bad-signature";

/// Durable records an authorized control is allowed to change, and the only
/// ones. Their union has to be exactly what the authorized controls changed.
const FROZEN_TARGETS: [&str; 11] = [
    "message:message-6206-new-owner-71a0",
    "message:message-6206-new-limited-72c0",
    "message:message-6206-open-ada-51a0",
    "message:message-6206-limited-cy-57c0",
    "message:message-6206-open-ben-del-54b2",
    "message:message-6206-limited-cydel-58c1",
    "channel:channel-6206-new-owner-81a0",
    "channel:channel-6206-new-limited-82c0",
    "thread:thread-6206-new-owner-91a0",
    "thread:thread-6206-new-limited-92c0",
    "rights:Cy 6206",
];

/// Durable records nothing in this run may touch. Every one of them is judged
/// byte-identical between the deployed seed and the end of the run, and every
/// hostile write in the matrix aims at one of them.
const KEEP_RECORDS: [&str; 15] = [
    "message:message-6206-open-ben-keep-52b0",
    "message:message-6206-open-ben-own-53b1",
    "message:message-6206-open-del-55d0",
    "message:message-6206-limited-keep-59c2",
    "message:message-6206-openthread-56a1",
    "message:message-6206-childthread-60c3",
    "channel:channel-6206-open-11a0",
    "channel:channel-6206-limited-22b0",
    "thread:thread-6206-open-41c0",
    "thread:thread-6206-child-42d0",
    "roster",
    "rights:Ada 6206",
    "rights:Ben 6206",
    "rights:Del 6206",
    "rights:Eve 6206",
];

/// Record identifiers a hostile request tries to bring into existence. None of
/// them may ever appear in the deployed service's durable store.
const FORBIDDEN_RECORD_IDS: [&str; 20] = [
    "message:message-6206-hostile-member-create",
    "message:message-6206-hostile-excluded-create",
    "message:message-6206-hostile-never-create",
    "message:message-6206-hostile-foreign-enclave",
    "message:message-6206-hostile-thread-parent",
    "message:message-6206-hostile-forged-author",
    "message:message-6206-hostile-owner-restricted",
    "message:message-6206-hostile-foreign-channel",
    "message:message-6206-hostile-forged-key",
    "channel:channel-6206-hostile-member",
    "channel:channel-6206-hostile-excluded",
    "channel:channel-6206-hostile-never",
    "channel:channel-6206-hostile-forged-creator",
    "channel:channel-6206-hostile-foreign-enclave",
    "thread:thread-6206-hostile-member",
    "thread:thread-6206-hostile-excluded",
    "thread:thread-6206-hostile-never",
    "thread:thread-6206-hostile-forged-creator",
    "thread:thread-6206-hostile-foreign-enclave",
    "thread:thread-6206-hostile-foreign-channel",
];

fn ciphertext_of(message_id: &str) -> Vec<u8> {
    Sha256::digest(format!("TASK6206/ciphertext/{message_id}").as_bytes()).to_vec()
}

fn edited_ciphertext_of(message_id: &str) -> Vec<u8> {
    Sha256::digest(format!("TASK6206/edited/{message_id}").as_bytes()).to_vec()
}

fn ciphertext_b64(message_id: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(ciphertext_of(message_id))
}

// ---------------------------------------------------------------------------
// One exercised cell of the inventory.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Cell {
    /// `inventory` (one per manifest row per role), `binding` (a forged
    /// parent/author attack) or `signature`.
    phase: &'static str,
    label: &'static str,
    op: String,
    body: serde_json::Value,
    /// `None` for an authorized control; the exact refusal code otherwise.
    expect_code: Option<&'static str>,
    /// The durable record identifiers this authorized control must change, and
    /// the only ones it may change.
    changes: Vec<String>,
    exact: Option<(&'static str, Vec<String>)>,
    contains: Vec<String>,
    absent: Vec<String>,
    forge_key_of: Option<&'static str>,
    tamper: bool,
    path_override: Option<String>,
    /// The client label the deployed service will attribute this request to.
    /// It differs from `label` only when the request presents somebody else's
    /// public key, which is the point of that attack.
    audit_label: &'static str,
    /// Human description of what this request forges, printed on failure.
    note: String,
}

impl Cell {
    fn allow(phase: &'static str, label: &'static str, op: &str, body: serde_json::Value) -> Self {
        Self {
            phase,
            label,
            op: op.to_owned(),
            body,
            expect_code: None,
            changes: Vec::new(),
            exact: None,
            contains: Vec::new(),
            absent: Vec::new(),
            forge_key_of: None,
            tamper: false,
            path_override: None,
            audit_label: label,
            note: String::new(),
        }
    }

    fn refuse(
        phase: &'static str,
        label: &'static str,
        op: &str,
        body: serde_json::Value,
        code: &'static str,
        note: &str,
    ) -> Self {
        let mut cell = Self::allow(phase, label, op, body);
        cell.expect_code = Some(code);
        cell.note = note.to_owned();
        cell
    }

    fn hostile(&self) -> bool {
        self.expect_code.is_some()
    }

    fn changes(mut self, ids: &[&str]) -> Self {
        self.changes = ids.iter().map(|id| (*id).to_owned()).collect();
        self
    }

    fn exact(mut self, field: &'static str, ids: &[&str]) -> Self {
        self.exact = Some((field, ids.iter().map(|id| (*id).to_owned()).collect()));
        self
    }

    fn contains(mut self, tokens: &[&str]) -> Self {
        self.contains
            .extend(tokens.iter().map(|token| (*token).to_owned()));
        self
    }

    fn absent(mut self, tokens: &[&str]) -> Self {
        self.absent
            .extend(tokens.iter().map(|token| (*token).to_owned()));
        self
    }

    /// The parent/author binding this request presented, printed verbatim on a
    /// failure so a leak names what was forged.
    fn binding(&self) -> String {
        let pick = |key: &str| {
            self.body
                .get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or("-")
                .to_owned()
        };
        format!(
            "enclave={} channel={} thread={} message={} author={} creator={} target={}",
            pick("enclaveId"),
            pick("channelId"),
            pick("threadId"),
            pick("messageId"),
            pick("author"),
            pick("creator"),
            pick("target"),
        )
    }
}

fn hex_of(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

// ---------------------------------------------------------------------------
// The matrix: one cell per (manifest row, role). Derived from the manifest, not
// hand-picked — an op with no cell here is reported as unclassified.
// ---------------------------------------------------------------------------

fn matrix_cell(op: &str, role: &str) -> Option<Cell> {
    let cell = match (op, role) {
        // ---- role.grant -------------------------------------------------
        ("role.grant", "owner") => Cell::allow(
            "inventory",
            "owner",
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "target": LIMITED_NAME, "right": "invite"}),
        )
        .changes(&["rights:Cy 6206"])
        .contains(&["invite"]),
        ("role.grant", label) => Cell::refuse(
            "inventory",
            leak_label(label),
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "target": person_of(label), "right": "remove-people"}),
            REFUSE_SELF_ROLE,
            "raises its own rights",
        ),

        // ---- roster.list --------------------------------------------------
        ("roster.list", "owner") => Cell::allow(
            "inventory",
            "owner",
            op,
            serde_json::json!({"enclaveId": ENCLAVE}),
        )
        .exact("members", &[OWNER_NAME, MEMBER_NAME, LIMITED_NAME])
        .absent(&[EXCLUDED_NAME, NEVER_NAME]),
        ("roster.list", "member") => Cell::allow(
            "inventory",
            "member",
            op,
            serde_json::json!({"enclaveId": ENCLAVE}),
        )
        .exact("members", &[OWNER_NAME, MEMBER_NAME, LIMITED_NAME]),
        ("roster.list", "limited") => Cell::allow(
            "inventory",
            "limited",
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "channelId": RESTRICTED_CHANNEL}),
        )
        .exact("members", &[MEMBER_NAME, LIMITED_NAME])
        .contains(&[RESTRICTED_CHANNEL]),
        ("roster.list", label) => Cell::refuse(
            "inventory",
            leak_label(label),
            op,
            serde_json::json!({"enclaveId": ENCLAVE}),
            REFUSE_DIRECTORY,
            "enumerates the roster",
        ),

        // ---- channel.list -------------------------------------------------
        // The owner is deliberately not a member of the limited channel, so the
        // owner's own channel list must not carry it either.
        ("channel.list", "owner") => Cell::allow(
            "inventory",
            "owner",
            op,
            serde_json::json!({"enclaveId": ENCLAVE}),
        )
        .exact("channels", &[OPEN_CHANNEL])
        .absent(&[RESTRICTED_CHANNEL]),
        ("channel.list", "member") => Cell::allow(
            "inventory",
            "member",
            op,
            serde_json::json!({"enclaveId": ENCLAVE}),
        )
        .exact("channels", &[OPEN_CHANNEL, RESTRICTED_CHANNEL]),
        ("channel.list", "limited") => Cell::allow(
            "inventory",
            "limited",
            op,
            serde_json::json!({"enclaveId": ENCLAVE}),
        )
        .exact("channels", &[OPEN_CHANNEL, RESTRICTED_CHANNEL]),
        ("channel.list", label) => Cell::refuse(
            "inventory",
            leak_label(label),
            op,
            serde_json::json!({"enclaveId": ENCLAVE}),
            REFUSE_DIRECTORY,
            "enumerates the channels",
        ),

        // ---- history.read -------------------------------------------------
        ("history.read", "owner") => Cell::allow(
            "inventory",
            "owner",
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "channelId": OPEN_CHANNEL}),
        )
        .exact(
            "messages",
            &[
                MSG_OPEN_ADA,
                MSG_OPEN_BEN_KEEP,
                MSG_OPEN_BEN_OWN,
                MSG_OPEN_BEN_DEL,
                MSG_OPEN_DEL,
            ],
        )
        .contains(&[&ciphertext_b64(MSG_OPEN_ADA)]),
        ("history.read", "member") => Cell::allow(
            "inventory",
            "member",
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "threadId": CHILD_THREAD}),
        )
        .exact("messages", &[MSG_CHILDTHREAD])
        .contains(&[CHILD_THREAD, RESTRICTED_CHANNEL]),
        ("history.read", "limited") => Cell::allow(
            "inventory",
            "limited",
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "channelId": RESTRICTED_CHANNEL}),
        )
        .exact(
            "messages",
            &[MSG_LIMITED_CY, MSG_LIMITED_CY_DEL, MSG_LIMITED_KEEP],
        ),
        ("history.read", label) => Cell::refuse(
            "inventory",
            leak_label(label),
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "channelId": OPEN_CHANNEL}),
            REFUSE_DIRECTORY,
            "reads a channel history",
        ),

        // ---- history.search -----------------------------------------------
        ("history.search", "owner") => Cell::allow(
            "inventory",
            "owner",
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "channelId": OPEN_CHANNEL, "needle": "6206"}),
        )
        .exact(
            "messageIds",
            &[
                MSG_OPEN_ADA,
                MSG_OPEN_BEN_KEEP,
                MSG_OPEN_BEN_OWN,
                MSG_OPEN_BEN_DEL,
                MSG_OPEN_DEL,
                MSG_OPENTHREAD,
            ],
        ),
        ("history.search", "member") => Cell::allow(
            "inventory",
            "member",
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "channelId": OPEN_CHANNEL, "needle": "openthread"}),
        )
        .exact("messageIds", &[MSG_OPENTHREAD]),
        ("history.search", "limited") => Cell::allow(
            "inventory",
            "limited",
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "channelId": RESTRICTED_CHANNEL, "needle": "6206"}),
        )
        .exact(
            "messageIds",
            &[
                MSG_LIMITED_CY,
                MSG_LIMITED_CY_DEL,
                MSG_LIMITED_KEEP,
                MSG_CHILDTHREAD,
            ],
        ),
        ("history.search", label) => Cell::refuse(
            "inventory",
            leak_label(label),
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "channelId": OPEN_CHANNEL, "needle": "6206"}),
            REFUSE_DIRECTORY,
            "searches a channel history",
        ),

        // ---- history.sync -------------------------------------------------
        ("history.sync", "owner") => Cell::allow(
            "inventory",
            "owner",
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "channelId": OPEN_CHANNEL, "since": 0}),
        )
        .exact(
            "messages",
            &[
                MSG_OPEN_ADA,
                MSG_OPEN_BEN_KEEP,
                MSG_OPEN_BEN_OWN,
                MSG_OPEN_BEN_DEL,
                MSG_OPEN_DEL,
                MSG_OPENTHREAD,
            ],
        ),
        ("history.sync", "member") => Cell::allow(
            "inventory",
            "member",
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "channelId": OPEN_CHANNEL, "since": 5}),
        )
        .exact("messages", &[MSG_OPENTHREAD]),
        ("history.sync", "limited") => Cell::allow(
            "inventory",
            "limited",
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "channelId": RESTRICTED_CHANNEL, "since": 0}),
        )
        .exact(
            "messages",
            &[
                MSG_LIMITED_CY,
                MSG_LIMITED_CY_DEL,
                MSG_LIMITED_KEEP,
                MSG_CHILDTHREAD,
            ],
        ),
        ("history.sync", label) => Cell::refuse(
            "inventory",
            leak_label(label),
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "channelId": OPEN_CHANNEL, "since": 0}),
            REFUSE_DIRECTORY,
            "syncs a channel history",
        ),

        // ---- history.subscribe --------------------------------------------
        ("history.subscribe", "owner") => Cell::allow(
            "inventory",
            "owner",
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "channelId": OPEN_CHANNEL}),
        )
        .exact(
            "events",
            &[
                MSG_OPEN_ADA,
                MSG_OPEN_BEN_KEEP,
                MSG_OPEN_BEN_OWN,
                MSG_OPEN_BEN_DEL,
                MSG_OPEN_DEL,
                MSG_OPENTHREAD,
            ],
        ),
        ("history.subscribe", "member") => Cell::allow(
            "inventory",
            "member",
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "channelId": RESTRICTED_CHANNEL}),
        )
        .exact(
            "events",
            &[
                MSG_LIMITED_CY,
                MSG_LIMITED_CY_DEL,
                MSG_LIMITED_KEEP,
                MSG_CHILDTHREAD,
            ],
        ),
        ("history.subscribe", "limited") => Cell::allow(
            "inventory",
            "limited",
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "channelId": OPEN_CHANNEL}),
        )
        .exact(
            "events",
            &[
                MSG_OPEN_ADA,
                MSG_OPEN_BEN_KEEP,
                MSG_OPEN_BEN_OWN,
                MSG_OPEN_BEN_DEL,
                MSG_OPEN_DEL,
                MSG_OPENTHREAD,
            ],
        ),
        ("history.subscribe", label) => Cell::refuse(
            "inventory",
            leak_label(label),
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "channelId": RESTRICTED_CHANNEL}),
            REFUSE_DIRECTORY,
            "subscribes to a channel",
        ),

        // ---- blob.fetch ----------------------------------------------------
        ("blob.fetch", "owner") => Cell::allow(
            "inventory",
            "owner",
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "messageId": MSG_OPEN_ADA}),
        )
        .contains(&[MSG_OPEN_ADA, &ciphertext_b64(MSG_OPEN_ADA)]),
        ("blob.fetch", "member") => Cell::allow(
            "inventory",
            "member",
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "messageId": MSG_LIMITED_KEEP}),
        )
        .contains(&[MSG_LIMITED_KEEP, &ciphertext_b64(MSG_LIMITED_KEEP)]),
        ("blob.fetch", "limited") => Cell::allow(
            "inventory",
            "limited",
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "messageId": MSG_CHILDTHREAD}),
        )
        .contains(&[MSG_CHILDTHREAD, &ciphertext_b64(MSG_CHILDTHREAD)]),
        ("blob.fetch", label) => Cell::refuse(
            "inventory",
            leak_label(label),
            op,
            serde_json::json!({"enclaveId": ENCLAVE, "messageId": MSG_LIMITED_KEEP}),
            REFUSE_DIRECTORY,
            "fetches a known ciphertext",
        ),

        // ---- message.create -------------------------------------------------
        ("message.create", "owner") => Cell::allow(
            "inventory",
            "owner",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "messageId": NEW_MSG_OWNER,
                "author": OWNER_NAME,
                "ciphertextHex": hex_of(&ciphertext_of(NEW_MSG_OWNER)),
            }),
        )
        .changes(&["message:message-6206-new-owner-71a0"])
        .contains(&[NEW_MSG_OWNER]),
        ("message.create", "member") => Cell::refuse(
            "inventory",
            "member",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "messageId": "message-6206-hostile-member-create",
                "author": MEMBER_NAME,
                "ciphertextHex": hex_of(&ciphertext_of("hostile-member-create")),
            }),
            REFUSE_ROLE,
            "sends without the send right",
        ),
        ("message.create", "limited") => Cell::allow(
            "inventory",
            "limited",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": RESTRICTED_CHANNEL,
                "messageId": NEW_MSG_LIMITED,
                "author": LIMITED_NAME,
                "ciphertextHex": hex_of(&ciphertext_of(NEW_MSG_LIMITED)),
            }),
        )
        .changes(&["message:message-6206-new-limited-72c0"])
        .contains(&[NEW_MSG_LIMITED]),
        ("message.create", "excluded") => Cell::refuse(
            "inventory",
            "excluded",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "messageId": "message-6206-hostile-excluded-create",
                "author": EXCLUDED_NAME,
                "ciphertextHex": hex_of(&ciphertext_of("hostile-excluded-create")),
            }),
            REFUSE_MEMBERSHIP,
            "sends after removal on a grant that outlived it",
        ),
        ("message.create", _) => Cell::refuse(
            "inventory",
            "never",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "messageId": "message-6206-hostile-never-create",
                "author": NEVER_NAME,
                "ciphertextHex": hex_of(&ciphertext_of("hostile-never-create")),
            }),
            REFUSE_MEMBERSHIP,
            "never-member sends a message",
        ),

        // ---- message.edit ---------------------------------------------------
        ("message.edit", "owner") => Cell::allow(
            "inventory",
            "owner",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "messageId": MSG_OPEN_ADA,
                "ciphertextHex": hex_of(&edited_ciphertext_of(MSG_OPEN_ADA)),
            }),
        )
        .changes(&["message:message-6206-open-ada-51a0"])
        .contains(&[MSG_OPEN_ADA]),
        ("message.edit", "member") => Cell::refuse(
            "inventory",
            "member",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "messageId": MSG_OPEN_BEN_OWN,
                "ciphertextHex": hex_of(&edited_ciphertext_of(MSG_OPEN_BEN_OWN)),
            }),
            REFUSE_ROLE,
            "edits its own message without the send right",
        ),
        ("message.edit", "limited") => Cell::allow(
            "inventory",
            "limited",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": RESTRICTED_CHANNEL,
                "messageId": MSG_LIMITED_CY,
                "ciphertextHex": hex_of(&edited_ciphertext_of(MSG_LIMITED_CY)),
            }),
        )
        .changes(&["message:message-6206-limited-cy-57c0"])
        .contains(&[MSG_LIMITED_CY]),
        ("message.edit", "excluded") => Cell::refuse(
            "inventory",
            "excluded",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "messageId": MSG_OPEN_DEL,
                "ciphertextHex": hex_of(&edited_ciphertext_of(MSG_OPEN_DEL)),
            }),
            REFUSE_MEMBERSHIP,
            "edits its own message after removal",
        ),
        ("message.edit", _) => Cell::refuse(
            "inventory",
            "never",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "messageId": MSG_OPEN_ADA,
                "ciphertextHex": hex_of(&edited_ciphertext_of("never")),
            }),
            REFUSE_MEMBERSHIP,
            "never-member edits a message",
        ),

        // ---- message.delete -------------------------------------------------
        ("message.delete", "owner") => Cell::allow(
            "inventory",
            "owner",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "messageId": MSG_OPEN_BEN_DEL,
            }),
        )
        .changes(&["message:message-6206-open-ben-del-54b2"])
        .contains(&[MSG_OPEN_BEN_DEL]),
        ("message.delete", "member") => Cell::refuse(
            "inventory",
            "member",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "messageId": MSG_OPEN_BEN_OWN,
            }),
            REFUSE_ROLE,
            "deletes its own message without the send right",
        ),
        ("message.delete", "limited") => Cell::allow(
            "inventory",
            "limited",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": RESTRICTED_CHANNEL,
                "messageId": MSG_LIMITED_CY_DEL,
            }),
        )
        .changes(&["message:message-6206-limited-cydel-58c1"])
        .contains(&[MSG_LIMITED_CY_DEL]),
        ("message.delete", "excluded") => Cell::refuse(
            "inventory",
            "excluded",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "messageId": MSG_OPEN_DEL,
            }),
            REFUSE_MEMBERSHIP,
            "deletes its own message after removal",
        ),
        ("message.delete", _) => Cell::refuse(
            "inventory",
            "never",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "messageId": MSG_OPEN_ADA,
            }),
            REFUSE_MEMBERSHIP,
            "never-member deletes a message",
        ),

        // ---- channel.create ---------------------------------------------------
        ("channel.create", "owner") => Cell::allow(
            "inventory",
            "owner",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": NEW_CHANNEL_OWNER,
                "creator": OWNER_NAME,
                "access": "open",
            }),
        )
        .changes(&["channel:channel-6206-new-owner-81a0"])
        .contains(&[NEW_CHANNEL_OWNER]),
        ("channel.create", "member") => Cell::refuse(
            "inventory",
            "member",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": "channel-6206-hostile-member",
                "creator": MEMBER_NAME,
                "access": "open",
            }),
            REFUSE_ROLE,
            "creates a channel without make-channels",
        ),
        ("channel.create", "limited") => Cell::allow(
            "inventory",
            "limited",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": NEW_CHANNEL_LIMITED,
                "creator": LIMITED_NAME,
                "access": "limited",
                "members": [MEMBER_NAME, LIMITED_NAME],
            }),
        )
        .changes(&["channel:channel-6206-new-limited-82c0"])
        .contains(&[NEW_CHANNEL_LIMITED]),
        ("channel.create", "excluded") => Cell::refuse(
            "inventory",
            "excluded",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": "channel-6206-hostile-excluded",
                "creator": EXCLUDED_NAME,
                "access": "open",
            }),
            REFUSE_MEMBERSHIP,
            "creates a channel after removal on a grant that outlived it",
        ),
        ("channel.create", _) => Cell::refuse(
            "inventory",
            "never",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": "channel-6206-hostile-never",
                "creator": NEVER_NAME,
                "access": "open",
            }),
            REFUSE_MEMBERSHIP,
            "never-member creates a channel",
        ),

        // ---- thread.create ----------------------------------------------------
        ("thread.create", "owner") => Cell::allow(
            "inventory",
            "owner",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "threadId": NEW_THREAD_OWNER,
                "creator": OWNER_NAME,
            }),
        )
        .changes(&["thread:thread-6206-new-owner-91a0"])
        .contains(&[NEW_THREAD_OWNER]),
        ("thread.create", "member") => Cell::refuse(
            "inventory",
            "member",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "threadId": "thread-6206-hostile-member",
                "creator": MEMBER_NAME,
            }),
            REFUSE_ROLE,
            "creates a thread without make-channels",
        ),
        ("thread.create", "limited") => Cell::allow(
            "inventory",
            "limited",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": RESTRICTED_CHANNEL,
                "threadId": NEW_THREAD_LIMITED,
                "creator": LIMITED_NAME,
            }),
        )
        .changes(&["thread:thread-6206-new-limited-92c0"])
        .contains(&[NEW_THREAD_LIMITED]),
        ("thread.create", "excluded") => Cell::refuse(
            "inventory",
            "excluded",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "threadId": "thread-6206-hostile-excluded",
                "creator": EXCLUDED_NAME,
            }),
            REFUSE_MEMBERSHIP,
            "creates a thread after removal",
        ),
        ("thread.create", _) => Cell::refuse(
            "inventory",
            "never",
            op,
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "threadId": "thread-6206-hostile-never",
                "creator": NEVER_NAME,
            }),
            REFUSE_MEMBERSHIP,
            "never-member creates a thread",
        ),

        _ => return None,
    };
    Some(cell)
}

fn person_of(label: &str) -> &'static str {
    ROLES
        .iter()
        .find(|(role, _)| *role == label)
        .map(|(_, person)| *person)
        .unwrap_or("")
}

/// `matrix_cell` is written per role label, and the label a cell carries has to
/// be the one that actually sends it; this keeps the two in step for the arms
/// that fall through to a catch-all.
fn leak_label(label: &str) -> &'static str {
    ROLES
        .iter()
        .find(|(role, _)| *role == label)
        .map(|(role, _)| *role)
        .unwrap_or("never")
}

/// Forged parent-binding and author-binding attacks. Each names exactly one
/// binding it lies about, so it is refused by exactly one production guard.
fn binding_attacks() -> Vec<Cell> {
    vec![
        // message.create — parent binding
        Cell::refuse(
            "binding",
            "limited",
            "message.create",
            serde_json::json!({
                "enclaveId": OTHER_ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "messageId": "message-6206-hostile-foreign-enclave",
                "author": LIMITED_NAME,
                "ciphertextHex": hex_of(&ciphertext_of("hostile-foreign-enclave")),
            }),
            REFUSE_PARENT,
            "posts into a different server id",
        ),
        Cell::refuse(
            "binding",
            "limited",
            "message.create",
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "threadId": CHILD_THREAD,
                "messageId": "message-6206-hostile-thread-parent",
                "author": LIMITED_NAME,
                "ciphertextHex": hex_of(&ciphertext_of("hostile-thread-parent")),
            }),
            REFUSE_PARENT,
            "posts into a thread whose real parent is another channel",
        ),
        Cell::refuse(
            "binding",
            "limited",
            "message.create",
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": FOREIGN_CHANNEL,
                "messageId": "message-6206-hostile-foreign-channel",
                "author": LIMITED_NAME,
                "ciphertextHex": hex_of(&ciphertext_of("hostile-foreign-channel")),
            }),
            REFUSE_MEMBERSHIP,
            "posts into a channel that is not in this enclave",
        ),
        // message.create — author binding
        Cell::refuse(
            "binding",
            "limited",
            "message.create",
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "messageId": "message-6206-hostile-forged-author",
                "author": OWNER_NAME,
                "ciphertextHex": hex_of(&ciphertext_of("hostile-forged-author")),
            }),
            REFUSE_AUTHOR,
            "posts under the owner's name",
        ),
        // message.create — restricted-channel send, by the owner, who is not in it
        Cell::refuse(
            "binding",
            "owner",
            "message.create",
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": RESTRICTED_CHANNEL,
                "messageId": "message-6206-hostile-owner-restricted",
                "author": OWNER_NAME,
                "ciphertextHex": hex_of(&ciphertext_of("hostile-owner-restricted")),
            }),
            REFUSE_MEMBERSHIP,
            "sends into a restricted channel it is not in",
        ),
        // message.edit — author binding: the owner rewriting somebody else
        Cell::refuse(
            "binding",
            "owner",
            "message.edit",
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "messageId": MSG_OPEN_BEN_KEEP,
                "ciphertextHex": hex_of(&edited_ciphertext_of("owner-forges-ben")),
            }),
            REFUSE_AUTHOR,
            "edits another author's message",
        ),
        // message.edit — parent binding: own message, wrong channel
        Cell::refuse(
            "binding",
            "limited",
            "message.edit",
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "messageId": MSG_LIMITED_KEEP,
                "ciphertextHex": hex_of(&edited_ciphertext_of("limited-moves-channel")),
            }),
            REFUSE_PARENT,
            "edits its own message under another channel id",
        ),
        // message.delete — author binding: deleting somebody else's message
        Cell::refuse(
            "binding",
            "limited",
            "message.delete",
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "messageId": MSG_OPEN_BEN_KEEP,
            }),
            REFUSE_AUTHOR,
            "deletes another author's message",
        ),
        // message.delete — parent binding: a moderator naming another server
        Cell::refuse(
            "binding",
            "owner",
            "message.delete",
            serde_json::json!({
                "enclaveId": OTHER_ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "messageId": MSG_OPEN_BEN_KEEP,
            }),
            REFUSE_PARENT,
            "deletes under a different server id",
        ),
        // message.delete — parent binding: a moderator naming the wrong channel
        Cell::refuse(
            "binding",
            "owner",
            "message.delete",
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "messageId": MSG_LIMITED_KEEP,
            }),
            REFUSE_PARENT,
            "deletes a restricted-channel message under the open channel id",
        ),
        // channel.create — author and parent binding
        Cell::refuse(
            "binding",
            "limited",
            "channel.create",
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": "channel-6206-hostile-forged-creator",
                "creator": OWNER_NAME,
                "access": "open",
            }),
            REFUSE_AUTHOR,
            "creates a channel attributed to the owner",
        ),
        Cell::refuse(
            "binding",
            "limited",
            "channel.create",
            serde_json::json!({
                "enclaveId": OTHER_ENCLAVE,
                "channelId": "channel-6206-hostile-foreign-enclave",
                "creator": LIMITED_NAME,
                "access": "open",
            }),
            REFUSE_PARENT,
            "creates a channel in a different server id",
        ),
        // thread.create — author and parent binding
        Cell::refuse(
            "binding",
            "limited",
            "thread.create",
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": OPEN_CHANNEL,
                "threadId": "thread-6206-hostile-forged-creator",
                "creator": OWNER_NAME,
            }),
            REFUSE_AUTHOR,
            "creates a thread attributed to the owner",
        ),
        Cell::refuse(
            "binding",
            "limited",
            "thread.create",
            serde_json::json!({
                "enclaveId": OTHER_ENCLAVE,
                "channelId": RESTRICTED_CHANNEL,
                "threadId": "thread-6206-hostile-foreign-enclave",
                "creator": LIMITED_NAME,
            }),
            REFUSE_PARENT,
            "creates a thread in a different server id",
        ),
        Cell::refuse(
            "binding",
            "limited",
            "thread.create",
            serde_json::json!({
                "enclaveId": ENCLAVE,
                "channelId": FOREIGN_CHANNEL,
                "threadId": "thread-6206-hostile-foreign-channel",
                "creator": LIMITED_NAME,
            }),
            REFUSE_MEMBERSHIP,
            "creates a thread under a channel that is not in this enclave",
        ),
    ]
}

/// Two signature attacks, so a refusal cannot be mistaken for a transport that
/// never checked who was asking.
fn signature_attacks() -> Vec<Cell> {
    let mut forged = Cell::refuse(
        "signature",
        "never",
        "message.create",
        serde_json::json!({
            "enclaveId": ENCLAVE,
            "channelId": OPEN_CHANNEL,
            "messageId": "message-6206-hostile-forged-key",
            "author": OWNER_NAME,
            "ciphertextHex": hex_of(&ciphertext_of("hostile-forged-key")),
        }),
        REFUSE_BAD_SIGNATURE,
        "presents the owner's public key",
    );
    // The deployed service attributes this request to whoever owns the key it
    // was shown, so its journal line is the owner's — and its signature still
    // does not check out.
    forged.forge_key_of = Some("owner");
    forged.audit_label = "owner";
    let mut tampered = Cell::refuse(
        "signature",
        "excluded",
        "message.delete",
        serde_json::json!({
            "enclaveId": ENCLAVE,
            "channelId": OPEN_CHANNEL,
            "messageId": MSG_OPEN_BEN_KEEP,
        }),
        REFUSE_BAD_SIGNATURE,
        "rewrites the body after signing it",
    );
    tampered.tamper = true;
    vec![forged, tampered]
}

// ---------------------------------------------------------------------------

fn body_of(raw: &[u8]) -> &[u8] {
    raw.windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| &raw[index + 4..])
        .unwrap_or(&[])
}

fn count_occurrences(haystack: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() || haystack.len() < needle.len() {
        return 0;
    }
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

fn ids_from(value: &serde_json::Value, field: &str) -> Vec<String> {
    let Some(array) = value.get(field).and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    array
        .iter()
        .map(|item| match item {
            serde_json::Value::String(text) => text.clone(),
            other => other
                .get("channelId")
                .or_else(|| other.get("messageId"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        })
        .collect()
}

fn wait_for_port(ready_file: &Path) -> Option<u16> {
    for _ in 0..600 {
        if let Ok(text) = std::fs::read_to_string(ready_file) {
            if let Ok(port) = text.trim().parse::<u16>() {
                if port != 0 {
                    return Some(port);
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    None
}

fn healthz(port: u16) -> String {
    let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) else {
        return String::new();
    };
    let request =
        format!("GET /healthz HTTP/1.1\r\nhost: 127.0.0.1:{port}\r\nconnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return String::new();
    }
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    String::from_utf8_lossy(&raw).into_owned()
}

fn copy_binary(from: &str, to: &Path) {
    std::fs::copy(from, to).unwrap_or_else(|error| panic!("cannot install {from}: {error}"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(to, std::fs::Permissions::from_mode(0o755));
    }
}

/// One sample of the deployed service's own state, read straight off its data
/// directory. Nothing here comes out of a response.
#[derive(Clone, Default)]
struct ServiceState {
    records: BTreeMap<String, String>,
    queue_bytes: usize,
    queue_lines: usize,
    broadcast_bytes: usize,
    broadcast_lines: usize,
}

fn read_service_state(data_dir: &Path) -> ServiceState {
    let mut records = BTreeMap::new();
    if let Ok(text) = std::fs::read_to_string(data_dir.join("store.json")) {
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(&text)
        {
            for (key, value) in map {
                records.insert(key, serde_json::to_string(&value).unwrap_or_default());
            }
        }
    }
    let queue = std::fs::read_to_string(data_dir.join("queue.jsonl")).unwrap_or_default();
    let broadcast = std::fs::read_to_string(data_dir.join("broadcast.jsonl")).unwrap_or_default();
    ServiceState {
        records,
        queue_bytes: queue.len(),
        queue_lines: queue.lines().filter(|line| !line.trim().is_empty()).count(),
        broadcast_bytes: broadcast.len(),
        broadcast_lines: broadcast
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count(),
    }
}

/// Record identifiers whose bytes differ between two samples, in either
/// direction — appeared, vanished, or changed.
fn changed_records(before: &ServiceState, after: &ServiceState) -> Vec<String> {
    let mut ids: BTreeSet<String> = BTreeSet::new();
    for (id, bytes) in &after.records {
        if before.records.get(id) != Some(bytes) {
            ids.insert(id.clone());
        }
    }
    for id in before.records.keys() {
        if !after.records.contains_key(id) {
            ids.insert(id.clone());
        }
    }
    ids.into_iter().collect()
}

fn manifest_ops(document: &serde_json::Value) -> Vec<serde_json::Value> {
    document
        .get("routes")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn field(row: &serde_json::Value, key: &str) -> String {
    row.get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn roles_of(row: &serde_json::Value) -> Vec<String> {
    row.get("allowedRoles")
        .and_then(serde_json::Value::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------

#[test]
fn task_6206_every_deployed_chats_content_write_is_authorized() {
    let build_tag = std::env::var("TASK6206_BUILD_TAG").unwrap_or_else(|_| "production".to_owned());
    let starve_role = std::env::var("TASK6206_STARVE_ROLE").unwrap_or_default();
    let starve_op = std::env::var("TASK6206_STARVE_OP").unwrap_or_default();
    let starve_manifest_row = std::env::var("TASK6206_STARVE_MANIFEST_ROW").unwrap_or_default();
    let starve_router_row = std::env::var("TASK6206_STARVE_ROUTER_ROW").unwrap_or_default();
    let starve_target = std::env::var("TASK6206_STARVE_TARGET").unwrap_or_default();
    let starve_keep = std::env::var("TASK6206_STARVE_KEEP").unwrap_or_default();
    let starve_authorized = std::env::var("TASK6206_STARVE_AUTHORIZED").is_ok();
    let starve_hostile = std::env::var("TASK6206_STARVE_HOSTILE").is_ok();
    let starve_observe = std::env::var("TASK6206_STARVE_OBSERVE").is_ok();
    let starve_bytes = std::env::var("TASK6206_STARVE_BYTES").is_ok();

    let mut failures: Vec<String> = Vec::new();

    let run_dir =
        std::env::temp_dir().join(format!("task6206-{build_tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&run_dir);
    std::fs::create_dir_all(&run_dir).expect("run dir");

    // ---- deploy the shipping binaries into a throwaway install prefix ----
    let deploy_bin = run_dir.join("deploy/bin");
    let client_bin_dir = run_dir.join("install/bin");
    std::fs::create_dir_all(&deploy_bin).expect("deploy dir");
    std::fs::create_dir_all(&client_bin_dir).expect("client dir");
    let service_path = deploy_bin.join("osl-chats-service");
    let client_path = client_bin_dir.join("osl-chats-client");
    copy_binary(env!("CARGO_BIN_EXE_osl-chats-service"), &service_path);
    copy_binary(env!("CARGO_BIN_EXE_osl-chats-client"), &client_path);

    // ---- install five independently keyed clients ----
    let mut public_keys: BTreeMap<&str, String> = BTreeMap::new();
    let mut install_dirs: BTreeMap<&str, PathBuf> = BTreeMap::new();
    let mut manifests: BTreeMap<&str, serde_json::Value> = BTreeMap::new();
    for (label, person) in ROLES {
        let install = run_dir.join(format!("install/{label}"));
        std::fs::create_dir_all(&install).expect("install dir");
        let output = Command::new(&client_path)
            .args([
                "--generate",
                "--install-dir",
                install.to_str().unwrap(),
                "--label",
                label,
                "--person",
                person,
            ])
            .output()
            .expect("client install");
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        let key = text
            .split_whitespace()
            .find_map(|token| token.strip_prefix("public="))
            .unwrap_or_default()
            .to_owned();
        if key.is_empty() {
            failures.push(format!("install produced no key for role {label}"));
        }
        let manifest: serde_json::Value = std::fs::read_to_string(install.join("route-manifest.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(serde_json::Value::Null);
        println!(
            "TASK6206 INSTALL label={label} person={person} public={key} manifest_routes={}",
            manifest_ops(&manifest).len()
        );
        public_keys.insert(label, key);
        install_dirs.insert(label, install);
        manifests.insert(label, manifest);
    }
    let distinct_keys: BTreeSet<&String> = public_keys.values().collect();
    if distinct_keys.len() != ROLES.len() {
        failures.push(format!(
            "installed clients are not independently keyed: {} distinct keys for {} roles",
            distinct_keys.len(),
            ROLES.len()
        ));
    }

    // ---- the installed client's route manifest, from the install itself ----
    let owner_manifest = manifests.get("owner").cloned().unwrap_or(serde_json::Value::Null);
    let mut manifest_rows = manifest_ops(&owner_manifest);
    if manifest_rows.is_empty() {
        failures.push(
            "the installed client shipped no route manifest, so the deployed inventory is unclassified"
                .to_owned(),
        );
    }
    for (label, manifest) in &manifests {
        if manifest_ops(manifest) != manifest_ops(&owner_manifest) {
            failures.push(format!(
                "the {label} install's route manifest differs from the owner install's"
            ));
        }
    }
    if !starve_manifest_row.is_empty() {
        manifest_rows.retain(|row| field(row, "op") != starve_manifest_row);
    }

    // ---- freeze the fixture and seed the deployed service from it ----
    let identity = |label: &str,
                    person: &str,
                    on_roster: bool,
                    excluded: bool,
                    owner: bool,
                    rights: &[&str]| {
        serde_json::json!({
            "label": label,
            "personName": person,
            "publicKeyB64": public_keys.get(label).cloned().unwrap_or_default(),
            "onRoster": on_roster,
            "excluded": excluded,
            "owner": owner,
            "rights": rights,
        })
    };
    let message = |id: &str, channel: &str, thread: Option<&str>, author: &str| {
        let mut value = serde_json::json!({
            "messageId": id,
            "channelId": channel,
            "author": author,
            "ciphertextHex": hex_of(&ciphertext_of(id)),
        });
        if let Some(thread) = thread {
            value["threadId"] = serde_json::json!(thread);
        }
        value
    };
    let fixture = serde_json::json!({
        "enclaveId": ENCLAVE,
        "ownerName": OWNER_NAME,
        "joinedAt": "2026-08-13T00:00:00Z",
        // The ordinary member is read-only and the removed member keeps the
        // grants they had. Both are deliberate: they are what make the role
        // guard and the membership guard each the single reason a request is
        // refused, so bypassing one of them in a throwaway build leaks.
        "identities": [
            identity("owner", OWNER_NAME, true, false, true,
                &["read", "send", "invite", "make-channels", "remove-messages", "remove-people", "change-server"]),
            identity("member", MEMBER_NAME, true, false, false, &["read"]),
            identity("limited", LIMITED_NAME, true, false, false, &["read", "send", "make-channels"]),
            identity("excluded", EXCLUDED_NAME, false, true, false,
                &["read", "send", "make-channels", "remove-messages"]),
            identity("never", NEVER_NAME, false, false, false, &[] as &[&str]),
        ],
        "retainRightsAfterExclusion": true,
        "openChannelId": OPEN_CHANNEL,
        "restrictedChannelId": RESTRICTED_CHANNEL,
        "restrictedPersonNames": [MEMBER_NAME, LIMITED_NAME],
        "openThreadId": OPEN_THREAD,
        "childThreadId": CHILD_THREAD,
        "messages": [
            message(MSG_OPEN_ADA, OPEN_CHANNEL, None, OWNER_NAME),
            message(MSG_OPEN_BEN_KEEP, OPEN_CHANNEL, None, MEMBER_NAME),
            message(MSG_OPEN_BEN_OWN, OPEN_CHANNEL, None, MEMBER_NAME),
            message(MSG_OPEN_BEN_DEL, OPEN_CHANNEL, None, MEMBER_NAME),
            message(MSG_OPEN_DEL, OPEN_CHANNEL, None, EXCLUDED_NAME),
            message(MSG_OPENTHREAD, OPEN_CHANNEL, Some(OPEN_THREAD), OWNER_NAME),
            message(MSG_LIMITED_CY, RESTRICTED_CHANNEL, None, LIMITED_NAME),
            message(MSG_LIMITED_CY_DEL, RESTRICTED_CHANNEL, None, LIMITED_NAME),
            message(MSG_LIMITED_KEEP, RESTRICTED_CHANNEL, None, LIMITED_NAME),
            message(MSG_CHILDTHREAD, RESTRICTED_CHANNEL, Some(CHILD_THREAD), LIMITED_NAME),
        ],
    });
    let fixture_path = run_dir.join("deploy/fixture.json");
    std::fs::write(
        &fixture_path,
        serde_json::to_vec_pretty(&fixture).expect("fixture"),
    )
    .expect("write fixture");
    println!(
        "TASK6206 FROZEN enclave={ENCLAVE} other_enclave={OTHER_ENCLAVE} open={OPEN_CHANNEL} \
restricted={RESTRICTED_CHANNEL} open_thread={OPEN_THREAD} child_thread={CHILD_THREAD} \
targets={} keeps={} forbidden_records={}",
        FROZEN_TARGETS.len(),
        KEEP_RECORDS.len(),
        FORBIDDEN_RECORD_IDS.len()
    );

    let data_dir = run_dir.join("deploy/data");
    let ready_file = run_dir.join("deploy/port");
    let mut service = Command::new(&service_path)
        .args([
            "--data-dir",
            data_dir.to_str().unwrap(),
            "--fixture",
            fixture_path.to_str().unwrap(),
            "--ready-file",
            ready_file.to_str().unwrap(),
            "--build-tag",
            &build_tag,
        ])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("deploy service");

    let port = match wait_for_port(&ready_file) {
        Some(port) => port,
        None => {
            let _ = service.kill();
            println!("TASK6206 FAIL deployed service never bound a port");
            let _ = std::io::stdout().flush();
            std::process::exit(1);
        }
    };
    let health = healthz(port);
    if !health.contains("200") || !health.contains(&format!("\"tag\":\"{build_tag}\"")) {
        failures.push(format!(
            "deployed service health probe did not answer for build {build_tag}: {}",
            health.replace('\n', " ").replace('\r', "")
        ));
    }
    println!(
        "TASK6206 DEPLOYED port={port} tag={build_tag} install={}",
        service_path.display()
    );

    // ---- the deployed service's own router, read off its data directory ----
    let router_document: serde_json::Value = if starve_observe {
        serde_json::Value::Null
    } else {
        std::fs::read_to_string(data_dir.join("router.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(serde_json::Value::Null)
    };
    let mut router_rows = manifest_ops(&router_document);
    if !starve_router_row.is_empty() {
        router_rows.retain(|row| field(row, "op") != starve_router_row);
    }
    let authority_ops: Vec<String> = router_document
        .get("authorityOps")
        .and_then(serde_json::Value::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if router_rows.is_empty() {
        failures.push(
            "independent raw read is missing: the deployed service's router was not read"
                .to_owned(),
        );
    }

    // ---- manifest vs router, one for one ----
    let manifest_set: BTreeSet<String> = manifest_rows.iter().map(|row| field(row, "op")).collect();
    let router_set: BTreeSet<String> = router_rows.iter().map(|row| field(row, "op")).collect();
    let authority_set: BTreeSet<String> = authority_ops.iter().cloned().collect();
    for op in &router_set {
        if !manifest_set.contains(op) {
            failures.push(format!(
                "unclassified deployed route: the deployed router exposes {op} but the installed client's route manifest does not classify it"
            ));
        }
    }
    for op in &manifest_set {
        if !router_set.contains(op) {
            failures.push(format!(
                "the installed client's route manifest classifies {op} but the deployed router does not expose it"
            ));
        }
    }
    if !authority_set.is_empty() {
        for op in &authority_set {
            if !router_set.contains(op) {
                failures.push(format!(
                    "unclassified deployed route: the deployed authority answers {op} but no router row reaches it"
                ));
            }
        }
        for op in &router_set {
            if !authority_set.contains(op) {
                failures.push(format!(
                    "the deployed router exposes {op} but the deployed authority does not answer it"
                ));
            }
        }
    }
    // Field-for-field, not just by name: a row that agrees on the op but lies
    // about the binding or the allowed roles is still a mismatch.
    for router_row in &router_rows {
        let op = field(router_row, "op");
        let Some(manifest_row) = manifest_rows.iter().find(|row| field(row, "op") == op) else {
            continue;
        };
        for key in ["method", "path", "kind", "parentBinding", "authorBinding"] {
            if field(router_row, key) != field(manifest_row, key) {
                failures.push(format!(
                    "inventory row {op} disagrees on {key}: router={:?} manifest={:?}",
                    field(router_row, key),
                    field(manifest_row, key)
                ));
            }
        }
        if roles_of(router_row) != roles_of(manifest_row) {
            failures.push(format!(
                "inventory row {op} disagrees on allowedRoles: router={:?} manifest={:?}",
                roles_of(router_row),
                roles_of(manifest_row)
            ));
        }
    }
    let content_write_rows: Vec<String> = manifest_rows
        .iter()
        .filter(|row| field(row, "kind") == "content-write")
        .map(|row| field(row, "op"))
        .collect();
    if content_write_rows.is_empty() {
        failures.push(
            "the inventory contains no content-write endpoint, so this matrix is read-only"
                .to_owned(),
        );
    }
    println!(
        "TASK6206 INVENTORY manifest_rows={} router_rows={} authority_ops={} content_write_rows={} ({})",
        manifest_set.len(),
        router_set.len(),
        authority_set.len(),
        content_write_rows.len(),
        content_write_rows.join(",")
    );

    // ---- build the matrix from the inventory rows, one cell per role ----
    let mut cells: Vec<Cell> = Vec::new();
    for row in &manifest_rows {
        let op = field(row, "op");
        let allowed = roles_of(row);
        for (label, _) in ROLES {
            match matrix_cell(&op, label) {
                Some(cell) => {
                    let should_allow = allowed.iter().any(|role| role == label);
                    if should_allow == cell.hostile() {
                        failures.push(format!(
                            "inventory row {op} claims allowedRoles={allowed:?} but the matrix treats the {label} role as {}",
                            if cell.hostile() { "hostile" } else { "authorized" }
                        ));
                    }
                    cells.push(cell);
                }
                None => failures.push(format!(
                    "unclassified deployed route: no request is planned for the {op} endpoint from the {label} role"
                )),
            }
        }
    }
    cells.extend(binding_attacks());
    cells.extend(signature_attacks());
    for cell in &cells {
        if !manifest_set.contains(&cell.op) {
            failures.push(format!(
                "the matrix exercises {} which is not an inventory row",
                cell.op
            ));
        }
    }

    if !starve_role.is_empty() {
        cells.retain(|cell| cell.label != starve_role);
    }
    if !starve_op.is_empty() {
        cells.retain(|cell| cell.op != starve_op);
    }
    if starve_authorized {
        cells.retain(Cell::hostile);
    }
    if starve_hostile {
        cells.retain(|cell| !cell.hostile());
    }

    // ---- unknown-route probes: nothing outside the router is reachable ----
    let unknown_probes = [
        "/v1/chats/message.purge",
        "/v1/chats/roster.remove",
        "/v1/admin/rights.set",
    ];

    struct Observed {
        cell: Cell,
        raw: Vec<u8>,
        status: String,
        changed: Vec<String>,
        queue_growth: usize,
        broadcast_growth: usize,
        queue_line_growth: usize,
        broadcast_line_growth: usize,
    }

    let replies_dir = run_dir.join("replies");
    std::fs::create_dir_all(&replies_dir).expect("replies dir");
    let mut observed: Vec<Observed> = Vec::new();

    // The deployed service's state as seeded, before any request.
    let seeded_state = read_service_state(&data_dir);
    if seeded_state.records.is_empty() && !starve_observe {
        failures.push(
            "independent raw read is missing: the deployed service's durable record store was not read"
                .to_owned(),
        );
    }
    let mut previous_state = seeded_state.clone();

    let run_request = |cell: &Cell, index: usize, previous: &ServiceState| -> (Observed, ServiceState) {
        let body_path = replies_dir.join(format!("req-{index:03}.json"));
        std::fs::write(
            &body_path,
            serde_json::to_vec(&cell.body).expect("request body"),
        )
        .expect("write body");
        let out_path = replies_dir.join(format!("rsp-{index:03}.bin"));
        let install = install_dirs.get(cell.label).expect("install dir").clone();
        let nonce = format!("n{index:04}");
        let base = format!("127.0.0.1:{port}");

        let mut command = Command::new(&client_path);
        command.args([
            "--install-dir",
            install.to_str().unwrap(),
            "--base",
            &base,
            "--op",
            &cell.op,
            "--nonce",
            &nonce,
            "--body",
            body_path.to_str().unwrap(),
        ]);
        if !starve_bytes {
            command.args(["--out", out_path.to_str().unwrap()]);
        }
        if let Some(forged) = cell.forge_key_of {
            command.args(["--forge-public", public_keys.get(forged).unwrap()]);
        }
        if cell.tamper {
            command.arg("--tamper");
        }
        if let Some(path) = &cell.path_override {
            command.args(["--path", path]);
        }
        let output = command.output().expect("installed client run");
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let status = stdout
            .split_whitespace()
            .find_map(|token| token.strip_prefix("status="))
            .unwrap_or_default()
            .to_owned();
        let raw = std::fs::read(&out_path).unwrap_or_default();

        // Read the deployed service's own state again, straight off its data
        // directory, before the next request is sent.
        let after = if starve_observe {
            previous.clone()
        } else {
            read_service_state(&data_dir)
        };
        let item = Observed {
            cell: cell.clone(),
            raw,
            status,
            changed: changed_records(previous, &after),
            queue_growth: after.queue_bytes.saturating_sub(previous.queue_bytes),
            broadcast_growth: after.broadcast_bytes.saturating_sub(previous.broadcast_bytes),
            queue_line_growth: after.queue_lines.saturating_sub(previous.queue_lines),
            broadcast_line_growth: after
                .broadcast_lines
                .saturating_sub(previous.broadcast_lines),
        };
        (item, after)
    };

    for (index, cell) in cells.iter().enumerate() {
        let (item, after) = run_request(cell, index, &previous_state);
        previous_state = after;
        observed.push(item);
    }

    // ---- unknown-route probes ----
    let mut unknown_route_refusals = 0usize;
    for (index, path) in unknown_probes.iter().enumerate() {
        let body_path = replies_dir.join(format!("probe-{index}.json"));
        std::fs::write(&body_path, b"{}").expect("probe body");
        let out_path = replies_dir.join(format!("probe-{index}.bin"));
        let install = install_dirs.get("owner").expect("install dir").clone();
        let output = Command::new(&client_path)
            .args([
                "--install-dir",
                install.to_str().unwrap(),
                "--base",
                &format!("127.0.0.1:{port}"),
                "--op",
                "history.read",
                "--nonce",
                &format!("probe{index}"),
                "--body",
                body_path.to_str().unwrap(),
                "--out",
                out_path.to_str().unwrap(),
                "--path",
                path,
            ])
            .output()
            .expect("probe run");
        let _ = output;
        let raw = std::fs::read(&out_path).unwrap_or_default();
        let text = String::from_utf8_lossy(&raw).into_owned();
        if text.contains(" 404 ") && text.contains("unknown-route") {
            unknown_route_refusals += 1;
        } else {
            failures.push(format!(
                "the deployed service answered the unclassified route {path}: {}",
                text.replace('\n', " ").replace('\r', "")
            ));
        }
    }

    // ---- stop the deployment, then read its own record off disk ----
    let _ = service.kill();
    let _ = service.wait();

    let final_state = if starve_observe {
        ServiceState::default()
    } else {
        read_service_state(&data_dir)
    };
    let audit_lines: Vec<serde_json::Value> = if starve_observe {
        Vec::new()
    } else {
        std::fs::read_to_string(data_dir.join("audit.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    };
    if audit_lines.is_empty() {
        failures.push(
            "independent raw read is missing: the deployed service's audit journal was not read"
                .to_owned(),
        );
    }
    if final_state.records.is_empty() {
        failures.push(
            "independent raw read is missing: the deployed service's durable record store was not read at the end of the run"
                .to_owned(),
        );
    }

    // ---- per-request judgement ----
    let mut authorized = 0usize;
    let mut hostile = 0usize;
    let mut hostile_response_bytes = 0usize;
    let mut hostile_durable_changes = 0usize;
    let mut hostile_queue_bytes = 0usize;
    let mut hostile_broadcast_bytes = 0usize;
    let mut authorized_queue_lines = 0usize;
    let mut authorized_broadcast_lines = 0usize;
    let mut scoped_leaks = 0usize;
    let mut seen_roles: BTreeSet<&str> = BTreeSet::new();
    let mut authorized_ops: BTreeSet<String> = BTreeSet::new();
    let mut hostile_ops: BTreeSet<String> = BTreeSet::new();
    let mut claimed_targets: BTreeSet<String> = BTreeSet::new();

    for (index, item) in observed.iter().enumerate() {
        let cell = &item.cell;
        seen_roles.insert(cell.label);
        if item.raw.is_empty() {
            failures.push(format!(
                "request #{index} ({} {}) captured no response bytes from the deployed service",
                cell.label, cell.op
            ));
            continue;
        }
        let body = body_of(&item.raw);
        let parsed: serde_json::Value =
            serde_json::from_slice(body).unwrap_or(serde_json::Value::Null);
        let ok = parsed
            .get("ok")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let code = parsed
            .get("code")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();

        if cell.hostile() {
            hostile += 1;
            hostile_ops.insert(cell.op.clone());
            hostile_response_bytes += item.raw.len();
            let expected = cell.expect_code.unwrap_or_default();
            if ok || code != expected {
                failures.push(format!(
                    "hostile request was not refused: endpoint={} identity=\"{}\"[{}] binding={{{}}} state={{records={:?} queue=+{} broadcast=+{}}} status={} code={} note={}",
                    cell.op,
                    person_of(cell.label),
                    cell.label,
                    cell.binding(),
                    item.changed,
                    item.queue_growth,
                    item.broadcast_growth,
                    item.status,
                    code,
                    cell.note
                ));
            }
            if !item.changed.is_empty()
                || item.queue_growth > 0
                || item.broadcast_growth > 0
            {
                hostile_durable_changes += item.changed.len();
                hostile_queue_bytes += item.queue_growth;
                hostile_broadcast_bytes += item.broadcast_growth;
                failures.push(format!(
                    "hostile write reached deployed state: endpoint={} identity=\"{}\"[{}] binding={{{}}} state={{records={:?} queue=+{} bytes broadcast=+{} bytes}} note={}",
                    cell.op,
                    person_of(cell.label),
                    cell.label,
                    cell.binding(),
                    item.changed,
                    item.queue_growth,
                    item.broadcast_growth,
                    cell.note
                ));
            }
        } else {
            authorized += 1;
            authorized_ops.insert(cell.op.clone());
            authorized_queue_lines += item.queue_line_growth;
            authorized_broadcast_lines += item.broadcast_line_growth;
            if !ok || item.status != "200" {
                failures.push(format!(
                    "authorized control {} by {} did not succeed: status={} body={}",
                    cell.op,
                    cell.label,
                    item.status,
                    String::from_utf8_lossy(body)
                ));
            }
            let mut expected: Vec<String> = cell.changes.clone();
            expected.sort();
            let mut got = item.changed.clone();
            got.sort();
            if expected != got {
                failures.push(format!(
                    "authorized control {} by {} changed {got:?}, its frozen target set is {expected:?}",
                    cell.op, cell.label
                ));
            }
            claimed_targets.extend(cell.changes.iter().cloned());
            if let Some((field_name, want)) = &cell.exact {
                let have = ids_from(&parsed, field_name);
                if &have != want {
                    failures.push(format!(
                        "authorized control {} by {} returned {field_name}={have:?}, frozen value is {want:?}",
                        cell.op, cell.label
                    ));
                }
            }
            for token in &cell.contains {
                if count_occurrences(&item.raw, token.as_bytes()) == 0 {
                    failures.push(format!(
                        "authorized control {} by {} did not return frozen value {token}",
                        cell.op, cell.label
                    ));
                }
            }
            for token in &cell.absent {
                let hits = count_occurrences(&item.raw, token.as_bytes());
                if hits > 0 {
                    scoped_leaks += hits;
                    failures.push(format!(
                        "scoped answer to {} by {} leaked {token}",
                        cell.op, cell.label
                    ));
                }
            }
        }
        println!(
            "TASK6206 REQ #{index:03} phase={} label={} op={} kind={} status={} bytes={} records={} queue=+{} broadcast=+{}",
            cell.phase,
            cell.label,
            cell.op,
            if cell.hostile() { "hostile" } else { "authorized" },
            item.status,
            item.raw.len(),
            item.changed.len(),
            item.queue_line_growth,
            item.broadcast_line_growth
        );
    }

    // ---- the service's own journal, matched against what the clients saw ----
    let mut audit_allowed = 0usize;
    let mut audit_denied = 0usize;
    if !audit_lines.is_empty() {
        let expected_requests = observed.len() + unknown_probes.len();
        // Unknown routes never reach the authority, so they are not journalled.
        if audit_lines.len() != observed.len() {
            failures.push(format!(
                "the deployed service journalled {} requests but {} were sent to a router row (of {expected_requests} total)",
                audit_lines.len(),
                observed.len()
            ));
        }
        for (index, entry) in audit_lines.iter().enumerate() {
            let decision = entry
                .get("decision")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if decision == "allow" {
                audit_allowed += 1;
            } else {
                audit_denied += 1;
            }
            let Some(item) = observed.get(index) else {
                continue;
            };
            let cell = &item.cell;
            let label = entry
                .get("clientLabel")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let op = entry
                .get("op")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let code = entry
                .get("code")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let tag = entry
                .get("buildTag")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if tag != build_tag {
                failures.push(format!(
                    "journal entry {index} came from build {tag}, not the deployed build {build_tag}"
                ));
            }
            if label != cell.audit_label || op != cell.op {
                failures.push(format!(
                    "journal entry {index} is {label}/{op}, the client sent {}/{}",
                    cell.audit_label, cell.op
                ));
            }
            if cell.hostile() == (decision == "allow") {
                failures.push(format!(
                    "the deployed service recorded {decision} for {}/{} but the request was {}",
                    cell.label,
                    cell.op,
                    if cell.hostile() { "forbidden" } else { "authorized" }
                ));
            }
            if let Some(expected) = cell.expect_code {
                if code != expected {
                    failures.push(format!(
                        "the deployed service recorded refusal {code} for {}/{}, expected {expected}",
                        cell.label, cell.op
                    ));
                }
            }
        }
    }

    // ---- frozen targets and keep records, from the service's own store ----
    let mut frozen_targets: Vec<String> = FROZEN_TARGETS.iter().map(|id| (*id).to_owned()).collect();
    if !starve_target.is_empty() {
        frozen_targets.retain(|id| id != &starve_target);
    }
    let mut keep_records: Vec<String> = KEEP_RECORDS.iter().map(|id| (*id).to_owned()).collect();
    if !starve_keep.is_empty() {
        keep_records.retain(|id| id != &starve_keep);
    }
    let frozen_set: BTreeSet<&String> = frozen_targets.iter().collect();
    let keep_set: BTreeSet<&String> = keep_records.iter().collect();

    let full_run = !starve_authorized && !starve_hostile && starve_op.is_empty() && starve_role.is_empty();
    if full_run {
        for target in &frozen_targets {
            if !claimed_targets.contains(target) {
                failures.push(format!(
                    "no authorized control changed the frozen target {target}"
                ));
            }
        }
    }
    for target in &claimed_targets {
        if !frozen_set.contains(target) {
            failures.push(format!(
                "an authorized control changed {target}, which is not a declared frozen target"
            ));
        }
    }

    let mut keep_identical = 0usize;
    if !seeded_state.records.is_empty() && !final_state.records.is_empty() {
        // Every record the deployed service was seeded with has to be
        // classified: either something an authorized control is allowed to
        // change, or something nothing in this run may touch.
        for id in seeded_state.records.keys() {
            if !frozen_set.contains(id) && !keep_set.contains(id) {
                failures.push(format!(
                    "durable record {id} is neither a declared frozen target nor a declared keep record"
                ));
            }
        }
        for id in &keep_records {
            match (seeded_state.records.get(id), final_state.records.get(id)) {
                (Some(before), Some(after)) if before == after => keep_identical += 1,
                (Some(before), Some(after)) => failures.push(format!(
                    "keep record {id} is not byte-identical: seeded={before} final={after}"
                )),
                (Some(_), None) => {
                    failures.push(format!("keep record {id} was removed from the deployed store"))
                }
                _ => failures.push(format!(
                    "keep record {id} was never present in the deployed store"
                )),
            }
        }
        for id in &frozen_targets {
            if !final_state.records.contains_key(id) {
                failures.push(format!(
                    "frozen target {id} is not in the deployed store at the end of the run"
                ));
            }
        }
        for id in FORBIDDEN_RECORD_IDS {
            if final_state.records.contains_key(id) {
                failures.push(format!(
                    "a hostile write created the forbidden durable record {id}"
                ));
            }
        }
    }

    // ---- starvation: nothing may be missing from the run ----
    for (label, _) in ROLES {
        if !seen_roles.contains(label) {
            failures.push(format!("no request was sent from the {label} role"));
        }
    }
    for op in &manifest_set {
        if !authorized_ops.contains(op) {
            failures.push(format!("no authorized control exercised the {op} endpoint"));
        }
        if !hostile_ops.contains(op) {
            failures.push(format!("no hostile request exercised the {op} endpoint"));
        }
    }
    // The exercised inventory is the set of rows that got both an authorized
    // control and a hostile request; it has to be the manifest set exactly.
    let exercised: BTreeSet<String> = authorized_ops.intersection(&hostile_ops).cloned().collect();
    for op in exercised.difference(&manifest_set) {
        failures.push(format!(
            "the run exercised {op}, which is not an inventory row"
        ));
    }
    if authorized == 0 {
        failures.push("no authorized control ran".to_owned());
    }
    if hostile == 0 {
        failures.push("no hostile request ran".to_owned());
    }
    if hostile_response_bytes == 0 {
        failures.push(
            "no hostile response bytes were observed, so the zero counts prove nothing".to_owned(),
        );
    }
    if authorized_queue_lines == 0 || authorized_broadcast_lines == 0 {
        failures.push(format!(
            "the deployed delivery queue and broadcast never moved for an authorized write ({authorized_queue_lines} queued, {authorized_broadcast_lines} broadcast), so refusing to move for a hostile one proves nothing"
        ));
    }

    println!(
        "TASK6206 SUMMARY clients={} distinct_keys={} manifest_rows={} router_rows={} authority_ops={} \
exercised_rows={} content_write_rows={} authorized_controls={authorized} hostile_requests={hostile} \
hostile_durable_changes={hostile_durable_changes} hostile_queue_bytes={hostile_queue_bytes} \
hostile_broadcast_bytes={hostile_broadcast_bytes} hostile_response_bytes={hostile_response_bytes} \
authorized_queue_lines={authorized_queue_lines} authorized_broadcast_lines={authorized_broadcast_lines} \
scoped_leaks={scoped_leaks} audit_allowed={audit_allowed} audit_denied={audit_denied} \
frozen_targets={} keep_records={} keep_identical={keep_identical} forbidden_records_present=0 \
unknown_route_refusals={unknown_route_refusals} roles={} run_dir={}",
        ROLES.len(),
        distinct_keys.len(),
        manifest_set.len(),
        router_set.len(),
        authority_set.len(),
        exercised.len(),
        content_write_rows.len(),
        frozen_targets.len(),
        keep_records.len(),
        seen_roles.len(),
        run_dir.display()
    );

    if !failures.is_empty() {
        for failure in &failures {
            println!("TASK6206 FAIL {failure}");
        }
        println!("TASK6206 RESULT fail failures={}", failures.len());
        let _ = std::io::stdout().flush();
        std::process::exit(1);
    }
    println!(
        "TASK6206 RESULT pass build={build_tag} rows={} authorized={authorized} hostile={hostile} \
hostile_durable_changes=0 hostile_queue_bytes=0 hostile_broadcast_bytes=0 keep_identical={keep_identical}",
        manifest_set.len()
    );
    let _ = std::io::stdout().flush();
}
