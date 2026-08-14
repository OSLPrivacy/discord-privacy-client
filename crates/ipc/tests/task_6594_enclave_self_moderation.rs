//! TASK 6594 — enclave-scoped roles and self-moderation, without OSL moderation.
//!
//! Two real Enclaves are founded, given *distinct* custom roles over the same
//! KEY/RELAY/TRUST permission catalogue, persisted encrypted, restarted from
//! disk, and then each governs itself: an authorized role-holder removes a
//! member, mutes them, restricts them to named channels, revokes their post
//! and invite abilities, and issues a signed instruction to delete one of
//! their messages, which every honest client in that Enclave honours exactly
//! once — while an unauthorized, stale, replayed, forged, cross-enclave or
//! over-broad instruction is refused by every one of them with no state change
//! at all.
//!
//! Removal is a key transition: it rotates to a fresh epoch through TASK
//! 6576's removal job, whose progress is observed from real per-member
//! authority wraps, and the removed device then opens nothing.
//!
//! Throughout, the control Enclave's durable bytes and the target's account
//! and direct-message bytes are compared byte for byte.
//!
//! ## Starvation
//!
//! `TASK6594_STARVE=<dimension>` removes exactly one required part of the
//! campaign. Every dimension has a guard that names it, so a starved run exits
//! 1 rather than passing with the feature half-absent.

use crypto::ed25519::{self, PublicKey, SecretKey};
use ipc::enclave_layout::{member_id_for_key, ChannelId, MessageId, RoleId};
use ipc::enclave_removal::{
    EnclaveEpochKey, EnclaveMemberAuthority, EnclaveRemovalError, EnclaveRemovalJob, RemovalStage,
    RestartBoundary, REMOVAL_DELAY_WARNING,
};
use ipc::enclave_self_moderation::{
    ClientMessage, CustomRole, EnclaveGovernance, EnclaveId, EnforcementClass, GovernanceAction,
    GovernanceInstruction, HonestMemberClient, InstructionRefusal, IsolationOracle, Permission,
    PermissionScope, SignedGovernanceInstruction, BOUND_INSTRUCTION_FIELDS,
};
use ipc::space_roster::SpaceMemberId;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Starvation dimensions
// ---------------------------------------------------------------------------

/// Every dimension a starved run may remove. Each has its own guard.
const STARVABLE: [&str; 9] = [
    "role",
    "permission-class",
    "direction",
    "client",
    "signature-field",
    "rekey",
    "progress",
    "isolation-oracle",
    "inventory",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Starve(Option<&'static str>);

impl Starve {
    fn from_environment() -> Self {
        let Ok(raw) = std::env::var("TASK6594_STARVE") else {
            return Self(None);
        };
        let raw = raw.trim().to_owned();
        if raw.is_empty() {
            return Self(None);
        }
        let dimension = STARVABLE
            .into_iter()
            .find(|candidate| *candidate == raw)
            .unwrap_or_else(|| panic!("TASK6594_STARVE must be one of {}", STARVABLE.join(", ")));
        Self(Some(dimension))
    }

    fn is(self, dimension: &str) -> bool {
        self.0 == Some(dimension)
    }

    fn label(self) -> &'static str {
        self.0.unwrap_or("none")
    }
}

// ---------------------------------------------------------------------------
// People, Enclaves and fixtures
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Person {
    label: String,
    secret: SecretKey,
    public: PublicKey,
    member: SpaceMemberId,
}

impl Person {
    fn new(label: impl Into<String>) -> Self {
        let (secret, public) = ed25519::generate_keypair();
        Self {
            label: label.into(),
            member: member_id_for_key(public.as_bytes()),
            secret,
            public,
        }
    }

    fn key(&self) -> &[u8; 32] {
        self.public.as_bytes()
    }
}

struct Enclave {
    name: &'static str,
    id: EnclaveId,
    people: Vec<Person>,
    role_ids: BTreeMap<&'static str, RoleId>,
    role_names: Vec<String>,
    role_grants: BTreeMap<String, BTreeSet<Permission>>,
    channels: BTreeMap<&'static str, ChannelId>,
    clients: Vec<HonestMemberClient>,
    message: MessageId,
    message_body: String,
}

impl Enclave {
    fn person(&self, label: &str) -> &Person {
        self.people
            .iter()
            .find(|person| person.label == label)
            .unwrap_or_else(|| panic!("{} has no person {label}", self.name))
    }

    fn role(&self, label: &str) -> RoleId {
        *self
            .role_ids
            .get(label)
            .unwrap_or_else(|| panic!("{} has no role {label}", self.name))
    }

    fn channel(&self, label: &str) -> ChannelId {
        *self
            .channels
            .get(label)
            .unwrap_or_else(|| panic!("{} has no channel {label}", self.name))
    }

    /// Mints against the owner's replica, which every client agrees with.
    fn governance(&self) -> &EnclaveGovernance {
        self.clients
            .first()
            .expect("an Enclave always has at least its owner's client")
            .governance()
    }

    fn save_all(&self, dir: &Path) -> Vec<PathBuf> {
        self.clients
            .iter()
            .map(|client| {
                client
                    .save(dir)
                    .unwrap_or_else(|error| panic!("{} could not persist: {error}", self.name))
            })
            .collect()
    }

    fn reopen_all(&mut self, dir: &Path) {
        let reopened: Vec<HonestMemberClient> = self
            .clients
            .iter()
            .map(|client| {
                HonestMemberClient::reopen(dir, self.id, client.member()).unwrap_or_else(|error| {
                    panic!(
                        "{} could not restart {}: {error}",
                        self.name,
                        client.label()
                    )
                })
            })
            .collect();
        self.clients = reopened;
    }
}

// ---------------------------------------------------------------------------
// Tallies
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Campaign {
    /// Authorized instructions honoured, per client, exactly once.
    honoured_once: usize,
    /// Second deliveries that were refused as replays.
    replay_refusals: usize,
    /// Refusals by token.
    refusals: BTreeMap<&'static str, usize>,
    /// Client serializations that changed across a refused instruction.
    refusal_state_changes: usize,
    /// Allowed decisions per enforcement class.
    allowed_by_class: BTreeMap<&'static str, usize>,
    /// Denied decisions per enforcement class.
    denied_by_class: BTreeMap<&'static str, usize>,
    /// Allowed/denied action exercises per Enclave and action kind.
    exercised: BTreeSet<(&'static str, &'static str, &'static str)>,
    /// Bound signature fields whose mutation was proved to break the signature.
    swept_signature_fields: BTreeSet<&'static str>,
    /// Message bodies destroyed by an honoured deletion.
    destroyed_bodies: usize,
    /// Clients that still hold a deleted body after honouring.
    surviving_bodies: usize,
    /// Completed re-key jobs.
    rekey_jobs: usize,
    /// Progress samples, per Enclave.
    progress_samples: BTreeMap<&'static str, Vec<usize>>,
    /// Reads by the removed member/device after the re-key.
    removed_opens: usize,
    /// Remaining members that opened the post-removal message exactly once.
    remaining_exact_opens: usize,
    /// Fresh epochs the two subsystems agreed on.
    fresh_epochs: Vec<(&'static str, u64, u64)>,
    /// Above-threshold removals that carried the measured warning.
    above_threshold_warnings: usize,
    /// Below-threshold removals that correctly carried no warning.
    below_threshold_silences: usize,
}

impl Campaign {
    fn note_decision(&mut self, class: EnforcementClass, allowed: bool) {
        let bucket = if allowed {
            &mut self.allowed_by_class
        } else {
            &mut self.denied_by_class
        };
        *bucket.entry(class.token()).or_default() += 1;
    }

    fn note_refusal(&mut self, refusal: &InstructionRefusal) {
        *self.refusals.entry(refusal.token()).or_default() += 1;
    }
}

// ---------------------------------------------------------------------------
// Support
// ---------------------------------------------------------------------------

struct FileKeyReset;

impl Drop for FileKeyReset {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
    }
}

fn id16(seed: u8, salt: u8) -> [u8; 16] {
    let mut bytes = [0_u8; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = seed
            .wrapping_mul(index as u8 + 1)
            .wrapping_add(salt.wrapping_mul(index as u8));
    }
    bytes[0] = seed;
    bytes[15] = salt;
    bytes
}

fn nonce(counter: u16) -> [u8; 16] {
    let mut bytes = [0_u8; 16];
    bytes[..2].copy_from_slice(&counter.to_be_bytes());
    bytes[2..10].copy_from_slice(
        &u64::from(counter)
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            .to_be_bytes(),
    );
    bytes[10] = 0x6d;
    bytes
}

fn encrypted_bytes(value: &serde_json::Value) -> Vec<u8> {
    let key = ipc::main_password::get_file_storage_key().expect("file storage key is unlocked");
    let plaintext = serde_json::to_vec(value).expect("json always serializes");
    ipc::main_password::encrypt_at_rest(&plaintext, &key).expect("at-rest sealing works")
}

fn write_encrypted(path: &Path, value: &serde_json::Value) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("artifact directory is creatable");
    }
    std::fs::write(path, encrypted_bytes(value)).expect("artifact is writable");
}

