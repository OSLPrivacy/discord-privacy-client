//! The deployed OSL Chats authorization authority.
//!
//! Everything OSL Chats refuses has, until now, been refused on the client:
//! `spaces.rs` refuses a self-permission raise on the screen that asked for it,
//! and `server_membership.rs` computes who may read a channel or an inheriting
//! child thread. A cooperative client obeys those answers. A hostile one
//! simply does not call them.
//!
//! This module is the server half. It is the single authorization surface the
//! deployed OSL Chats service consults before it emits one byte of roster,
//! channel, thread, or message ciphertext, and it derives every answer from
//! the shipping permission types rather than from a second, parallel rule set:
//!
//! * roster facts come from [`ServerMembershipStore`] / [`ServerMemberList`],
//! * a channel's reader set comes from
//!   [`ServerChannelAccessRecord::effective_readers`] (the open-vs-limited rule),
//! * a child thread's reader set comes from
//!   [`ServerThreadPermissionStore::effective_thread_readers`], which resolves
//!   through the parent channel, so a thread never widens its channel,
//! * a person's rights come from [`ServerPermissionStore`].
//!
//! Enforcement is concentrated in five named guards — [`self_role_raise_allowed`],
//! [`directory_access_allowed`], [`restricted_history_allowed`],
//! [`child_thread_inheritance_allowed`] and [`known_id_fetch_allowed`]. Each is
//! the only place its rule is applied, so bypassing one bypasses exactly one
//! production check and nothing else.
//!
//! Refusals deliberately carry no names and no identifiers. A hostile client
//! learns that it was refused and nothing more: a refusal that echoed the
//! channel it asked for would be an existence oracle, and a refusal that named
//! a person would leak roster membership to somebody who is not on the roster.
//! The person-facing "X is not allowed to Y" sentence is rendered by the
//! client, which already knows who X is.

use std::collections::{BTreeMap, BTreeSet};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::server_membership::{
    ServerChannelAccess, ServerChannelAccessRecord, ServerMemberList, ServerMembershipStore,
    ServerPermission, ServerPermissionStore, ServerThreadPermissionStore,
};

// ---------------------------------------------------------------------------
// The five production checks.
//
// Each guard receives a fact computed by the shipping permission types above
// and decides whether the deployed service may act on it. Each body is one
// line so that a throwaway build can bypass exactly one of them.
// ---------------------------------------------------------------------------

/// Nobody may raise their own rights, and only the enclave owner may raise
/// anybody else's. This is the server-side twin of the refusal
/// `spaces::ServerMemberPermissionList::grant_right` renders on the client.
#[allow(unused_variables)]
pub fn self_role_raise_allowed(actor_person: &str, target_person: &str, actor_is_owner: bool) -> bool {
    actor_person != target_person && actor_is_owner // TASK6120-GUARD-SELF-ROLE
}

/// Only somebody currently on the enclave roster may enumerate the roster or
/// the channel list. Having once been a member is not membership.
#[allow(unused_variables)]
pub fn directory_access_allowed(actor_is_current_member: bool) -> bool {
    actor_is_current_member // TASK6120-GUARD-DIRECTORY
}

/// Reading, searching, syncing or subscribing to a channel's history requires
/// being in that channel's effective reader set: everybody on the roster for an
/// open channel, only the named people for a limited one.
#[allow(unused_variables)]
pub fn restricted_history_allowed(actor_in_channel_readers: bool) -> bool {
    actor_in_channel_readers // TASK6120-GUARD-RESTRICTED-HISTORY
}

/// A child thread inherits its parent channel's reader set. Naming the thread
/// directly is not a way around the channel it hangs under.
#[allow(unused_variables)]
pub fn child_thread_inheritance_allowed(actor_in_inherited_readers: bool) -> bool {
    actor_in_inherited_readers // TASK6120-GUARD-THREAD-INHERITANCE
}

/// Knowing a message identifier is not authorization to fetch its ciphertext.
/// The identifier is resolved back to its channel (or its thread's channel) and
/// the same reader set decides.
#[allow(unused_variables)]
pub fn known_id_fetch_allowed(actor_in_resolved_readers: bool) -> bool {
    actor_in_resolved_readers // TASK6120-GUARD-KNOWN-ID-FETCH
}