fn serialize(client: &HonestMemberClient) -> Vec<u8> {
    serde_json::to_vec(client).expect("an honest client always serializes")
}

// ---------------------------------------------------------------------------
// Enclave construction
// ---------------------------------------------------------------------------

struct EnclaveRecipe {
    name: &'static str,
    seed: u8,
    /// Owner first; the moderation target is named explicitly.
    people: Vec<&'static str>,
    target: &'static str,
    /// role label -> (display name, grants)
    roles: Vec<(&'static str, &'static str, Vec<Permission>)>,
    /// person label -> role label
    grants: Vec<(&'static str, &'static str)>,
    channels: Vec<&'static str>,
    message_body: &'static str,
}

/// `shared` carries identities that already exist elsewhere. One person can be
/// a member of two Enclaves, and the isolation oracle depends on it being the
/// *same* identity in both.
fn build(recipe: &EnclaveRecipe, starve: Starve, is_target: bool, shared: &[Person]) -> Enclave {
    let id = EnclaveId::from_bytes(id16(recipe.seed, 0x01));
    let people: Vec<Person> = recipe
        .people
        .iter()
        .map(|label| {
            shared
                .iter()
                .find(|person| person.label == *label)
                .cloned()
                .unwrap_or_else(|| Person::new(*label))
        })
        .collect();
    let owner = &people[0];

    let mut genesis = EnclaveGovernance::found(id, owner.key(), 1);
    for person in people.iter().skip(1) {
        genesis
            .admit_member(person.member)
            .expect("admitting a member of one's own Enclave");
    }

    let mut channels = BTreeMap::new();
    for (index, label) in recipe.channels.iter().enumerate() {
        let channel = ChannelId::from_bytes(id16(recipe.seed, 0x20 + index as u8));
        genesis.declare_channel(channel);
        channels.insert(*label, channel);
    }

    // A starved run drops one custom role from the target Enclave, so the
    // "distinct custom roles" guard has something to catch.
    let mut declared: Vec<(&'static str, &'static str, Vec<Permission>)> = recipe.roles.clone();
    if is_target && starve.is("role") {
        declared.pop();
    }

    let mut role_ids = BTreeMap::new();
    let mut role_names = Vec::new();
    let mut role_grants = BTreeMap::new();
    for (index, (label, display, permissions)) in declared.iter().enumerate() {
        let role = RoleId::from_bytes(id16(recipe.seed, 0x40 + index as u8));
        // A starved run removes the whole RELAY class from every role, so no
        // RELAY-class decision can be exercised in either direction.
        let grants: BTreeSet<Permission> = permissions
            .iter()
            .copied()
            .filter(|permission| {
                !starve.is("permission-class") || permission.class() != EnforcementClass::Relay
            })
            .collect();
        genesis
            .define_role(CustomRole::new(role, *display, grants.clone()))
            .unwrap_or_else(|error| panic!("{} could not define {display}: {error}", recipe.name));
        role_ids.insert(*label, role);
        role_names.push((*display).to_owned());
        role_grants.insert((*display).to_owned(), grants);
    }
    for (person_label, role_label) in &recipe.grants {
        let Some(role) = role_ids.get(role_label) else {
            continue;
        };
        let person = people
            .iter()
            .find(|person| person.label == *person_label)
            .unwrap_or_else(|| panic!("{} has no person {person_label}", recipe.name));
        genesis
            .grant_role(person.member, *role)
            .unwrap_or_else(|error| {
                panic!("{} could not grant {role_label}: {error}", recipe.name)
            });
    }

    let message = MessageId::from_bytes(id16(recipe.seed, 0x60));
    let mut clients: Vec<HonestMemberClient> = people
        .iter()
        .map(|person| HonestMemberClient::new(&person.label, person.member, genesis.clone()))
        .collect();
    let target_member = people
        .iter()
        .find(|person| person.label == recipe.target)
        .expect("the recipe names a real target")
        .member;
    let general = *channels
        .get(recipe.channels[0])
        .expect("an Enclave always has a first channel");
    for client in &mut clients {
        client.receive(ClientMessage {
            id: message,
            channel: general,
            author: target_member,
            body: recipe.message_body.to_owned(),
        });
    }
    // A starved run keeps only the owner's client, so "all honest clients"
    // cannot be satisfied by a fleet of one.
    if is_target && starve.is("client") {
        clients.truncate(1);
    }

    Enclave {
        name: recipe.name,
        id,
        people,
        role_ids,
        role_names,
        role_grants,
        channels,
        clients,
        message,
        message_body: recipe.message_body.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// Broadcasting instructions to every honest client
// ---------------------------------------------------------------------------

/// Delivers an authorized instruction to every honest client, then delivers it
/// again to prove each honours it exactly once.
fn broadcast_authorized(
    enclave: &mut Enclave,
    signed: &SignedGovernanceInstruction,
    campaign: &mut Campaign,
    direction_tag: &'static str,
) {
    let action = signed.instruction.action.token();
    let body = enclave.message_body.clone();
    let message_id = enclave.message;
    let enclave_name = enclave.name;
    for client in &mut enclave.clients {
        let before = client.honoured_nonces().len();
        let honoured = match client.honour(signed) {
            Ok(honoured) => honoured,
            Err(refusal) => panic!(
                "{} in {enclave_name} refused an authorized {action}: {refusal}",
                client.label()
            ),
        };
        assert_eq!(
            client.honoured_nonces().len(),
            before + 1,
            "{} did not record honouring {action}",
            client.label()
        );
        campaign.honoured_once += 1;
        campaign.note_decision(honoured.class, true);
        if honoured.destroyed_body_bytes > 0 {
            campaign.destroyed_bodies += 1;
        }
        if matches!(
            signed.instruction.action,
            GovernanceAction::DeleteMessage { .. }
        ) {
            let stored = serialize(client);
            if String::from_utf8_lossy(&stored).contains(&body) {
                campaign.surviving_bodies += 1;
            }
            if client.message(message_id).is_some() {
                campaign.surviving_bodies += 1;
            }
        }

        // Exactly once: the same instruction again is a replay and changes
        // nothing at all.
        let snapshot = serialize(client);
        let replayed = match client.honour(signed) {
            Ok(_) => panic!(
                "{} honoured a replayed {action} a second time",
                client.label()
            ),
            Err(refusal) => refusal,
        };
        assert!(
            matches!(replayed, InstructionRefusal::Replayed { .. }),
            "{} treated a replay as {replayed}",
            client.label()
        );
        campaign.replay_refusals += 1;
        campaign.note_refusal(&replayed);
        if serialize(client) != snapshot {
            campaign.refusal_state_changes += 1;
        }
        assert_eq!(
            client
                .honoured_nonces()
                .iter()
                .filter(|held| **held == signed.instruction.nonce)
                .count(),
            1,
            "{} honoured {action} more than once",
            client.label()
        );
    }
    campaign
        .exercised
        .insert((enclave.name, action, direction_tag));
}

/// Delivers an instruction that must be refused by every honest client, and
/// proves each one's durable state is byte-for-byte unchanged.
fn broadcast_refused(
    enclave: &mut Enclave,
    signed: &SignedGovernanceInstruction,
    campaign: &mut Campaign,
    expected: &'static str,
    direction_tag: Option<&'static str>,
) {
    let action = signed.instruction.action.token();
    let enclave_name = enclave.name;
    for client in &mut enclave.clients {
        let snapshot = serialize(client);
        let refusal = match client.honour(signed) {
            Ok(_) => panic!(
                "{} in {enclave_name} honoured an instruction it had to refuse ({expected}) for {action}",
                client.label()
            ),
            Err(refusal) => refusal,
        };
        assert_eq!(
            refusal.token(),
            expected,
            "{} refused {action} as {} instead of {expected}",
            client.label(),
            refusal.token()
        );
        campaign.note_refusal(&refusal);
        if serialize(client) != snapshot {
            campaign.refusal_state_changes += 1;
        }
    }
    if let Some(tag) = direction_tag {
        campaign.exercised.insert((enclave.name, action, tag));
    }
}

/// Builds an instruction directly, bypassing the minting guard, so the honest
/// clients' own admission is what refuses it.
fn hand_built(
    enclave: &Enclave,
    actor: &Person,
    actor_role: RoleId,
    action: GovernanceAction,
    target: SpaceMemberId,
    counter: u16,
) -> SignedGovernanceInstruction {
    let governance = enclave.governance();
    GovernanceInstruction {
        enclave: enclave.id,
        epoch: governance.epoch(),
        authority_version: governance.authority_version(),
        actor: *actor.key(),
        actor_role,
        action,
        target,
        nonce: nonce(counter),
    }
    .sign(&actor.secret)
}

// ---------------------------------------------------------------------------
// The campaign
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
fn govern(
    enclave: &mut Enclave,
    campaign: &mut Campaign,
    starve: Starve,
    counter_base: u16,
    remover_label: &'static str,
    remover_role: &'static str,
    steward_label: &'static str,
    steward_role: &'static str,
    unauthorized: &[(&'static str, &'static str, GovernanceAction, &'static str)],
) {
    let enclave_name = enclave.name;
    let target_label = enclave_target(enclave);
    let builder_label = enclave_builder(enclave);
    let target = enclave.person(target_label).clone();
    let steward = enclave.person(steward_label).clone();
    let remover = enclave.person(remover_label).clone();
    let steward_role_id = enclave.role(steward_role);
    let remover_role_id = enclave.role(remover_role);
    let general = enclave.channel("general");
    let quiet = enclave.channel("quiet");
    let builder = enclave.person(builder_label).clone();
    let message = enclave.message;
    let mut counter = counter_base;

    // ---- denied direction: an unauthorized role-holder ------------------
    if !starve.is("direction") {
        for (actor_label, role_label, action, tag) in unauthorized {
            let actor = enclave.person(actor_label).clone();
            let role_id = enclave.role(role_label);
            let permission = action.required_permission();
            let refused = enclave.governance().mint(
                actor.key(),
                role_id,
                action.clone(),
                target.member,
                nonce(counter),
            );
            assert!(
                refused.is_err(),
                "{} minted {} without authority in {enclave_name}",
                actor.label,
                permission.token(),
            );
            campaign.note_decision(permission.class(), false);
            let forged = hand_built(
                enclave,
                &actor,
                role_id,
                action.clone(),
                target.member,
                counter,
            );
            counter += 1;
            broadcast_refused(enclave, &forged, campaign, "unauthorized", Some(tag));
        }
    }

    // ---- a pre-minted instruction that a role change makes stale ---------
    let stale_version = enclave
        .governance()
        .mint(
            steward.key(),
            steward_role_id,
            GovernanceAction::RevokePost,
            target.member,
            nonce(counter),
        )
        .expect("the steward may revoke a post ability")
        .sign(&steward.secret);
    counter += 1;
    let extra_role = RoleId::from_bytes(id16(0xa1, counter as u8));
    for client in &mut enclave.clients {
        client
            .governance_mut()
            .define_role(CustomRole::new(
                extra_role,
                format!("{enclave_name} Late Role"),
                [Permission::ReadChannel],
            ))
            .expect("defining a late role advances the role/permission version");
    }
    broadcast_refused(
        enclave,
        &stale_version,
        campaign,
        "stale-authority-version",
        None,
    );

    // ---- allowed direction ----------------------------------------------
    let mut allowed: Vec<(GovernanceAction, &'static str)> = vec![
        (GovernanceAction::MuteMember, "mute-member"),
        (
            GovernanceAction::RestrictToChannels {
                allowed: BTreeSet::from([general]),
            },
            "restrict-to-channels",
        ),
        (GovernanceAction::RevokePost, "revoke-post"),
        (
            GovernanceAction::DeleteMessage {
                channel: general,
                message,
            },
            "delete-message",
        ),
    ];
    if !starve.is("permission-class") {
        allowed.insert(3, (GovernanceAction::RevokeInvite, "revoke-invite"));
    }
    for (action, _) in allowed {
        let signed = enclave
            .governance()
            .mint(
                steward.key(),
                steward_role_id,
                action.clone(),
                target.member,
                nonce(counter),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "{enclave_name} refused an authorized {}: {error}",
                    action.token()
                )
            })
            .sign(&steward.secret);
        counter += 1;
        broadcast_authorized(enclave, &signed, campaign, "allowed");
    }

    // ---- forged: a real instruction whose action was swapped -------------
    let honest = enclave
        .governance()
        .mint(
            steward.key(),
            steward_role_id,
            GovernanceAction::MuteMember,
            target.member,
            nonce(counter),
        )
        .expect("minting a fresh mute")
        .sign(&steward.secret);
    counter += 1;
    let mut tampered = honest.clone();
    tampered.instruction.action = GovernanceAction::RevokePost;
    assert!(
        !tampered.verify(),
        "a swapped action must break the signature"
    );
    broadcast_refused(enclave, &tampered, campaign, "forged", None);

    // ---- over-broad: a restriction naming a channel of another Enclave ---
    let foreign_channel = ChannelId::from_bytes(id16(0xfe, 0xfe));
    let over_broad = hand_built(
        enclave,
        &steward,
        steward_role_id,
        GovernanceAction::RestrictToChannels {
            allowed: BTreeSet::from([foreign_channel]),
        },
        target.member,
        counter,
    );
    counter += 1;
    broadcast_refused(enclave, &over_broad, campaign, "over-broad-action", None);

    // ---- over-broad: a target who is not in this Enclave -----------------
    let outsider = member_id_for_key(&[0xab; 32]);
    let outside_target = hand_built(
        enclave,
        &steward,
        steward_role_id,
        GovernanceAction::MuteMember,
        outsider,
        counter,
    );
    counter += 1;
    broadcast_refused(
        enclave,
        &outside_target,
        campaign,
        "target-outside-enclave",
        None,
    );

    // ---- resolver decisions on the real, moderated state -----------------
    for (member, permission, scope) in [
        (
            builder.member,
            Permission::ReadChannel,
            PermissionScope::Channel(general),
        ),
        (
            builder.member,
            Permission::CreateInvite,
            PermissionScope::Enclave,
        ),
        (
            steward.member,
            Permission::MuteMember,
            PermissionScope::Enclave,
        ),
        (
            target.member,
            Permission::ReadChannel,
            PermissionScope::Channel(quiet),
        ),
        (
            target.member,
            Permission::CreateInvite,
            PermissionScope::Enclave,
        ),
        (
            target.member,
            Permission::PostMessage,
            PermissionScope::Channel(general),
        ),
        (
            target.member,
            Permission::DeleteMessage,
            PermissionScope::Enclave,
        ),
    ] {
        let decision = enclave
            .clients
            .first()
            .expect("owner client")
            .resolve_for(member, permission, scope);
        campaign.note_decision(decision.class, decision.allowed);
    }

    // ---- the removal: a fresh epoch, and a stale instruction after it ----
    let stale_epoch = enclave
        .governance()
        .mint(
            steward.key(),
            steward_role_id,
            GovernanceAction::MuteMember,
            target.member,
            nonce(counter),
        )
        .expect("minting before the removal")
        .sign(&steward.secret);
    counter += 1;

    let removal = enclave
        .governance()
        .mint(
            remover.key(),
            remover_role_id,
            GovernanceAction::RemoveMember,
            target.member,
            nonce(counter),
        )
        .unwrap_or_else(|error| panic!("{} refused an authorized removal: {error}", enclave.name))
        .sign(&remover.secret);
    let predecessor_epoch = enclave.governance().epoch();
    broadcast_authorized(enclave, &removal, campaign, "allowed");
    let successor_epoch = enclave.governance().epoch();
    assert_eq!(
        successor_epoch,
        predecessor_epoch + 1,
        "{enclave_name} did not rotate to a fresh epoch on removal"
    );
    for client in &enclave.clients {
        assert_eq!(
            client.governance().epoch(),
            successor_epoch,
            "{} disagrees about the fresh epoch",
            client.label()
        );
        assert!(
            !client.governance().is_member(target.member),
            "{} still has the removed member",
            client.label()
        );
        // The removed member opens nothing later, on the governance side too.
        let decision = client.resolve_for(
            target.member,
            Permission::ReadChannel,
            PermissionScope::Channel(general),
        );
        if decision.allowed {
            campaign.removed_opens += 1;
        }
        campaign.note_decision(decision.class, decision.allowed);
    }
    broadcast_refused(enclave, &stale_epoch, campaign, "stale-epoch", None);
}

fn enclave_target(enclave: &Enclave) -> &'static str {
    if enclave.name == "Northgate" {
        "mallory"
    } else {
        "rook"
    }
}

fn enclave_builder(enclave: &Enclave) -> &'static str {
    if enclave.name == "Northgate" {
        "builder"
    } else {
        "scribe"
    }
}

// ---------------------------------------------------------------------------
// The cryptographic re-key half, driven from TASK 6576's removal job
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn rekey(
    enclave: &Enclave,
    campaign: &mut Campaign,
    starve: Starve,
    directory: &Path,
    predecessor_epoch: u64,
    measured_n: usize,
    removed: SpaceMemberId,
    restart_midway: bool,
) {
    if starve.is("rekey") {
        return;
    }
    let path = directory.join(format!("removal-{}.json", enclave.name));
    let enclave_id = hex::encode(enclave.id.as_bytes());
    let epoch_key = EnclaveEpochKey::generate().expect("a fresh predecessor epoch key");
    let authorities: Vec<EnclaveMemberAuthority> = enclave
        .people
        .iter()
        .map(|person| {
            EnclaveMemberAuthority::generate(hex::encode(person.member.as_bytes()))
                .expect("a per-member wrapping authority")
        })
        .collect();
    let removed_id = hex::encode(removed.as_bytes());
    let mut packaged: Vec<_> = authorities
        .iter()
        .map(|authority| {
            authority
                .package_client(&enclave_id, predecessor_epoch, &epoch_key)
                .expect("packaging a member's client")
        })
        .collect();
    let members = authorities.len();

    let mut job = EnclaveRemovalJob::begin(
        &path,
        &enclave_id,
        predecessor_epoch,
        authorities,
        &removed_id,
        measured_n,
    )
    .expect("beginning a removal in one's own Enclave");
    let confirmation = job.confirmation();
    assert_eq!(confirmation.member_count, members);
    assert_eq!(confirmation.measured_progress_n, measured_n);
    if members >= measured_n {
        assert_eq!(
            confirmation.warning,
            Some(REMOVAL_DELAY_WARNING),
            "an above-threshold removal must say removal takes time"
        );
        campaign.above_threshold_warnings += 1;
    } else {
        assert_eq!(
            confirmation.warning, None,
            "a below-threshold removal must not invent a delay warning"
        );
        campaign.below_threshold_silences += 1;
    }

    let mut progress = job.confirm().expect("confirming the removal");
    let mut samples = vec![progress.completed];
    let mut restarted = false;
    while progress.stage != RemovalStage::Succeeded {
        if restart_midway && !restarted && progress.completed * 2 >= progress.total {
            job = EnclaveRemovalJob::reopen(&path, RestartBoundary::Client)
                .expect("a removal survives a client restart");
            restarted = true;
            let resumed = job.observed_progress().expect("progress after restart");
            assert_eq!(
                resumed.completed, progress.completed,
                "a restart must not lose measured re-key progress"
            );
        }
        // Exactly one real per-member authority wrap per step, so every sample
        // is an observed fact rather than a counter.
        progress = job.step(1).expect("stepping the re-key");
        samples.push(progress.completed);
    }
    assert_eq!(progress.completed, members - 1);
    assert_eq!(progress.remaining, 0);
    assert_eq!(progress.total, members - 1);
    assert_eq!(job.successor_epoch(), Some(predecessor_epoch + 1));
    assert!(!job.active_member_ids().contains(&removed_id));
    campaign.rekey_jobs += 1;
    if !starve.is("progress") {
        campaign.progress_samples.insert(enclave.name, samples);
    }
    campaign
        .fresh_epochs
        .push((enclave.name, predecessor_epoch, predecessor_epoch + 1));

    // The removed device opens nothing that comes after the re-key.
    let canary = format!("{}-post-removal-canary", enclave.name);
    let message = job
        .encrypt_new_message(canary.as_bytes())
        .expect("the Enclave keeps talking after the re-key");
    let removed_index = enclave
        .people
        .iter()
        .position(|person| person.member == removed)
        .expect("the removed member is one of this Enclave's people");
    {
        let removed_client = &mut packaged[removed_index];
        let cached = removed_client.read_cached(&message);
        if matches!(&cached, Ok(Some(_))) {
            campaign.removed_opens += 1;
        }
        assert!(
            matches!(&cached, Err(EnclaveRemovalError::ReadRefused)),
            "a removed device was not explicitly refused from its own cache"
        );
        let direct = job.direct_service_read(removed_client, &message);
        if matches!(&direct, Ok(Some(_))) {
            campaign.removed_opens += 1;
        }
        assert!(
            matches!(&direct, Err(EnclaveRemovalError::ReadRefused)),
            "a removed device was not explicitly refused by the service"
        );
    }
    for (index, client) in packaged.iter_mut().enumerate() {
        if index == removed_index {
            continue;
        }
        let first = job
            .direct_service_read(client, &message)
            .expect("a remaining member reads the fresh epoch");
        assert_eq!(first.as_deref(), Some(canary.as_bytes()));
        assert_eq!(
            job.direct_service_read(client, &message)
                .expect("a second read is refused by the ledger, not by an error"),
            None,
            "a remaining member read the same message twice"
        );
        campaign.remaining_exact_opens += 1;
    }
    job.discard().expect("the removal job is disposed of");
    assert!(!path.exists());
}