// ---------------------------------------------------------------------------
// Refusal codes. None of these carries a name, an identifier or a byte of
// content; they are stable strings the client maps to its own wording.
// ---------------------------------------------------------------------------

pub const REFUSE_SELF_ROLE: &str = "self-role-raise-refused";
pub const REFUSE_DIRECTORY: &str = "directory-refused";
pub const REFUSE_RESTRICTED_HISTORY: &str = "restricted-history-refused";
pub const REFUSE_THREAD_INHERITANCE: &str = "thread-inheritance-refused";
pub const REFUSE_KNOWN_ID_FETCH: &str = "known-id-fetch-refused";
pub const REFUSE_NOT_A_MEMBER: &str = "not-a-member";
pub const REFUSE_UNKNOWN_IDENTITY: &str = "unknown-identity";
pub const REFUSE_BAD_REQUEST: &str = "bad-request";
pub const REFUSE_UNKNOWN_OP: &str = "unknown-op";
pub const REFUSE_BAD_SIGNATURE: &str = "bad-signature";
pub const REFUSE_REPLAYED_NONCE: &str = "replayed-nonce";

/// The generic sentence a refused client receives. It names nothing.
pub const REFUSAL_SENTENCE: &str = "OSL: this request is not allowed";

/// The eight endpoints the deployed service exposes.
pub const OPS: [&str; 8] = [
    "role.grant",
    "roster.list",
    "channel.list",
    "history.read",
    "history.search",
    "history.sync",
    "history.subscribe",
    "blob.fetch",
];

// ---------------------------------------------------------------------------
// Seed fixture — the frozen enclave the service is deployed with.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureIdentity {
    /// The install label of the client that holds this key ("owner", "member", …).
    pub label: String,
    pub person_name: String,
    pub public_key_b64: String,
    /// `true` while this person is on the enclave roster at seed time.
    pub on_roster: bool,
    /// `true` if this person was admitted and then removed (the excluded role).
    #[serde(default)]
    pub excluded: bool,
    pub owner: bool,
    pub rights: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureMessage {
    pub message_id: String,
    pub channel_id: String,
    #[serde(default)]
    pub thread_id: Option<String>,
    pub ciphertext_hex: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatsFixture {
    pub enclave_id: String,
    pub owner_name: String,
    pub joined_at: String,
    pub identities: Vec<FixtureIdentity>,
    pub open_channel_id: String,
    pub restricted_channel_id: String,
    pub restricted_person_names: Vec<String>,
    pub open_thread_id: String,
    pub child_thread_id: String,
    pub messages: Vec<FixtureMessage>,
}

#[derive(Clone, Debug)]
pub struct Identity {
    pub label: String,
    pub person_name: String,
    pub public_key_b64: String,
    pub fingerprint: String,
}

// ---------------------------------------------------------------------------
// Audit — service-side state, written by the service and never sent to a client.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub seq: u64,
    pub build_tag: String,
    pub client_label: String,
    pub person_name: String,
    pub key_fingerprint: String,
    pub op: String,
    /// "allow" or "refuse", as the service decided it.
    pub decision: String,
    pub code: String,
    pub http_status: u16,
    pub enclave_id: String,
    /// The resource the request named, as the service resolved it.
    pub resource: String,
    pub rights_subject: String,
    pub rights_before: Vec<String>,
    pub rights_after: Vec<String>,
    pub response_bytes: usize,
}

pub struct Outcome {
    pub status: u16,
    pub body: Vec<u8>,
    pub audit: AuditEntry,
}

// ---------------------------------------------------------------------------

pub struct ChatsAuthority {
    build_tag: String,
    enclave_id: String,
    owner_name: String,
    members: ServerMembershipStore,
    permissions: ServerPermissionStore,
    threads: ServerThreadPermissionStore,
    channels: BTreeMap<String, ServerChannelAccessRecord>,
    /// Declaration order, so `channel.list` is stable.
    channel_order: Vec<String>,
    /// thread id -> parent channel id.
    thread_parent: BTreeMap<String, String>,
    /// message id -> (channel id, optional thread id).
    message_home: BTreeMap<String, (String, Option<String>)>,
    ciphertext: BTreeMap<String, Vec<u8>>,
    channel_history: BTreeMap<String, Vec<String>>,
    thread_history: BTreeMap<String, Vec<String>>,
    identities: BTreeMap<String, Identity>,
    seq: u64,
}