// ---------------------------------------------------------------------------
// Signature-field binding sweep
// ---------------------------------------------------------------------------

fn sweep_signature_fields(enclave: &mut Enclave, campaign: &mut Campaign, starve: Starve) {
    let steward = enclave.person(enclave_steward(enclave)).clone();
    let steward_role = enclave.role(enclave_steward_role(enclave));
    let target = enclave.person(enclave_target(enclave)).clone();
    let base = GovernanceInstruction {
        enclave: enclave.id,
        epoch: enclave.governance().epoch(),
        authority_version: enclave.governance().authority_version(),
        actor: *steward.key(),
        actor_role: steward_role,
        action: GovernanceAction::MuteMember,
        target: target.member,
        nonce: nonce(0x7f00),
    };
    let signed = base.clone().sign(&steward.secret);
    assert!(signed.verify(), "an untouched instruction must verify");

    let mut mutations: Vec<(&'static str, GovernanceInstruction)> = Vec::new();
    let mut mutated = base.clone();
    mutated.enclave = EnclaveId::from_bytes(id16(0xcd, 0xcd));
    mutations.push(("enclave", mutated));
    let mut mutated = base.clone();
    mutated.epoch = base.epoch.wrapping_add(1);
    mutations.push(("epoch", mutated));
    let mut mutated = base.clone();
    mutated.authority_version = base.authority_version.wrapping_add(1);
    mutations.push(("authority_version", mutated));
    let mut mutated = base.clone();
    mutated.actor = [0x5c; 32];
    mutations.push(("actor", mutated));
    let mut mutated = base.clone();
    mutated.actor_role = RoleId::from_bytes(id16(0xce, 0xce));
    mutations.push(("actor_role", mutated));
    let mut mutated = base.clone();
    mutated.action = GovernanceAction::RevokeInvite;
    mutations.push(("action", mutated));
    let mut mutated = base.clone();
    mutated.target = member_id_for_key(&[0x3d; 32]);
    mutations.push(("target", mutated));
    let mut mutated = base.clone();
    mutated.nonce = nonce(0x7f01);
    mutations.push(("nonce", mutated));

    // A starved run leaves one bound field unproved.
    if starve.is("signature-field") {
        mutations.pop();
    }

    for (field, instruction) in mutations {
        let forged = SignedGovernanceInstruction {
            instruction,
            signature: signed.signature,
        };
        assert!(
            !forged.verify(),
            "signing bytes do not bind the {field} field"
        );
        for client in &mut enclave.clients {
            let snapshot = serialize(client);
            let refusal = client
                .honour(&forged)
                .expect_err("a forged instruction is never honoured");
            assert_eq!(
                refusal.token(),
                "forged",
                "{} refused a {field} forgery as {}",
                client.label(),
                refusal.token()
            );
            campaign.note_refusal(&refusal);
            if serialize(client) != snapshot {
                campaign.refusal_state_changes += 1;
            }
        }
        let field_name = BOUND_INSTRUCTION_FIELDS
            .into_iter()
            .find(|candidate| *candidate == field)
            .unwrap_or_else(|| panic!("{field} is not a declared bound field"));
        campaign.swept_signature_fields.insert(field_name);
    }
}

fn enclave_steward(enclave: &Enclave) -> &'static str {
    if enclave.name == "Northgate" {
        "steward"
    } else {
        "warden"
    }
}