impl ChatsAuthority {
    pub fn seed(build_tag: &str, fixture: &ChatsFixture) -> Result<Self, String> {
        let mut members = ServerMembershipStore::default();
        members
            .upsert_server(
                fixture.enclave_id.clone(),
                fixture.owner_name.clone(),
                fixture.joined_at.clone(),
            )
            .map_err(|error| error.to_string())?;

        let mut permissions = ServerPermissionStore::default();
        let mut identities = BTreeMap::new();

        for identity in &fixture.identities {
            if !identity.owner && (identity.on_roster || identity.excluded) {
                members
                    .add_member(
                        &fixture.enclave_id,
                        identity.person_name.clone(),
                        fixture.joined_at.clone(),
                    )
                    .map_err(|error| error.to_string())?;
            }
            let mut rights = Vec::new();
            for name in &identity.rights {
                rights.push(parse_permission(name)?);
            }
            if identity.on_roster || identity.excluded {
                permissions
                    .set_person_permissions(
                        fixture.enclave_id.clone(),
                        identity.person_name.clone(),
                        rights,
                    )
                    .map_err(|error| error.to_string())?;
            }
            let fingerprint = fingerprint_of(&identity.public_key_b64)?;
            identities.insert(
                identity.public_key_b64.clone(),
                Identity {
                    label: identity.label.clone(),
                    person_name: identity.person_name.clone(),
                    public_key_b64: identity.public_key_b64.clone(),
                    fingerprint,
                },
            );
        }

        let mut threads = ServerThreadPermissionStore::default();
        let mut channels = BTreeMap::new();
        let mut channel_order = Vec::new();
        {
            let list = members
                .list(&fixture.enclave_id)
                .map_err(|error| error.to_string())?;
            let open = threads
                .set_open_channel(&list, fixture.open_channel_id.clone())
                .map_err(|error| error.to_string())?;
            let limited = threads
                .set_limited_channel(
                    &list,
                    fixture.restricted_channel_id.clone(),
                    fixture.restricted_person_names.clone(),
                )
                .map_err(|error| error.to_string())?;
            channel_order.push(open.channel_id.clone());
            channel_order.push(limited.channel_id.clone());
            channels.insert(open.channel_id.clone(), open);
            channels.insert(limited.channel_id.clone(), limited);
        }

        let mut thread_parent = BTreeMap::new();
        threads
            .create_thread(
                fixture.enclave_id.clone(),
                fixture.open_channel_id.clone(),
                fixture.open_thread_id.clone(),
            )
            .map_err(|error| error.to_string())?;
        thread_parent.insert(
            fixture.open_thread_id.clone(),
            fixture.open_channel_id.clone(),
        );
        threads
            .create_thread(
                fixture.enclave_id.clone(),
                fixture.restricted_channel_id.clone(),
                fixture.child_thread_id.clone(),
            )
            .map_err(|error| error.to_string())?;
        thread_parent.insert(
            fixture.child_thread_id.clone(),
            fixture.restricted_channel_id.clone(),
        );

        let mut message_home = BTreeMap::new();
        let mut ciphertext = BTreeMap::new();
        let mut channel_history: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut thread_history: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for message in &fixture.messages {
            let bytes = hex::decode(&message.ciphertext_hex)
                .map_err(|error| format!("ciphertext for {}: {error}", message.message_id))?;
            if bytes.is_empty() {
                return Err(format!("ciphertext for {} is empty", message.message_id));
            }
            match &message.thread_id {
                Some(thread_id) => {
                    threads
                        .add_thread_message(
                            fixture.enclave_id.clone(),
                            message.channel_id.clone(),
                            thread_id.clone(),
                            message.message_id.clone(),
                            hex::encode(&bytes),
                            true,
                        )
                        .map_err(|error| error.to_string())?;
                    thread_history
                        .entry(thread_id.clone())
                        .or_default()
                        .push(message.message_id.clone());
                }
                None => channel_history
                    .entry(message.channel_id.clone())
                    .or_default()
                    .push(message.message_id.clone()),
            }
            message_home.insert(
                message.message_id.clone(),
                (message.channel_id.clone(), message.thread_id.clone()),
            );
            ciphertext.insert(message.message_id.clone(), bytes);
        }

        // The excluded role is admitted and then removed, so the roster the
        // service serves has genuinely lost them rather than never having had
        // them: `never-member` and `excluded member` are different histories
        // with the same answer.
        for identity in &fixture.identities {
            if identity.excluded {
                members
                    .remove_member_by_name(&fixture.enclave_id, identity.person_name.clone())
                    .map_err(|error| error.to_string())?;
                permissions
                    .set_person_permissions(
                        fixture.enclave_id.clone(),
                        identity.person_name.clone(),
                        Vec::new(),
                    )
                    .map_err(|error| error.to_string())?;
            }
        }

        Ok(Self {
            build_tag: build_tag.to_owned(),
            enclave_id: fixture.enclave_id.clone(),
            owner_name: fixture.owner_name.clone(),
            members,
            permissions,
            threads,
            channels,
            channel_order,
            thread_parent,
            message_home,
            ciphertext,
            channel_history,
            thread_history,
            identities,
            seq: 0,
        })
    }