fn enclave_steward_role(enclave: &Enclave) -> &'static str {
    if enclave.name == "Northgate" {
        "steward"
    } else {
        "warden"
    }
}

// ---------------------------------------------------------------------------
// The inventory
// ---------------------------------------------------------------------------

fn generated_inventory(directory: &Path, starve: Starve) -> serde_json::Value {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_task_6594_inventory"));
    command
        .arg("--artifact-dir")
        .arg(directory)
        .arg("--run-id")
        .arg("task6594");
    // `TASK6594_INVENTORY_STARVE` removes one required part of the *inventory*
    // rather than of the campaign, so each of the generator's own mutations can
    // be shown to make this check exit 1 too.
    let inventory_starve = std::env::var("TASK6594_INVENTORY_STARVE").ok();
    if let Some(dimension) = inventory_starve
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        command.arg("--starve").arg(dimension);
    } else if starve.is("inventory") {
        command.arg("--starve").arg("central-absence");
    }
    let output = command.output().expect("the inventory generator runs");
    assert!(
        output.status.success(),
        "the inventory generator failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("the inventory prints UTF-8");
    let line = stdout
        .lines()
        .rev()
        .find(|line| line.trim_start().starts_with('{'))
        .expect("the inventory prints one JSON record");
    serde_json::from_str(line).expect("the inventory record is JSON")
}

// ---------------------------------------------------------------------------
// The check
// ---------------------------------------------------------------------------

#[test]
#[allow(clippy::too_many_lines)]
fn task_6594_two_enclaves_govern_themselves_and_osl_governs_nobody() {
    let starve = Starve::from_environment();
    let _reset = FileKeyReset;
    ipc::main_password::set_file_storage_key(Some([0x59; 32]));

    let measured_n: usize = std::env::var("TASK6594_MEASURED_N")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8);
    assert!(
        measured_n >= 2,
        "the measured threshold N must be at least 2"
    );
    let above_threshold = measured_n + 4;

    let directory = tempfile::tempdir().expect("a scratch directory");
    let storage = directory.path().join("storage");
    let mut campaign = Campaign::default();

    // ---- two real Enclaves with distinct custom roles --------------------
    let mut northgate_people: Vec<&'static str> = vec!["owner", "steward", "builder", "mallory"];
    const BYSTANDERS: [&str; 12] = [
        "bystander-0",
        "bystander-1",
        "bystander-2",
        "bystander-3",
        "bystander-4",
        "bystander-5",
        "bystander-6",
        "bystander-7",
        "bystander-8",
        "bystander-9",
        "bystander-10",
        "bystander-11",
    ];
    assert!(
        above_threshold >= northgate_people.len(),
        "the target Enclave must be able to hold an above-threshold roster"
    );
    let bystanders_needed = above_threshold - northgate_people.len();
    assert!(
        bystanders_needed <= BYSTANDERS.len(),
        "TASK6594_MEASURED_N={measured_n} needs more bystanders than this campaign declares"
    );
    northgate_people.extend(BYSTANDERS.iter().take(bystanders_needed).copied());
    let mut northgate_grants: Vec<(&'static str, &'static str)> = vec![
        ("owner", "founder"),
        ("steward", "steward"),
        ("builder", "builder"),
        ("mallory", "everyone"),
    ];
    northgate_grants.extend(
        BYSTANDERS
            .iter()
            .take(bystanders_needed)
            .map(|label| (*label, "everyone")),
    );

    let northgate_recipe = EnclaveRecipe {
        name: "Northgate",
        seed: 0x6e,
        people: northgate_people,
        target: "mallory",
        roles: vec![
            (
                "founder",
                "Northgate Founder",
                Permission::CATALOGUE.to_vec(),
            ),
            (
                "steward",
                "Northgate Steward",
                vec![
                    Permission::ReadChannel,
                    Permission::PostMessage,
                    Permission::CreateInvite,
                    Permission::MuteMember,
                    Permission::RestrictMemberChannels,
                    Permission::RevokeMemberPost,
                    Permission::RevokeMemberInvite,
                    Permission::DeleteMessage,
                ],
            ),
            (
                "builder",
                "Northgate Builder",
                vec![
                    Permission::ReadChannel,
                    Permission::PostMessage,
                    Permission::CreateInvite,
                ],
            ),
            (
                "everyone",
                "Northgate Everyone",
                vec![Permission::ReadChannel, Permission::PostMessage],
            ),
        ],
        grants: northgate_grants,
        channels: vec!["general", "quiet"],
        message_body: "northgate-target-message-body-canary",
    };

    let southgate_recipe = EnclaveRecipe {
        name: "Southgate",
        seed: 0x73,
        people: vec!["keeper", "warden", "scribe", "rook", "witness", "mallory"],
        target: "rook",
        roles: vec![
            ("keeper", "Southgate Keeper", Permission::CATALOGUE.to_vec()),
            (
                "warden",
                "Southgate Warden",
                vec![
                    Permission::ReadChannel,
                    Permission::PostMessage,
                    Permission::RemoveMember,
                    Permission::MuteMember,
                    Permission::RestrictMemberChannels,
                    Permission::RevokeMemberPost,
                    Permission::RevokeMemberInvite,
                    Permission::DeleteMessage,
                ],
            ),
            (
                "scribe",
                "Southgate Scribe",
                vec![
                    Permission::ReadChannel,
                    Permission::PostMessage,
                    Permission::CreateInvite,
                ],
            ),
            (
                "everyone",
                "Southgate Everyone",
                vec![Permission::ReadChannel],
            ),
        ],
        grants: vec![
            ("keeper", "keeper"),
            ("warden", "warden"),
            ("scribe", "scribe"),
            ("rook", "everyone"),
            ("witness", "everyone"),
            ("mallory", "everyone"),
        ],
        channels: vec!["general", "quiet"],
        message_body: "southgate-target-message-body-canary",
    };

    let mut northgate = build(&northgate_recipe, starve, true, &[]);
    let shared = vec![northgate.person("mallory").clone()];
    let mut southgate = build(&southgate_recipe, starve, false, &shared);

    // Distinct custom roles, checked before anything is exercised.
    let north_names: BTreeSet<String> = northgate.role_names.iter().cloned().collect();
    let south_names: BTreeSet<String> = southgate.role_names.iter().cloned().collect();
    assert!(
        north_names.is_disjoint(&south_names),
        "the two Enclaves must have distinct custom roles"
    );
    assert!(
        northgate.role_names.len() >= 4 && southgate.role_names.len() >= 4,
        "starved role dimension: {} has {} custom roles and {} has {}; each Enclave needs at least 4",
        northgate.name,
        northgate.role_names.len(),
        southgate.name,
        southgate.role_names.len()
    );

    // ---- persist, then restart both Enclaves -----------------------------
    let north_files = northgate.save_all(&storage);
    let south_files = southgate.save_all(&storage);
    for path in north_files.iter().chain(south_files.iter()) {
        let bytes = std::fs::read(path).expect("persisted governance is readable");
        assert!(
            ipc::main_password::has_enc_magic(&bytes),
            "governance state must never be written in plaintext"
        );
    }
    let north_before_restart: Vec<Vec<u8>> = northgate.clients.iter().map(serialize).collect();
    let south_before_restart: Vec<Vec<u8>> = southgate.clients.iter().map(serialize).collect();
    northgate.reopen_all(&storage);
    southgate.reopen_all(&storage);
    assert_eq!(
        northgate
            .clients
            .iter()
            .map(serialize)
            .collect::<Vec<Vec<u8>>>(),
        north_before_restart,
        "the target Enclave did not survive a restart byte for byte"
    );
    assert_eq!(
        southgate
            .clients
            .iter()
            .map(serialize)
            .collect::<Vec<Vec<u8>>>(),
        south_before_restart,
        "the control Enclave did not survive a restart byte for byte"
    );
    let restarted_roles = northgate.governance().roles().len();
    let restarted_catalogue = northgate.governance().catalogue().len();
    assert_eq!(restarted_roles, northgate.role_names.len());
    assert_eq!(restarted_catalogue, Permission::CATALOGUE.len());

    // ---- the account and direct-message state that must not move ---------
    let mallory = northgate.person("mallory").clone();
    let rook = southgate.person("rook").clone();
    let outside = directory.path().join("outside");
    let mallory_account = outside.join("account_mallory.json");
    let mallory_dm = outside.join("dm_mallory.json");
    let rook_account = outside.join("account_rook.json");
    let rook_dm = outside.join("dm_rook.json");
    write_encrypted(
        &mallory_account,
        &serde_json::json!({
            "account": hex::encode(mallory.member.as_bytes()),
            "display_name": "mallory",
            "standing": "no OSL standing exists to change",
        }),
    );
    write_encrypted(
        &mallory_dm,
        &serde_json::json!({
            "peer": "a-friend",
            "messages": ["mallory-direct-message-body-canary"],
        }),
    );
    write_encrypted(
        &rook_account,
        &serde_json::json!({
            "account": hex::encode(rook.member.as_bytes()),
            "display_name": "rook",
            "standing": "no OSL standing exists to change",
        }),
    );
    write_encrypted(
        &rook_dm,
        &serde_json::json!({
            "peer": "another-friend",
            "messages": ["rook-direct-message-body-canary"],
        }),
    );

    let mut oracle = IsolationOracle::new();
    if !starve.is("isolation-oracle") {
        for (index, path) in south_files.iter().enumerate() {
            oracle
                .capture_file(format!("control-enclave-client-{index}"), path)
                .expect("capturing the control Enclave");
        }
        for (subject, path) in [
            ("target-account", &mallory_account),
            ("target-dm", &mallory_dm),
            ("control-target-account", &rook_account),
            ("control-target-dm", &rook_dm),
        ] {
            oracle
                .capture_file(subject, path)
                .expect("capturing account and direct-message state");
        }
    }

    // ---- the target Enclave governs itself -------------------------------
    let north_predecessor_epoch = northgate.governance().epoch();
    govern(
        &mut northgate,
        &mut campaign,
        starve,
        0x1000,
        "owner",
        "founder",
        "steward",
        "steward",
        &[
            (
                "steward",
                "steward",
                GovernanceAction::RemoveMember,
                "denied",
            ),
            ("builder", "builder", GovernanceAction::MuteMember, "denied"),
            (
                "builder",
                "builder",
                GovernanceAction::RestrictToChannels {
                    allowed: BTreeSet::new(),
                },
                "denied",
            ),
            (
                "mallory",
                "everyone",
                GovernanceAction::DeleteMessage {
                    channel: ChannelId::from_bytes(id16(0x6e, 0x20)),
                    message: MessageId::from_bytes(id16(0x6e, 0x60)),
                },
                "denied",
            ),
            (
                "builder",
                "builder",
                GovernanceAction::RevokeInvite,
                "denied",
            ),
        ],
    );
    sweep_signature_fields(&mut northgate, &mut campaign, starve);
    rekey(
        &northgate,
        &mut campaign,
        starve,
        directory.path(),
        north_predecessor_epoch,
        measured_n,
        mallory.member,
        true,
    );
    northgate.save_all(&storage);

    // ---- the control Enclave, the account and the DMs did not move -------
    let mut isolation_breaches = Vec::new();
    if !starve.is("isolation-oracle") {
        for (index, path) in south_files.iter().enumerate() {
            if let Err(breach) =
                oracle.verify_file(&format!("control-enclave-client-{index}"), path)
            {
                isolation_breaches.push(breach.to_string());
            }
        }
        for (subject, path) in [
            ("target-account", &mallory_account),
            ("target-dm", &mallory_dm),
            ("control-target-account", &rook_account),
            ("control-target-dm", &rook_dm),
        ] {
            if let Err(breach) = oracle.verify_file(subject, path) {
                isolation_breaches.push(breach.to_string());
            }
        }
    }
    // The removed member is still a full member of the other Enclave, read
    // back from that Enclave's own durable state.
    let control_replica =
        HonestMemberClient::reopen(&storage, southgate.id, southgate.person("keeper").member)
            .expect("the control Enclave reopens");
    assert!(
        control_replica.governance().is_member(mallory.member),
        "removing somebody from one Enclave removed them from another"
    );
    assert!(
        !control_replica.governance().is_muted(mallory.member),
        "muting somebody in one Enclave muted them in another"
    );

    // ---- the control Enclave governs itself, independently ---------------
    let north_after: Vec<Vec<u8>> = northgate
        .clients
        .iter()
        .map(|client| std::fs::read(client.state_path(&storage)).expect("northgate is durable"))
        .collect();
    let south_predecessor_epoch = southgate.governance().epoch();
    govern(
        &mut southgate,
        &mut campaign,
        starve,
        0x2000,
        "warden",
        "warden",
        "warden",
        "warden",
        &[
            ("scribe", "scribe", GovernanceAction::RemoveMember, "denied"),
            (
                "witness",
                "everyone",
                GovernanceAction::MuteMember,
                "denied",
            ),
            (
                "scribe",
                "scribe",
                GovernanceAction::RestrictToChannels {
                    allowed: BTreeSet::new(),
                },
                "denied",
            ),
            (
                "witness",
                "everyone",
                GovernanceAction::DeleteMessage {
                    channel: ChannelId::from_bytes(id16(0x73, 0x20)),
                    message: MessageId::from_bytes(id16(0x73, 0x60)),
                },
                "denied",
            ),
            ("scribe", "scribe", GovernanceAction::RevokeInvite, "denied"),
        ],
    );
    rekey(
        &southgate,
        &mut campaign,
        starve,
        directory.path(),
        south_predecessor_epoch,
        measured_n,
        rook.member,
        false,
    );
    southgate.save_all(&storage);
    for (index, client) in northgate.clients.iter().enumerate() {
        let now = std::fs::read(client.state_path(&storage)).expect("northgate is still durable");
        if now != north_after[index] {
            isolation_breaches.push(format!(
                "the target Enclave changed while the control Enclave governed itself: {}",
                client.label()
            ));
        }
    }

    // ---- a cross-enclave instruction is refused everywhere ---------------
    let warden = southgate.person("warden").clone();
    let warden_role = southgate.role("warden");
    let witness = southgate.person("witness").clone();
    let cross = southgate
        .governance()
        .mint(
            warden.key(),
            warden_role,
            GovernanceAction::MuteMember,
            witness.member,
            nonce(0x3f00),
        )
        .expect("the warden may mute inside their own Enclave")
        .sign(&warden.secret);
    broadcast_refused(&mut northgate, &cross, &mut campaign, "cross-enclave", None);

    // ---- the generated inventory -----------------------------------------
    let inventory = generated_inventory(&directory.path().join("inventory"), starve);
    let required = &inventory["required_state"];
    let central = &inventory["central_absence"];

    // ---- guards ----------------------------------------------------------
    let clients_in_target = northgate.clients.len();
    assert!(
        clients_in_target >= 3,
        "starved client dimension: the target Enclave ran {clients_in_target} honest clients"
    );
    assert_eq!(
        clients_in_target,
        northgate.people.len(),
        "every member of the target Enclave must run an honest client"
    );

    let authorized_per_enclave = if starve.is("permission-class") { 5 } else { 6 };
    assert_eq!(
        campaign.honoured_once,
        authorized_per_enclave * (northgate.clients.len() + southgate.clients.len()),
        "every honest client must honour every authorized instruction exactly once"
    );
    assert_eq!(
        campaign.replay_refusals, campaign.honoured_once,
        "every honoured instruction must be refused on its second delivery"
    );
    assert_eq!(
        campaign.refusal_state_changes, 0,
        "a refused instruction changed durable client state"
    );
    assert_eq!(
        campaign.surviving_bodies, 0,
        "an honoured deletion left the message body behind"
    );
    assert_eq!(
        campaign.destroyed_bodies,
        northgate.clients.len() + southgate.clients.len(),
        "every honest client must destroy the deleted body, exactly as burn does"
    );

    for direction in ["allowed", "denied"] {
        let count = campaign
            .exercised
            .iter()
            .filter(|(_, _, tag)| *tag == direction)
            .count();
        assert!(
            count > 0,
            "starved allowed/denied direction: the campaign exercised {count} {direction} actions"
        );
    }
    for enclave in ["Northgate", "Southgate"] {
        for action in [
            "remove-member",
            "mute-member",
            "restrict-to-channels",
            "delete-message",
        ] {
            for direction in ["allowed", "denied"] {
                assert!(
                    campaign
                        .exercised
                        .contains(&(enclave, action, direction)),
                    "starved allowed/denied direction: {enclave} never exercised {direction} {action}"
                );
            }
        }
    }

    for class in EnforcementClass::ALL {
        let allowed = campaign
            .allowed_by_class
            .get(class.token())
            .copied()
            .unwrap_or(0);
        let denied = campaign
            .denied_by_class
            .get(class.token())
            .copied()
            .unwrap_or(0);
        assert!(
            allowed > 0 && denied > 0,
            "starved permission class: {} had {allowed} allowed and {denied} denied decisions",
            class.token()
        );
    }

    let north_grant_sets: BTreeSet<BTreeSet<Permission>> =
        northgate.role_grants.values().cloned().collect();
    assert_eq!(
        north_grant_sets.len(),
        northgate.role_names.len(),
        "the target Enclave's roles must grant genuinely different permissions"
    );

    assert_eq!(
        campaign.swept_signature_fields.len(),
        BOUND_INSTRUCTION_FIELDS.len(),
        "starved signature field: only {:?} of {:?} were proved bound",
        campaign.swept_signature_fields,
        BOUND_INSTRUCTION_FIELDS
    );

    assert_eq!(
        campaign.rekey_jobs, 2,
        "starved re-key: {} of 2 removals rotated to a fresh epoch",
        campaign.rekey_jobs
    );
    assert_eq!(campaign.fresh_epochs.len(), 2);
    for (name, predecessor, successor) in &campaign.fresh_epochs {
        assert_eq!(successor, &(predecessor + 1), "{name} reused an epoch");
        let governance_epoch = if *name == "Northgate" {
            northgate.governance().epoch()
        } else {
            southgate.governance().epoch()
        };
        assert_eq!(
            governance_epoch, *successor,
            "{name}'s roles and its keys disagree about the fresh epoch"
        );
    }
    assert_eq!(
        campaign.above_threshold_warnings, 1,
        "an above-threshold removal must exist and say removal takes time"
    );
    assert_eq!(
        campaign.below_threshold_silences, 1,
        "a below-threshold removal must exist and stay silent"
    );

    assert_eq!(
        campaign.progress_samples.len(),
        2,
        "starved progress: {} Enclaves reported measured re-key progress",
        campaign.progress_samples.len()
    );
    for (name, samples) in &campaign.progress_samples {
        assert!(
            samples.len() >= 3,
            "starved progress: {name} reported {} samples",
            samples.len()
        );
        assert_eq!(samples.first(), Some(&0), "{name} started part-way through");
        for pair in samples.windows(2) {
            assert!(
                pair[1] > pair[0],
                "{name}'s measured progress did not advance"
            );
        }
    }
    assert_eq!(
        campaign.removed_opens, 0,
        "a removed member/device opened later content"
    );
    assert_eq!(
        campaign.remaining_exact_opens,
        (northgate.people.len() - 1) + (southgate.people.len() - 1),
        "every remaining member must open the post-removal message exactly once"
    );

    assert!(
        oracle.len() >= 4,
        "starved isolation oracle: {} subjects were captured",
        oracle.len()
    );
    assert!(
        isolation_breaches.is_empty(),
        "isolation breach: {isolation_breaches:?}"
    );

    assert_eq!(
        central["swept"].as_bool(),
        Some(true),
        "starved central-absence inventory: the sweep did not run"
    );
    assert!(
        central["swept_files"].as_u64().unwrap_or(0) >= 20,
        "starved central-absence inventory: only {} files were swept",
        central["swept_files"]
    );
    assert_eq!(
        central["forbidden_central_systems"].as_u64(),
        Some(0),
        "forbidden OSL-central systems found: {}",
        central["forbidden_hits"]
    );
    assert_eq!(
        inventory["plaintext_bytes"].as_u64(),
        Some(0),
        "the inventory found plaintext governance state"
    );
    for (field, minimum) in [
        ("roles", 3_u64),
        ("permissions", 9),
        ("permission_classes", 3),
        ("signed_instructions", 1),
        ("signed_instruction_fields", 8),
        ("allowed_decisions", 1),
        ("denied_decisions", 1),
    ] {
        let value = required[field].as_u64().unwrap_or(0);
        assert!(
            value >= minimum,
            "the inventory's required {field} state is starved: {value} < {minimum}"
        );
    }
    assert!(
        inventory["surfaces"]["storage"]["files"]
            .as_u64()
            .unwrap_or(0)
            > 0
            && inventory["surfaces"]["storage"]["files"]
                == inventory["surfaces"]["storage"]["encrypted_files"],
        "the inventory's storage surface is empty or not encrypted"
    );
    for surface in ["server", "api", "ui"] {
        let files = inventory["surfaces"][surface]["source_files"]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0);
        assert!(files > 0, "the inventory's {surface} surface is empty");
    }

    // ---- the printed record ----------------------------------------------
    println!("TASK-6594 starve={}", starve.label());
    println!(
        "TASK-6594 enclaves=2 target={} control={} distinct_roles={} north_roles={} south_roles={}",
        northgate.name,
        southgate.name,
        north_names.len() + south_names.len(),
        northgate.role_names.len(),
        southgate.role_names.len()
    );
    println!(
        "TASK-6594 restart_survived=2 roles={restarted_roles} catalogue={restarted_catalogue} clients={}",
        northgate.clients.len() + southgate.clients.len()
    );
    println!(
        "TASK-6594 honoured_exactly_once={} replay_refusals={} refusal_state_changes={}",
        campaign.honoured_once, campaign.replay_refusals, campaign.refusal_state_changes
    );
    let mut refusal_line = String::new();
    for (token, count) in &campaign.refusals {
        refusal_line.push_str(&format!(" {token}={count}"));
    }
    println!("TASK-6594 refusals{refusal_line}");
    println!(
        "TASK-6594 classes allowed KEY={} RELAY={} TRUST={} denied KEY={} RELAY={} TRUST={}",
        campaign.allowed_by_class.get("KEY").copied().unwrap_or(0),
        campaign.allowed_by_class.get("RELAY").copied().unwrap_or(0),
        campaign.allowed_by_class.get("TRUST").copied().unwrap_or(0),
        campaign.denied_by_class.get("KEY").copied().unwrap_or(0),
        campaign.denied_by_class.get("RELAY").copied().unwrap_or(0),
        campaign.denied_by_class.get("TRUST").copied().unwrap_or(0),
    );
    println!(
        "TASK-6594 action_directions={} signature_fields_bound={}/{}",
        campaign.exercised.len(),
        campaign.swept_signature_fields.len(),
        BOUND_INSTRUCTION_FIELDS.len()
    );
    println!(
        "TASK-6594 deletions destroyed_bodies={} surviving_bodies={}",
        campaign.destroyed_bodies, campaign.surviving_bodies
    );
    for (name, samples) in &campaign.progress_samples {
        println!(
            "TASK-6594 rekey enclave={name} n={measured_n} members={} samples={} progress={:?}",
            if *name == "Northgate" {
                northgate.people.len()
            } else {
                southgate.people.len()
            },
            samples.len(),
            samples
        );
    }
    println!(
        "TASK-6594 epochs {:?} above_threshold_warnings={} below_threshold_silences={}",
        campaign.fresh_epochs, campaign.above_threshold_warnings, campaign.below_threshold_silences
    );
    println!(
        "TASK-6594 removed_opens={} remaining_exact_opens={}",
        campaign.removed_opens, campaign.remaining_exact_opens
    );
    println!(
        "TASK-6594 isolation_subjects={} breaches={}",
        oracle.len(),
        isolation_breaches.len()
    );
    println!(
        "TASK-6594 inventory roles={} permissions={} classes={} signed_instructions={} fields={} storage_files={} encrypted={} api_symbols={} ui_files={} server_files={}",
        required["roles"],
        required["permissions"],
        required["permission_classes"],
        required["signed_instructions"],
        required["signed_instruction_fields"],
        inventory["surfaces"]["storage"]["files"],
        inventory["surfaces"]["storage"]["encrypted_files"],
        inventory["surfaces"]["api"]["public_symbols"],
        inventory["surfaces"]["ui"]["source_files"].as_array().map(Vec::len).unwrap_or(0),
        inventory["surfaces"]["server"]["source_files"].as_array().map(Vec::len).unwrap_or(0),
    );
    println!(
        "TASK-6594 central_absence swept_files={} candidate_systems={} candidate_identifiers={} forbidden_central_systems={} plaintext_bytes={} excluded={}",
        central["swept_files"],
        central["candidate_systems"],
        central["candidate_identifiers"],
        central["forbidden_central_systems"],
        inventory["plaintext_bytes"],
        central["excluded_catalogue_file"],
    );
}