    pub fn enclave_id(&self) -> &str {
        &self.enclave_id
    }

    pub fn identity_for_key(&self, public_key_b64: &str) -> Option<&Identity> {
        self.identities.get(public_key_b64)
    }

    fn member_list(&self) -> ServerMemberList {
        self.members
            .list(&self.enclave_id)
            .expect("the seeded enclave is always present")
    }

    fn is_current_member(&self, person_name: &str) -> bool {
        self.member_list()
            .members()
            .iter()
            .any(|member| member.name == person_name)
    }

    /// Reads a person's rights back out through the shipping permission store
    /// rather than from a private mirror.
    pub fn rights_of(&self, person_name: &str) -> Vec<String> {
        ServerPermission::ALL
            .iter()
            .filter(|permission| {
                self.permissions
                    .require_person_permission(&self.enclave_id, person_name, **permission)
                    .is_ok()
            })
            .map(|permission| permission_token(*permission).to_owned())
            .collect()
    }

    pub fn rights_snapshot(&self) -> BTreeMap<String, Vec<String>> {
        let mut snapshot = BTreeMap::new();
        for identity in self.identities.values() {
            snapshot.insert(identity.person_name.clone(), self.rights_of(&identity.person_name));
        }
        snapshot
    }

    /// Who may read this channel, per the shipping open-vs-limited rule.
    fn channel_readers(&self, channel_id: &str) -> Option<Vec<String>> {
        let record = self.channels.get(channel_id)?;
        Some(record.effective_readers(&self.member_list()))
    }

    /// Who may read this child thread, resolved through its parent channel by
    /// the shipping inheritance rule.
    fn thread_readers(&self, thread_id: &str) -> Option<(String, Vec<String>)> {
        let channel_id = self.thread_parent.get(thread_id)?.clone();
        let readers = self
            .threads
            .effective_thread_readers(&self.member_list(), &channel_id, thread_id)
            .ok()?;
        Some((channel_id, readers))
    }

    pub fn handle(&mut self, public_key_b64: &str, op: &str, body: &[u8]) -> Outcome {
        self.seq += 1;
        let identity = self.identities.get(public_key_b64).cloned();
        let (label, person, fingerprint) = match &identity {
            Some(identity) => (
                identity.label.clone(),
                identity.person_name.clone(),
                identity.fingerprint.clone(),
            ),
            None => (
                "unknown".to_owned(),
                String::new(),
                fingerprint_of(public_key_b64).unwrap_or_else(|_| "invalid".to_owned()),
            ),
        };

        if identity.is_none() {
            return self.finish(label, person, fingerprint, op, String::new(), None, 403, Err(REFUSE_UNKNOWN_IDENTITY));
        }

        let parsed: serde_json::Value = match serde_json::from_slice(body) {
            Ok(value) => value,
            Err(_) => {
                return self.finish(label, person, fingerprint, op, String::new(), None, 400, Err(REFUSE_BAD_REQUEST))
            }
        };

        let resource = request_resource(op, &parsed);
        match op {
            "role.grant" => self.op_role_grant(label, person, fingerprint, &parsed),
            "roster.list" => self.op_roster_list(label, person, fingerprint, &parsed, resource),
            "channel.list" => self.op_channel_list(label, person, fingerprint, resource),
            "history.read" => self.op_history_read(label, person, fingerprint, &parsed, resource),
            "history.search" | "history.sync" | "history.subscribe" => {
                self.op_channel_stream(op, label, person, fingerprint, &parsed, resource)
            }
            "blob.fetch" => self.op_blob_fetch(label, person, fingerprint, &parsed, resource),
            _ => self.finish(label, person, fingerprint, op, resource, None, 404, Err(REFUSE_UNKNOWN_OP)),
        }
    }

    // -- endpoints ---------------------------------------------------------

    fn op_role_grant(
        &mut self,
        label: String,
        person: String,
        fingerprint: String,
        body: &serde_json::Value,
    ) -> Outcome {
        let target = body
            .get("target")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let right_token = body
            .get("right")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let resource = target.clone();
        if target.is_empty() || right_token.is_empty() {
            return self.finish(label, person, fingerprint, "role.grant", resource, None, 400, Err(REFUSE_BAD_REQUEST));
        }
        let right = match parse_permission(&right_token) {
            Ok(right) => right,
            Err(_) => {
                return self.finish(label, person, fingerprint, "role.grant", resource, None, 400, Err(REFUSE_BAD_REQUEST))
            }
        };

        let actor_is_owner = person == self.owner_name && self.is_current_member(&person);
        // PRODUCTION CHECK 1 — self-role raise.
        if !self_role_raise_allowed(&person, &target, actor_is_owner) {
            return self.finish(label, person.clone(), fingerprint, "role.grant", resource, Some(target), 403, Err(REFUSE_SELF_ROLE));
        }
        if !self.is_current_member(&person) || !self.is_current_member(&target) {
            return self.finish(label, person, fingerprint, "role.grant", resource, Some(target), 403, Err(REFUSE_NOT_A_MEMBER));
        }

        let mut rights: BTreeSet<ServerPermission> = ServerPermission::ALL
            .iter()
            .copied()
            .filter(|permission| {
                self.permissions
                    .require_person_permission(&self.enclave_id, &target, *permission)
                    .is_ok()
            })
            .collect();
        rights.insert(right);
        if self
            .permissions
            .set_person_permissions(
                self.enclave_id.clone(),
                target.clone(),
                rights.into_iter().collect(),
            )
            .is_err()
        {
            return self.finish(label, person, fingerprint, "role.grant", resource, Some(target), 400, Err(REFUSE_BAD_REQUEST));
        }

        let payload = serde_json::json!({
            "ok": true,
            "op": "role.grant",
            "enclave": self.enclave_id,
            "target": target,
            "rights": self.rights_of(&target),
        });
        self.finish(label, person, fingerprint, "role.grant", resource, Some(target), 200, Ok(payload))
    }

    fn op_roster_list(
        &mut self,
        label: String,
        person: String,
        fingerprint: String,
        body: &serde_json::Value,
        resource: String,
    ) -> Outcome {
        // PRODUCTION CHECK 2 — roster / channel-list directory access.
        if !directory_access_allowed(self.is_current_member(&person)) {
            return self.finish(label, person, fingerprint, "roster.list", resource, None, 403, Err(REFUSE_DIRECTORY));
        }
        let scope = body.get("channelId").and_then(serde_json::Value::as_str);
        match scope {
            None => {
                let names: Vec<String> = self
                    .member_list()
                    .members()
                    .into_iter()
                    .map(|member| member.name)
                    .collect();
                let payload = serde_json::json!({
                    "ok": true,
                    "op": "roster.list",
                    "enclave": self.enclave_id,
                    "scope": "enclave",
                    "members": names,
                    "count": names.len(),
                });
                self.finish(label, person, fingerprint, "roster.list", resource, None, 200, Ok(payload))
            }
            Some(channel_id) => {
                let readers = match self.channel_readers(channel_id) {
                    Some(readers) => readers,
                    None => {
                        return self.finish(label, person, fingerprint, "roster.list", resource, None, 403, Err(REFUSE_RESTRICTED_HISTORY))
                    }
                };
                // PRODUCTION CHECK 3 — restricted-channel access.
                if !restricted_history_allowed(readers.iter().any(|name| name == &person)) {
                    return self.finish(label, person, fingerprint, "roster.list", resource, None, 403, Err(REFUSE_RESTRICTED_HISTORY));
                }
                let payload = serde_json::json!({
                    "ok": true,
                    "op": "roster.list",
                    "enclave": self.enclave_id,
                    "scope": "channel",
                    "channel": channel_id,
                    "members": readers,
                    "count": readers.len(),
                });
                self.finish(label, person, fingerprint, "roster.list", resource, None, 200, Ok(payload))
            }
        }
    }

    fn op_channel_list(
        &mut self,
        label: String,
        person: String,
        fingerprint: String,
        resource: String,
    ) -> Outcome {
        // PRODUCTION CHECK 2 — roster / channel-list directory access.
        if !directory_access_allowed(self.is_current_member(&person)) {
            return self.finish(label, person, fingerprint, "channel.list", resource, None, 403, Err(REFUSE_DIRECTORY));
        }
        let mut visible = Vec::new();
        for channel_id in self.channel_order.clone() {
            let readers = self.channel_readers(&channel_id).unwrap_or_default();
            // PRODUCTION CHECK 3 — restricted-channel access, per channel.
            if !restricted_history_allowed(readers.iter().any(|name| name == &person)) {
                continue;
            }
            let access = match self.channels.get(&channel_id).map(|record| &record.access) {
                Some(ServerChannelAccess::Open) => "open",
                _ => "limited",
            };
            visible.push(serde_json::json!({ "channelId": channel_id, "access": access }));
        }
        let payload = serde_json::json!({
            "ok": true,
            "op": "channel.list",
            "enclave": self.enclave_id,
            "channels": visible,
            "count": visible.len(),
        });
        self.finish(label, person, fingerprint, "channel.list", resource, None, 200, Ok(payload))
    }

    fn op_history_read(
        &mut self,
        label: String,
        person: String,
        fingerprint: String,
        body: &serde_json::Value,
        resource: String,
    ) -> Outcome {
        // PRODUCTION CHECK 2 — the enclave directory gate fronts every read.
        if !directory_access_allowed(self.is_current_member(&person)) {
            return self.finish(label, person, fingerprint, "history.read", resource, None, 403, Err(REFUSE_DIRECTORY));
        }
        if let Some(thread_id) = body.get("threadId").and_then(serde_json::Value::as_str) {
            let (channel_id, readers) = match self.thread_readers(thread_id) {
                Some(found) => found,
                None => {
                    return self.finish(label, person, fingerprint, "history.read", resource, None, 403, Err(REFUSE_THREAD_INHERITANCE))
                }
            };
            // PRODUCTION CHECK 4 — child-thread inheritance.
            if !child_thread_inheritance_allowed(readers.iter().any(|name| name == &person)) {
                return self.finish(label, person, fingerprint, "history.read", resource, None, 403, Err(REFUSE_THREAD_INHERITANCE));
            }
            let messages = self.render_messages(self.thread_history.get(thread_id).cloned().unwrap_or_default());
            let payload = serde_json::json!({
                "ok": true,
                "op": "history.read",
                "enclave": self.enclave_id,
                "channel": channel_id,
                "thread": thread_id,
                "messages": messages,
                "count": messages.len(),
            });
            return self.finish(label, person, fingerprint, "history.read", resource, None, 200, Ok(payload));
        }

        let channel_id = body
            .get("channelId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let readers = match self.channel_readers(&channel_id) {
            Some(readers) => readers,
            None => {
                return self.finish(label, person, fingerprint, "history.read", resource, None, 403, Err(REFUSE_RESTRICTED_HISTORY))
            }
        };
        // PRODUCTION CHECK 3 — restricted-channel history.
        if !restricted_history_allowed(readers.iter().any(|name| name == &person)) {
            return self.finish(label, person, fingerprint, "history.read", resource, None, 403, Err(REFUSE_RESTRICTED_HISTORY));
        }
        let messages =
            self.render_messages(self.channel_history.get(&channel_id).cloned().unwrap_or_default());
        let payload = serde_json::json!({
            "ok": true,
            "op": "history.read",
            "enclave": self.enclave_id,
            "channel": channel_id,
            "messages": messages,
            "count": messages.len(),
        });
        self.finish(label, person, fingerprint, "history.read", resource, None, 200, Ok(payload))
    }

    /// `history.search`, `history.sync` and `history.subscribe` share one
    /// admission decision: they are three shapes of the same channel history.
    fn op_channel_stream(
        &mut self,
        op: &str,
        label: String,
        person: String,
        fingerprint: String,
        body: &serde_json::Value,
        resource: String,
    ) -> Outcome {
        // PRODUCTION CHECK 2 — the enclave directory gate fronts every read.
        if !directory_access_allowed(self.is_current_member(&person)) {
            return self.finish(label, person, fingerprint, op, resource, None, 403, Err(REFUSE_DIRECTORY));
        }
        let channel_id = body
            .get("channelId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let readers = match self.channel_readers(&channel_id) {
            Some(readers) => readers,
            None => {
                return self.finish(label, person, fingerprint, op, resource, None, 403, Err(REFUSE_RESTRICTED_HISTORY))
            }
        };
        // PRODUCTION CHECK 3 — restricted-channel history.
        if !restricted_history_allowed(readers.iter().any(|name| name == &person)) {
            return self.finish(label, person, fingerprint, op, resource, None, 403, Err(REFUSE_RESTRICTED_HISTORY));
        }

        let mut ids = self.channel_history.get(&channel_id).cloned().unwrap_or_default();
        for (thread_id, parent) in &self.thread_parent {
            if parent == &channel_id {
                ids.extend(self.thread_history.get(thread_id).cloned().unwrap_or_default());
            }
        }
        let payload = match op {
            "history.search" => {
                let needle = body
                    .get("needle")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let hits: Vec<String> = ids
                    .into_iter()
                    .filter(|id| needle.is_empty() || id.contains(&needle))
                    .collect();
                serde_json::json!({
                    "ok": true,
                    "op": "history.search",
                    "enclave": self.enclave_id,
                    "channel": channel_id,
                    "needle": needle,
                    "messageIds": hits,
                    "count": hits.len(),
                })
            }
            "history.sync" => {
                let since = body.get("since").and_then(serde_json::Value::as_u64).unwrap_or(0) as usize;
                let tail: Vec<String> = ids.into_iter().skip(since).collect();
                let messages = self.render_messages(tail);
                serde_json::json!({
                    "ok": true,
                    "op": "history.sync",
                    "enclave": self.enclave_id,
                    "channel": channel_id,
                    "since": since,
                    "messages": messages,
                    "count": messages.len(),
                })
            }
            _ => {
                let events = self.render_messages(ids);
                serde_json::json!({
                    "ok": true,
                    "op": "history.subscribe",
                    "enclave": self.enclave_id,
                    "channel": channel_id,
                    "events": events,
                    "count": events.len(),
                })
            }
        };
        self.finish(label, person, fingerprint, op, resource, None, 200, Ok(payload))
    }

    fn op_blob_fetch(
        &mut self,
        label: String,
        person: String,
        fingerprint: String,
        body: &serde_json::Value,
        resource: String,
    ) -> Outcome {
        // PRODUCTION CHECK 2 — the enclave directory gate fronts every read.
        if !directory_access_allowed(self.is_current_member(&person)) {
            return self.finish(label, person, fingerprint, "blob.fetch", resource, None, 403, Err(REFUSE_DIRECTORY));
        }
        let message_id = body
            .get("messageId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        // An unknown identifier and a forbidden one get the same refusal, so a
        // hostile client cannot probe for which messages exist.
        let readers = match self.message_home.get(&message_id).cloned() {
            Some((channel_id, None)) => self.channel_readers(&channel_id).unwrap_or_default(),
            Some((_, Some(thread_id))) => self
                .thread_readers(&thread_id)
                .map(|(_, readers)| readers)
                .unwrap_or_default(),
            None => Vec::new(),
        };
        // PRODUCTION CHECK 5 — known-identifier ciphertext fetch.
        if !known_id_fetch_allowed(readers.iter().any(|name| name == &person)) {
            return self.finish(label, person, fingerprint, "blob.fetch", resource, None, 403, Err(REFUSE_KNOWN_ID_FETCH));
        }
        let bytes = self.ciphertext.get(&message_id).cloned().unwrap_or_default();
        let payload = serde_json::json!({
            "ok": true,
            "op": "blob.fetch",
            "enclave": self.enclave_id,
            "messageId": message_id,
            "ciphertextB64": base64::engine::general_purpose::STANDARD.encode(&bytes),
            "bytes": bytes.len(),
        });
        self.finish(label, person, fingerprint, "blob.fetch", resource, None, 200, Ok(payload))
    }

    // -- shared ------------------------------------------------------------

    fn render_messages(&self, ids: Vec<String>) -> Vec<serde_json::Value> {
        ids.into_iter()
            .map(|id| {
                let bytes = self.ciphertext.get(&id).cloned().unwrap_or_default();
                serde_json::json!({
                    "messageId": id,
                    "ciphertextB64": base64::engine::general_purpose::STANDARD.encode(&bytes),
                    "bytes": bytes.len(),
                })
            })
            .collect()
    }

    /// Builds the response bytes and the service-side audit entry together, so
    /// no code path can answer a client without leaving a record.
    #[allow(clippy::too_many_arguments)]
    fn finish(
        &mut self,
        label: String,
        person: String,
        fingerprint: String,
        op: &str,
        resource: String,
        rights_subject: Option<String>,
        status: u16,
        result: Result<serde_json::Value, &'static str>,
    ) -> Outcome {
        let subject = rights_subject.unwrap_or_else(|| person.clone());
        let rights_after = if subject.is_empty() {
            Vec::new()
        } else {
            self.rights_of(&subject)
        };
        let (decision, code, body) = match result {
            Ok(payload) => (
                "allow",
                "ok".to_owned(),
                serde_json::to_vec(&payload).unwrap_or_default(),
            ),
            Err(code) => (
                "refuse",
                code.to_owned(),
                serde_json::to_vec(&serde_json::json!({
                    "ok": false,
                    "code": code,
                    "osl": REFUSAL_SENTENCE,
                }))
                .unwrap_or_default(),
            ),
        };
        let audit = AuditEntry {
            seq: self.seq,
            build_tag: self.build_tag.clone(),
            client_label: label,
            person_name: person,
            key_fingerprint: fingerprint,
            op: op.to_owned(),
            decision: decision.to_owned(),
            code,
            http_status: status,
            enclave_id: self.enclave_id.clone(),
            resource,
            rights_subject: subject,
            // `rights_before` is a placeholder here; the service overwrites it
            // with the snapshot it took before the request was handled, so a
            // bypassed guard cannot hide a raise by reporting the post-write
            // set on both sides of the record.
            rights_before: rights_after.clone(),
            rights_after,
            response_bytes: body.len(),
        };
        Outcome { status, body, audit }
    }

    pub fn audit_only(
        &mut self,
        label: String,
        person: String,
        fingerprint: String,
        op: &str,
        code: &'static str,
        status: u16,
    ) -> Outcome {
        self.seq += 1;
        self.finish(label, person, fingerprint, op, String::new(), None, status, Err(code))
    }
}

fn request_resource(op: &str, body: &serde_json::Value) -> String {
    let pick = |key: &str| {
        body.get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    match op {
        "blob.fetch" => pick("messageId"),
        "role.grant" => pick("target"),
        _ => {
            let thread = pick("threadId");
            if thread.is_empty() {
                pick("channelId")
            } else {
                thread
            }
        }
    }
}

pub fn permission_token(permission: ServerPermission) -> &'static str {
    match permission {
        ServerPermission::Read => "read",
        ServerPermission::Send => "send",
        ServerPermission::Invite => "invite",
        ServerPermission::MakeChannels => "make-channels",
        ServerPermission::RemoveMessages => "remove-messages",
        ServerPermission::RemovePeople => "remove-people",
        ServerPermission::ChangeServer => "change-server",
    }
}

pub fn parse_permission(token: &str) -> Result<ServerPermission, String> {
    ServerPermission::ALL
        .iter()
        .copied()
        .find(|permission| permission_token(*permission) == token)
        .ok_or_else(|| format!("unknown right {token}"))
}

pub fn fingerprint_of(public_key_b64: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(public_key_b64)
        .map_err(|error| error.to_string())?;
    let digest = Sha256::digest(&bytes);
    Ok(hex::encode(digest)[..16].to_owned())
}

/// The exact bytes a client signs. The op and the nonce ride inside the
/// signature so a captured signature cannot be replayed onto another endpoint.
pub fn signing_payload(op: &str, nonce: &str, body: &[u8]) -> Vec<u8> {
    let digest = hex::encode(Sha256::digest(body));
    format!("OSL-CHATS-v1\n{op}\n{nonce}\n{digest}").into_bytes()
}
