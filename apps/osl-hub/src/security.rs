//! Trusted-local People, friend-code, and scope-security backend for the app.
//!
//! The platform webviews do not receive this API. Friend codes contain public
//! identity material only and are signed by the exporting identity. A valid
//! signature proves that the code is internally authentic; the separate
//! `safety_number_verified` bit records the user's out-of-band confirmation.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;
use std::sync::Mutex;

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use ipc::commands::is_discord_snowflake_shaped;
use ipc::peer_map::{PeerEntry, WhitelistEntry};
use ipc::scope::{Scope, ScopeInput, ScopeKind};
use ipc::tofu::KeyBundle;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use crate::core_bridge::HubCoreState;

const FRIEND_CODE_PREFIX: &str = "OSLFR1.";
const FRIEND_CODE_VERSION: u32 = 1;
const MAX_FRIEND_CODE_BYTES: usize = 8 * 1024;
const MAX_SECURITY_STATE_BYTES: u64 = 8 * 1024 * 1024;
const PEOPLE_FILE: &str = "hub_people.json";
const SECURITY_PREFS_FILE: &str = "hub_security_preferences.json";
const PEER_REPLAY_FILE: &str = "hub_peer_replay.json";
const ATTACHMENT_BURN_FILE: &str = "scope_attachments.json";
/// Receiver-side bilateral-burn replay state. Encrypted at rest, and keyed
/// entirely by pair-specific commitments — it contains no scope name, service
/// name or account handle.
const REVOCATION_LEDGER_FILE: &str = "hub_revocation_ledger.json";
/// Durable sender-side revocation queue. Encrypted at rest.
const REVOCATION_OUTBOX_FILE: &str = "hub_revocation_outbox.json";
/// Sender-local monotonic `send_seq` / `burn_epoch` counters. Encrypted at rest.
const REVOCATION_COUNTERS_FILE: &str = "hub_revocation_counters.json";
const SNOWFLAKE_IDENTITY_REFUSAL: &str =
    "OSL cannot add a friend whose identity is a Discord identifier";
/// Shown when a burn floor refuses content. Identical to [`PEER_OPEN_ERROR`] so
/// "burned" and "could not be opened" are indistinguishable to a peer probing
/// the UI.
const REVOCATION_REFUSED_ERROR: &str = PEER_OPEN_ERROR;
const MAX_ATTACHMENT_BURN_ENTRIES_PER_SCOPE: usize = 256;
const MAX_ATTACHMENT_BURN_ENTRIES_TOTAL: usize = 2_048;
const MAX_PEER_REPLAY_SCOPES: usize = 512;
const MAX_PEER_REPLAY_ENTRIES_PER_SCOPE: usize = 4_096;
const MAX_PEER_REPLAY_ENTRIES_TOTAL: usize = 32_768;
const PEER_OPEN_ERROR: &str = "This encrypted message could not be opened";
const MAX_ALIAS_BYTES: usize = 80;
const MAX_ALIAS_CHARS: usize = 48;
const MAX_VISIBLE_WHITELIST_SCOPES: usize = 512;
/// Roster key for the person-level DM approval. `WhitelistEntry::Dm` carries no
/// conversation id, so it has no `Scope::storage_key`; this sentinel can never
/// collide with a real key (every real key contains a `:` separator).
const DM_REACH_STORAGE_KEY: &str = "dm";
const MAX_REACH_NARROWED_SCOPES_PER_PERSON: usize = 512;
const MAX_STORAGE_KEY_BYTES: usize = 512;
const X25519_PUBLIC_BYTES: usize = 32;
const ED25519_PUBLIC_BYTES: usize = 32;
const ED25519_SIGNATURE_BYTES: usize = 64;
const MLKEM768_PUBLIC_BYTES: usize = 1184;
const RATCHET_PUBLIC_BYTES: usize = 32;
const SAFETY_NUMBER_BUNDLE_REFUSAL: &str = "OSL friend key bundle is invalid";
const SAFETY_NUMBER_MISMATCH_REFUSAL: &str = "OSL safety number does not match";
const PENDING_KEY_CHANGE_REFUSAL: &str = "OSL friend key change state is incomplete";

#[derive(Debug, Default)]
pub struct HubSecurityState {
    transition: Mutex<()>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendCodeExport {
    pub friend_code: String,
    pub osl_user_id: String,
    pub safety_number: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AddFriendDisposition {
    Added,
    AlreadyPresent,
    KeyChangeRequiresVerification,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddFriendResult {
    pub disposition: AddFriendDisposition,
    pub person_id: String,
    pub osl_user_id: String,
    pub safety_number: String,
    pub code_signature_valid: bool,
    pub safety_number_verified: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveFriendResult {
    pub person_id: String,
    pub approvals_withdrawn: usize,
    pub peer_key_removed: bool,
    pub revocations_queued: usize,
    pub revocation_queue_complete: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonDto {
    pub person_id: String,
    pub osl_user_id: String,
    pub alias: Option<String>,
    pub safety_number: String,
    pub safety_number_verified: bool,
    pub whitelist_count: usize,
    pub whitelisted_scopes: Vec<PersonWhitelistScopeDto>,
    pub whitelisted_scopes_truncated: bool,
    pub pending_key_change: bool,
    /// True when the user deliberately extended this person's trust to the
    /// other scopes they share. Never set by an ordinary scope approval.
    pub reach_broadened: bool,
    /// When reach was last widened, for the roster's audit line.
    pub reach_broadened_at: Option<String>,
    /// Scope storage keys explicitly taken back from this person; they stay
    /// denied while reach is broadened.
    pub reach_narrowed_scopes: Vec<String>,
}

/// A local-only description of one approved encryption scope. It deliberately
/// contains no service or account handle: current friend codes do not prove
/// either relationship, so OSL must not infer one from a conversation id.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonWhitelistScopeDto {
    pub kind: String,
    pub context_id: Option<String>,
    /// Canonical storage key for this recorded approval — the bare `dm`
    /// sentinel for the person-level DM entry, which carries no conversation
    /// id. The roster sends this value back to revoke exactly one recorded
    /// approval; OSL only ever matches it against keys it recorded itself, so
    /// an unrecognised key revokes nothing.
    pub storage_key: String,
    /// Mirrors the recorded entry's `user_specific` flag: `true` when the
    /// approval covers only this person inside a shared conversation.
    pub user_specific: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeSecurityDto {
    pub storage_key: String,
    pub ttl_seconds: u32,
    pub decrypt_display_enabled: bool,
}

/// The minimum friend state needed to create a manual peer-messaging lease.
/// Key material stays in the original core; callers receive only stable local
/// and public identity identifiers.
#[derive(Clone, Eq, PartialEq)]
pub struct ManualPeerBinding {
    pub person_id: String,
    pub peer_osl_user_id: String,
    pub peer_x25519_public: [u8; X25519_PUBLIC_BYTES],
    pub peer_mlkem768_public: [u8; MLKEM768_PUBLIC_BYTES],
}

impl fmt::Debug for ManualPeerBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ManualPeerBinding")
            .field("person_id", &"[REDACTED]")
            .field("peer_osl_user_id", &"[REDACTED]")
            .field("peer_x25519_public", &"[REDACTED]")
            .field("peer_mlkem768_public", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScopedTrustConsent {
    ExplicitUserAction,
    Absent,
}

/// A friend-scoped local trust capability.
///
/// This is only a local authorization record: it binds one already-verified
/// friend to one exact Hub manual DM scope. It carries no plaintext, provider
/// credential, send authority, or platform account claim, and absence of any of
/// those inputs refuses construction rather than widening trust.
#[derive(Clone, Eq, PartialEq)]
pub struct ScopedTrustGrant {
    person_id: String,
    service_id: String,
    account_id: String,
    binding_commitment: [u8; 32],
    scope: Scope,
    storage_key: String,
}

impl ScopedTrustGrant {
    pub fn for_manual_peer(
        binding: &ManualPeerBinding,
        service_id: &str,
        account_id: &str,
        scope_input: ScopeInput,
        consent: ScopedTrustConsent,
    ) -> Result<Self, String> {
        if consent != ScopedTrustConsent::ExplicitUserAction {
            return Err("OSL scoped trust requires explicit approval".to_owned());
        }
        require_exact_manual_peer_scope_input(&scope_input, "OSL scoped trust scope is invalid")?;
        let scope: Scope = scope_input
            .try_into()
            .map_err(|_| "OSL scoped trust scope is invalid".to_owned())?;
        require_exact_manual_peer_scope(
            service_id,
            account_id,
            &binding.person_id,
            &scope,
            "OSL scoped trust scope is invalid",
        )?;
        Ok(Self {
            person_id: binding.person_id.clone(),
            service_id: service_id.to_owned(),
            account_id: account_id.to_owned(),
            binding_commitment: manual_peer_binding_commitment(binding),
            // Derived, not copied from the caller's scope: the revoke side
            // computes the same key from the same three inputs (A5-F4).
            storage_key: manual_peer_scope_storage_key(service_id, account_id, &binding.person_id)?,
            scope,
        })
    }

    pub fn person_id(&self) -> &str {
        &self.person_id
    }

    pub fn service_id(&self) -> &str {
        &self.service_id
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    pub fn storage_key(&self) -> &str {
        &self.storage_key
    }

    pub fn require_binding(&self, binding: Option<&ManualPeerBinding>) -> Result<(), String> {
        let binding = binding.ok_or_else(|| "OSL scoped trust binding is missing".to_owned())?;
        if binding.person_id.as_str() != self.person_id.as_str()
            || !constant_time_eq_32(
                &manual_peer_binding_commitment(binding),
                &self.binding_commitment,
            )
        {
            return Err("OSL scoped trust binding does not match".to_owned());
        }
        Ok(())
    }
}

impl fmt::Debug for ScopedTrustGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScopedTrustGrant")
            .field("person_id", &"[REDACTED]")
            .field("service_id", &"[REDACTED]")
            .field("account_id", &"[REDACTED]")
            .field("binding_commitment", &"[REDACTED]")
            .field("scope_kind", &self.scope.kind)
            .field("storage_key", &"[REDACTED]")
            .finish()
    }
}

impl fmt::Display for ManualPeerBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ManualPeerBinding([REDACTED])")
    }
}

impl fmt::Display for ScopedTrustGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "ScopedTrustGrant(scope_kind={:?}, identifiers=[REDACTED])",
            self.scope.kind
        )
    }
}

fn manual_peer_binding_commitment(binding: &ManualPeerBinding) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"OSL-SCOPED-TRUST-BINDING-v1");
    for part in [
        binding.person_id.as_bytes(),
        binding.peer_osl_user_id.as_bytes(),
        binding.peer_x25519_public.as_slice(),
        binding.peer_mlkem768_public.as_slice(),
    ] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part);
    }
    let digest = hash.finalize();
    let mut commitment = [0u8; 32];
    commitment.copy_from_slice(&digest);
    commitment
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubScopeBurnResult {
    pub storage_key: String,
    pub rows_destroyed: usize,
    pub channels_destroyed: usize,
    pub whitelist_entries_removed: usize,
    pub remote_blobs_deleted: usize,
    pub remote_blob_deletions_failed: usize,
    pub remote_cleanup_complete: bool,
    pub local_cleanup_complete: bool,
    pub channel_coverage_complete: bool,
    /// Bilateral burn: how many peer revocation notices were queued for
    /// delivery. Queued, not delivered — see `revocation_status`.
    #[serde(default)]
    pub revocations_queued: usize,
    /// False when at least one peer's revocation could not even be queued (a
    /// full outbox, or missing key state for that peer). The local burn still
    /// happened; the operator is told the notice did not.
    #[serde(default)]
    pub revocation_queue_complete: bool,
    /// The three separate claims, in the order they should be shown. Never
    /// collapsed into one sentence and never rendered as "Deleted".
    #[serde(default)]
    pub claims: Vec<String>,
}

/// Delivery state of the burn notices for one conversation. Three separate
/// claims plus one status string, per `docs/design/osl-gui-final-plan.md:494-500`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubRevocationStatusDto {
    pub storage_key: String,
    /// `Sent request` | `Acknowledged by peer` | `Not acknowledged`.
    pub status: String,
    pub peers_pending: usize,
    pub peers_acknowledged: usize,
    pub claims: Vec<String>,
}

/// One revocation the broker must seal and POST. The notice is already CBOR and
/// carries only commitments; the broker's job is the `encrypt_v3` envelope
/// (`MSG_TYPE_REVOCATION`) and the control-inbox POST on the revocation lane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubDueRevocation {
    pub recipient_osl_user_id: String,
    /// Keyserver routing label recorded at queue time.
    ///
    /// **Advisory.** The authoritative label is whatever the broker's own
    /// `native_overlay_relay_scope_id` derives for this conversation — that
    /// helper lives in `broker.rs` and is not reachable from here, so the queue
    /// records the local storage key instead. The broker must derive the label at
    /// send time. That is safe because the derivation is deterministic in the
    /// identity pair, so every retry addresses the same lane and the collapse key
    /// keeps pointing at the same row.
    pub scope_id_label: String,
    pub storage_key: String,
    /// Base64 CBOR [`ipc::control_messages::RevocationNotice`].
    pub notice_b64: String,
    pub burn_id_hex: String,
    /// Opaque `(scope, epoch)` collapse key for the keyserver revocation lane,
    /// 64 lowercase hex. Must be passed verbatim to
    /// `post_control_inbox_lane(.., Some("revocation"), Some(collapse_key_hex))`.
    pub collapse_key_hex: String,
    pub attempts: u32,
}

/// Outcome of applying one inbound peer revocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubInboundRevocation {
    /// Base64 CBOR [`ipc::control_messages::RevocationAck`] to seal as
    /// `MSG_TYPE_REVOCATION_ACK` and post back. Always present, including for a
    /// refusal, so a peer always learns the outcome.
    pub ack_b64: String,
    /// Whether the burn is in force on this side. Applied and already-applied
    /// are the same value here; see [`ipc::revocation::InboundDecision`].
    pub applied: bool,
    /// Which of our conversations it matched, when it matched one.
    pub storage_key: Option<String>,
    /// Highest sender sequence now refused in that conversation.
    pub burn_floor: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FriendCodeUnsigned {
    version: u32,
    osl_user_id: String,
    x25519_public: String,
    ed25519_public: String,
    mlkem768_public: String,
    ratchet_initial_public: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedFriendCode {
    payload: FriendCodeUnsigned,
    signature: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct PersonMetadata {
    osl_user_id: String,
    ed25519_public: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    alias: Option<String>,
    #[serde(default)]
    safety_number_verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pending_ed25519_public: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pending_key_bundle: Option<FriendCodeUnsigned>,
}

#[derive(Default, Serialize, Deserialize)]
struct PeopleFile {
    version: u32,
    #[serde(default)]
    people: BTreeMap<String, PersonMetadata>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct SecurityPreferences {
    version: u32,
    #[serde(default)]
    decrypt_display_by_scope: BTreeMap<String, bool>,
    #[serde(default)]
    manual_approved_scopes: BTreeSet<String>,
    #[serde(default)]
    burned_manual_scopes: BTreeSet<String>,
    /// Person-level reach narrowing: friend id → the scope storage keys the
    /// user has explicitly taken back from that friend. A narrowed scope is
    /// denied even while the friend's reach is broadened, so revoking one
    /// conversation never requires switching reach off first. Recorded here
    /// (alongside the other local trust decisions) rather than in the shared
    /// peer-map schema, and always written before the matching grant is
    /// removed so a failed write can only leave OSL more restrictive.
    #[serde(default)]
    reach_narrowed_scopes: BTreeMap<String, BTreeSet<String>>,
    /// Manual approval attribution: scope storage key → the `person_id` the
    /// approval was granted to. [`manual_peer_scope_id`] is a one-way hash over
    /// service, account and person, so an approved key cannot be attributed to
    /// a friend after the fact without this index.
    ///
    /// It exists because two separate things need the attribution and neither
    /// can guess it: the roster has to display the approvals that are actually
    /// enforced (`manual_approved_scopes`, not the peer map's
    /// `outgoing_whitelists`), and removing a friend has to find every grant
    /// that friend holds. Written in the same transition as the grant itself,
    /// so it can never outlive one.
    #[serde(default)]
    manual_approved_scope_people: BTreeMap<String, String>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PeerReplayLedger {
    version: u32,
    #[serde(default)]
    consumed_by_scope: BTreeMap<String, BTreeMap<String, i64>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachmentBurnEntry {
    object_id: String,
    fetch_token: String,
    expires_at: i64,
}

impl Drop for AttachmentBurnEntry {
    fn drop(&mut self) {
        self.fetch_token.zeroize();
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachmentBurnLedger {
    version: u32,
    #[serde(default)]
    entries_by_scope: BTreeMap<String, Vec<AttachmentBurnEntry>>,
}

fn friend_code_key_bundle(payload: &FriendCodeUnsigned) -> Result<KeyBundle, String> {
    Ok(KeyBundle {
        ed25519_pub: standard_base64(&payload.ed25519_public)?,
        x25519_pub: standard_base64(&payload.x25519_public)?,
        mlkem768_pub: standard_base64(&payload.mlkem768_public)?,
        ratchet_initial_pub: payload
            .ratchet_initial_public
            .as_deref()
            .map(standard_base64)
            .transpose()?,
    })
}

fn safety_number_for_bundle(bundle: &KeyBundle) -> Result<String, String> {
    ipc::tofu::safety_number(bundle).map_err(|_| SAFETY_NUMBER_BUNDLE_REFUSAL.to_owned())
}

fn reject_discord_identifier_identity(osl_user_id: &str) -> Result<(), String> {
    // Native OSL user ids are `osl_` plus hex, so they can never be all digits.
    // Refusing a Discord-snowflake-shaped value only rejects identities that
    // migration 0029 guarantees cannot resolve on the keyserver.
    if is_discord_snowflake_shaped(osl_user_id) {
        return Err(SNOWFLAKE_IDENTITY_REFUSAL.to_owned());
    }
    Ok(())
}

fn stage_pending_key_bundle(
    metadata: &mut PersonMetadata,
    payload: &FriendCodeUnsigned,
    trusted: &KeyBundle,
    presented: &KeyBundle,
) -> bool {
    if trusted == presented {
        return false;
    }
    metadata.safety_number_verified = false;
    metadata.pending_ed25519_public = None;
    metadata.pending_key_bundle = Some(payload.clone());
    true
}

pub fn export_friend_code(core: &HubCoreState) -> Result<FriendCodeExport, String> {
    require_unlocked()?;
    let identity = core
        .osl
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())?
        .clone()
        .ok_or_else(|| "OSL identity is not loaded".to_owned())?;
    let payload = FriendCodeUnsigned {
        version: FRIEND_CODE_VERSION,
        osl_user_id: identity.user_id.clone(),
        x25519_public: STANDARD.encode(identity.x25519_public.as_bytes()),
        ed25519_public: STANDARD.encode(identity.ed25519_public.as_bytes()),
        mlkem768_public: STANDARD.encode(identity.mlkem_public_bytes),
        ratchet_initial_public: identity
            .ratchet_initial_pub
            .map(|key| STANDARD.encode(key.as_bytes())),
    };
    let canonical = serde_json::to_vec(&payload)
        .map_err(|_| "OSL friend code could not be encoded".to_owned())?;
    let signature = crypto::ed25519::sign(&identity.ed25519_secret, &canonical);
    let signed = SignedFriendCode {
        payload,
        signature: URL_SAFE_NO_PAD.encode(signature.as_bytes()),
    };
    let safety_number = safety_number_for_bundle(&friend_code_key_bundle(&signed.payload)?)?;
    let encoded = serde_json::to_vec(&signed)
        .map_err(|_| "OSL friend code could not be encoded".to_owned())?;
    Ok(FriendCodeExport {
        friend_code: format!("{FRIEND_CODE_PREFIX}{}", URL_SAFE_NO_PAD.encode(encoded)),
        // `Identity` zeroizes on drop, so its fields cannot be moved out of —
        // the export gets a copy and the original is still wiped on the way out.
        osl_user_id: identity.user_id.clone(),
        safety_number,
    })
}

pub fn add_friend_code(
    core: &HubCoreState,
    security: &HubSecurityState,
    friend_code: String,
    alias: Option<String>,
) -> Result<AddFriendResult, String> {
    require_unlocked()?;
    let alias = normalise_alias(alias.as_deref())?;
    let parsed = parse_friend_code(&friend_code)?;
    reject_discord_identifier_identity(&parsed.payload.osl_user_id)?;
    let self_user_id = active_user_id(core)?;
    if parsed.payload.osl_user_id == self_user_id {
        return Err("OSL refuses to add the active identity as a friend".to_owned());
    }
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL People state is unavailable".to_owned())?;
    let dir = config_dir()?;
    let mut people = load_encrypted_json::<PeopleFile>(&dir.join(PEOPLE_FILE))?;

    if let Some(existing_id) = people
        .people
        .iter()
        .find(|(_, value)| value.osl_user_id == parsed.payload.osl_user_id)
        .map(|(person_id, _)| person_id.clone())
    {
        // Refuse an identity-key change here, BEFORE the stored record is
        // touched at all.
        //
        // `osl_user_id` is an attacker-chosen field inside a *self*-signed
        // code: the signature only proves possession of `ed25519_public`, so
        // matching on the user id proves nothing except that this code
        // *claims* to be someone OSL already knows. The identity OSL actually
        // trusts is the signing key, and `person_id` is derived from it.
        // Rebinding an existing entry to a new key would hand that key every
        // scope the operator approved for the old one — `manual_peer_scope_id`
        // is keyed on `person_id`, so no approval would even look different.
        //
        // A same-Ed25519 transport-bundle update is different: the existing
        // identity key signs it, but the new transport keys are not adopted
        // until the operator verifies their complete-bundle number below.
        if person_id(&parsed.payload.ed25519_public) != existing_id
            || people
                .people
                .get(&existing_id)
                .is_none_or(|value| value.ed25519_public != parsed.payload.ed25519_public)
        {
            return Err(
                "OSL refuses this invite: it claims a friend you already have but carries a \
                 different identity key. Add them as a new friend and verify the new safety \
                 number with them."
                    .to_owned(),
            );
        }
        let existing_metadata = people
            .people
            .get(&existing_id)
            .ok_or_else(|| "OSL friend is unknown".to_owned())?;
        let existing_peer = core
            .osl
            .peer_map
            .lock()
            .map_err(|_| "OSL peer state is unavailable".to_owned())?
            .get(&existing_id)
            .cloned()
            .ok_or_else(|| "OSL friend key state is missing".to_owned())?;
        validate_manual_peer_identity(&existing_id, existing_metadata, &existing_peer)?;
        let trusted_bundle =
            trusted_peer_key_bundle(&existing_id, existing_metadata, &existing_peer)?;
        let presented_bundle = friend_code_key_bundle(&parsed.payload)?;
        let presented_safety_number = safety_number_for_bundle(&presented_bundle)?;
        let existing = people
            .people
            .get_mut(&existing_id)
            .ok_or_else(|| "OSL friend is unknown".to_owned())?;
        let alias_changed = alias.is_some();
        if alias_changed {
            existing.alias = alias;
        }
        if stage_pending_key_bundle(
            existing,
            &parsed.payload,
            &trusted_bundle,
            &presented_bundle,
        ) {
            write_encrypted_json(&dir.join(PEOPLE_FILE), &people)?;
            return Ok(AddFriendResult {
                disposition: AddFriendDisposition::KeyChangeRequiresVerification,
                person_id: existing_id,
                osl_user_id: parsed.payload.osl_user_id.clone(),
                safety_number: presented_safety_number,
                code_signature_valid: true,
                safety_number_verified: false,
            });
        }
        let safety_number_verified = existing.safety_number_verified;
        if alias_changed {
            write_encrypted_json(&dir.join(PEOPLE_FILE), &people)?;
        }
        return Ok(AddFriendResult {
            disposition: AddFriendDisposition::AlreadyPresent,
            person_id: existing_id,
            osl_user_id: parsed.payload.osl_user_id.clone(),
            safety_number: presented_safety_number,
            code_signature_valid: true,
            safety_number_verified,
        });
    }

    let person_id = person_id(&parsed.payload.ed25519_public);
    // The same refusal from the other side: a code carrying a key OSL already
    // knows, but claiming a different `osl_user_id`, would otherwise insert
    // over that person's record — resetting their verification and replacing
    // their peer entry — without the operator ever being told a friend changed.
    if people.people.contains_key(&person_id) {
        return Err(
            "OSL already knows this identity key under another friend. Remove that friend \
             first if you are replacing them."
                .to_owned(),
        );
    }
    let peer = peer_entry(&parsed.payload)?;
    let safety_number = safety_number_for_bundle(&friend_code_key_bundle(&parsed.payload)?)?;
    let mut peer_map = core
        .osl
        .peer_map
        .lock()
        .map_err(|_| "OSL peer state is unavailable".to_owned())?
        .clone();
    peer_map.insert(person_id.clone(), peer);
    write_encrypted_json(&dir.join("peer_map.json"), &peer_map)
        .map_err(|_| "OSL friend keys could not be persisted".to_owned())?;
    people.people.insert(
        person_id.clone(),
        PersonMetadata {
            osl_user_id: parsed.payload.osl_user_id.clone(),
            ed25519_public: parsed.payload.ed25519_public.clone(),
            alias,
            safety_number_verified: false,
            pending_ed25519_public: None,
            pending_key_bundle: None,
        },
    );
    if let Err(error) = write_encrypted_json(&dir.join(PEOPLE_FILE), &people) {
        peer_map.remove(&person_id);
        let _ = write_encrypted_json(&dir.join("peer_map.json"), &peer_map);
        return Err(error);
    }
    *core
        .osl
        .peer_map
        .lock()
        .map_err(|_| "OSL peer state is unavailable".to_owned())? = peer_map;
    Ok(AddFriendResult {
        disposition: AddFriendDisposition::Added,
        person_id,
        osl_user_id: parsed.payload.osl_user_id.clone(),
        safety_number,
        code_signature_valid: true,
        safety_number_verified: false,
    })
}

/// Remove one friend and withdraw every manual conversation approval attributed
/// to them. Removal is unilateral: revocation notices are best-effort and can
/// never roll the local trust decision back.
pub fn remove_friend(
    core: &HubCoreState,
    security: &HubSecurityState,
    person_id: String,
) -> Result<RemoveFriendResult, String> {
    require_unlocked()?;
    validate_person_id(&person_id)?;
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL People state is unavailable".to_owned())?;
    let dir = config_dir()?;
    let people_path = dir.join(PEOPLE_FILE);
    let mut people = load_encrypted_json::<PeopleFile>(&people_path)?;
    if !people.people.contains_key(&person_id) {
        return Err("OSL friend is unknown".to_owned());
    }

    // Resolve who must be told BEFORE any trust or key state is removed. The
    // binding depends on the People record and peer key this transition is
    // about to delete; resolving afterwards would produce an empty recipient
    // set and the notice would go to nobody. Broken key state does not make a
    // friend impossible to remove — it only makes notification incomplete.
    let revocation_peers = match manual_peer_binding(core, person_id.clone()) {
        Ok(binding) => {
            RevocationRecipients::single(binding.peer_osl_user_id, binding.peer_x25519_public)
        }
        Err(_) => RevocationRecipients::unresolved(),
    };

    let prefs_path = dir.join(SECURITY_PREFS_FILE);
    let mut prefs = load_encrypted_json::<SecurityPreferences>(&prefs_path)?;
    let withdrawn_scope_keys: Vec<String> = prefs
        .manual_approved_scope_people
        .iter()
        .filter(|(_, approved_person_id)| *approved_person_id == &person_id)
        .map(|(storage_key, _)| storage_key.clone())
        .collect();
    let approvals_withdrawn = withdraw_person_grants(&mut prefs, &person_id);
    write_encrypted_json(&prefs_path, &prefs)
        .map_err(|_| "OSL friend approvals could not be withdrawn".to_owned())?;

    let previous_peers = core
        .osl
        .peer_map
        .lock()
        .map_err(|_| "OSL peer state is unavailable".to_owned())?
        .clone();
    let mut peers = previous_peers.clone();
    let peer_key_removed = peers.remove(&person_id).is_some();
    people.people.remove(&person_id);
    persist_friend_removal(&dir, &previous_peers, &peers, &people)?;
    *core
        .osl
        .peer_map
        .lock()
        .map_err(|_| "OSL peer state is unavailable".to_owned())? = peers;

    // Notice LAST. The local friend, keys and grants are already gone, so a
    // queue failure is reported but never propagated or used to undo removal.
    // No withdrawn approvals means there was nothing to notify anybody about, so
    // the phase starts complete and only a scope that fails to queue — including
    // one whose recipient could not be resolved above — can take it away.
    let mut revocations_queued = 0usize;
    let mut revocation_queue_complete = true;
    let now = ipc::main_password::now_unix_secs_pub();
    for storage_key in &withdrawn_scope_keys {
        match queue_scope_revocations_locked(
            core,
            storage_key,
            storage_key,
            &revocation_peers,
            &[],
            now,
        ) {
            Ok((queued, complete)) => {
                revocations_queued = revocations_queued.saturating_add(queued);
                revocation_queue_complete &= complete;
            }
            Err(_) => revocation_queue_complete = false,
        }
    }

    Ok(RemoveFriendResult {
        person_id,
        approvals_withdrawn,
        peer_key_removed,
        revocations_queued,
        revocation_queue_complete,
    })
}

/// Persist one friend removal across both files that hold the friend, keys
/// first, with the same write-then-rollback ordering `add_friend_code` uses.
///
/// The two files disagree in opposite directions, and only one of the two
/// disagreements is dangerous. A peer key with no People record is an
/// unattributed key OSL will still seal to and still match against an inbound
/// sender — the friend is *not* removed. A People record with no peer key is
/// inert: there is nothing to encrypt to and nothing to attribute. So the key
/// is destroyed first, and a failed People write restores the key state rather
/// than leaving the operator with a friend they were told was gone.
fn persist_friend_removal(
    dir: &Path,
    previous_peers: &ipc::peer_map::PeerMap,
    peers: &ipc::peer_map::PeerMap,
    people: &PeopleFile,
) -> Result<(), String> {
    let peer_map_path = dir.join("peer_map.json");
    write_encrypted_json(&peer_map_path, peers)
        .map_err(|_| "OSL friend keys could not be removed".to_owned())?;
    if let Err(error) = write_encrypted_json(&dir.join(PEOPLE_FILE), people) {
        // Best-effort restore: if this also fails the on-disk state is the
        // *safer* half-state (key gone, record present), never the reverse.
        let _ = write_encrypted_json(&peer_map_path, previous_peers);
        return Err(error);
    }
    Ok(())
}

/// Complete the safety-number ceremony for one friend.
///
/// `safety_number` is what the **operator typed**, from the number their peer
/// read to them out of band. It is never the number OSL just displayed: a value
/// the app hands to itself and back records only that a button was pressed.
///
/// The comparison is against the key OSL currently holds for this person, and
/// nothing else. An unaccepted key-change claim is discarded rather than
/// adopted — see `add_friend_code` for why a rotation is a new person, not a
/// rekey of this one — so completing the ceremony against the trusted key both
/// records the verification and clears the claim that was blocking the friend.
pub fn verify_friend_safety_number(
    core: &HubCoreState,
    security: &HubSecurityState,
    person_id: String,
    safety_number: String,
) -> Result<PersonDto, String> {
    require_unlocked()?;
    validate_person_id(&person_id)?;
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL People state is unavailable".to_owned())?;
    let dir = config_dir()?;
    let mut people = load_encrypted_json::<PeopleFile>(&dir.join(PEOPLE_FILE))?;
    let metadata = people
        .people
        .get(&person_id)
        .cloned()
        .ok_or_else(|| "OSL friend is unknown".to_owned())?;
    let previous_peer_map = core
        .osl
        .peer_map
        .lock()
        .map_err(|_| "OSL peer state is unavailable".to_owned())?
        .clone();
    let current_peer = previous_peer_map
        .get(&person_id)
        .cloned()
        .ok_or_else(|| "OSL friend key state is missing".to_owned())?;
    validate_manual_peer_identity(&person_id, &metadata, &current_peer)?;
    let pending = metadata.pending_key_bundle.as_ref();
    if pending.is_none() && metadata.pending_ed25519_public.is_some() {
        return Err(PENDING_KEY_CHANGE_REFUSAL.to_owned());
    }
    let expected_bundle = match pending {
        Some(payload) => {
            if crate::security::person_id(&payload.ed25519_public) != person_id
                || payload.ed25519_public != metadata.ed25519_public
                || payload.osl_user_id != metadata.osl_user_id
            {
                return Err(PENDING_KEY_CHANGE_REFUSAL.to_owned());
            }
            reject_discord_identifier_identity(&payload.osl_user_id)?;
            friend_code_key_bundle(payload)?
        }
        None => trusted_peer_key_bundle(&person_id, &metadata, &current_peer)?,
    };
    let expected = safety_number_for_bundle(&expected_bundle)?;
    if !safety_number_matches(&expected, &safety_number) {
        return Err(SAFETY_NUMBER_MISMATCH_REFUSAL.to_owned());
    }

    let mut next_peer_map = previous_peer_map.clone();
    if let Some(payload) = pending {
        let next_peer = next_peer_map
            .get_mut(&person_id)
            .ok_or_else(|| "OSL friend key state is missing".to_owned())?;
        next_peer.osl_user_id = Some(payload.osl_user_id.clone());
        next_peer.pubkey = Some(expected_bundle.x25519_pub.clone());
        next_peer.ik_mlkem768_pub = Some(expected_bundle.mlkem768_pub.clone());
        next_peer.ik_ratchet_initial_pub = expected_bundle.ratchet_initial_pub.clone();
        next_peer.ratchet_state = None;
        next_peer.tofu_ed25519_pub = Some(expected_bundle.ed25519_pub.clone());
        next_peer.tofu_key_bundle = Some(expected_bundle);
        write_encrypted_json(&dir.join("peer_map.json"), &next_peer_map)
            .map_err(|_| "OSL friend keys could not be persisted".to_owned())?;
    }

    let updated = people
        .people
        .get_mut(&person_id)
        .ok_or_else(|| "OSL friend is unknown".to_owned())?;
    updated.pending_ed25519_public = None;
    updated.pending_key_bundle = None;
    updated.safety_number_verified = true;
    let updated = updated.clone();
    if let Err(error) = write_encrypted_json(&dir.join(PEOPLE_FILE), &people) {
        if pending.is_some() {
            let _ = write_encrypted_json(&dir.join("peer_map.json"), &previous_peer_map);
        }
        return Err(error);
    }
    if pending.is_some() {
        *core
            .osl
            .peer_map
            .lock()
            .map_err(|_| "OSL peer state is unavailable".to_owned())? = next_peer_map;
    }
    person_dto(core, &person_id, &updated, &load_security_preferences()?)
}

pub fn list_people(core: &HubCoreState) -> Result<Vec<PersonDto>, String> {
    require_unlocked()?;
    let people = load_encrypted_json::<PeopleFile>(&config_dir()?.join(PEOPLE_FILE))?;
    let prefs = load_security_preferences()?;
    people
        .people
        .iter()
        .map(|(person_id, metadata)| person_dto(core, person_id, metadata, &prefs))
        .collect()
}

/// Set or clear a user-owned nickname for one friend. The nickname is written
/// only to the encrypted device-local People file; it is never included in a
/// friend code, peer key lookup, or Cloudflare request.
pub fn set_friend_alias(
    core: &HubCoreState,
    security: &HubSecurityState,
    person_id: String,
    alias: Option<String>,
) -> Result<PersonDto, String> {
    require_unlocked()?;
    validate_person_id(&person_id)?;
    let alias = normalise_alias(alias.as_deref())?;
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL People state is unavailable".to_owned())?;
    let dir = config_dir()?;
    let mut people = load_encrypted_json::<PeopleFile>(&dir.join(PEOPLE_FILE))?;
    let metadata = people
        .people
        .get_mut(&person_id)
        .ok_or_else(|| "OSL friend is unknown".to_owned())?;
    metadata.alias = alias;
    let updated = metadata.clone();
    write_encrypted_json(&dir.join(PEOPLE_FILE), &people)?;
    person_dto(core, &person_id, &updated, &load_security_preferences()?)
}

/// Grant or revoke one friend's approval for exactly one scope.
///
/// Deliberately reach-neutral: entries are always written non-broadened, so an
/// ordinary approval can never widen a person's trust to the other scopes
/// shared with them. Widening is a separate, recorded action —
/// [`set_friend_scope_reach`].
///
/// Approving clears any recorded narrowing for the exact scope (the newer,
/// narrower decision wins). Revoking records a narrowing whenever the friend's
/// reach is broadened, so a revocation takes effect immediately and never
/// requires switching reach off first. The narrowing is written before the
/// grant is removed, so a failed write can only leave OSL more restrictive.
pub fn set_friend_scope_permission(
    core: &HubCoreState,
    security: &HubSecurityState,
    person_id: String,
    scope_input: ScopeInput,
    enabled: bool,
) -> Result<(), String> {
    require_unlocked()?;
    validate_person_id(&person_id)?;
    let scope: Scope = scope_input
        .try_into()
        .map_err(|_| "OSL scope is invalid".to_owned())?;
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL People state is unavailable".to_owned())?;
    let dir = config_dir()?;
    if enabled {
        let people = load_encrypted_json::<PeopleFile>(&dir.join(PEOPLE_FILE))?;
        let metadata = people
            .people
            .get(&person_id)
            .ok_or_else(|| "OSL friend is unknown".to_owned())?;
        ensure_friend_can_be_enabled(metadata)?;
    }
    let prefs_path = dir.join(SECURITY_PREFS_FILE);
    let previous_prefs = load_encrypted_json::<SecurityPreferences>(&prefs_path)?;
    let mut prefs = previous_prefs.clone();
    prefs.version = 2;
    let storage_key = scope.storage_key();
    let previous_peers = core
        .osl
        .peer_map
        .lock()
        .map_err(|_| "OSL peer state is unavailable".to_owned())?
        .clone();
    let mut peers = previous_peers.clone();
    let peer = peers
        .get_mut(&person_id)
        .ok_or_else(|| "OSL friend is unknown".to_owned())?;
    // `true`, not the recorded attribution, and this is the one site where that
    // is right. Everywhere else `whitelist_entry_matches_scope` answers a query
    // — "does some *other* friend's entry cover this scope?" — and must refuse
    // to guess. Here the caller has named both the person and the scope, and we
    // are replacing that person's own entry for it. A person holds at most one
    // `Dm` entry, so it is theirs by construction.
    //
    // Deriving it from `manual_approved_scope_people` instead would fail in the
    // widening direction: an unattributed DM scope would leave the old entry in
    // place, so revoking could not clear a `broadened` reach, and re-enabling
    // would append a duplicate entry every time.
    peer.outgoing_whitelists
        .retain(|entry| !whitelist_entry_matches_scope(entry, &scope, true));
    if enabled {
        peer.outgoing_whitelists
            .push(whitelist_entry(&scope, false));
        clear_reach_narrowing(&mut prefs, &person_id, &storage_key);
    } else if person_reach_broadened_at(&peer.outgoing_whitelists).is_some()
        && !record_reach_narrowing(&mut prefs, &person_id, &storage_key)
    {
        // The exclusion list is full: withdraw the person's reach entirely
        // rather than leave a scope the user just revoked inside it.
        collapse_person_reach(&mut peer.outgoing_whitelists);
    }
    let previous_whitelist_state = core
        .osl
        .whitelist_state
        .lock()
        .map_err(|_| "OSL whitelist state is unavailable".to_owned())?
        .clone();
    let mut whitelist_state = previous_whitelist_state.clone();
    if enabled {
        let scope_state = whitelist_state.entry(storage_key.clone()).or_default();
        scope_state.encrypt_toggle = true;
        scope_state.auto_enabled = true;
    } else {
        let another_approved_friend = peers.iter().any(|(candidate_id, candidate)| {
            whitelist_matches(
                &candidate.outgoing_whitelists,
                &scope,
                dm_scope_person(&prefs, &scope) == Some(candidate_id.as_str()),
                prefs.reach_narrowed_scopes.get(candidate_id),
            )
        });
        revoke_auto_scope_if_uncovered(&mut whitelist_state, &storage_key, another_approved_friend);
    }
    let server_defaults = core
        .osl
        .server_defaults
        .lock()
        .map_err(|_| "OSL server-default state is unavailable".to_owned())?
        .clone();
    let whitelist_document = ipc::whitelist_state::WhitelistStateFile {
        migrated_c1: true,
        scopes: whitelist_state.clone(),
        server_defaults: server_defaults.clone(),
    };
    // Revocation persists the restrictive record first: if the grant removal
    // below fails, the scope is already excluded from any broadened reach.
    if !enabled {
        write_encrypted_json(&prefs_path, &prefs)
            .map_err(|_| "OSL whitelist could not be persisted".to_owned())?;
    }
    write_encrypted_json(&dir.join("peer_map.json"), &peers)
        .map_err(|_| "OSL whitelist could not be persisted".to_owned())?;
    // The legacy IPC convenience writer drops server_defaults and its raw
    // rename cannot replace an existing destination on Windows. OSL Privacy writes
    // the complete envelope through its authenticated, recoverable path.
    if write_encrypted_json(&dir.join("whitelist_state.json"), &whitelist_document).is_err() {
        let _ = write_encrypted_json(&dir.join("peer_map.json"), &previous_peers);
        return Err("OSL whitelist could not be persisted".to_owned());
    }
    // An approval only relaxes the exclusion list, so it is written last: a
    // failure here rolls the grant back instead of dropping the exclusion.
    if enabled {
        if let Err(error) = write_encrypted_json(&prefs_path, &prefs) {
            let _ = write_encrypted_json(&dir.join("peer_map.json"), &previous_peers);
            let _ = write_encrypted_json(
                &dir.join("whitelist_state.json"),
                &ipc::whitelist_state::WhitelistStateFile {
                    migrated_c1: true,
                    scopes: previous_whitelist_state,
                    server_defaults,
                },
            );
            return Err(error);
        }
    }
    *core
        .osl
        .peer_map
        .lock()
        .map_err(|_| "OSL peer state is unavailable".to_owned())? = peers;
    *core
        .osl
        .whitelist_state
        .lock()
        .map_err(|_| "OSL whitelist state is unavailable".to_owned())? = whitelist_state;
    Ok(())
}

/// Extend or withdraw one verified friend's person-level reach.
///
/// This is the only path that may set `broadened`, and it exists precisely so
/// that widening trust is a separate, deliberate act the roster can audit: the
/// ordinary approve path always writes non-broadened entries and
/// `Broker::manual_permission_target` still refuses a broadened request.
///
/// Reach extends trust the user already granted, so a friend with no recorded
/// approval at all cannot be broadened. Reach is anchored on the person-level
/// DM entry, so widening also records the DM approval itself — that is what
/// "trust this person across the chats we share" means, and the roster lists it
/// as its own revocable row. Withdrawing reach leaves the per-scope approvals
/// (including the DM) exactly as they were and keeps every recorded narrowing,
/// so re-broadening later cannot silently re-grant a scope the user took back.
pub fn set_friend_scope_reach(
    core: &HubCoreState,
    security: &HubSecurityState,
    service_id: &str,
    account_id: &str,
    person_id: String,
    broadened: bool,
) -> Result<PersonDto, String> {
    require_unlocked()?;
    validate_person_id(&person_id)?;
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL People state is unavailable".to_owned())?;
    let dir = config_dir()?;
    let people = load_encrypted_json::<PeopleFile>(&dir.join(PEOPLE_FILE))?;
    let metadata = people
        .people
        .get(&person_id)
        .ok_or_else(|| "OSL friend is unknown".to_owned())?
        .clone();
    if broadened {
        ensure_friend_can_be_enabled(&metadata)?;
    }
    let previous_peers = core
        .osl
        .peer_map
        .lock()
        .map_err(|_| "OSL peer state is unavailable".to_owned())?
        .clone();
    let mut peers = previous_peers.clone();
    let peer = peers
        .get_mut(&person_id)
        .ok_or_else(|| "OSL friend is unknown".to_owned())?;
    if broadened {
        // Reach widens trust that already exists: either a recorded per-scope
        // approval, or the approved conversation the caller is standing in.
        let approved_here = manual_scope_preference_approved(
            &load_encrypted_json::<SecurityPreferences>(&dir.join(SECURITY_PREFS_FILE))?,
            &Scope::dm(manual_peer_scope_id(service_id, account_id, &person_id)?).storage_key(),
        );
        if peer.outgoing_whitelists.is_empty() && !approved_here {
            return Err("Approve this friend for one chat before widening their reach".to_owned());
        }
        let now = ipc::main_password::now_unix_secs_pub().to_string();
        let mut recorded = false;
        for entry in peer.outgoing_whitelists.iter_mut() {
            if let WhitelistEntry::Dm {
                broadened: entry_broadened,
                enabled_at,
            } = entry
            {
                *entry_broadened = true;
                // The reach change is the audited event on this entry.
                *enabled_at = Some(now.clone());
                recorded = true;
            }
        }
        if !recorded {
            peer.outgoing_whitelists.push(WhitelistEntry::Dm {
                broadened: true,
                enabled_at: Some(now),
            });
        }
    } else {
        collapse_person_reach(&mut peer.outgoing_whitelists);
    }
    if peers
        .get(&person_id)
        .map(|entry| entry.outgoing_whitelists.len())
        .unwrap_or_default()
        > MAX_VISIBLE_WHITELIST_SCOPES
    {
        return Err("OSL friend has too many approved chats".to_owned());
    }
    write_encrypted_json(&dir.join("peer_map.json"), &peers)
        .map_err(|_| "OSL whitelist could not be persisted".to_owned())?;
    *core
        .osl
        .peer_map
        .lock()
        .map_err(|_| "OSL peer state is unavailable".to_owned())? = peers;
    person_dto(core, &person_id, &metadata, &load_security_preferences()?)
}

/// Revoke exactly one recorded approval from the roster.
///
/// `storage_key` selects an approval OSL itself recorded — it is matched
/// against the keys of that friend's own entries and never parsed into a new
/// scope, so the renderer cannot name a conversation OSL never approved. A
/// non-DM revocation also records a narrowing, so the scope stays denied while
/// the friend's reach is broadened.
pub fn revoke_friend_scope_entry(
    core: &HubCoreState,
    security: &HubSecurityState,
    service_id: &str,
    account_id: &str,
    person_id: String,
    storage_key: String,
) -> Result<PersonDto, String> {
    require_unlocked()?;
    validate_person_id(&person_id)?;
    validate_storage_key(&storage_key)?;
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL People state is unavailable".to_owned())?;
    let dir = config_dir()?;
    let people = load_encrypted_json::<PeopleFile>(&dir.join(PEOPLE_FILE))?;
    let metadata = people
        .people
        .get(&person_id)
        .ok_or_else(|| "OSL friend is unknown".to_owned())?
        .clone();
    let prefs_path = dir.join(SECURITY_PREFS_FILE);
    let mut prefs = load_encrypted_json::<SecurityPreferences>(&prefs_path)?;
    prefs.version = 2;

    // A5-F4. Every scope row the roster renders comes from the manual approval
    // ledger (`manual_approved_scopes_for_person`), whose keys are built by
    // `manual_peer_scope_storage_key`. The peer-map search below can only ever
    // see a person-level DM entry keyed `dm`, so without this branch the
    // roster's own "revoke" button matched nothing and the grant survived.
    if storage_key == manual_peer_scope_storage_key(service_id, account_id, &person_id)? {
        if prefs
            .manual_approved_scope_people
            .get(&storage_key)
            .map(String::as_str)
            != Some(person_id.as_str())
            && !prefs.manual_approved_scopes.contains(&storage_key)
        {
            return Err("OSL friend approval is unknown".to_owned());
        }
        withdraw_manual_scope_grant(&mut prefs, &storage_key);
        write_encrypted_json(&prefs_path, &prefs)
            .map_err(|_| "OSL whitelist could not be persisted".to_owned())?;
        return person_dto(core, &person_id, &metadata, &prefs);
    }

    let previous_peers = core
        .osl
        .peer_map
        .lock()
        .map_err(|_| "OSL peer state is unavailable".to_owned())?
        .clone();
    let mut peers = previous_peers.clone();
    let peer = peers
        .get_mut(&person_id)
        .ok_or_else(|| "OSL friend is unknown".to_owned())?;
    let before = peer.outgoing_whitelists.len();
    peer.outgoing_whitelists
        .retain(|entry| whitelist_entry_storage_key(entry) != storage_key);
    if peer.outgoing_whitelists.len() == before {
        return Err("OSL friend approval is unknown".to_owned());
    }
    if storage_key != DM_REACH_STORAGE_KEY
        && person_reach_broadened_at(&peer.outgoing_whitelists).is_some()
        && !record_reach_narrowing(&mut prefs, &person_id, &storage_key)
    {
        collapse_person_reach(&mut peer.outgoing_whitelists);
    }
    // Restrictive record first: a failure below cannot leave the revoked scope
    // reachable through a broadened reach.
    write_encrypted_json(&prefs_path, &prefs)
        .map_err(|_| "OSL whitelist could not be persisted".to_owned())?;
    let whitelist_state = revoked_scope_whitelist_state(core, &peers, &prefs, &storage_key)?;
    write_encrypted_json(&dir.join("peer_map.json"), &peers)
        .map_err(|_| "OSL whitelist could not be persisted".to_owned())?;
    if let Some((state, document)) = whitelist_state {
        if write_encrypted_json(&dir.join("whitelist_state.json"), &document).is_err() {
            let _ = write_encrypted_json(&dir.join("peer_map.json"), &previous_peers);
            return Err("OSL whitelist could not be persisted".to_owned());
        }
        *core
            .osl
            .whitelist_state
            .lock()
            .map_err(|_| "OSL whitelist state is unavailable".to_owned())? = state;
    }
    *core
        .osl
        .peer_map
        .lock()
        .map_err(|_| "OSL peer state is unavailable".to_owned())? = peers;
    person_dto(core, &person_id, &metadata, &prefs)
}

/// Drop the auto-enabled encryption toggle for a revoked scope when no friend
/// still covers it. Returns `None` for the person-level DM key, which has no
/// `whitelist_state.json` scope of its own.
#[allow(clippy::type_complexity)]
fn revoked_scope_whitelist_state(
    core: &HubCoreState,
    peers: &ipc::peer_map::PeerMap,
    prefs: &SecurityPreferences,
    storage_key: &str,
) -> Result<
    Option<(
        ipc::whitelist_state::WhitelistState,
        ipc::whitelist_state::WhitelistStateFile,
    )>,
    String,
> {
    let Some(scope) = Scope::parse(storage_key) else {
        return Ok(None);
    };
    let mut whitelist_state = core
        .osl
        .whitelist_state
        .lock()
        .map_err(|_| "OSL whitelist state is unavailable".to_owned())?
        .clone();
    let another_approved_friend = peers.iter().any(|(candidate_id, candidate)| {
        whitelist_matches(
            &candidate.outgoing_whitelists,
            &scope,
            dm_scope_person(prefs, &scope) == Some(candidate_id.as_str()),
            prefs.reach_narrowed_scopes.get(candidate_id),
        )
    });
    revoke_auto_scope_if_uncovered(&mut whitelist_state, storage_key, another_approved_friend);
    let server_defaults = core
        .osl
        .server_defaults
        .lock()
        .map_err(|_| "OSL server-default state is unavailable".to_owned())?
        .clone();
    let document = ipc::whitelist_state::WhitelistStateFile {
        migrated_c1: true,
        scopes: whitelist_state.clone(),
        server_defaults,
    };
    Ok(Some((whitelist_state, document)))
}

/// Record that `storage_key` is explicitly taken back from `person_id`.
/// Returns `false` when that friend's exclusion list is full; the caller then
/// withdraws their reach instead, which is strictly more restrictive.
fn record_reach_narrowing(
    prefs: &mut SecurityPreferences,
    person_id: &str,
    storage_key: &str,
) -> bool {
    let keys = prefs
        .reach_narrowed_scopes
        .entry(person_id.to_owned())
        .or_default();
    if keys.contains(storage_key) {
        return true;
    }
    if keys.len() >= MAX_REACH_NARROWED_SCOPES_PER_PERSON {
        return false;
    }
    keys.insert(storage_key.to_owned());
    true
}

/// Forget one recorded narrowing. Only an explicit approval of that exact scope
/// clears it; withdrawing reach deliberately does not.
fn clear_reach_narrowing(prefs: &mut SecurityPreferences, person_id: &str, storage_key: &str) {
    let Some(keys) = prefs.reach_narrowed_scopes.get_mut(person_id) else {
        return;
    };
    keys.remove(storage_key);
    if keys.is_empty() {
        prefs.reach_narrowed_scopes.remove(person_id);
    }
}

/// Withdraw person-level reach without touching any per-scope approval.
fn collapse_person_reach(entries: &mut [WhitelistEntry]) {
    for entry in entries.iter_mut() {
        if let WhitelistEntry::Dm { broadened, .. } = entry {
            *broadened = false;
        }
    }
}

/// Prove that the People record and peer-map record name the same identity.
fn validate_manual_peer_identity(
    person_id: &str,
    metadata: &PersonMetadata,
    peer: &PeerEntry,
) -> Result<(), String> {
    if peer.osl_user_id.as_deref() != Some(metadata.osl_user_id.as_str()) {
        return Err("OSL friend identity state does not match".to_owned());
    }
    if crate::security::person_id(&metadata.ed25519_public) != person_id {
        return Err("OSL friend identity key does not match its recorded identity".to_owned());
    }
    if peer
        .tofu_ed25519_pub
        .as_deref()
        .is_none_or(|recorded| crate::security::person_id(recorded) != person_id)
    {
        return Err("OSL friend key state does not match its recorded identity".to_owned());
    }
    Ok(())
}

/// Resolve one existing, verified friend for manual peer messaging. This is
/// deliberately re-run by every prepare/open operation rather than treating
/// activation as a durable authorization decision.
pub fn manual_peer_binding(
    core: &HubCoreState,
    person_id: String,
) -> Result<ManualPeerBinding, String> {
    require_unlocked()?;
    validate_person_id(&person_id)?;
    let dir = config_dir()?;
    let people = load_encrypted_json::<PeopleFile>(&dir.join(PEOPLE_FILE))?;
    let metadata = people
        .people
        .get(&person_id)
        .ok_or_else(|| "OSL friend is unknown".to_owned())?;
    let peer = core
        .osl
        .peer_map
        .lock()
        .map_err(|_| "OSL peer state is unavailable".to_owned())?
        .get(&person_id)
        .cloned()
        .ok_or_else(|| "OSL friend key state is missing".to_owned())?;
    ensure_manual_peer_available(metadata, true)?;
    // `person_id` is derived from the identity key, and every approval —
    // `manual_peer_scope_id` included — is keyed on `person_id`. If a stored
    // record ever drifts so that its key no longer derives its own id, the
    // approvals on file were granted to a different key than the one we are
    // about to encrypt to. Nothing should be able to produce that state
    // (`add_friend_code` refuses key changes), so a peer map that has already
    // drifted is a file written by an older build or by someone else, and it
    // fails closed here rather than being repaired on a guess.
    validate_manual_peer_identity(&person_id, metadata, &peer)?;
    let _trusted_bundle = trusted_peer_key_bundle(&person_id, metadata, &peer)?;
    let peer_x25519_public = strict_peer_x25519_public(&peer)?;
    let peer_mlkem768_public = strict_peer_mlkem768_public(&peer)?;
    let self_x25519_public = core
        .osl
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())?
        .as_ref()
        .map(|identity| *identity.x25519_public.as_bytes())
        .ok_or_else(|| "OSL identity is not loaded".to_owned())?;
    if constant_time_eq_32(&peer_x25519_public, &self_x25519_public) {
        return Err("OSL friend key cannot be the active identity key".to_owned());
    }
    Ok(ManualPeerBinding {
        person_id,
        peer_osl_user_id: metadata.osl_user_id.clone(),
        peer_x25519_public,
        peer_mlkem768_public,
    })
}

/// Return whether this exact friend has explicitly approved the supplied
/// manual DM scope and the scope remains enabled. Friend verification and key
/// stability are validated even when the answer is false.
pub fn manual_peer_scope_approved(
    core: &HubCoreState,
    service_id: &str,
    account_id: &str,
    person_id: String,
    scope_input: ScopeInput,
) -> Result<bool, String> {
    let binding = manual_peer_binding(core, person_id)?;
    manual_peer_scope_approved_for_binding(service_id, account_id, &binding, scope_input)
}

fn manual_peer_scope_approved_for_binding(
    service_id: &str,
    account_id: &str,
    binding: &ManualPeerBinding,
    scope_input: ScopeInput,
) -> Result<bool, String> {
    require_exact_manual_peer_scope_input(&scope_input, "OSL manual peer scope is invalid")?;
    let scope: Scope = scope_input
        .try_into()
        .map_err(|_| "OSL manual peer scope is invalid".to_owned())?;
    require_exact_manual_peer_scope(
        service_id,
        account_id,
        &binding.person_id,
        &scope,
        "OSL manual peer scope is invalid",
    )?;
    let dir = config_dir()?;
    let prefs = load_encrypted_json::<SecurityPreferences>(&dir.join(SECURITY_PREFS_FILE))?;
    let storage_key = scope.storage_key();
    Ok(manual_scope_preference_approved(&prefs, &storage_key))
}

fn manual_scope_preference_approved(prefs: &SecurityPreferences, storage_key: &str) -> bool {
    prefs.manual_approved_scopes.contains(storage_key)
        && !prefs.burned_manual_scopes.contains(storage_key)
}

/// Withdraw every manual grant attributed to one person. Burn records are
/// terminal facts, not grants, and deliberately survive roster removal.
fn withdraw_person_grants(prefs: &mut SecurityPreferences, person_id: &str) -> usize {
    prefs.version = 2;
    let storage_keys: Vec<String> = prefs
        .manual_approved_scope_people
        .iter()
        .filter(|(_, approved_person_id)| approved_person_id.as_str() == person_id)
        .map(|(storage_key, _)| storage_key.clone())
        .collect();
    for storage_key in &storage_keys {
        withdraw_manual_scope_grant(prefs, storage_key);
        prefs.decrypt_display_by_scope.remove(storage_key);
    }
    prefs.reach_narrowed_scopes.remove(person_id);
    storage_keys.len()
}

pub fn require_manual_peer_scope_approved(
    core: &HubCoreState,
    service_id: &str,
    account_id: &str,
    person_id: String,
    scope_input: ScopeInput,
) -> Result<ManualPeerBinding, String> {
    let binding = manual_peer_binding(core, person_id)?;
    if !manual_peer_scope_approved_for_binding(service_id, account_id, &binding, scope_input)? {
        return Err("Approve encryption for this friend before continuing".to_owned());
    }
    Ok(binding)
}

pub fn manual_peer_scope_id(
    service_id: &str,
    account_id: &str,
    person_id: &str,
) -> Result<String, String> {
    validate_manual_peer_service_account(service_id, account_id)?;
    validate_person_id(person_id)?;
    let mut hash = Sha256::new();
    for part in [
        "OSL-MANUAL-LOCAL-SCOPE-v2",
        service_id,
        account_id,
        person_id,
    ] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part.as_bytes());
    }
    Ok(format!(
        "manual-scope-{}",
        URL_SAFE_NO_PAD.encode(&hash.finalize()[..18])
    ))
}

/// The single place a manual per-person scope's on-disk namespace is built.
///
/// A5-F4: a grant was recorded under `dm:manual-scope-<b64>` while the roster's
/// revoke path searched the peer map, whose person-level DM entry is keyed by
/// the bare reach key `dm`. The two strings could never meet, so pressing
/// "revoke" on a roster scope row left the grant fully in force while telling
/// the user it was gone. Granting ([`ScopedTrustGrant::for_manual_peer`]) and
/// revoking ([`revoke_friend_scope_entry`]) now both derive the key here, so
/// the namespaces cannot drift apart again.
pub fn manual_peer_scope_storage_key(
    service_id: &str,
    account_id: &str,
    person_id: &str,
) -> Result<String, String> {
    Ok(Scope::dm(manual_peer_scope_id(service_id, account_id, person_id)?).storage_key())
}

/// The single place a manual grant is withdrawn from the approval ledger the
/// roster projects. Returns whether anything was actually recorded there.
fn withdraw_manual_scope_grant(prefs: &mut SecurityPreferences, storage_key: &str) -> bool {
    prefs.version = 2;
    let approved = prefs.manual_approved_scopes.remove(storage_key);
    let attributed = prefs
        .manual_approved_scope_people
        .remove(storage_key)
        .is_some();
    approved || attributed
}

fn validate_manual_peer_service_account(service_id: &str, account_id: &str) -> Result<(), String> {
    match service_id {
        // First-party OSL chat is not a hosted web service, so it is absent from
        // the service manifest. The broker still fixes production activation to
        // `osl-main`; this helper only needs the account id to be a bounded,
        // exact binding component.
        "osl-chat" => {
            crate::service_host::validate_opaque_id(account_id).map_err(|error| error.to_string())
        }
        "discord" if valid_native_discord_account_id(account_id) => Ok(()),
        _ => {
            crate::service_host::service_manifest(service_id).map_err(|error| error.to_string())?;
            crate::service_host::validate_opaque_id(account_id).map_err(|error| error.to_string())
        }
    }
}

fn valid_native_discord_account_id(account_id: &str) -> bool {
    let Some(suffix) = account_id.strip_prefix("native-discord-") else {
        return false;
    };
    !suffix.is_empty()
        && suffix.len() <= 64
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && suffix
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && suffix
            .as_bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

fn require_exact_manual_peer_scope(
    service_id: &str,
    account_id: &str,
    person_id: &str,
    scope: &Scope,
    error: &str,
) -> Result<(), String> {
    let expected_id = manual_peer_scope_id(service_id, account_id, person_id)?;
    if scope.kind != ScopeKind::Dm
        || scope.id.as_str() != expected_id.as_str()
        || scope.server_id.is_some()
        || !manual_peer_channel_id_is_allowed(&scope.id, scope.channel_id.as_deref())
    {
        return Err(error.to_owned());
    }
    Ok(())
}

fn require_exact_manual_peer_scope_input(
    scope_input: &ScopeInput,
    error: &str,
) -> Result<(), String> {
    if scope_input.kind != ScopeKind::Dm
        || scope_input.id.is_empty()
        || scope_input.server_id.is_some()
        || !manual_peer_channel_id_is_allowed(&scope_input.id, scope_input.channel_id.as_deref())
    {
        return Err(error.to_owned());
    }
    Ok(())
}

fn manual_peer_channel_id_is_allowed(scope_id: &str, channel_id: Option<&str>) -> bool {
    match channel_id {
        None => true,
        Some(channel_id) if channel_id == scope_id => true,
        Some(channel_id) => valid_manual_dm_channel_binding(channel_id),
    }
}

/// Length of the canonical manual-DM binding suffix.
///
/// broker::manual_dm_channel_id builds it with short_hex(), which hex-encodes the
/// FIRST 16 BYTES of a domain-separated SHA-256 over the sorted identity pair, i.e.
/// 32 hex characters. This guard originally required 64 -- an assumption that
/// matched no producer -- so it rejected every scope production actually creates and
/// took the whole native-Discord receive path down with it. Derive the length from
/// the producer rather than restating it.
const MANUAL_DM_BINDING_HEX_LEN: usize = 32;

fn valid_manual_dm_channel_binding(channel_id: &str) -> bool {
    let Some(suffix) = channel_id.strip_prefix("manual-dm-") else {
        return false;
    };
    suffix.len() == MANUAL_DM_BINDING_HEX_LEN
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub fn set_manual_peer_scope_permission(
    core: &HubCoreState,
    security: &HubSecurityState,
    service_id: &str,
    account_id: &str,
    person_id: String,
    scope_input: ScopeInput,
    enabled: bool,
) -> Result<(), String> {
    let binding = manual_peer_binding(core, person_id)?;
    require_exact_manual_peer_scope_input(&scope_input, "OSL manual peer scope is invalid")?;
    let scope: Scope = scope_input
        .clone()
        .try_into()
        .map_err(|_| "OSL manual peer scope is invalid".to_owned())?;
    require_exact_manual_peer_scope(
        service_id,
        account_id,
        &binding.person_id,
        &scope,
        "OSL manual peer scope is invalid",
    )?;
    if enabled {
        let grant = ScopedTrustGrant::for_manual_peer(
            &binding,
            service_id,
            account_id,
            scope_input,
            ScopedTrustConsent::ExplicitUserAction,
        )?;
        return apply_scoped_trust_grant(security, &binding, &grant);
    }
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL manual peer settings are unavailable".to_owned())?;
    let dir = config_dir()?;
    let path = dir.join(SECURITY_PREFS_FILE);
    let mut prefs = load_encrypted_json::<SecurityPreferences>(&path)?;
    let storage_key = manual_peer_scope_storage_key(service_id, account_id, &binding.person_id)?;
    withdraw_manual_scope_grant(&mut prefs, &storage_key);
    write_encrypted_json(&path, &prefs)
}

/// Apply one already-minted scoped trust grant to the hub's manual approval
/// preferences, preserving the friend attribution needed for later refusal or
/// revocation.
pub fn apply_scoped_trust_grant(
    security: &HubSecurityState,
    binding: &ManualPeerBinding,
    grant: &ScopedTrustGrant,
) -> Result<(), String> {
    grant.require_binding(Some(binding))?;
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL manual peer settings are unavailable".to_owned())?;
    let dir = config_dir()?;
    let path = dir.join(SECURITY_PREFS_FILE);
    let mut prefs = load_encrypted_json::<SecurityPreferences>(&path)?;
    if prefs.burned_manual_scopes.contains(grant.storage_key()) {
        return Err("This manual conversation was burned and cannot be reapproved".to_owned());
    }
    prefs.version = 2;
    prefs
        .manual_approved_scopes
        .insert(grant.storage_key().to_owned());
    prefs
        .manual_approved_scope_people
        .insert(grant.storage_key().to_owned(), grant.person_id().to_owned());
    write_encrypted_json(&path, &prefs)
}

/// Persist one uploaded prose-token blob in the encrypted burn ledger. The
/// caller must delete the remote blob and fail the send if this returns an
/// error, so a successful manual send can never orphan remote ciphertext.
pub fn record_peer_prose_blob(
    security: &HubSecurityState,
    scope_input: ScopeInput,
    blob_id: String,
) -> Result<(), String> {
    let file_key = require_unlocked()?;
    if blob_id.len() != 16 || !blob_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("OSL remote message identifier is invalid".to_owned());
    }
    let scope: Scope = scope_input
        .try_into()
        .map_err(|_| "OSL manual peer scope is invalid".to_owned())?;
    let dir = config_dir()?;
    record_peer_prose_blob_at_path(
        security,
        &dir.join("scope_blobs.json"),
        scope,
        blob_id,
        &file_key,
    )
}

fn record_peer_prose_blob_at_path(
    security: &HubSecurityState,
    path: &Path,
    scope: Scope,
    blob_id: String,
    file_key: &[u8; 32],
) -> Result<(), String> {
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL security state is unavailable".to_owned())?;
    let mut blobs = load_scope_blobs_strict_with_key(path, file_key)?;
    ipc::scope_blobs_file::record_blob(&mut blobs, scope.storage_key(), blob_id);
    write_scope_blobs_with_key(path, &blobs, file_key)
}

/// Retain the capability needed to burn an uploaded attachment. This ledger
/// is encrypted with the account storage key, bounded independently from the
/// prose ledger, and never crosses renderer IPC.
pub fn record_peer_attachment_burn_capability(
    security: &HubSecurityState,
    scope_input: ScopeInput,
    object_id: String,
    mut fetch_token: String,
    expires_at: i64,
) -> Result<(), String> {
    let result = (|| {
        let file_key = require_unlocked()?;
        let scope: Scope = scope_input
            .try_into()
            .map_err(|_| "OSL manual peer scope is invalid".to_owned())?;
        let dir = config_dir()?;
        record_peer_attachment_burn_capability_at_path(
            security,
            &dir.join(ATTACHMENT_BURN_FILE),
            scope.storage_key(),
            object_id,
            &fetch_token,
            expires_at,
            ipc::main_password::now_unix_secs_pub(),
            &file_key,
        )
    })();
    fetch_token.zeroize();
    result
}

#[allow(clippy::too_many_arguments)]
fn record_peer_attachment_burn_capability_at_path(
    security: &HubSecurityState,
    path: &Path,
    storage_key: String,
    object_id: String,
    fetch_token: &str,
    expires_at: i64,
    now: i64,
    file_key: &[u8; 32],
) -> Result<(), String> {
    validate_attachment_burn_entry(&object_id, &fetch_token, expires_at, now)?;
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL security state is unavailable".to_owned())?;
    let mut ledger = load_attachment_burn_ledger_with_key(path, file_key)?;
    prune_expired_attachment_burn_entries(&mut ledger, now);
    let total = ledger
        .entries_by_scope
        .values()
        .map(Vec::len)
        .sum::<usize>();
    let entries = ledger.entries_by_scope.entry(storage_key).or_default();
    if let Some(existing) = entries
        .iter_mut()
        .find(|entry| entry.object_id == object_id)
    {
        existing.fetch_token.zeroize();
        existing.fetch_token = fetch_token.to_owned();
        existing.expires_at = expires_at;
    } else {
        if entries.len() >= MAX_ATTACHMENT_BURN_ENTRIES_PER_SCOPE
            || total >= MAX_ATTACHMENT_BURN_ENTRIES_TOTAL
        {
            return Err("OSL attachment burn ledger is full".to_owned());
        }
        entries.push(AttachmentBurnEntry {
            object_id,
            fetch_token: fetch_token.to_owned(),
            expires_at,
        });
    }
    ledger.version = 1;
    write_encrypted_json_with_key(path, &ledger, file_key)
        .map_err(|_| "OSL attachment burn ledger could not be persisted".to_owned())
}

pub fn remove_peer_attachment_burn_capability(
    security: &HubSecurityState,
    scope_input: ScopeInput,
    object_id: &str,
) -> Result<(), String> {
    let file_key = require_unlocked()?;
    let scope: Scope = scope_input
        .try_into()
        .map_err(|_| "OSL manual peer scope is invalid".to_owned())?;
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL security state is unavailable".to_owned())?;
    let path = config_dir()?.join(ATTACHMENT_BURN_FILE);
    let mut ledger = load_attachment_burn_ledger_with_key(&path, &file_key)?;
    if let Some(entries) = ledger.entries_by_scope.get_mut(&scope.storage_key()) {
        entries.retain(|entry| entry.object_id != object_id);
        if entries.is_empty() {
            ledger.entries_by_scope.remove(&scope.storage_key());
        }
    }
    write_encrypted_json_with_key(&path, &ledger, &file_key)
        .map_err(|_| "OSL attachment burn ledger could not be persisted".to_owned())
}

/// Atomically marks one authenticated peer message consumed for its exact
/// local scope. Callers must complete this before returning plaintext.
pub fn consume_peer_message(
    security: &HubSecurityState,
    scope_input: ScopeInput,
    message_id: &str,
    expires_at: i64,
    now: i64,
) -> Result<(), String> {
    let file_key = require_unlocked().map_err(|_| PEER_OPEN_ERROR.to_owned())?;
    let scope: Scope = scope_input
        .try_into()
        .map_err(|_| PEER_OPEN_ERROR.to_owned())?;
    let dir = config_dir().map_err(|_| PEER_OPEN_ERROR.to_owned())?;
    consume_peer_message_at_path(
        security,
        &dir.join(PEER_REPLAY_FILE),
        &scope.storage_key(),
        message_id,
        expires_at,
        now,
        &file_key,
    )
    .map_err(|_| PEER_OPEN_ERROR.to_owned())
}

/// Read the durable encrypted replay ledger after a relay was authenticated.
/// This lets a later drain finish deleting an inbox row when the prior DELETE
/// failed after local consumption, without displaying the plaintext twice.
pub fn peer_message_was_consumed(
    security: &HubSecurityState,
    scope_input: ScopeInput,
    message_id: &str,
    now: i64,
) -> Result<bool, String> {
    let valid_message_id = message_id.strip_prefix("peer-").is_some_and(|value| {
        value.len() == 32
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    });
    if !valid_message_id {
        return Err(PEER_OPEN_ERROR.to_owned());
    }
    let file_key = require_unlocked().map_err(|_| PEER_OPEN_ERROR.to_owned())?;
    let scope: Scope = scope_input
        .try_into()
        .map_err(|_| PEER_OPEN_ERROR.to_owned())?;
    let dir = config_dir().map_err(|_| PEER_OPEN_ERROR.to_owned())?;
    let _transition = security
        .transition
        .lock()
        .map_err(|_| PEER_OPEN_ERROR.to_owned())?;
    let mut ledger =
        load_encrypted_json_with_key::<PeerReplayLedger>(&dir.join(PEER_REPLAY_FILE), &file_key)?;
    if !matches!(ledger.version, 0 | 1) {
        return Err(PEER_OPEN_ERROR.to_owned());
    }
    prune_peer_replay_ledger(&mut ledger, now);
    Ok(ledger
        .consumed_by_scope
        .get(&scope.storage_key())
        .is_some_and(|entries| entries.contains_key(message_id)))
}

fn consume_peer_message_at_path(
    security: &HubSecurityState,
    path: &Path,
    storage_key: &str,
    message_id: &str,
    expires_at: i64,
    now: i64,
    file_key: &[u8; 32],
) -> Result<(), String> {
    let valid_message_id = message_id.strip_prefix("peer-").is_some_and(|value| {
        value.len() == 32
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    });
    if !valid_message_id || storage_key.is_empty() || storage_key.len() > 512 {
        return Err(PEER_OPEN_ERROR.to_owned());
    }
    let _transition = security
        .transition
        .lock()
        .map_err(|_| PEER_OPEN_ERROR.to_owned())?;
    let mut ledger = load_encrypted_json_with_key::<PeerReplayLedger>(path, file_key)?;
    if !matches!(ledger.version, 0 | 1) {
        return Err(PEER_OPEN_ERROR.to_owned());
    }
    prune_peer_replay_ledger(&mut ledger, now);
    if ledger
        .consumed_by_scope
        .get(storage_key)
        .is_some_and(|scope| scope.contains_key(message_id))
    {
        return Err(PEER_OPEN_ERROR.to_owned());
    }
    let total_entries = ledger
        .consumed_by_scope
        .values()
        .map(BTreeMap::len)
        .sum::<usize>();
    let scope_is_new = !ledger.consumed_by_scope.contains_key(storage_key);
    let scope_entries = ledger
        .consumed_by_scope
        .get(storage_key)
        .map_or(0, BTreeMap::len);
    if expires_at <= now
        || (scope_is_new && ledger.consumed_by_scope.len() >= MAX_PEER_REPLAY_SCOPES)
        || scope_entries >= MAX_PEER_REPLAY_ENTRIES_PER_SCOPE
        || total_entries >= MAX_PEER_REPLAY_ENTRIES_TOTAL
    {
        return Err(PEER_OPEN_ERROR.to_owned());
    }
    ledger.version = 1;
    ledger
        .consumed_by_scope
        .entry(storage_key.to_owned())
        .or_default()
        .insert(message_id.to_owned(), expires_at);
    write_encrypted_json_with_key(path, &ledger, file_key)
}

fn prune_peer_replay_ledger(ledger: &mut PeerReplayLedger, now: i64) {
    ledger.consumed_by_scope.retain(|_, messages| {
        messages.retain(|_, expires_at| *expires_at > now);
        !messages.is_empty()
    });
}

fn remove_peer_replay_scope_at_path(
    path: &Path,
    storage_key: &str,
    file_key: &[u8; 32],
) -> Result<(), String> {
    let mut ledger = load_encrypted_json_with_key::<PeerReplayLedger>(path, file_key)?;
    if !matches!(ledger.version, 0 | 1) {
        return Err("OSL peer replay state has an unsupported version".to_owned());
    }
    if ledger.consumed_by_scope.remove(storage_key).is_some() {
        write_encrypted_json_with_key(path, &ledger, file_key)?;
    }
    Ok(())
}

fn ensure_manual_peer_available(
    metadata: &PersonMetadata,
    peer_map_entry_exists: bool,
) -> Result<(), String> {
    ensure_friend_can_be_enabled(metadata)?;
    if !peer_map_entry_exists {
        return Err("OSL friend key state is missing".to_owned());
    }
    Ok(())
}

fn strict_peer_x25519_public(peer: &PeerEntry) -> Result<[u8; X25519_PUBLIC_BYTES], String> {
    strict_peer_public_key(peer.pubkey.as_deref(), "X25519")
}

fn strict_peer_mlkem768_public(peer: &PeerEntry) -> Result<[u8; MLKEM768_PUBLIC_BYTES], String> {
    strict_peer_public_key(peer.ik_mlkem768_pub.as_deref(), "ML-KEM-768")
}

fn strict_peer_public_key<const N: usize>(
    encoded: Option<&str>,
    label: &str,
) -> Result<[u8; N], String> {
    let encoded = encoded.ok_or_else(|| format!("OSL friend {label} key state is missing"))?;
    let decoded = STANDARD
        .decode(encoded)
        .map_err(|_| format!("OSL friend {label} key state is malformed"))?;
    if STANDARD.encode(&decoded) != encoded {
        return Err(format!("OSL friend {label} key state is malformed"));
    }
    decoded
        .try_into()
        .map_err(|_| format!("OSL friend {label} key state has the wrong length"))
}

fn constant_time_eq_32(left: &[u8; 32], right: &[u8; 32]) -> bool {
    let mut difference = 0u8;
    for index in 0..32 {
        difference |= left[index] ^ right[index];
    }
    difference == 0
}

fn ensure_friend_can_be_enabled(metadata: &PersonMetadata) -> Result<(), String> {
    if !metadata.safety_number_verified {
        return Err("Verify this friend's safety number before enabling encryption".to_owned());
    }
    if metadata.pending_ed25519_public.is_some() || metadata.pending_key_bundle.is_some() {
        return Err(
            "Resolve this friend's pending key change before enabling encryption".to_owned(),
        );
    }
    Ok(())
}

fn revoke_auto_scope_if_uncovered(
    whitelist_state: &mut ipc::whitelist_state::WhitelistState,
    storage_key: &str,
    another_approved_friend: bool,
) {
    if another_approved_friend {
        return;
    }
    // Revoke only a toggle that the whitelist enabled. An explicit user
    // toggle remains their choice, but removing the last friend must not
    // leave an apparently approved auto-encryption scope.
    if let Some(scope_state) = whitelist_state.get_mut(storage_key) {
        if scope_state.auto_enabled {
            scope_state.encrypt_toggle = false;
            scope_state.auto_enabled = false;
        }
    }
}

pub fn scope_security(scope_input: ScopeInput) -> Result<ScopeSecurityDto, String> {
    require_unlocked()?;
    let scope: Scope = scope_input
        .try_into()
        .map_err(|_| "OSL scope is invalid".to_owned())?;
    let dir = config_dir()?;
    let storage_key = scope.storage_key();
    let ttl_file =
        load_encrypted_json::<ipc::scope_ttl_file::ScopeTtlFile>(&dir.join("scope_ttl.json"))?;
    let prefs = load_encrypted_json::<SecurityPreferences>(&dir.join(SECURITY_PREFS_FILE))?;
    Ok(ScopeSecurityDto {
        ttl_seconds: ipc::scope_ttl_file::get_scope_ttl(&ttl_file, &storage_key),
        decrypt_display_enabled: prefs
            .decrypt_display_by_scope
            .get(&storage_key)
            .copied()
            .unwrap_or(true),
        storage_key,
    })
}

pub fn set_scope_security(
    security: &HubSecurityState,
    scope_input: ScopeInput,
    ttl_seconds: u32,
    decrypt_display_enabled: bool,
) -> Result<ScopeSecurityDto, String> {
    require_unlocked()?;
    validate_hub_ttl(ttl_seconds)?;
    let scope: Scope = scope_input
        .try_into()
        .map_err(|_| "OSL scope is invalid".to_owned())?;
    let storage_key = scope.storage_key();
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL security settings are unavailable".to_owned())?;
    let dir = config_dir()?;
    let prefs_path = dir.join(SECURITY_PREFS_FILE);
    let mut prefs = load_encrypted_json::<SecurityPreferences>(&prefs_path)?;
    if prefs.burned_manual_scopes.contains(&storage_key) {
        return Err("This manual conversation was burned and cannot be changed".to_owned());
    }
    let ttl_path = dir.join("scope_ttl.json");
    let mut ttl_file = load_encrypted_json::<ipc::scope_ttl_file::ScopeTtlFile>(&ttl_path)?;
    let effective_ttl =
        ipc::scope_ttl_file::set_scope_ttl(&mut ttl_file, storage_key.clone(), ttl_seconds);
    write_encrypted_json(&ttl_path, &ttl_file)?;
    prefs.version = 2;
    prefs
        .decrypt_display_by_scope
        .insert(storage_key.clone(), decrypt_display_enabled);
    write_encrypted_json(&prefs_path, &prefs)?;
    Ok(ScopeSecurityDto {
        storage_key,
        ttl_seconds: effective_ttl,
        decrypt_display_enabled,
    })
}

/// Burn all OSL Privacy history in one scope and delete every recorded remote
/// cipher-store blob. A full-space/server burn requires the trusted adapter to
/// provide a complete channel enumeration; partial coverage is rejected before
/// any destructive mutation.
pub fn burn_scope(
    core: &HubCoreState,
    security: &HubSecurityState,
    scope_input: ScopeInput,
    known_channel_ids: Vec<String>,
    channel_enumeration_complete: bool,
    burned_message_ids: Vec<String>,
) -> Result<HubScopeBurnResult, String> {
    require_unlocked()?;
    let scope: Scope = scope_input
        .clone()
        .try_into()
        .map_err(|_| "OSL scope is invalid".to_owned())?;
    validate_burn_ids(&known_channel_ids, 512, "channel")?;
    validate_burn_ids(&burned_message_ids, 10_000, "message")?;
    let channels = burn_channels(&scope, known_channel_ids, channel_enumeration_complete)?;
    // Resolve who must be told BEFORE anything is destroyed. Every recipient
    // resolver filters on the whitelist entries this burn is about to remove, so
    // a set captured afterwards is empty and the notice would go to nobody. The
    // legacy client documents the same ordering requirement on
    // `cmd_osl_send_burn_marker`; capturing here is read-only and cannot fail
    // the burn.
    // A read error is not "nobody was approved". It is "this device does not
    // know who was approved", which forces the peer-notification phase to report
    // incomplete rather than succeed by default.
    let revocation_peers = revocation_peers_for_scope(core, &scope)
        .unwrap_or_else(|_| RevocationRecipients::unresolved());
    let explicit_message_ids = burned_message_ids.clone();
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL security state is unavailable".to_owned())?;
    let dir = config_dir()?;
    // Resolve DM ownership before the destructive sequence too. The peer-map
    // cleanup below must use the same recorded attribution as recipient
    // resolution; an absent mapping matches nothing rather than guessing.
    let prefs = load_encrypted_json::<SecurityPreferences>(&dir.join(SECURITY_PREFS_FILE))?;
    let dm_scope_person = dm_scope_person(&prefs, &scope).map(str::to_owned);
    let blobs_path = dir.join("scope_blobs.json");
    // Validate the encrypted remote-deletion ledger before destroying
    // anything. Losing this ledger could strand server-held ciphertext.
    let mut blobs_file = load_scope_blobs_strict(&blobs_path)?;

    let rows_destroyed = {
        let store = core
            .osl
            .message_store
            .lock()
            .map_err(|_| "OSL message store is unavailable".to_owned())?;
        match store.as_ref() {
            Some(store) => {
                let mut rows = 0usize;
                for channel_id in &channels {
                    rows =
                        rows.saturating_add(store.delete_messages_in_channel(channel_id).map_err(
                            |_| "OSL scope history could not be securely deleted".to_owned(),
                        )?);
                }
                rows
            }
            None => 0,
        }
    };
    ipc::commands::cmd_osl_apply_burn(&core.osl, scope_input.clone())?;

    let mut peers = core
        .osl
        .peer_map
        .lock()
        .map_err(|_| "OSL peer state is unavailable".to_owned())?
        .clone();
    let mut whitelist_entries_removed = 0usize;
    for (person_id, peer) in peers.iter_mut() {
        let before = peer.outgoing_whitelists.len();
        peer.outgoing_whitelists.retain(|entry| {
            !whitelist_entry_matches_scope(
                entry,
                &scope,
                dm_scope_person.as_deref() == Some(person_id.as_str()),
            )
        });
        whitelist_entries_removed = whitelist_entries_removed
            .saturating_add(before.saturating_sub(peer.outgoing_whitelists.len()));
    }
    write_encrypted_json(&dir.join("peer_map.json"), &peers)
        .map_err(|_| "OSL burned whitelist state could not be persisted".to_owned())?;
    *core
        .osl
        .peer_map
        .lock()
        .map_err(|_| "OSL peer state is unavailable".to_owned())? = peers;

    *core
        .osl
        .last_persist_error
        .lock()
        .map_err(|_| "OSL persistence state is unavailable".to_owned())? = None;
    let (scope_kind, server_id, channel_id) = burn_scope_fields(&scope);
    ipc::commands::cmd_osl_mark_scope_burned(
        &core.osl,
        scope_kind,
        scope.id.clone(),
        server_id,
        channel_id,
        burned_message_ids,
    )?;
    if core
        .osl
        .last_persist_error
        .lock()
        .map_err(|_| "OSL persistence state is unavailable".to_owned())?
        .take()
        .is_some()
    {
        return Err("OSL burned-scope ledger could not be persisted".to_owned());
    }

    let blob_ids = ipc::scope_blobs_file::take_blobs(&mut blobs_file, &scope.storage_key());
    let mut failed_blob_ids = Vec::new();
    let mut remote_blobs_deleted = 0usize;
    for blob_id in blob_ids {
        match ipc::prose_token::prose_token_burn_id(&dir, &scope_input, &blob_id) {
            Ok(()) => remote_blobs_deleted += 1,
            Err(_) => failed_blob_ids.push(blob_id),
        }
    }
    for blob_id in &failed_blob_ids {
        ipc::scope_blobs_file::record_blob(&mut blobs_file, scope.storage_key(), blob_id.clone());
    }
    write_scope_blobs(&blobs_path, &blobs_file)?;
    let remote_blob_deletions_failed = failed_blob_ids.len();

    // Notice LAST. Everything above is unilateral local-row cleanup plus
    // best-effort deletion of each known OSL cipher-store blob; failures remain
    // counted in `remote_blobs_deleted` / `remote_cleanup_complete` below.
    // Burn does not spend or delete native-overlay server-held wrapped-key
    // rows. Successfully removing an OSL blob blocks a later fetch through
    // that store, but does not erase connected-service/provider copies or
    // destroy the recipient's long-term decryption authority. The cooperative
    // notice covers peer-side residue and its queue failure is reported rather
    // than propagated.
    let (revocations_queued, revocation_queue_complete) = queue_scope_revocations_locked(
        core,
        &scope.storage_key(),
        &scope.storage_key(),
        &revocation_peers,
        &explicit_message_ids,
        ipc::main_password::now_unix_secs_pub(),
    )
    .unwrap_or((0, false));

    Ok(HubScopeBurnResult {
        storage_key: scope.storage_key(),
        rows_destroyed,
        channels_destroyed: channels.len(),
        whitelist_entries_removed,
        remote_blobs_deleted,
        remote_blob_deletions_failed,
        remote_cleanup_complete: remote_blob_deletions_failed == 0,
        local_cleanup_complete: true,
        channel_coverage_complete: true,
        revocations_queued,
        revocation_queue_complete,
        claims: burn_claims(),
    })
}

/// Burn only one Hub-owned manual app+friend scope. This deliberately avoids
/// the generic DM peer-map and burned-scope machinery, whose DM keys are
/// friend-global and cannot represent an app-specific manual conversation.
pub fn burn_manual_peer_scope(
    core: &HubCoreState,
    security: &HubSecurityState,
    service_id: &str,
    account_id: &str,
    person_id: &str,
    scope_input: ScopeInput,
) -> Result<HubScopeBurnResult, String> {
    require_unlocked()?;
    require_exact_manual_peer_scope_input(&scope_input, "OSL manual peer scope is invalid")?;
    let scope: Scope = scope_input
        .clone()
        .try_into()
        .map_err(|_| "OSL manual peer scope is invalid".to_owned())?;
    require_exact_manual_peer_scope(
        service_id,
        account_id,
        person_id,
        &scope,
        "OSL manual peer scope is invalid",
    )?;
    // Resolved before the destructive sequence, for the same reason as in
    // `burn_scope`: afterwards the approval this lookup depends on is gone. A
    // binding this device cannot read leaves the peer unnotified, so it is
    // recorded as an unresolved recipient set and never as an empty one.
    let revocation_peers = match manual_peer_binding(core, person_id.to_owned()) {
        Ok(binding) => {
            RevocationRecipients::single(binding.peer_osl_user_id, binding.peer_x25519_public)
        }
        Err(_) => RevocationRecipients::unresolved(),
    };
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL manual peer burn state is unavailable".to_owned())?;
    let dir = config_dir()?;
    let prefs_path = dir.join(SECURITY_PREFS_FILE);
    let ttl_path = dir.join("scope_ttl.json");
    let blobs_path = dir.join("scope_blobs.json");
    let attachments_path = dir.join(ATTACHMENT_BURN_FILE);
    let replay_path = dir.join(PEER_REPLAY_FILE);
    let storage_key = scope.storage_key();

    let mut prefs = load_encrypted_json::<SecurityPreferences>(&prefs_path)?;
    let mut ttl = load_encrypted_json::<ipc::scope_ttl_file::ScopeTtlFile>(&ttl_path)?;
    let mut blobs = load_scope_blobs_strict(&blobs_path)?;
    let file_key = require_unlocked()?;
    let mut attachments = load_attachment_burn_ledger_with_key(&attachments_path, &file_key)?;
    let whitelist_entries_removed =
        usize::from(prefs.manual_approved_scopes.contains(&storage_key));
    let blob_ids = revoke_manual_scope_state(&mut prefs, &mut ttl, &mut blobs, &storage_key);
    write_encrypted_json(&prefs_path, &prefs)?;
    write_encrypted_json(&ttl_path, &ttl)?;
    remove_peer_replay_scope_at_path(&replay_path, &storage_key, &file_key)
        .map_err(|_| "OSL manual peer replay state could not be removed".to_owned())?;

    // Manual copy/paste send/open passes no service message id and does not
    // persist plaintext rows. Its relay channel binding is intentionally
    // symmetric across the two users and may also be shared by another local
    // account for the same app+friend. Never delete MessageStore rows by that
    // shared channel during a local app+account burn.
    let rows_destroyed = 0;

    let mut failed_blob_ids = Vec::new();
    let mut remote_blobs_deleted = 0usize;
    for blob_id in blob_ids {
        match ipc::prose_token::prose_token_burn_id(&dir, &scope_input, &blob_id) {
            Ok(()) => remote_blobs_deleted = remote_blobs_deleted.saturating_add(1),
            Err(_) => failed_blob_ids.push(blob_id),
        }
    }
    for blob_id in &failed_blob_ids {
        ipc::scope_blobs_file::record_blob(&mut blobs, storage_key.clone(), blob_id.clone());
    }
    write_scope_blobs(&blobs_path, &blobs)?;
    let attachment_entries = take_attachment_burn_entries(&mut attachments, &storage_key);
    let attachment_client = ipc::cipher_store_client::CipherStoreClient::new(
        ipc::cipher_store_client::resolve_cipher_store_base_url(&dir),
    )
    .map_err(|_| "OSL attachment cleanup is unavailable".to_owned())?;
    let mut failed_attachment_entries = Vec::new();
    let mut remote_attachments_deleted = 0usize;
    for entry in attachment_entries {
        let mut token = match parse_attachment_fetch_token(&entry.fetch_token) {
            Ok(token) => token,
            Err(_) => {
                failed_attachment_entries.push(entry);
                continue;
            }
        };
        let deleted = attachment_client
            .delete_attachment(&entry.object_id, &token)
            .is_ok();
        token.zeroize();
        if deleted {
            remote_attachments_deleted = remote_attachments_deleted.saturating_add(1);
        } else {
            failed_attachment_entries.push(entry);
        }
    }
    let remote_attachment_deletions_failed = failed_attachment_entries.len();
    if !failed_attachment_entries.is_empty() {
        attachments
            .entries_by_scope
            .insert(storage_key.clone(), failed_attachment_entries);
    }
    write_encrypted_json_with_key(&attachments_path, &attachments, &file_key)
        .map_err(|_| "OSL attachment burn ledger could not be persisted".to_owned())?;
    let remote_blobs_deleted = remote_blobs_deleted.saturating_add(remote_attachments_deleted);
    let remote_blob_deletions_failed = failed_blob_ids
        .len()
        .saturating_add(remote_attachment_deletions_failed);

    // Notice last, after the unilateral destruction above.
    let (revocations_queued, revocation_queue_complete) = queue_scope_revocations_locked(
        core,
        &storage_key,
        &storage_key,
        &revocation_peers,
        &[],
        ipc::main_password::now_unix_secs_pub(),
    )
    .unwrap_or((0, false));

    Ok(HubScopeBurnResult {
        storage_key,
        rows_destroyed,
        channels_destroyed: 1,
        whitelist_entries_removed,
        remote_blobs_deleted,
        remote_blob_deletions_failed,
        remote_cleanup_complete: remote_blob_deletions_failed == 0,
        local_cleanup_complete: true,
        channel_coverage_complete: true,
        revocations_queued,
        revocation_queue_complete,
        claims: burn_claims(),
    })
}

fn revoke_manual_scope_state(
    prefs: &mut SecurityPreferences,
    ttl: &mut ipc::scope_ttl_file::ScopeTtlFile,
    blobs: &mut ipc::scope_blobs_file::ScopeBlobsFile,
    storage_key: &str,
) -> Vec<String> {
    prefs.version = 2;
    prefs.manual_approved_scopes.remove(storage_key);
    // The attribution outlives nothing: the grant it described is gone, and a
    // burned scope can never be reapproved.
    prefs.manual_approved_scope_people.remove(storage_key);
    prefs.burned_manual_scopes.insert(storage_key.to_owned());
    prefs
        .decrypt_display_by_scope
        .insert(storage_key.to_owned(), false);
    ttl.entries.remove(storage_key);
    ipc::scope_blobs_file::take_blobs(blobs, storage_key)
}

// ---- Bilateral burn ----------------------------------------------------
//
// "if u burn the other person needs to burn ur stuff too."
//
// # Order of operations, which is the highest-value part of this whole feature
//
// Destroy first, notify second. Every caller below is invoked *after* the local
// destruction in `burn_scope` / `burn_manual_peer_scope` has already committed.
// That ordering is not a nicety:
//
// - Deleting our own local rows and OSL-managed remote cipher-store blobs is
//   **unilateral**. It needs no peer cooperation. It destroys no per-message
//   key or long-term decryption authority; a peer that already retained the
//   payload or keys remains outside this device's control.
// - The notice only ever has to cover the residue: peers who already fetched.
//
// Reversing the order would be strictly worse — a notice sent first, then a
// failed local deletion, leaves content alive *and* announces the intent.
//
// # What we can honestly claim
//
// Three separate claims, never merged (`docs/design/osl-gui-final-plan.md:496-500`):
// [`ipc::revocation::CLAIM_CONTENT_EXPIRY`] (strong, lead with it),
// [`ipc::revocation::CLAIM_LOCAL_REMOVAL`] (verifiable here),
// [`ipc::revocation::CLAIM_RECIPIENT_COPIES`] (the honest bound). The delivery
// line is `Sent request` / `Acknowledged by peer` / `Not acknowledged`, and per
// `:494` it is never `Deleted`.
//
// `README.md:78-88` now says the peer-notification path is not end-to-end proved,
// is unavailable as a working peer action, and must not be relied on to remove
// another member's copy. Nothing here deletes a Discord message.

/// The three claims, in display order.
pub fn burn_claims() -> Vec<String> {
    vec![
        ipc::revocation::CLAIM_CONTENT_EXPIRY.to_owned(),
        ipc::revocation::CLAIM_LOCAL_REMOVAL.to_owned(),
        ipc::revocation::CLAIM_RECIPIENT_COPIES.to_owned(),
    ]
}

/// Self identity X25519 public key, for commitment-key derivation.
fn self_x25519_public(core: &HubCoreState) -> Result<[u8; X25519_PUBLIC_BYTES], String> {
    let guard = core
        .osl
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())?;
    let identity = guard
        .as_ref()
        .ok_or_else(|| "OSL identity is not loaded".to_owned())?;
    Ok(*identity.x25519_public.as_bytes())
}

/// Strict, fail-closed load of the receiver ledger.
///
/// `docs/design/burn-contract.md`: replay journals "fail closed instead of
/// silently forgetting live replay state". So every caller treats an `Err` as
/// "already burned" — never as "fresh". A malformed or over-large ledger is an
/// error, not an empty ledger.
fn load_revocation_ledger(
    path: &Path,
    file_key: &[u8; 32],
) -> Result<ipc::revocation::RevocationLedger, String> {
    let ledger = load_encrypted_json_with_key::<ipc::revocation::RevocationLedger>(path, file_key)
        .map_err(|_| "OSL burn state could not be opened".to_owned())?;
    if ledger.version > 1
        || ledger.scopes.len() > ipc::revocation::MAX_LEDGER_SCOPES
        || ledger.total_journal_entries() > ipc::revocation::MAX_JOURNAL_ENTRIES_TOTAL
        || ledger
            .scopes
            .values()
            .any(|s| s.journal.len() > ipc::revocation::MAX_JOURNAL_ENTRIES_PER_SCOPE)
        || ledger
            .scopes
            .keys()
            .any(|slot| !canonical_lower_hex(slot, 64))
        || ledger
            .scopes
            .values()
            .flat_map(|s| s.journal.keys())
            .any(|slot| !canonical_lower_hex(slot, 64))
    {
        return Err("OSL burn state is malformed".to_owned());
    }
    Ok(ledger)
}

fn load_revocation_outbox(
    path: &Path,
    file_key: &[u8; 32],
) -> Result<ipc::revocation::RevocationOutbox, String> {
    let outbox = load_encrypted_json_with_key::<ipc::revocation::RevocationOutbox>(path, file_key)
        .map_err(|_| "OSL burn queue could not be opened".to_owned())?;
    if outbox.version > 1 || outbox.entries.len() > ipc::revocation::MAX_OUTBOX_ENTRIES {
        return Err("OSL burn queue is malformed".to_owned());
    }
    Ok(outbox)
}

fn load_revocation_counters(
    path: &Path,
    file_key: &[u8; 32],
) -> Result<ipc::revocation::SendCounters, String> {
    let counters = load_encrypted_json_with_key::<ipc::revocation::SendCounters>(path, file_key)
        .map_err(|_| "OSL burn counters could not be opened".to_owned())?;
    if counters.version > 1
        || counters.send_seq.len() > ipc::revocation::MAX_COUNTER_SCOPES
        || counters.burn_epoch.len() > ipc::revocation::MAX_COUNTER_SCOPES
    {
        return Err("OSL burn counters are malformed".to_owned());
    }
    Ok(counters)
}

/// Implemented-unwired allocator for an authenticated per-peer `send_seq`.
///
/// A future sequence-bearing broker path would need to call this once per
/// outgoing protected message and authenticate the result inside its envelope,
/// alongside the scope commitment. Current production send paths do neither.
/// Without that integration the receiver has nothing to compare against a burn
/// floor.
pub fn next_peer_send_seq(
    core: &HubCoreState,
    security: &HubSecurityState,
    peer_x25519_public: &[u8; X25519_PUBLIC_BYTES],
    storage_key: &str,
) -> Result<u64, String> {
    let file_key = require_unlocked()?;
    validate_storage_key(storage_key)?;
    let self_pub = self_x25519_public(core)?;
    let commit_key = ipc::revocation::scope_commit_key(&self_pub, peer_x25519_public)
        .map_err(|_| "OSL burn commitment key is unavailable".to_owned())?;
    let commitment = ipc::revocation::scope_commitment(&commit_key, storage_key);
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL burn state is unavailable".to_owned())?;
    let path = config_dir()?.join(REVOCATION_COUNTERS_FILE);
    let mut counters = load_revocation_counters(&path, &file_key)?;
    let seq = counters
        .next_send_seq(&commitment)
        .map_err(|_| "OSL burn counters are full".to_owned())?;
    counters.version = 1;
    write_encrypted_json_with_key(&path, &counters, &file_key)
        .map_err(|_| "OSL burn counters could not be persisted".to_owned())?;
    Ok(seq)
}

/// Implemented-unwired opaque commitment helper for one (peer, conversation).
///
/// The intended sequence-bearing broker would authenticate this on the wire
/// next to `send_seq`; current production envelopes do not carry either value.
pub fn peer_scope_commitment(
    core: &HubCoreState,
    peer_x25519_public: &[u8; X25519_PUBLIC_BYTES],
    storage_key: &str,
) -> Result<String, String> {
    validate_storage_key(storage_key)?;
    let self_pub = self_x25519_public(core)?;
    let commit_key = ipc::revocation::scope_commit_key(&self_pub, peer_x25519_public)
        .map_err(|_| "OSL burn commitment key is unavailable".to_owned())?;
    Ok(STANDARD.encode(ipc::revocation::scope_commitment(&commit_key, storage_key)))
}

/// Implemented-unwired admission helper for a peer burn floor.
///
/// An integrated content path must call this before returning plaintext to a
/// renderer. Current production decrypt paths do not call it.
///
/// Fails closed three ways: a burnt sequence is refused, a sequence we cannot
/// evaluate is refused, and a ledger we cannot read is refused. The error string
/// is the generic open failure, so a peer cannot use the UI to learn whether a
/// particular sequence was burnt.
pub fn admit_peer_content_seq(
    core: &HubCoreState,
    security: &HubSecurityState,
    peer_x25519_public: &[u8; X25519_PUBLIC_BYTES],
    storage_key: &str,
    send_seq: u64,
) -> Result<(), String> {
    let file_key = require_unlocked().map_err(|_| REVOCATION_REFUSED_ERROR.to_owned())?;
    validate_storage_key(storage_key).map_err(|_| REVOCATION_REFUSED_ERROR.to_owned())?;
    if send_seq == 0 {
        // Sequences start at 1. A zero means the sender did not authenticate
        // one, which makes the burn floor unenforceable for this message.
        return Err(REVOCATION_REFUSED_ERROR.to_owned());
    }
    let self_pub = self_x25519_public(core).map_err(|_| REVOCATION_REFUSED_ERROR.to_owned())?;
    let commit_key = ipc::revocation::scope_commit_key(&self_pub, peer_x25519_public)
        .map_err(|_| REVOCATION_REFUSED_ERROR.to_owned())?;
    let commitment = ipc::revocation::scope_commitment(&commit_key, storage_key);
    let _transition = security
        .transition
        .lock()
        .map_err(|_| REVOCATION_REFUSED_ERROR.to_owned())?;
    let path = config_dir()
        .map_err(|_| REVOCATION_REFUSED_ERROR.to_owned())?
        .join(REVOCATION_LEDGER_FILE);
    // A read error is "already burned", never "fresh".
    let mut ledger = load_revocation_ledger(&path, &file_key)
        .map_err(|_| REVOCATION_REFUSED_ERROR.to_owned())?;
    match ipc::revocation::accept_content(&ledger, &commitment, send_seq) {
        ipc::revocation::ContentDecision::Accept => {}
        _ => return Err(REVOCATION_REFUSED_ERROR.to_owned()),
    }
    ipc::revocation::record_content_accepted(&mut ledger, &commitment, send_seq)
        .map_err(|_| REVOCATION_REFUSED_ERROR.to_owned())?;
    write_encrypted_json_with_key(&path, &ledger, &file_key)
        .map_err(|_| REVOCATION_REFUSED_ERROR.to_owned())
}

/// Apply an inbound `MSG_TYPE_REVOCATION` from an authenticated peer.
///
/// `notice_b64` is the base64 CBOR body the broker recovered from the envelope;
/// `peer_x25519_public` must be the key the envelope authenticated against, and
/// `candidate_storage_keys` the conversations we hold for that peer. The
/// commitment is matched by recomputation and constant-time comparison, so this
/// function never learns which conversation the *peer* meant except by
/// recognising one of our own.
///
/// Implemented contract helper for a future sequence-bearing content path.
///
/// A durable floor is not proof that a burn is enforced: production content
/// currently carries no authenticated `send_seq`/scope commitment and does not
/// call [`admit_peer_content_seq`] before plaintext release. Production callers
/// must therefore retain the notice and emit no acknowledgement until that
/// admission chain is wired.
pub fn apply_peer_revocation(
    core: &HubCoreState,
    security: &HubSecurityState,
    peer_x25519_public: &[u8; X25519_PUBLIC_BYTES],
    candidate_storage_keys: &[String],
    notice_b64: &str,
    now: i64,
) -> Result<HubInboundRevocation, String> {
    let file_key = require_unlocked()?;
    let raw = STANDARD
        .decode(notice_b64)
        .map_err(|_| "OSL burn notice is malformed".to_owned())?;
    let notice = ipc::control_messages::deserialize_revocation_notice(&raw)
        .map_err(|_| "OSL burn notice is malformed".to_owned())?;
    let self_pub = self_x25519_public(core)?;
    let commit_key = ipc::revocation::scope_commit_key(&self_pub, peer_x25519_public)
        .map_err(|_| "OSL burn commitment key is unavailable".to_owned())?;
    let matched = ipc::revocation::match_scope_commitment(
        &commit_key,
        &notice.scope_commitment,
        candidate_storage_keys.iter().map(String::as_str),
    )
    .map(str::to_owned);

    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL burn state is unavailable".to_owned())?;
    let path = config_dir()?.join(REVOCATION_LEDGER_FILE);
    let mut ledger = load_revocation_ledger(&path, &file_key)?;
    let outcome = ipc::revocation::apply_inbound_revocation(&mut ledger, &commit_key, &notice, now)
        .map_err(|_| "OSL burn state is full".to_owned())?;
    ledger.version = 1;
    // Durable first. Only then may an ack claim the burn is in force.
    write_encrypted_json_with_key(&path, &ledger, &file_key)
        .map_err(|_| "OSL burn state could not be persisted".to_owned())?;

    // Forget this sender's cached attachment capabilities in the matched
    // conversation. This helper also records the intended text burn floor, but
    // recording is not enforcement: the production decrypt path does not yet
    // call `admit_peer_content_seq`.
    if outcome.decision == ipc::revocation::InboundDecision::Applied {
        if let Some(storage_key) = matched.as_deref() {
            let attachments_path = config_dir()?.join(ATTACHMENT_BURN_FILE);
            if let Ok(mut attachments) =
                load_attachment_burn_ledger_with_key(&attachments_path, &file_key)
            {
                if attachments.entries_by_scope.remove(storage_key).is_some() {
                    let _ =
                        write_encrypted_json_with_key(&attachments_path, &attachments, &file_key);
                }
            }
        }
    }

    let burn_floor = ledger
        .scopes
        .values()
        .map(|s| s.burn_floor)
        .max()
        .unwrap_or(0);
    let ack_bytes = ipc::control_messages::serialize_revocation_ack(&outcome.ack)
        .map_err(|_| "OSL burn receipt could not be encoded".to_owned())?;
    Ok(HubInboundRevocation {
        ack_b64: STANDARD.encode(&ack_bytes),
        applied: outcome.ack.applied,
        storage_key: matched,
        burn_floor,
    })
}

/// Honour a legacy `MSG_TYPE_BURN` (`0x01`) from an old peer.
///
/// Converted to a bounded revocation at "everything of theirs I currently hold"
/// and applied through exactly the same path as a `0x0A`. It is specifically
/// **not** recorded as a permanent scope flag: the legacy client's
/// `peer_map.burned_scopes` behaviour turned one stale or replayed marker into a
/// conversation that could never be used again, and that denial of service is
/// fixed here for old peers too, without asking them to upgrade.
pub fn apply_legacy_peer_burn(
    core: &HubCoreState,
    security: &HubSecurityState,
    peer_x25519_public: &[u8; X25519_PUBLIC_BYTES],
    storage_key: &str,
    now: i64,
) -> Result<HubInboundRevocation, String> {
    let file_key = require_unlocked()?;
    validate_storage_key(storage_key)?;
    let self_pub = self_x25519_public(core)?;
    let commit_key = ipc::revocation::scope_commit_key(&self_pub, peer_x25519_public)
        .map_err(|_| "OSL burn commitment key is unavailable".to_owned())?;
    let commitment = ipc::revocation::scope_commitment(&commit_key, storage_key);
    let notice = {
        let _transition = security
            .transition
            .lock()
            .map_err(|_| "OSL burn state is unavailable".to_owned())?;
        let path = config_dir()?.join(REVOCATION_LEDGER_FILE);
        let ledger = load_revocation_ledger(&path, &file_key)?;
        ipc::revocation::legacy_burn_notice(&ledger, &commit_key, &commitment, now)
    };
    let notice_bytes = ipc::control_messages::serialize_revocation_notice(&notice)
        .map_err(|_| "OSL burn notice could not be encoded".to_owned())?;
    apply_peer_revocation(
        core,
        security,
        peer_x25519_public,
        std::slice::from_ref(&storage_key.to_owned()),
        &STANDARD.encode(&notice_bytes),
        now,
    )
}

/// The peers one burn has to notify, together with everything resolution could
/// not deliver.
///
/// This type exists because the two ways of ending up with no recipients are
/// opposites and used to be the same empty `Vec`:
///
/// - nobody was ever approved for this conversation, so there is nothing to
///   tell and the peer-notification phase really is complete; and
/// - somebody *was* approved and this device cannot address them — their OSL id
///   or transport key is missing or malformed, or the approval state could not
///   be read at all — in which case a burn that reports "every peer notified"
///   is telling the operator something false.
///
/// `peers` is the resolved set, `skipped` the approved recipients with no usable
/// address, and `resolver_failed` records that the read itself failed, which
/// makes `skipped` unknowable rather than zero. Only
/// [`RevocationRecipients::fully_addressed`] may report a conversation complete.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RevocationRecipients {
    peers: Vec<(String, [u8; X25519_PUBLIC_BYTES])>,
    skipped: usize,
    resolver_failed: bool,
}

impl RevocationRecipients {
    /// Resolution failed. Nothing derived from this may claim completeness —
    /// note this is deliberately **not** `Default`, which is the honest empty
    /// answer for "there was nobody approved".
    fn unresolved() -> Self {
        Self {
            peers: Vec::new(),
            skipped: 0,
            resolver_failed: true,
        }
    }

    /// The single binding the Hub's manual app+friend paths resolve.
    fn single(osl_user_id: String, peer_public: [u8; X25519_PUBLIC_BYTES]) -> Self {
        Self {
            peers: vec![(osl_user_id, peer_public)],
            skipped: 0,
            resolver_failed: false,
        }
    }

    /// Approved recipients this resolution accounted for, addressable or not.
    fn expected(&self) -> usize {
        self.peers.len().saturating_add(self.skipped)
    }

    /// True only when the read succeeded **and** every approved recipient it
    /// found has an address to seal a notice to. An empty, successful, nobody-
    /// was-approved resolution is complete; every other empty one is not.
    fn fully_addressed(&self) -> bool {
        !self.resolver_failed && self.skipped == 0
    }
}

/// Queue one revocation notice per peer for a conversation we just burned.
///
/// Called **after** local destruction. `peers` is `(osl_user_id, x25519_public)`
/// captured *before* the whitelist entries were removed — otherwise the peers who
/// need the notice have already been filtered out of every recipient resolver,
/// which is the ordering bug the legacy client documents on
/// `cmd_osl_send_burn_marker`.
///
/// Returns `(queued, complete)`. `complete == false` means at least one peer's
/// notice could not be queued **or could not even be addressed**, and the
/// operator must be shown `Not acknowledged` for the conversation — never a
/// success.
pub fn queue_scope_revocations(
    core: &HubCoreState,
    security: &HubSecurityState,
    storage_key: &str,
    scope_id_label: &str,
    recipients: &RevocationRecipients,
    explicit_message_ids: &[String],
    now: i64,
) -> Result<(usize, bool), String> {
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL burn state is unavailable".to_owned())?;
    queue_scope_revocations_locked(
        core,
        storage_key,
        scope_id_label,
        recipients,
        explicit_message_ids,
        now,
    )
}

/// Body of [`queue_scope_revocations`], for callers already holding the
/// security-transition lock. `std::sync::Mutex` is not reentrant, so `burn_scope`
/// — which holds that lock across its whole destructive sequence — must use this
/// entry point rather than the public one.
fn queue_scope_revocations_locked(
    core: &HubCoreState,
    storage_key: &str,
    scope_id_label: &str,
    recipients: &RevocationRecipients,
    explicit_message_ids: &[String],
    now: i64,
) -> Result<(usize, bool), String> {
    // Completeness starts at what resolution proved, not at `true`. An empty
    // list is complete only when resolution succeeded and found nobody to tell;
    // an empty list produced by a failed read, or by dropping an approved peer
    // this device cannot address, is the one case this function used to report
    // as a fully notified conversation.
    let mut complete = recipients.fully_addressed();
    let peers = recipients.peers.as_slice();
    if peers.is_empty() {
        return Ok((0, complete));
    }
    let file_key = require_unlocked()?;
    validate_storage_key(storage_key)?;
    let self_pub = self_x25519_public(core)?;
    let dir = config_dir()?;
    let counters_path = dir.join(REVOCATION_COUNTERS_FILE);
    let outbox_path = dir.join(REVOCATION_OUTBOX_FILE);
    let mut counters = load_revocation_counters(&counters_path, &file_key)?;
    let mut outbox = load_revocation_outbox(&outbox_path, &file_key)?;

    let mut queued = 0usize;
    for (recipient_osl_user_id, peer_pub) in peers {
        let Ok(commit_key) = ipc::revocation::scope_commit_key(&self_pub, peer_pub) else {
            complete = false;
            continue;
        };
        let commitment = ipc::revocation::scope_commitment(&commit_key, storage_key);
        let Ok(burn_epoch) = counters.next_burn_epoch(&commitment) else {
            complete = false;
            continue;
        };
        // Everything we have ever said to this peer in this conversation.
        let burn_upto_seq = counters.current_send_seq(&commitment);
        let message_commitments: Vec<[u8; 32]> = explicit_message_ids
            .iter()
            .take(ipc::control_messages::MAX_REVOCATION_MESSAGE_COMMITMENTS)
            .map(|id| ipc::revocation::message_commitment(&commit_key, &commitment, id))
            .collect();
        let burn_id = ipc::revocation::burn_id(&commit_key, &commitment, burn_epoch, burn_upto_seq);
        let collapse_key = ipc::revocation::lane_collapse_key(&commit_key, &commitment, burn_epoch);
        let notice = ipc::control_messages::RevocationNotice {
            scope_commitment: commitment,
            burn_epoch,
            burn_upto_seq,
            message_commitments,
            burn_id,
            issued_at: now,
        };
        let Ok(bytes) = ipc::control_messages::serialize_revocation_notice(&notice) else {
            complete = false;
            continue;
        };
        let entry = ipc::revocation::RevocationOutboxEntry {
            recipient_id: recipient_osl_user_id.clone(),
            scope_id_label: scope_id_label.to_owned(),
            storage_key: storage_key.to_owned(),
            burn_id_hex: lower_hex(&burn_id),
            collapse_key_hex: lower_hex(&collapse_key),
            burn_epoch,
            burn_upto_seq,
            notice_b64: STANDARD.encode(&bytes),
            attempts: 0,
            next_attempt_at: now,
            acknowledged: false,
            created_at: now,
        };
        if outbox.enqueue(entry).is_err() {
            complete = false;
            continue;
        }
        queued = queued.saturating_add(1);
    }

    counters.version = 1;
    outbox.version = 1;
    write_encrypted_json_with_key(&counters_path, &counters, &file_key)
        .map_err(|_| "OSL burn counters could not be persisted".to_owned())?;
    write_encrypted_json_with_key(&outbox_path, &outbox, &file_key)
        .map_err(|_| "OSL burn queue could not be persisted".to_owned())?;
    Ok((queued, complete))
}

/// Revocations due for a delivery attempt. The broker calls this on every drain
/// and on a tick, seals each as `MSG_TYPE_REVOCATION`, and POSTs it on the
/// revocation lane.
pub fn due_revocations(
    security: &HubSecurityState,
    now: i64,
) -> Result<Vec<HubDueRevocation>, String> {
    let file_key = require_unlocked()?;
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL burn state is unavailable".to_owned())?;
    let path = config_dir()?.join(REVOCATION_OUTBOX_FILE);
    let outbox = load_revocation_outbox(&path, &file_key)?;
    Ok(outbox
        .due(now)
        .into_iter()
        .map(|e| HubDueRevocation {
            recipient_osl_user_id: e.recipient_id.clone(),
            scope_id_label: e.scope_id_label.clone(),
            storage_key: e.storage_key.clone(),
            notice_b64: e.notice_b64.clone(),
            burn_id_hex: e.burn_id_hex.clone(),
            collapse_key_hex: e.collapse_key_hex.clone(),
            attempts: e.attempts,
        })
        .collect())
}

/// Record a delivery attempt (successful POST or not) and schedule the next one.
///
/// A successful POST is still only an attempt: the burn is not acknowledged
/// until the peer's `0x0B` arrives, so this never marks an entry done.
pub fn record_revocation_attempt(
    security: &HubSecurityState,
    burn_id_hex: &str,
    now: i64,
) -> Result<(), String> {
    let file_key = require_unlocked()?;
    if !canonical_lower_hex(burn_id_hex, 64) {
        return Err("OSL burn identifier is invalid".to_owned());
    }
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL burn state is unavailable".to_owned())?;
    let path = config_dir()?.join(REVOCATION_OUTBOX_FILE);
    let mut outbox = load_revocation_outbox(&path, &file_key)?;
    outbox.record_attempt(burn_id_hex, now);
    write_encrypted_json_with_key(&path, &outbox, &file_key)
        .map_err(|_| "OSL burn queue could not be persisted".to_owned())
}

/// Record an inbound `MSG_TYPE_REVOCATION_ACK`.
///
/// Returns `true` when the ack marked a queued revocation acknowledged. An ack
/// with `applied == false` is a refusal: the entry stays queued for retry, so a
/// peer cannot clear our queue by refusing.
pub fn record_revocation_ack(security: &HubSecurityState, ack_b64: &str) -> Result<bool, String> {
    let file_key = require_unlocked()?;
    let raw = STANDARD
        .decode(ack_b64)
        .map_err(|_| "OSL burn receipt is malformed".to_owned())?;
    let ack = ipc::control_messages::deserialize_revocation_ack(&raw)
        .map_err(|_| "OSL burn receipt is malformed".to_owned())?;
    if !ack.applied {
        return Ok(false);
    }
    let burn_id_hex = lower_hex(&ack.burn_id);
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL burn state is unavailable".to_owned())?;
    let path = config_dir()?.join(REVOCATION_OUTBOX_FILE);
    let mut outbox = load_revocation_outbox(&path, &file_key)?;
    let known = outbox
        .entries
        .iter()
        .any(|e| e.burn_id_hex == burn_id_hex && !e.acknowledged);
    if !known {
        return Ok(false);
    }
    outbox.record_acknowledged(&burn_id_hex);
    write_encrypted_json_with_key(&path, &outbox, &file_key)
        .map_err(|_| "OSL burn queue could not be persisted".to_owned())?;
    Ok(true)
}

/// Delivery status + the three claims for one conversation.
pub fn revocation_status(
    security: &HubSecurityState,
    scope_input: ScopeInput,
) -> Result<HubRevocationStatusDto, String> {
    let file_key = require_unlocked()?;
    let scope: Scope = scope_input
        .try_into()
        .map_err(|_| "OSL scope is invalid".to_owned())?;
    let storage_key = scope.storage_key();
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL burn state is unavailable".to_owned())?;
    let path = config_dir()?.join(REVOCATION_OUTBOX_FILE);
    let outbox = load_revocation_outbox(&path, &file_key)?;
    Ok(HubRevocationStatusDto {
        status: outbox.status_for_scope(&storage_key).to_owned(),
        peers_pending: outbox.pending_for_scope(&storage_key),
        peers_acknowledged: outbox.acknowledged_for_scope(&storage_key),
        storage_key,
        claims: burn_claims(),
    })
}

/// [`revocation_status`] addressed by the storage key a burn just returned.
///
/// The burn result the renderer already holds carries `storage_key`, not a
/// `ScopeInput`, and the hosted context the scope came from is *gone* by the
/// time the burn returns — `burn_local_protected_context` tears it down — so a
/// context-token-addressed status command could never be called on the one path
/// that needs it. [`Scope::parse`] is the documented inverse of
/// [`Scope::storage_key`], so this re-derives the exact same key and refuses
/// anything that is not a canonical scope key rather than guessing.
pub fn revocation_status_for_storage_key(
    security: &HubSecurityState,
    storage_key: &str,
) -> Result<HubRevocationStatusDto, String> {
    validate_storage_key(storage_key)?;
    let scope = Scope::parse(storage_key).ok_or_else(|| "OSL scope is invalid".to_owned())?;
    revocation_status(
        security,
        ScopeInput {
            kind: scope.kind,
            id: scope.id,
            server_id: scope.server_id,
            channel_id: scope.channel_id,
        },
    )
}

/// Peers to notify for a scope, resolved from the peer map **before** any
/// whitelist entry is removed.
fn revocation_peers_for_scope(
    core: &HubCoreState,
    scope: &Scope,
) -> Result<RevocationRecipients, String> {
    let prefs = load_security_preferences()?;
    let dm_scope_person = dm_scope_person(&prefs, scope);
    let peers = core
        .osl
        .peer_map
        .lock()
        .map_err(|_| "OSL peer state is unavailable".to_owned())?
        .clone();
    Ok(revocation_peers_from_map(&peers, scope, dm_scope_person))
}

/// Pure half of [`revocation_peers_for_scope`].
///
/// A peer is notified when they hold an approval covering this scope. A peer
/// whose key state is missing or malformed is **skipped, not guessed at**: there
/// is no key to seal a notice to, and the caller reports the conversation as not
/// fully notified rather than silently claiming success. That second half is the
/// whole reason this returns [`RevocationRecipients`] and not a bare `Vec`: a
/// list that has already forgotten who fell out of it cannot make the promise
/// this comment makes.
fn revocation_peers_from_map(
    peers: &ipc::peer_map::PeerMap,
    scope: &Scope,
    dm_scope_person: Option<&str>,
) -> RevocationRecipients {
    let mut out: Vec<(String, [u8; X25519_PUBLIC_BYTES])> = Vec::new();
    let mut skipped = 0usize;
    for (person_id, peer) in peers.iter() {
        if !peer.outgoing_whitelists.iter().any(|entry| {
            whitelist_entry_matches_scope(entry, scope, dm_scope_person == Some(person_id.as_str()))
        }) {
            // Not approved for this conversation, so never a recipient and never
            // a miss. Only the branches below drop somebody who *was* approved.
            continue;
        }
        let Some(osl_user_id) = peer.osl_user_id.clone() else {
            skipped = skipped.saturating_add(1);
            continue;
        };
        let Ok(peer_pub) = strict_peer_x25519_public(peer) else {
            skipped = skipped.saturating_add(1);
            continue;
        };
        if out.iter().any(|(existing, _)| existing == &osl_user_id) {
            // The same OSL identity reached twice. Already addressed, so this is
            // deduplication and not an omission.
            continue;
        }
        out.push((osl_user_id, peer_pub));
    }
    // Deterministic order so a burn queues the same set every time and a test
    // can assert on it.
    out.sort_by(|a, b| a.0.cmp(&b.0));
    RevocationRecipients {
        peers: out,
        skipped,
        resolver_failed: false,
    }
}

fn lower_hex(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in bytes {
        out.push(char::from_digit((byte >> 4) as u32, 16).unwrap_or('0'));
        out.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap_or('0'));
    }
    out
}

fn parse_friend_code(value: &str) -> Result<SignedFriendCode, String> {
    if value.len() > MAX_FRIEND_CODE_BYTES {
        return Err("OSL friend code is too large".to_owned());
    }
    let encoded = value
        .strip_prefix(FRIEND_CODE_PREFIX)
        .ok_or_else(|| "OSL friend code has an unsupported version".to_owned())?;
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| "OSL friend code is malformed".to_owned())?;
    let signed: SignedFriendCode =
        serde_json::from_slice(&bytes).map_err(|_| "OSL friend code is malformed".to_owned())?;
    if signed.payload.version != FRIEND_CODE_VERSION
        || signed.payload.osl_user_id.is_empty()
        || signed.payload.osl_user_id.len() > 160
    {
        return Err("OSL friend code payload is invalid".to_owned());
    }
    let ed = decode_key_exact::<ED25519_PUBLIC_BYTES>(&signed.payload.ed25519_public, "Ed25519")?;
    let signature =
        decode_transport_exact::<ED25519_SIGNATURE_BYTES>(&signed.signature, "signature")?;
    decode_key_exact::<X25519_PUBLIC_BYTES>(&signed.payload.x25519_public, "X25519")?;
    decode_key_exact::<MLKEM768_PUBLIC_BYTES>(&signed.payload.mlkem768_public, "ML-KEM")?;
    if let Some(value) = signed.payload.ratchet_initial_public.as_deref() {
        decode_key_exact::<RATCHET_PUBLIC_BYTES>(value, "ratchet")?;
    }
    let canonical = serde_json::to_vec(&signed.payload)
        .map_err(|_| "OSL friend code could not be verified".to_owned())?;
    let public = crypto::ed25519::PublicKey::from_bytes(ed);
    let signature = crypto::ed25519::Signature::from_bytes(signature);
    if !crypto::ed25519::verify(&public, &canonical, &signature)
        .map_err(|_| "OSL friend code signature is invalid".to_owned())?
    {
        return Err("OSL friend code signature is invalid".to_owned());
    }
    Ok(signed)
}

fn peer_entry(payload: &FriendCodeUnsigned) -> Result<PeerEntry, String> {
    let key_bundle = friend_code_key_bundle(payload)?;
    Ok(PeerEntry {
        osl_user_id: Some(payload.osl_user_id.clone()),
        pubkey: Some(key_bundle.x25519_pub.clone()),
        ik_mlkem768_pub: Some(key_bundle.mlkem768_pub.clone()),
        ik_ratchet_initial_pub: key_bundle.ratchet_initial_pub.clone(),
        tofu_ed25519_pub: Some(key_bundle.ed25519_pub.clone()),
        tofu_key_bundle: Some(key_bundle),
        first_seen: Some(ipc::main_password::now_unix_secs_pub().to_string()),
        ..PeerEntry::default()
    })
}

fn trusted_peer_key_bundle(
    person_id: &str,
    metadata: &PersonMetadata,
    peer: &PeerEntry,
) -> Result<KeyBundle, String> {
    validate_manual_peer_identity(person_id, metadata, peer)?;
    let bundle = peer.tofu_key_bundle.clone().or_else(|| {
        Some(KeyBundle {
            ed25519_pub: peer.tofu_ed25519_pub.clone()?,
            x25519_pub: peer.pubkey.clone()?,
            mlkem768_pub: peer.ik_mlkem768_pub.clone()?,
            ratchet_initial_pub: peer.ik_ratchet_initial_pub.clone(),
        })
    });
    let bundle = bundle.ok_or_else(|| SAFETY_NUMBER_BUNDLE_REFUSAL.to_owned())?;
    if bundle.ed25519_pub != metadata.ed25519_public
        || peer.tofu_ed25519_pub.as_deref() != Some(bundle.ed25519_pub.as_str())
        || peer.pubkey.as_deref() != Some(bundle.x25519_pub.as_str())
        || peer.ik_mlkem768_pub.as_deref() != Some(bundle.mlkem768_pub.as_str())
        || peer.ik_ratchet_initial_pub != bundle.ratchet_initial_pub
    {
        return Err(SAFETY_NUMBER_BUNDLE_REFUSAL.to_owned());
    }
    safety_number_for_bundle(&bundle)?;
    Ok(bundle)
}

fn standard_base64(value: &str) -> Result<String, String> {
    let decoded = STANDARD
        .decode(value)
        .map_err(|_| "OSL friend code key is malformed".to_owned())?;
    Ok(STANDARD.encode(decoded))
}

fn decode_key_exact<const N: usize>(value: &str, label: &str) -> Result<[u8; N], String> {
    let decoded = STANDARD
        .decode(value)
        .map_err(|_| format!("OSL friend code {label} key is malformed"))?;
    decoded
        .try_into()
        .map_err(|_| format!("OSL friend code {label} key has the wrong length"))
}

fn decode_transport_exact<const N: usize>(value: &str, label: &str) -> Result<[u8; N], String> {
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| format!("OSL friend code {label} is malformed"))?;
    decoded
        .try_into()
        .map_err(|_| format!("OSL friend code {label} has the wrong length"))
}

/// Read the local trust decisions the roster projects. Keeping the manual
/// approval and attribution ledgers together prevents the display from
/// inventing ownership for an unattributable scope.
fn load_security_preferences() -> Result<SecurityPreferences, String> {
    let path = config_dir()?.join(SECURITY_PREFS_FILE);
    load_encrypted_json::<SecurityPreferences>(&path)
}

fn manual_approved_scopes_for_person(
    prefs: &SecurityPreferences,
    person_id: &str,
) -> Vec<PersonWhitelistScopeDto> {
    prefs
        .manual_approved_scope_people
        .iter()
        .filter(|(storage_key, approved_person_id)| {
            approved_person_id.as_str() == person_id
                && manual_scope_preference_approved(prefs, storage_key)
        })
        .map(|(storage_key, _)| PersonWhitelistScopeDto {
            kind: "dm".to_owned(),
            context_id: None,
            storage_key: storage_key.clone(),
            user_specific: true,
        })
        .collect()
}

fn person_dto(
    core: &HubCoreState,
    person_id: &str,
    metadata: &PersonMetadata,
    prefs: &SecurityPreferences,
) -> Result<PersonDto, String> {
    let peer_map = core
        .osl
        .peer_map
        .lock()
        .map_err(|_| "OSL peer state is unavailable".to_owned())?;
    let peer = peer_map
        .get(person_id)
        .ok_or_else(|| "OSL friend key state is missing".to_owned())?;
    let trusted_bundle = trusted_peer_key_bundle(person_id, metadata, peer)?;
    let displayed_bundle = match metadata.pending_key_bundle.as_ref() {
        Some(payload)
            if crate::security::person_id(&payload.ed25519_public) == person_id
                && payload.ed25519_public == metadata.ed25519_public
                && payload.osl_user_id == metadata.osl_user_id =>
        {
            friend_code_key_bundle(payload)?
        }
        Some(_) => return Err(PENDING_KEY_CHANGE_REFUSAL.to_owned()),
        None if metadata.pending_ed25519_public.is_some() => {
            return Err(PENDING_KEY_CHANGE_REFUSAL.to_owned())
        }
        None => trusted_bundle,
    };
    let safety_number = safety_number_for_bundle(&displayed_bundle)?;
    let manual_whitelists = manual_approved_scopes_for_person(prefs, person_id);
    let whitelist_count = manual_whitelists.len();
    let whitelisted_scopes: Vec<PersonWhitelistScopeDto> = manual_whitelists
        .into_iter()
        .take(MAX_VISIBLE_WHITELIST_SCOPES)
        .collect();
    let reach_broadened_at = person_reach_broadened_at(&peer.outgoing_whitelists);
    let reach_narrowed_scopes = prefs
        .reach_narrowed_scopes
        .get(person_id)
        .map(|keys| {
            keys.iter()
                .take(MAX_REACH_NARROWED_SCOPES_PER_PERSON)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    Ok(PersonDto {
        person_id: person_id.to_owned(),
        osl_user_id: metadata.osl_user_id.clone(),
        alias: metadata.alias.clone(),
        // Ordinarily this is the complete bundle OSL holds and encrypts to. A
        // signed pending transport-key update is shown only while encryption
        // is blocked; successful comparison adopts that exact bundle
        // atomically before verification is restored.
        safety_number,
        safety_number_verified: metadata.safety_number_verified,
        whitelist_count,
        whitelisted_scopes,
        whitelisted_scopes_truncated: whitelist_count > MAX_VISIBLE_WHITELIST_SCOPES,
        pending_key_change: metadata.pending_ed25519_public.is_some()
            || metadata.pending_key_bundle.is_some(),
        reach_broadened: reach_broadened_at.is_some(),
        reach_broadened_at: reach_broadened_at.flatten(),
        reach_narrowed_scopes,
    })
}

fn whitelist_scope_dto(entry: &WhitelistEntry) -> PersonWhitelistScopeDto {
    let user_specific = whitelist_entry_user_specific(entry);
    let storage_key = whitelist_entry_storage_key(entry);
    match entry {
        WhitelistEntry::Dm { .. } => PersonWhitelistScopeDto {
            kind: "dm".to_owned(),
            context_id: None,
            storage_key,
            user_specific,
        },
        WhitelistEntry::Gc { id, .. } => PersonWhitelistScopeDto {
            kind: "group".to_owned(),
            context_id: bounded_context_id(id),
            storage_key,
            user_specific,
        },
        WhitelistEntry::ServerChannel {
            server_id,
            channel_id,
            ..
        } => PersonWhitelistScopeDto {
            kind: "channel".to_owned(),
            context_id: bounded_context_id(&format!("{server_id}:{channel_id}")),
            storage_key,
            user_specific,
        },
        WhitelistEntry::ServerFull { server_id, .. } => PersonWhitelistScopeDto {
            kind: "space".to_owned(),
            context_id: bounded_context_id(server_id),
            storage_key,
            user_specific,
        },
    }
}

fn bounded_context_id(value: &str) -> Option<String> {
    if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        None
    } else {
        Some(value.to_owned())
    }
}

fn person_id(ed25519_public: &str) -> String {
    let digest = Sha256::digest(ed25519_public.as_bytes());
    format!("hub-person-{}", URL_SAFE_NO_PAD.encode(&digest[..18]))
}

fn validate_person_id(value: &str) -> Result<(), String> {
    if !value.starts_with("hub-person-")
        || value.len() > 80
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("OSL person id is invalid".to_owned());
    }
    Ok(())
}

fn normalise_alias(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = value else { return Ok(None) };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let forbidden = |character: char| {
        character.is_control()
            || matches!(
                character,
                '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}'
                    | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
            )
    };
    if value.len() > MAX_ALIAS_BYTES
        || value.chars().count() > MAX_ALIAS_CHARS
        || value.contains('<')
        || value.contains('>')
        || value.chars().any(forbidden)
    {
        return Err("OSL friend nickname is invalid".to_owned());
    }
    Ok(Some(value.to_owned()))
}

fn normalise_safety_number(value: &str) -> String {
    value.chars().filter(char::is_ascii_digit).collect()
}

/// Does the number the operator typed match the one OSL derived from the key it
/// holds?
///
/// Digits only on both sides. The operator reads a grouped number aloud and
/// retypes what they heard, so spacing, dashes, newlines and grouping must
/// never decide the ceremony — a ceremony people cannot complete is a ceremony
/// they route around, and routing around this one means encrypting to an
/// unverified key.
///
/// Constant time over the digits, because the dialog can be reopened as often
/// as an attacker likes: a comparison that returns early would leak the number
/// one digit at a time to anyone who can observe the refusal.
///
/// An empty expected or supplied number never matches. There is no "no number
/// to check" success case here.
fn safety_number_matches(expected: &str, supplied: &str) -> bool {
    let expected = normalise_safety_number(expected);
    let supplied = normalise_safety_number(supplied);
    let comparison_value = |digits: &str| {
        let mut value = [0u8; 32];
        for (slot, digit) in value[..30].iter_mut().zip(digits.bytes()) {
            *slot = digit;
        }
        let length = u16::try_from(digits.len()).unwrap_or(u16::MAX);
        value[30..].copy_from_slice(&length.to_be_bytes());
        value
    };
    let equal = ipc::revocation::ct_eq(&comparison_value(&expected), &comparison_value(&supplied));
    equal && expected.len() == 30 && supplied.len() == 30
}

/// Which friend does this DM-kind scope belong to, as far as OSL can prove?
/// `None` for non-DM scopes and for any DM scope with no recorded manual
/// approval — an unattributable DM scope must match nothing.
fn dm_scope_person<'a>(prefs: &'a SecurityPreferences, scope: &Scope) -> Option<&'a str> {
    if scope.kind == ScopeKind::Dm {
        prefs
            .manual_approved_scope_people
            .get(&scope.storage_key())
            .map(String::as_str)
    } else {
        None
    }
}

/// Does this single recorded entry describe *exactly* this scope?
///
/// Entry identity only: it is what add/remove/burn bookkeeping matches on, so a
/// per-scope revocation or burn can never delete an unrelated approval. It
/// deliberately ignores person-level reach — cross-scope trust is answered by
/// [`whitelist_matches`].
///
/// `dm_scope_is_this_person` is the answer to "does this DM scope belong to the
/// friend whose peer entry this entry was found in?", which the caller resolves
/// from [`SecurityPreferences::manual_approved_scope_people`]. It is needed
/// because `WhitelistEntry::Dm` carries no conversation id at all
/// (`crates/ipc/src/peer_map.rs` — the variant holds only `broadened` and
/// `enabled_at`), while every hub conversation is a DM-kind scope. Matching on
/// kind alone therefore made one recorded DM entry cover *every* friend's
/// conversation: burning one chat would have queued a revocation notice to all
/// of them, and a per-scope revocation would have deleted an unrelated
/// approval. There is no id on the entry to compare `scope.id` against, so the
/// owner is compared instead, and a DM scope OSL cannot attribute to anyone
/// matches nothing.
fn whitelist_entry_matches_scope(
    entry: &WhitelistEntry,
    scope: &Scope,
    dm_scope_is_this_person: bool,
) -> bool {
    match (entry, scope.kind) {
        (WhitelistEntry::Dm { .. }, ScopeKind::Dm) => dm_scope_is_this_person,
        (WhitelistEntry::Gc { id, .. }, ScopeKind::Gc) => id == &scope.id,
        (
            WhitelistEntry::ServerChannel {
                server_id,
                channel_id,
                ..
            },
            ScopeKind::ServerChannel,
        ) => {
            scope.server_id.as_ref() == Some(server_id)
                && scope.channel_id.as_ref() == Some(channel_id)
        }
        (WhitelistEntry::ServerFull { server_id, .. }, ScopeKind::ServerFull) => {
            scope.server_id.as_ref() == Some(server_id)
        }
        _ => false,
    }
}

/// Is this person trusted in `scope`?
///
/// Person-level decision over every entry OSL recorded for one friend, plus the
/// scope keys the user explicitly took back from them (`narrowed`).
///
/// | recorded for the person        | scope asked about | narrowed? | result |
/// |-------------------------------|-------------------|-----------|--------|
/// | nothing                       | any               | –         | denied |
/// | `Dm { broadened: false }`     | that person's DM  | –         | allowed (exact) |
/// | `Dm { .. }`                   | another person's or unattributable DM | – | denied |
/// | `Dm { broadened: false }`     | Gc / channel / space | –      | denied |
/// | `Dm { broadened: true }`      | that person's DM  | –         | allowed (exact) |
/// | `Dm { broadened: true }`      | Gc / channel / space | no     | allowed (reach) |
/// | `Dm { broadened: true }`      | Gc / channel / space | yes    | denied (narrowing wins) |
/// | `Gc { id }`                   | that Gc           | –         | allowed (exact) |
/// | `Gc { id }`                   | another Gc / other kind | –   | denied |
/// | `ServerChannel { s, c }`      | `ServerFull { s }` | –        | denied |
/// | `ServerFull { s }`            | `ServerChannel { s, c }` | –  | denied |
/// | any exact entry               | that scope        | yes       | allowed — an explicit approval of the exact scope is the narrower, later decision, and approving clears the narrowing |
///
/// Absence stays fail-closed and reach is only ever honoured when the user
/// recorded it deliberately through [`set_friend_scope_reach`].
fn whitelist_matches(
    entries: &[WhitelistEntry],
    scope: &Scope,
    dm_scope_is_this_person: bool,
    narrowed: Option<&BTreeSet<String>>,
) -> bool {
    if entries
        .iter()
        .any(|entry| whitelist_entry_matches_scope(entry, scope, dm_scope_is_this_person))
    {
        return true;
    }
    // A DM is the scope reach is anchored to; it is never granted by reach.
    if scope.kind == ScopeKind::Dm {
        return false;
    }
    if narrowed.is_some_and(|keys| keys.contains(&scope.storage_key())) {
        return false;
    }
    person_reach_broadened_at(entries).is_some()
}

/// The recorded instant this person's reach was widened, or `None` when their
/// trust is still limited to the scopes approved one by one.
fn person_reach_broadened_at(entries: &[WhitelistEntry]) -> Option<Option<String>> {
    entries.iter().find_map(|entry| match entry {
        WhitelistEntry::Dm {
            broadened: true,
            enabled_at,
        } => Some(enabled_at.clone()),
        _ => None,
    })
}

/// Canonical roster key for one recorded approval. `None` is impossible: the
/// person-level DM entry uses [`DM_REACH_STORAGE_KEY`].
fn whitelist_entry_storage_key(entry: &WhitelistEntry) -> String {
    match entry {
        WhitelistEntry::Dm { .. } => DM_REACH_STORAGE_KEY.to_owned(),
        WhitelistEntry::Gc { id, .. } => Scope::gc(id.clone()).storage_key(),
        WhitelistEntry::ServerChannel {
            server_id,
            channel_id,
            ..
        } => Scope::server_channel(server_id.clone(), channel_id.clone()).storage_key(),
        WhitelistEntry::ServerFull { server_id, .. } => {
            Scope::server_full(server_id.clone()).storage_key()
        }
    }
}

fn whitelist_entry_user_specific(entry: &WhitelistEntry) -> bool {
    match entry {
        // Reach, not per-conversation membership, is the DM entry's flag.
        WhitelistEntry::Dm { .. } => false,
        WhitelistEntry::Gc { user_specific, .. }
        | WhitelistEntry::ServerChannel { user_specific, .. }
        | WhitelistEntry::ServerFull { user_specific, .. } => *user_specific,
    }
}

fn validate_storage_key(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > MAX_STORAGE_KEY_BYTES
        || value.chars().any(char::is_control)
    {
        return Err("OSL scope key is invalid".to_owned());
    }
    Ok(())
}

fn whitelist_entry(scope: &Scope, broadened: bool) -> WhitelistEntry {
    match scope.kind {
        ScopeKind::Dm => WhitelistEntry::Dm {
            broadened,
            enabled_at: Some(ipc::main_password::now_unix_secs_pub().to_string()),
        },
        ScopeKind::Gc => WhitelistEntry::Gc {
            id: scope.id.clone(),
            user_specific: true,
        },
        ScopeKind::ServerChannel => WhitelistEntry::ServerChannel {
            server_id: scope.server_id.clone().unwrap_or_default(),
            channel_id: scope.channel_id.clone().unwrap_or_default(),
            user_specific: true,
        },
        ScopeKind::ServerFull => WhitelistEntry::ServerFull {
            server_id: scope.server_id.clone().unwrap_or_default(),
            user_specific: true,
        },
    }
}

fn burn_channels(
    scope: &Scope,
    known_channel_ids: Vec<String>,
    channel_enumeration_complete: bool,
) -> Result<Vec<String>, String> {
    let channels = match scope.kind {
        ScopeKind::Dm | ScopeKind::Gc => vec![scope.id.clone()],
        ScopeKind::ServerChannel => vec![scope
            .channel_id
            .clone()
            .ok_or_else(|| "OSL channel scope is incomplete".to_owned())?],
        ScopeKind::ServerFull => {
            if !channel_enumeration_complete {
                return Err(
                    "OSL full-space burn requires a complete trusted channel enumeration"
                        .to_owned(),
                );
            }
            known_channel_ids
        }
    };
    if channels.is_empty() {
        return Err("OSL scope burn has no channels to delete".to_owned());
    }
    let mut unique = std::collections::BTreeSet::new();
    unique.extend(channels);
    Ok(unique.into_iter().collect())
}

fn validate_burn_ids(values: &[String], maximum: usize, label: &str) -> Result<(), String> {
    if values.len() > maximum {
        return Err(format!("OSL scope burn has too many {label} ids"));
    }
    if values
        .iter()
        .any(|value| value.is_empty() || value.len() > 160 || value.chars().any(char::is_control))
    {
        return Err(format!("OSL scope burn contains an invalid {label} id"));
    }
    if values.iter().map(String::len).sum::<usize>() > 2 * 1024 * 1024 {
        return Err(format!("OSL scope burn {label} ids are too large"));
    }
    Ok(())
}

fn burn_scope_fields(scope: &Scope) -> (String, Option<String>, Option<String>) {
    match scope.kind {
        ScopeKind::Dm => ("dm".to_owned(), None, Some(scope.id.clone())),
        ScopeKind::Gc => ("gc".to_owned(), None, Some(scope.id.clone())),
        ScopeKind::ServerChannel => (
            "server_channel".to_owned(),
            scope.server_id.clone(),
            scope.channel_id.clone(),
        ),
        ScopeKind::ServerFull => ("server_full".to_owned(), scope.server_id.clone(), None),
    }
}

fn active_user_id(core: &HubCoreState) -> Result<String, String> {
    core.osl
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())?
        .as_ref()
        .map(|identity| identity.user_id.clone())
        .ok_or_else(|| "OSL identity is not loaded".to_owned())
}

fn require_unlocked() -> Result<[u8; 32], String> {
    ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "OSL main password must be unlocked".to_owned())
}

fn validate_hub_ttl(ttl_seconds: u32) -> Result<(), String> {
    if matches!(ttl_seconds, 3_600 | 86_400 | 259_200 | 604_800) {
        Ok(())
    } else {
        Err("OSL message lifetime is unsupported".to_owned())
    }
}

fn config_dir() -> Result<std::path::PathBuf, String> {
    keystore::osl_config_dir().map_err(|_| "OSL account storage is unavailable".to_owned())
}

fn load_encrypted_json<T: Default + for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let key = require_unlocked()?;
    load_encrypted_json_with_key(path, &key)
}

fn load_encrypted_json_with_key<T: Default + for<'de> Deserialize<'de>>(
    path: &Path,
    key: &[u8; 32],
) -> Result<T, String> {
    let Some(bytes) = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_SECURITY_STATE_BYTES,
        "OSL encrypted state",
    )?
    else {
        return Ok(T::default());
    };
    if !ipc::main_password::has_enc_magic(&bytes) {
        return Err("OSL Privacy security state is not encrypted".to_owned());
    }
    let plain = ipc::main_password::decrypt_at_rest(&bytes, key)
        .map_err(|_| "OSL encrypted state could not be opened".to_owned())?;
    serde_json::from_slice(&plain).map_err(|_| "OSL encrypted state is malformed".to_owned())
}

fn load_scope_blobs_strict(path: &Path) -> Result<ipc::scope_blobs_file::ScopeBlobsFile, String> {
    let key = require_unlocked()?;
    load_scope_blobs_strict_with_key(path, &key)
}

fn load_scope_blobs_strict_with_key(
    path: &Path,
    key: &[u8; 32],
) -> Result<ipc::scope_blobs_file::ScopeBlobsFile, String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || metadata.len() > MAX_SECURITY_STATE_BYTES =>
        {
            return Err("OSL scope blob ledger is not a bounded regular file".to_owned())
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ipc::scope_blobs_file::ScopeBlobsFile::default())
        }
        Err(_) => return Err("OSL scope blob ledger metadata could not be read".to_owned()),
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return Err("OSL scope blob ledger could not be read".to_owned()),
    };
    if !ipc::main_password::has_enc_magic(&bytes) {
        return Err("OSL scope blob ledger is not encrypted".to_owned());
    }
    let plain = ipc::main_password::decrypt_at_rest(&bytes, key)
        .map_err(|_| "OSL scope blob ledger could not be opened".to_owned())?;
    serde_json::from_slice(&plain).map_err(|_| "OSL scope blob ledger is malformed".to_owned())
}

fn load_attachment_burn_ledger_with_key(
    path: &Path,
    key: &[u8; 32],
) -> Result<AttachmentBurnLedger, String> {
    let ledger = load_encrypted_json_with_key::<AttachmentBurnLedger>(path, key)
        .map_err(|_| "OSL attachment burn ledger could not be opened".to_owned())?;
    if ledger.version > 1
        || ledger.entries_by_scope.len() > MAX_PEER_REPLAY_SCOPES
        || ledger
            .entries_by_scope
            .values()
            .any(|entries| entries.len() > MAX_ATTACHMENT_BURN_ENTRIES_PER_SCOPE)
        || ledger
            .entries_by_scope
            .values()
            .map(Vec::len)
            .sum::<usize>()
            > MAX_ATTACHMENT_BURN_ENTRIES_TOTAL
        || ledger.entries_by_scope.values().flatten().any(|entry| {
            !canonical_lower_hex(&entry.object_id, 32)
                || !canonical_lower_hex(&entry.fetch_token, 32)
                || entry.expires_at <= 0
        })
    {
        return Err("OSL attachment burn ledger is malformed".to_owned());
    }
    Ok(ledger)
}

fn validate_attachment_burn_entry(
    object_id: &str,
    fetch_token: &str,
    expires_at: i64,
    now: i64,
) -> Result<(), String> {
    if !canonical_lower_hex(object_id, 32)
        || !canonical_lower_hex(fetch_token, 32)
        || expires_at <= now
        || expires_at > now.saturating_add(604_800)
    {
        return Err("OSL attachment burn capability is invalid".to_owned());
    }
    Ok(())
}

fn canonical_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn prune_expired_attachment_burn_entries(ledger: &mut AttachmentBurnLedger, now: i64) {
    ledger.entries_by_scope.retain(|_, entries| {
        entries.retain(|entry| entry.expires_at > now);
        !entries.is_empty()
    });
}

fn take_attachment_burn_entries(
    ledger: &mut AttachmentBurnLedger,
    storage_key: &str,
) -> Vec<AttachmentBurnEntry> {
    ledger
        .entries_by_scope
        .remove(storage_key)
        .unwrap_or_default()
}

fn parse_attachment_fetch_token(value: &str) -> Result<[u8; 16], String> {
    if !canonical_lower_hex(value, 32) {
        return Err("OSL attachment burn capability is invalid".to_owned());
    }
    let mut token = [0u8; 16];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk)
            .map_err(|_| "OSL attachment burn capability is invalid".to_owned())?;
        token[index] = u8::from_str_radix(text, 16)
            .map_err(|_| "OSL attachment burn capability is invalid".to_owned())?;
    }
    Ok(token)
}

fn write_encrypted_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let key = require_unlocked()?;
    write_encrypted_json_with_key(path, value, &key)
}

fn write_scope_blobs(
    path: &Path,
    value: &ipc::scope_blobs_file::ScopeBlobsFile,
) -> Result<(), String> {
    let key = require_unlocked()?;
    write_scope_blobs_with_key(path, value, &key)
}

fn write_scope_blobs_with_key(
    path: &Path,
    value: &ipc::scope_blobs_file::ScopeBlobsFile,
    key: &[u8; 32],
) -> Result<(), String> {
    write_encrypted_json_with_key(path, value, key)
        .map_err(|_| "OSL remote-message burn ledger could not be persisted".to_owned())
}

fn write_encrypted_json_with_key<T: Serialize>(
    path: &Path,
    value: &T,
    key: &[u8; 32],
) -> Result<(), String> {
    let body = serde_json::to_vec(value)
        .map_err(|_| "OSL security state could not be encoded".to_owned())?;
    let sealed = ipc::main_password::encrypt_at_rest(&body, key)
        .map_err(|_| "OSL security state could not be encrypted".to_owned())?;
    if sealed.len() as u64 > MAX_SECURITY_STATE_BYTES {
        return Err("OSL security state exceeds its storage limit".to_owned());
    }
    crate::atomic_file::write_recoverable(path, &sealed, "OSL security state")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_FILE_KEY: [u8; 32] = [0x91; 32];

    struct FileBackedSecurityHarness {
        dir: std::path::PathBuf,
        previous_active_account_dir: Option<std::path::PathBuf>,
        previous_file_key: Option<[u8; 32]>,
        _serial: std::sync::MutexGuard<'static, ()>,
    }

    impl FileBackedSecurityHarness {
        fn new(label: &str) -> Self {
            let serial = crate::global_keystore_test_lock();
            let dir = fresh_test_dir(label);
            let previous_active_account_dir = keystore::active_account_dir();
            let previous_file_key = ipc::main_password::get_file_storage_key();
            keystore::set_active_account_dir(Some(dir.clone()));
            ipc::main_password::set_file_storage_key(Some(TEST_FILE_KEY));
            Self {
                dir,
                previous_active_account_dir,
                previous_file_key,
                _serial: serial,
            }
        }

        fn path(&self) -> &Path {
            &self.dir
        }
    }

    impl Drop for FileBackedSecurityHarness {
        fn drop(&mut self) {
            keystore::set_active_account_dir(self.previous_active_account_dir.clone());
            ipc::main_password::set_file_storage_key(self.previous_file_key);
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn fresh_test_dir(label: &str) -> std::path::PathBuf {
        let base = std::env::temp_dir();
        for attempt in 0..100 {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let dir = base.join(format!(
                "osl-hub-security-{label}-{}-{nonce}-{attempt}",
                std::process::id()
            ));
            match std::fs::create_dir(&dir) {
                Ok(()) => return dir,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("create test config dir: {error}"),
            }
        }
        panic!("could not allocate unique test config dir");
    }

    fn test_friend(seed: u8) -> (String, PersonMetadata, PeerEntry) {
        let ed25519_public = STANDARD.encode([seed; ED25519_PUBLIC_BYTES]);
        let x25519_public = STANDARD.encode([seed.wrapping_add(1); X25519_PUBLIC_BYTES]);
        let mlkem768_public = STANDARD.encode([seed.wrapping_add(2); MLKEM768_PUBLIC_BYTES]);
        let person_id = person_id(&ed25519_public);
        let key_bundle = KeyBundle {
            ed25519_pub: ed25519_public.clone(),
            x25519_pub: x25519_public.clone(),
            mlkem768_pub: mlkem768_public.clone(),
            ratchet_initial_pub: None,
        };
        (
            person_id,
            PersonMetadata {
                osl_user_id: format!("osl-test-peer-{seed}"),
                ed25519_public,
                safety_number_verified: true,
                ..PersonMetadata::default()
            },
            PeerEntry {
                osl_user_id: Some(format!("osl-test-peer-{seed}")),
                pubkey: Some(x25519_public),
                ik_mlkem768_pub: Some(mlkem768_public),
                tofu_ed25519_pub: Some(key_bundle.ed25519_pub.clone()),
                tofu_key_bundle: Some(key_bundle),
                ..PeerEntry::default()
            },
        )
    }

    fn write_people(dir: &Path, person_id: &str, metadata: PersonMetadata) {
        let mut people = PeopleFile {
            version: 1,
            ..PeopleFile::default()
        };
        people.people.insert(person_id.to_owned(), metadata);
        write_encrypted_json(&dir.join(PEOPLE_FILE), &people).unwrap();
    }

    fn install_peer_map(core: &HubCoreState, dir: &Path, person_id: &str, peer: PeerEntry) {
        let mut peers = ipc::peer_map::PeerMap::new();
        peers.insert(person_id.to_owned(), peer);
        write_encrypted_json(&dir.join("peer_map.json"), &peers).unwrap();
        *core.osl.peer_map.lock().unwrap() = peers;
    }

    fn load_peer_map(dir: &Path) -> ipc::peer_map::PeerMap {
        load_encrypted_json::<ipc::peer_map::PeerMap>(&dir.join("peer_map.json")).unwrap()
    }

    fn install_self_identity(core: &HubCoreState) {
        *core.osl.identity.lock().unwrap() = Some(keystore::generate_native_identity());
    }

    fn friend_code_for_identity(identity: &keystore::Identity) -> String {
        let payload = FriendCodeUnsigned {
            version: FRIEND_CODE_VERSION,
            osl_user_id: identity.user_id.clone(),
            x25519_public: STANDARD.encode(identity.x25519_public.as_bytes()),
            ed25519_public: STANDARD.encode(identity.ed25519_public.as_bytes()),
            mlkem768_public: STANDARD.encode(identity.mlkem_public_bytes),
            ratchet_initial_public: identity
                .ratchet_initial_pub
                .map(|key| STANDARD.encode(key.as_bytes())),
        };
        let canonical = serde_json::to_vec(&payload).unwrap();
        let signature = crypto::ed25519::sign(&identity.ed25519_secret, &canonical);
        let signed = SignedFriendCode {
            payload,
            signature: URL_SAFE_NO_PAD.encode(signature.as_bytes()),
        };
        format!(
            "{FRIEND_CODE_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&signed).unwrap())
        )
    }

    fn friend_code_with_osl_user_id(osl_user_id: &str) -> String {
        let mut identity = keystore::generate_native_identity();
        identity.user_id = osl_user_id.to_owned();
        friend_code_for_identity(&identity)
    }

    fn test_manual_binding(person_id: String) -> ManualPeerBinding {
        ManualPeerBinding {
            person_id,
            peer_osl_user_id: "peer-private-handle".to_owned(),
            peer_x25519_public: [0x21; X25519_PUBLIC_BYTES],
            peer_mlkem768_public: [0x42; MLKEM768_PUBLIC_BYTES],
        }
    }

    fn dm_scope_input(id: String) -> ScopeInput {
        ScopeInput {
            kind: ScopeKind::Dm,
            id,
            server_id: None,
            channel_id: None,
        }
    }

    #[test]
    fn manual_peer_scope_accepts_broker_dm_channel_binding() {
        let (person_id, _, _) = test_friend(6);
        let binding = test_manual_binding(person_id.clone());
        let account_id = "native-discord-00112233445566778899aabbccddeeff0011223344556677";
        let scope_id = manual_peer_scope_id("discord", account_id, &person_id).unwrap();
        let scope = ScopeInput {
            kind: ScopeKind::Dm,
            id: scope_id.clone(),
            server_id: None,
            channel_id: Some(format!(
                "manual-dm-{}",
                "a".repeat(MANUAL_DM_BINDING_HEX_LEN)
            )),
        };

        ScopedTrustGrant::for_manual_peer(
            &binding,
            "discord",
            account_id,
            scope,
            ScopedTrustConsent::ExplicitUserAction,
        )
        .unwrap();

        let wrong_channel = ScopedTrustGrant::for_manual_peer(
            &binding,
            "discord",
            account_id,
            ScopeInput {
                kind: ScopeKind::Dm,
                id: scope_id,
                server_id: None,
                channel_id: Some("other-channel".to_owned()),
            },
            ScopedTrustConsent::ExplicitUserAction,
        )
        .unwrap_err();
        assert_eq!(wrong_channel, "OSL scoped trust scope is invalid");
    }

    #[test]
    fn first_party_osl_chat_scope_hashes_account_as_exact_binding_component() {
        let (person_id, _, _) = test_friend(8);
        let main = manual_peer_scope_id("osl-chat", "osl-main", &person_id).unwrap();
        let other = manual_peer_scope_id("osl-chat", "account-private-1", &person_id).unwrap();

        assert_ne!(main, other);
        assert!(manual_peer_scope_id("unknown-service", "account-private-1", &person_id).is_err());
    }

    #[test]
    fn scoped_trust_grant_requires_explicit_consent_and_exact_friend_scope() {
        let (person_id, _, _) = test_friend(7);
        let binding = test_manual_binding(person_id.clone());
        let account_id = "account-private-1";
        let scope_id = manual_peer_scope_id("osl-chat", account_id, &person_id).unwrap();

        let grant = ScopedTrustGrant::for_manual_peer(
            &binding,
            "osl-chat",
            account_id,
            dm_scope_input(scope_id.clone()),
            ScopedTrustConsent::ExplicitUserAction,
        )
        .unwrap();

        assert_eq!(grant.person_id(), person_id.as_str());
        assert_eq!(grant.service_id(), "osl-chat");
        assert_eq!(grant.account_id(), account_id);
        assert_eq!(grant.scope().kind, ScopeKind::Dm);
        assert_eq!(grant.storage_key(), format!("dm:{scope_id}"));
        grant.require_binding(Some(&binding)).unwrap();

        let absent_consent = ScopedTrustGrant::for_manual_peer(
            &binding,
            "osl-chat",
            account_id,
            dm_scope_input(scope_id.clone()),
            ScopedTrustConsent::Absent,
        )
        .unwrap_err();
        assert_eq!(
            absent_consent,
            "OSL scoped trust requires explicit approval"
        );

        let wrong_scope = ScopedTrustGrant::for_manual_peer(
            &binding,
            "osl-chat",
            account_id,
            dm_scope_input("manual-scope-other".to_owned()),
            ScopedTrustConsent::ExplicitUserAction,
        )
        .unwrap_err();
        assert_eq!(wrong_scope, "OSL scoped trust scope is invalid");

        let (other_person_id, _, _) = test_friend(8);
        let other_scope = manual_peer_scope_id("osl-chat", account_id, &other_person_id).unwrap();
        let wrong_friend_scope = ScopedTrustGrant::for_manual_peer(
            &binding,
            "osl-chat",
            account_id,
            dm_scope_input(other_scope),
            ScopedTrustConsent::ExplicitUserAction,
        )
        .unwrap_err();
        assert_eq!(wrong_friend_scope, "OSL scoped trust scope is invalid");

        let wrong_channel = ScopedTrustGrant::for_manual_peer(
            &binding,
            "osl-chat",
            account_id,
            ScopeInput {
                kind: ScopeKind::Dm,
                id: scope_id,
                server_id: None,
                channel_id: Some("manual-scope-other".to_owned()),
            },
            ScopedTrustConsent::ExplicitUserAction,
        )
        .unwrap_err();
        assert_eq!(wrong_channel, "OSL scoped trust scope is invalid");
    }

    #[test]
    fn scoped_trust_grant_refuses_absent_or_different_friend_binding() {
        let (person_id, _, _) = test_friend(9);
        let binding = test_manual_binding(person_id.clone());
        let account_id = "account-private-2";
        let scope_id = manual_peer_scope_id("osl-chat", account_id, &person_id).unwrap();
        let grant = ScopedTrustGrant::for_manual_peer(
            &binding,
            "osl-chat",
            account_id,
            dm_scope_input(scope_id),
            ScopedTrustConsent::ExplicitUserAction,
        )
        .unwrap();

        assert_eq!(
            grant.require_binding(None).unwrap_err(),
            "OSL scoped trust binding is missing"
        );

        let (other_person_id, _, _) = test_friend(10);
        let other_binding = test_manual_binding(other_person_id);
        assert_eq!(
            grant.require_binding(Some(&other_binding)).unwrap_err(),
            "OSL scoped trust binding does not match"
        );

        let mut changed_handle = binding.clone();
        changed_handle.peer_osl_user_id = "changed-private-handle".to_owned();
        assert_eq!(
            grant.require_binding(Some(&changed_handle)).unwrap_err(),
            "OSL scoped trust binding does not match"
        );

        let mut changed_x25519 = binding.clone();
        changed_x25519.peer_x25519_public[0] ^= 1;
        assert_eq!(
            grant.require_binding(Some(&changed_x25519)).unwrap_err(),
            "OSL scoped trust binding does not match"
        );

        let mut changed_mlkem = binding.clone();
        changed_mlkem.peer_mlkem768_public[0] ^= 1;
        assert_eq!(
            grant.require_binding(Some(&changed_mlkem)).unwrap_err(),
            "OSL scoped trust binding does not match"
        );
    }

    #[test]
    fn scoped_trust_debug_output_redacts_identifiers_and_key_material() {
        let (person_id, _, _) = test_friend(11);
        let binding = test_manual_binding(person_id.clone());
        let account_id = "account-private-3";
        let scope_id = manual_peer_scope_id("osl-chat", account_id, &person_id).unwrap();
        let grant = ScopedTrustGrant::for_manual_peer(
            &binding,
            "osl-chat",
            account_id,
            dm_scope_input(scope_id.clone()),
            ScopedTrustConsent::ExplicitUserAction,
        )
        .unwrap();

        let grant_debug = format!("{grant:?}");
        assert!(grant_debug.contains("ScopedTrustGrant"));
        assert!(grant_debug.contains("scope_kind"));
        assert!(!grant_debug.contains(&person_id));
        assert!(!grant_debug.contains(account_id));
        assert!(!grant_debug.contains(&scope_id));
        assert!(!grant_debug.contains(&format!("dm:{scope_id}")));

        let binding_debug = format!("{binding:?}");
        assert!(binding_debug.contains("ManualPeerBinding"));
        assert!(!binding_debug.contains(&person_id));
        assert!(!binding_debug.contains("peer-private-handle"));
        assert!(!binding_debug.contains("33"));
        assert!(!binding_debug.contains("66"));

        let grant_display = format!("{grant}");
        assert!(grant_display.contains("ScopedTrustGrant"));
        assert!(!grant_display.contains(&person_id));
        assert!(!grant_display.contains(account_id));
        assert!(!grant_display.contains(&scope_id));
        assert!(!grant_display.contains(&format!("dm:{scope_id}")));

        let binding_display = format!("{binding}");
        assert!(binding_display.contains("ManualPeerBinding"));
        assert!(!binding_display.contains(&person_id));
        assert!(!binding_display.contains("peer-private-handle"));
        assert!(!binding_display.contains("33"));
        assert!(!binding_display.contains("66"));
    }

    #[test]
    fn scoped_trust_acceptance_friend_request_grant_roundtrip() {
        let harness = FileBackedSecurityHarness::new("scoped-trust-friend-request");
        let core = HubCoreState::default();
        let security = HubSecurityState::default();
        install_self_identity(&core);

        let friend = keystore::generate_native_identity();
        let added = add_friend_code(&core, &security, friend_code_for_identity(&friend), None)
            .expect("signed friend request code imports");
        assert_eq!(added.disposition, AddFriendDisposition::Added);
        assert!(
            !added.safety_number_verified,
            "a friend request import must not imply ceremony verification"
        );
        assert_eq!(
            manual_peer_binding(&core, added.person_id.clone()).unwrap_err(),
            "Verify this friend's safety number before enabling encryption"
        );

        let verified = verify_friend_safety_number(
            &core,
            &security,
            added.person_id.clone(),
            added.safety_number.clone(),
        )
        .expect("friend request safety-number ceremony verifies");
        assert!(verified.safety_number_verified);
        assert_eq!(verified.whitelist_count, 0);

        let binding = manual_peer_binding(&core, added.person_id.clone())
            .expect("verified friend resolves to a manual peer binding");
        let scope_id = manual_peer_scope_id("osl-chat", "osl-main", &added.person_id).unwrap();
        let scope_input = dm_scope_input(scope_id.clone());
        assert_eq!(
            ScopedTrustGrant::for_manual_peer(
                &binding,
                "osl-chat",
                "osl-main",
                scope_input.clone(),
                ScopedTrustConsent::Absent,
            )
            .unwrap_err(),
            "OSL scoped trust requires explicit approval"
        );
        assert_eq!(
            require_manual_peer_scope_approved(
                &core,
                "osl-chat",
                "osl-main",
                added.person_id.clone(),
                scope_input.clone(),
            )
            .unwrap_err(),
            "Approve encryption for this friend before continuing"
        );

        let grant = ScopedTrustGrant::for_manual_peer(
            &binding,
            "osl-chat",
            "osl-main",
            scope_input.clone(),
            ScopedTrustConsent::ExplicitUserAction,
        )
        .expect("explicit consent mints the exact scoped trust grant");
        grant.require_binding(Some(&binding)).unwrap();
        assert_eq!(grant.person_id(), added.person_id.as_str());
        assert_eq!(grant.service_id(), "osl-chat");
        assert_eq!(grant.account_id(), "osl-main");
        assert_eq!(grant.storage_key(), format!("dm:{scope_id}"));

        let requester_discord_id = scope_id.clone();
        core.osl.peer_map.lock().unwrap().insert(
            requester_discord_id.clone(),
            PeerEntry {
                discord_id: Some(requester_discord_id.clone()),
                tofu_key_bundle: Some(KeyBundle {
                    ed25519_pub: "requester-ed25519".to_owned(),
                    x25519_pub: "requester-x25519".to_owned(),
                    mlkem768_pub: "requester-mlkem768".to_owned(),
                    ratchet_initial_pub: Some("requester-ratchet".to_owned()),
                }),
                ..PeerEntry::default()
            },
        );
        let typed_request = ipc::commands::cmd_osl_send_friend_request(
            &core.osl,
            requester_discord_id.clone(),
            scope_input.clone(),
        )
        .expect("trusted peer binding mints typed friend request")
        .request;
        ipc::commands::cmd_osl_accept_friend_request(
            &core.osl,
            requester_discord_id.clone(),
            typed_request,
        )
        .expect("typed friend request adopts its scoped grant in the original core");
        {
            let peer_map = core.osl.peer_map.lock().unwrap();
            let requester = peer_map
                .get(&requester_discord_id)
                .expect("friend request acceptance records the requester binding");
            assert!(
                requester.outgoing_whitelists.iter().any(|entry| matches!(
                    entry,
                    WhitelistEntry::Dm {
                        broadened: false,
                        ..
                    }
                )),
                "accepted request must adopt the exact scoped trust grant"
            );
        }
        {
            let whitelist_state = core.osl.whitelist_state.lock().unwrap();
            let adopted = whitelist_state
                .get(grant.storage_key())
                .expect("accepted request enables the granted scope");
            assert!(adopted.encrypt_toggle);
            assert!(adopted.auto_enabled);
        }
        assert_eq!(
            require_manual_peer_scope_approved(
                &core,
                "osl-chat",
                "osl-main",
                added.person_id.clone(),
                scope_input.clone(),
            )
            .unwrap_err(),
            "Approve encryption for this friend before continuing",
            "IPC adoption alone must not bypass the hub's friend-attributed scoped grant"
        );

        let mut wrong_binding = binding.clone();
        wrong_binding.peer_x25519_public[0] ^= 1;
        assert_eq!(
            apply_scoped_trust_grant(&security, &wrong_binding, &grant).unwrap_err(),
            "OSL scoped trust binding does not match"
        );

        apply_scoped_trust_grant(&security, &binding, &grant)
            .expect("accepted friend request persists the scoped grant");

        let approved_binding = require_manual_peer_scope_approved(
            &core,
            "osl-chat",
            "osl-main",
            added.person_id.clone(),
            scope_input.clone(),
        )
        .expect("persisted grant authorizes only the verified friend binding");
        assert_eq!(approved_binding, binding);
        assert!(manual_peer_scope_approved(
            &core,
            "osl-chat",
            "osl-main",
            added.person_id.clone(),
            scope_input.clone()
        )
        .unwrap());
        assert_eq!(
            manual_peer_scope_approved(
                &core,
                "osl-chat",
                "osl-main",
                added.person_id.clone(),
                dm_scope_input("manual-scope-other".to_owned())
            )
            .unwrap_err(),
            "OSL manual peer scope is invalid"
        );

        let stored: SecurityPreferences =
            load_encrypted_json(&harness.path().join(SECURITY_PREFS_FILE)).unwrap();
        assert!(stored.manual_approved_scopes.contains(grant.storage_key()));
        assert_eq!(
            stored
                .manual_approved_scope_people
                .get(grant.storage_key())
                .map(String::as_str),
            Some(added.person_id.as_str()),
            "manual approval must remain attributed to the accepted friend"
        );

        let people = list_people(&core).unwrap();
        let friend_row = people
            .iter()
            .find(|person| person.person_id.as_str() == added.person_id.as_str())
            .expect("accepted friend remains on the roster");
        assert_eq!(friend_row.whitelist_count, 1);
        assert_eq!(friend_row.whitelisted_scopes.len(), 1);
        assert_eq!(
            friend_row.whitelisted_scopes[0].storage_key,
            grant.storage_key()
        );
        assert!(friend_row.whitelisted_scopes[0].user_specific);
        assert!(!friend_row.reach_broadened);
    }

    #[test]
    fn friend_code_import_refuses_snowflake_identity_without_writing_state() {
        let harness = FileBackedSecurityHarness::new("snowflake-refused");
        let core = HubCoreState::default();
        install_self_identity(&core);

        let error = add_friend_code(
            &core,
            &HubSecurityState::default(),
            friend_code_with_osl_user_id("123456789012345678"),
            None,
        )
        .unwrap_err();

        assert_eq!(error, SNOWFLAKE_IDENTITY_REFUSAL);
        assert!(!harness.path().join(PEOPLE_FILE).exists());
        assert!(!harness.path().join("peer_map.json").exists());
        assert!(core.osl.peer_map.lock().unwrap().is_empty());
    }

    #[test]
    fn friend_code_import_accepts_native_osl_identity() {
        let harness = FileBackedSecurityHarness::new("native-accepted");
        let core = HubCoreState::default();
        install_self_identity(&core);
        let friend = keystore::generate_native_identity();
        let friend_osl_user_id = friend.user_id.clone();

        let added = add_friend_code(
            &core,
            &HubSecurityState::default(),
            friend_code_for_identity(&friend),
            None,
        )
        .unwrap();

        assert_eq!(added.disposition, AddFriendDisposition::Added);
        assert_eq!(added.osl_user_id, friend_osl_user_id);
        let people: PeopleFile = load_encrypted_json(&harness.path().join(PEOPLE_FILE)).unwrap();
        assert!(people
            .people
            .values()
            .any(|person| person.osl_user_id == friend_osl_user_id));
        let peers = load_peer_map(harness.path());
        assert!(peers
            .values()
            .any(|peer| peer.osl_user_id.as_deref() == Some(friend_osl_user_id.as_str())));
    }

    #[test]
    fn friend_code_import_snowflake_shape_boundaries_are_exact() {
        for (label, osl_user_id, accepted) in [
            ("sixteen", "1234567890123456", true),
            ("seventeen", "12345678901234567", false),
            ("twenty", "12345678901234567890", false),
            ("twenty-one", "123456789012345678901", true),
        ] {
            let harness = FileBackedSecurityHarness::new(label);
            let core = HubCoreState::default();
            install_self_identity(&core);
            let result = add_friend_code(
                &core,
                &HubSecurityState::default(),
                friend_code_with_osl_user_id(osl_user_id),
                None,
            );

            if accepted {
                let added = result.unwrap();
                assert_eq!(added.disposition, AddFriendDisposition::Added);
                assert_eq!(added.osl_user_id, osl_user_id);
                assert!(harness.path().join(PEOPLE_FILE).exists());
                assert!(harness.path().join("peer_map.json").exists());
            } else {
                assert_eq!(result.unwrap_err(), SNOWFLAKE_IDENTITY_REFUSAL);
                assert!(!harness.path().join(PEOPLE_FILE).exists());
                assert!(!harness.path().join("peer_map.json").exists());
            }
        }
    }

    #[test]
    fn signed_friend_code_rejects_tampering() {
        let identity = keystore::generate_identity("osl-test".to_owned());
        let payload = FriendCodeUnsigned {
            version: FRIEND_CODE_VERSION,
            osl_user_id: identity.user_id.clone(),
            x25519_public: STANDARD.encode(identity.x25519_public.as_bytes()),
            ed25519_public: STANDARD.encode(identity.ed25519_public.as_bytes()),
            mlkem768_public: STANDARD.encode(identity.mlkem_public_bytes),
            ratchet_initial_public: None,
        };
        let canonical = serde_json::to_vec(&payload).unwrap();
        let signature = crypto::ed25519::sign(&identity.ed25519_secret, &canonical);
        let mut signed = SignedFriendCode {
            payload,
            signature: URL_SAFE_NO_PAD.encode(signature.as_bytes()),
        };
        signed.payload.osl_user_id.push_str("-tampered");
        let code = format!(
            "{FRIEND_CODE_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&signed).unwrap())
        );
        let error = match parse_friend_code(&code) {
            Ok(_) => panic!("tampered friend code accepted"),
            Err(error) => error,
        };
        assert!(error.contains("signature"));
    }

    #[test]
    fn safety_number_comparison_requires_the_complete_number_but_ignores_grouping() {
        let expected = "12345 67890 12345 67890 12345 67890";
        assert!(!safety_number_matches(
            expected,
            "12345 67890 12345 67890 12345 67891"
        ));
        assert!(safety_number_matches(
            expected,
            "123-456\n789 012 345-678-901-234-567-890"
        ));
        assert!(!safety_number_matches(expected, ""));
        assert!(!safety_number_matches("", "123456789012345678901234567890"));
        assert!(!safety_number_matches(
            expected,
            "12345678901234567890123456789"
        ));
        assert!(!safety_number_matches(
            expected,
            "1234567890123456789012345678901"
        ));
    }

    #[test]
    fn verify_friend_safety_number_refuses_safety_number_mismatch_mutants() {
        let harness = FileBackedSecurityHarness::new("safety-mismatch-refusal");
        let core = HubCoreState::default();
        let security = HubSecurityState::default();
        let (person_id, mut metadata, peer) = test_friend(61);
        metadata.safety_number_verified = false;
        let expected =
            safety_number_for_bundle(peer.tofu_key_bundle.as_ref().expect("fixture bundle"))
                .unwrap();
        let mut wrong = normalise_safety_number(&expected).into_bytes();
        let last = wrong.last_mut().expect("fixture has safety-number digits");
        *last = if *last == b'9' { b'8' } else { *last + 1 };
        let wrong = String::from_utf8(wrong).unwrap();
        write_people(harness.path(), &person_id, metadata.clone());
        install_peer_map(&core, harness.path(), &person_id, peer.clone());

        let error = verify_friend_safety_number(&core, &security, person_id.clone(), wrong)
            .expect_err("mismatched safety number must refuse");
        assert_eq!(error, SAFETY_NUMBER_MISMATCH_REFUSAL);
        let people: PeopleFile = load_encrypted_json(&harness.path().join(PEOPLE_FILE)).unwrap();
        assert_eq!(
            people
                .people
                .get(&person_id)
                .unwrap()
                .safety_number_verified,
            false,
            "refusal must not mark the friend verified"
        );
        assert_eq!(
            load_peer_map(harness.path()).get(&person_id),
            Some(&peer),
            "refusal must not rewrite trusted peer keys"
        );

        let verified =
            verify_friend_safety_number(&core, &security, person_id.clone(), expected.clone())
                .expect("matching safety number verifies the fixture");
        assert!(verified.safety_number_verified);
        let people: PeopleFile = load_encrypted_json(&harness.path().join(PEOPLE_FILE)).unwrap();
        assert!(
            people
                .people
                .get(&person_id)
                .unwrap()
                .safety_number_verified
        );
    }

    #[test]
    fn key_change_alert_ceremony_paths_persist_only_after_verified_safety() {
        let harness = FileBackedSecurityHarness::new("key-change-ceremony-persist");
        let core = HubCoreState::default();
        let security = HubSecurityState::default();
        install_self_identity(&core);
        let friend = keystore::generate_native_identity();
        let added =
            add_friend_code(&core, &security, friend_code_for_identity(&friend), None).unwrap();
        verify_friend_safety_number(
            &core,
            &security,
            added.person_id.clone(),
            added.safety_number.clone(),
        )
        .unwrap();
        let trusted_peer = load_peer_map(harness.path())
            .get(&added.person_id)
            .expect("trusted peer persisted")
            .clone();

        let changed_payload = FriendCodeUnsigned {
            version: FRIEND_CODE_VERSION,
            osl_user_id: friend.user_id.clone(),
            x25519_public: STANDARD.encode([71u8; X25519_PUBLIC_BYTES]),
            ed25519_public: STANDARD.encode(friend.ed25519_public.as_bytes()),
            mlkem768_public: STANDARD.encode([72u8; MLKEM768_PUBLIC_BYTES]),
            ratchet_initial_public: Some(STANDARD.encode([73u8; RATCHET_PUBLIC_BYTES])),
        };
        let canonical = serde_json::to_vec(&changed_payload).unwrap();
        let signature = crypto::ed25519::sign(&friend.ed25519_secret, &canonical);
        let changed_code = format!(
            "{FRIEND_CODE_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(
                serde_json::to_vec(&SignedFriendCode {
                    payload: changed_payload.clone(),
                    signature: URL_SAFE_NO_PAD.encode(signature.as_bytes()),
                })
                .unwrap()
            )
        );
        let staged = add_friend_code(&core, &security, changed_code, None).unwrap();
        assert_eq!(
            staged.disposition,
            AddFriendDisposition::KeyChangeRequiresVerification
        );
        assert!(!staged.safety_number_verified);
        assert_ne!(staged.safety_number, added.safety_number);
        assert_eq!(
            load_peer_map(harness.path()).get(&added.person_id),
            Some(&trusted_peer),
            "staging a key change must not persist new encryption keys"
        );
        let people: PeopleFile = load_encrypted_json(&harness.path().join(PEOPLE_FILE)).unwrap();
        let staged_metadata = people.people.get(&added.person_id).unwrap();
        assert!(!staged_metadata.safety_number_verified);
        assert_eq!(
            staged_metadata.pending_key_bundle.as_ref(),
            Some(&changed_payload)
        );

        let mut wrong = normalise_safety_number(&staged.safety_number).into_bytes();
        let last = wrong.last_mut().expect("fixture has safety-number digits");
        *last = if *last == b'9' { b'8' } else { *last + 1 };
        let wrong = String::from_utf8(wrong).unwrap();
        assert_eq!(
            verify_friend_safety_number(&core, &security, added.person_id.clone(), wrong)
                .unwrap_err(),
            SAFETY_NUMBER_MISMATCH_REFUSAL
        );
        assert_eq!(
            load_peer_map(harness.path()).get(&added.person_id),
            Some(&trusted_peer),
            "mismatch refusal must leave old keys persisted"
        );
        let people: PeopleFile = load_encrypted_json(&harness.path().join(PEOPLE_FILE)).unwrap();
        assert!(
            people
                .people
                .get(&added.person_id)
                .unwrap()
                .pending_key_bundle
                .is_some(),
            "mismatch refusal must leave pending ceremony state"
        );

        let verified = verify_friend_safety_number(
            &core,
            &security,
            added.person_id.clone(),
            staged.safety_number,
        )
        .unwrap();
        assert!(verified.safety_number_verified);
        assert!(!verified.pending_key_change);
        let people: PeopleFile = load_encrypted_json(&harness.path().join(PEOPLE_FILE)).unwrap();
        let verified_metadata = people.people.get(&added.person_id).unwrap();
        assert!(verified_metadata.safety_number_verified);
        assert!(verified_metadata.pending_key_bundle.is_none());
        let persisted_peer = load_peer_map(harness.path())
            .get(&added.person_id)
            .expect("verified peer persisted")
            .clone();
        assert_eq!(persisted_peer.pubkey, Some(changed_payload.x25519_public));
        assert_eq!(
            persisted_peer.ik_mlkem768_pub,
            Some(changed_payload.mlkem768_public)
        );
        assert_eq!(
            persisted_peer.ik_ratchet_initial_pub,
            changed_payload.ratchet_initial_public
        );
    }

    #[test]
    fn every_transport_key_change_changes_the_ceremony_and_clears_verification() {
        let base = FriendCodeUnsigned {
            version: FRIEND_CODE_VERSION,
            osl_user_id: "osl-test-peer".to_owned(),
            x25519_public: STANDARD.encode([1u8; X25519_PUBLIC_BYTES]),
            ed25519_public: STANDARD.encode([2u8; ED25519_PUBLIC_BYTES]),
            mlkem768_public: STANDARD.encode([3u8; MLKEM768_PUBLIC_BYTES]),
            ratchet_initial_public: Some(STANDARD.encode([4u8; RATCHET_PUBLIC_BYTES])),
        };
        let trusted = friend_code_key_bundle(&base).unwrap();
        let trusted_number = safety_number_for_bundle(&trusted).unwrap();
        let changed_payloads = [
            FriendCodeUnsigned {
                x25519_public: STANDARD.encode([9u8; X25519_PUBLIC_BYTES]),
                ..base.clone()
            },
            FriendCodeUnsigned {
                mlkem768_public: STANDARD.encode([9u8; MLKEM768_PUBLIC_BYTES]),
                ..base.clone()
            },
            FriendCodeUnsigned {
                ratchet_initial_public: Some(STANDARD.encode([9u8; RATCHET_PUBLIC_BYTES])),
                ..base.clone()
            },
        ];

        for changed in changed_payloads {
            let presented = friend_code_key_bundle(&changed).unwrap();
            let presented_number = safety_number_for_bundle(&presented).unwrap();
            assert_ne!(trusted_number, presented_number);
            let mut metadata = PersonMetadata {
                osl_user_id: base.osl_user_id.clone(),
                ed25519_public: base.ed25519_public.clone(),
                safety_number_verified: true,
                ..PersonMetadata::default()
            };
            assert!(stage_pending_key_bundle(
                &mut metadata,
                &changed,
                &trusted,
                &presented
            ));
            assert!(!metadata.safety_number_verified);
            assert_eq!(metadata.pending_key_bundle.as_ref(), Some(&changed));

            let digits = normalise_safety_number(&presented_number);
            let regrouped = digits
                .as_bytes()
                .chunks(3)
                .map(|chunk| std::str::from_utf8(chunk).unwrap())
                .collect::<Vec<_>>()
                .join("-");
            assert!(safety_number_matches(&presented_number, &regrouped));

            let mut wrong = digits.into_bytes();
            let last = wrong.last_mut().unwrap();
            *last = if *last == b'9' { b'8' } else { *last + 1 };
            assert!(!safety_number_matches(
                &presented_number,
                std::str::from_utf8(&wrong).unwrap()
            ));
        }
    }

    #[test]
    fn claimed_user_id_never_overrides_the_identity_key_a_code_proves() {
        let key_a = STANDARD.encode([1u8; ED25519_PUBLIC_BYTES]);
        let key_b = STANDARD.encode([2u8; ED25519_PUBLIC_BYTES]);
        let payload_a = FriendCodeUnsigned {
            version: FRIEND_CODE_VERSION,
            osl_user_id: "same-claimed-user".to_owned(),
            x25519_public: String::new(),
            ed25519_public: key_a.clone(),
            mlkem768_public: String::new(),
            ratchet_initial_public: None,
        };
        let payload_b = FriendCodeUnsigned {
            ed25519_public: key_b.clone(),
            ..payload_a.clone()
        };
        let person_a = person_id(&payload_a.ed25519_public);
        assert_ne!(person_a, person_id(&payload_b.ed25519_public));

        let peer = PeerEntry {
            osl_user_id: Some(payload_a.osl_user_id.clone()),
            tofu_ed25519_pub: Some(key_a.clone()),
            ..PeerEntry::default()
        };
        let valid_metadata = PersonMetadata {
            osl_user_id: payload_a.osl_user_id,
            ed25519_public: key_a,
            ..PersonMetadata::default()
        };
        assert!(validate_manual_peer_identity(&person_a, &valid_metadata, &peer).is_ok());

        let wrong_key_metadata = PersonMetadata {
            ed25519_public: key_b,
            ..valid_metadata
        };
        assert!(validate_manual_peer_identity(&person_a, &wrong_key_metadata, &peer).is_err());
    }

    #[test]
    fn scope_match_is_service_neutral() {
        let scope = Scope::server_channel("space", "conversation");
        let entry = whitelist_entry(&scope, false);
        assert!(whitelist_entry_matches_scope(&entry, &scope, false));
        assert!(!whitelist_entry_matches_scope(
            &entry,
            &Scope::gc("conversation"),
            false,
        ));
    }

    #[test]
    fn person_ids_do_not_expose_keys() {
        let id = person_id("sensitive-public-key-material");
        assert!(id.starts_with("hub-person-"));
        assert!(!id.contains("sensitive"));
    }

    #[test]
    fn friend_nicknames_are_trimmed_bounded_and_reject_invisible_controls() {
        assert_eq!(
            normalise_alias(Some("  Rose  ")).unwrap(),
            Some("Rose".to_owned())
        );
        assert_eq!(normalise_alias(Some("   ")).unwrap(), None);
        assert!(normalise_alias(Some(&"a".repeat(MAX_ALIAS_BYTES + 1))).is_err());
        assert!(normalise_alias(Some("Rose\u{202e}hidden")).is_err());
        assert!(normalise_alias(Some("Rose\nOther")).is_err());
    }

    #[test]
    fn whitelist_descriptions_do_not_invent_service_or_account_links() {
        assert_eq!(
            whitelist_scope_dto(&WhitelistEntry::Dm {
                broadened: false,
                enabled_at: None
            }),
            PersonWhitelistScopeDto {
                kind: "dm".to_owned(),
                context_id: None,
                storage_key: "dm".to_owned(),
                user_specific: false,
            }
        );
        assert_eq!(
            whitelist_scope_dto(&WhitelistEntry::Gc {
                id: "gc-1".to_owned(),
                user_specific: true
            }),
            PersonWhitelistScopeDto {
                kind: "group".to_owned(),
                context_id: Some("gc-1".to_owned()),
                storage_key: "gc:gc-1".to_owned(),
                user_specific: true,
            }
        );
        assert_eq!(bounded_context_id(&"x".repeat(513)), None);
        assert_eq!(bounded_context_id("unsafe\ncontext"), None);
    }

    fn dm_entry(broadened: bool) -> WhitelistEntry {
        WhitelistEntry::Dm {
            broadened,
            enabled_at: Some("1770000000".to_owned()),
        }
    }

    #[test]
    fn dm_approval_matches_only_its_attributed_friend() {
        let scope = Scope::dm("manual-scope-a");
        let entry = dm_entry(false);
        assert!(whitelist_entry_matches_scope(&entry, &scope, true));
        assert!(!whitelist_entry_matches_scope(&entry, &scope, false));

        let mut peers = ipc::peer_map::PeerMap::new();
        peers.insert(
            "person-a".to_owned(),
            PeerEntry {
                osl_user_id: Some("osl-a".to_owned()),
                pubkey: Some(STANDARD.encode([1u8; X25519_PUBLIC_BYTES])),
                outgoing_whitelists: vec![entry.clone()],
                ..PeerEntry::default()
            },
        );
        peers.insert(
            "person-b".to_owned(),
            PeerEntry {
                osl_user_id: Some("osl-b".to_owned()),
                pubkey: Some(STANDARD.encode([2u8; X25519_PUBLIC_BYTES])),
                outgoing_whitelists: vec![entry],
                ..PeerEntry::default()
            },
        );

        let resolved = revocation_peers_from_map(&peers, &scope, Some("person-a"));
        assert_eq!(resolved.peers.len(), 1);
        assert_eq!(resolved.peers[0].0, "osl-a");
        assert!(resolved.fully_addressed());
        // Nobody is attributed this DM, so neither peer is a recipient — and
        // neither is a miss.
        let unattributed = revocation_peers_from_map(&peers, &scope, None);
        assert!(unattributed.peers.is_empty());
        assert!(unattributed.fully_addressed());
    }

    /// A burn may report the peer-notification phase complete only when it can
    /// prove there was nobody left to tell. The three ways of ending up with an
    /// empty recipient list that are NOT that proof — a malformed transport key,
    /// a missing OSL identifier, and a resolver that could not read the approval
    /// state at all — each have to force `revocation_queue_complete = false`.
    ///
    /// Audit: "Burn can claim revocation queuing is complete after silently
    /// omitting an approved peer" (`docs/security/osl-audit-2026-07-26-codex.md`).
    #[test]
    fn burn_never_reports_complete_after_dropping_an_unaddressable_peer() {
        let scope = Scope::gc("gc-1");
        let entry = WhitelistEntry::Gc {
            id: "gc-1".to_owned(),
            user_specific: false,
        };
        let mut peers = ipc::peer_map::PeerMap::new();
        peers.insert(
            "person-a".to_owned(),
            PeerEntry {
                osl_user_id: Some("osl-a".to_owned()),
                // Approved, and its transport key is corrupt.
                pubkey: Some(STANDARD.encode([1u8; 31])),
                outgoing_whitelists: vec![entry.clone()],
                ..PeerEntry::default()
            },
        );
        peers.insert(
            "person-b".to_owned(),
            PeerEntry {
                // Approved, and its OSL id is gone.
                osl_user_id: None,
                pubkey: Some(STANDARD.encode([2u8; X25519_PUBLIC_BYTES])),
                outgoing_whitelists: vec![entry],
                ..PeerEntry::default()
            },
        );

        let core = HubCoreState::default();
        let queue = |recipients: &RevocationRecipients| {
            queue_scope_revocations_locked(
                &core,
                &scope.storage_key(),
                &scope.storage_key(),
                recipients,
                &[],
                0,
            )
            .unwrap()
        };

        let resolved = revocation_peers_from_map(&peers, &scope, None);
        assert!(resolved.peers.is_empty());
        assert_eq!(resolved.skipped, 2);
        assert_eq!(
            queue(&resolved),
            (0, false),
            "two approved peers were dropped, so the conversation is not fully notified"
        );

        // Each half of the defect on its own, so a later change cannot fix one
        // and quietly leave the other.
        let malformed_key_only = RevocationRecipients {
            peers: Vec::new(),
            skipped: 1,
            resolver_failed: false,
        };
        assert_eq!(queue(&malformed_key_only), (0, false));

        // The resolver itself failed: how many peers were approved is unknown,
        // which is never the same as none.
        assert_eq!(queue(&RevocationRecipients::unresolved()), (0, false));

        // The only empty list that may report complete: resolution succeeded and
        // proved there were no approved recipients.
        let nobody_approved =
            revocation_peers_from_map(&ipc::peer_map::PeerMap::new(), &scope, None);
        assert!(nobody_approved.fully_addressed());
        assert_eq!(queue(&nobody_approved), (0, true));
    }

    /// A5-F4. Grant a per-scope permission, take the storage key from the exact
    /// roster projection the "−" button renders, hand it to the exact function
    /// `revoke_active_hub_friend_scope` calls, and require the permission to
    /// actually be gone afterwards. Before the namespace was shared, the grant
    /// lived under `dm:manual-scope-<b64>` while this path only ever compared
    /// against the peer map's bare `dm` reach key, so the revoke matched
    /// nothing and the approval survived.
    #[test]
    fn roster_scope_revoke_actually_withdraws_the_grant_it_displays() {
        let harness = FileBackedSecurityHarness::new("roster-scope-revoke");
        let core = HubCoreState::default();
        install_self_identity(&core);
        let security = HubSecurityState::default();
        let (person_id, metadata, peer) = test_friend(57);
        write_people(harness.path(), &person_id, metadata);
        install_peer_map(&core, harness.path(), &person_id, peer);
        write_encrypted_json(
            &harness.path().join(SECURITY_PREFS_FILE),
            &SecurityPreferences::default(),
        )
        .unwrap();

        let scope_id = manual_peer_scope_id("osl-chat", "osl-main", &person_id).unwrap();
        set_manual_peer_scope_permission(
            &core,
            &security,
            "osl-chat",
            "osl-main",
            person_id.clone(),
            dm_scope_input(scope_id.clone()),
            true,
        )
        .unwrap();
        assert!(manual_peer_scope_approved(
            &core,
            "osl-chat",
            "osl-main",
            person_id.clone(),
            dm_scope_input(scope_id.clone())
        )
        .unwrap());

        // Exactly the row the roster renders the revoke button from.
        let granted: SecurityPreferences =
            load_encrypted_json(&harness.path().join(SECURITY_PREFS_FILE)).unwrap();
        let rows = manual_approved_scopes_for_person(&granted, &person_id);
        assert_eq!(rows.len(), 1, "the roster must show the grant it just made");
        let storage_key = rows[0].storage_key.clone();

        let dto = revoke_friend_scope_entry(
            &core,
            &security,
            "osl-chat",
            "osl-main",
            person_id.clone(),
            storage_key.clone(),
        )
        .expect("the roster's own scope key must be revocable");
        assert!(
            dto.whitelisted_scopes.is_empty(),
            "the revoked scope is still projected to the roster"
        );
        assert_eq!(dto.whitelist_count, 0);

        let stored: SecurityPreferences =
            load_encrypted_json(&harness.path().join(SECURITY_PREFS_FILE)).unwrap();
        assert!(!stored.manual_approved_scopes.contains(&storage_key));
        assert!(!stored
            .manual_approved_scope_people
            .contains_key(&storage_key));

        // The enforcement predicate, not just the display, must now refuse.
        assert!(!manual_peer_scope_approved(
            &core,
            "osl-chat",
            "osl-main",
            person_id,
            dm_scope_input(scope_id)
        )
        .unwrap());
    }

    /// Read the burn identifiers the outbox actually persisted, so the ack half
    /// of this lifecycle is driven by the real queue rather than by recomputing
    /// the commitment chain a second time (which would only prove the test
    /// agrees with itself).
    fn queued_burn_ids(storage_key: &str) -> Vec<String> {
        let file_key = ipc::main_password::get_file_storage_key().expect("file key is set");
        let path = config_dir().unwrap().join(REVOCATION_OUTBOX_FILE);
        let mut ids: Vec<String> = load_revocation_outbox(&path, &file_key)
            .unwrap()
            .entries
            .iter()
            .filter(|entry| entry.storage_key == storage_key)
            .map(|entry| entry.burn_id_hex.clone())
            .collect();
        ids.sort();
        ids
    }

    fn ack_b64(burn_id_hex: &str, applied: bool) -> String {
        let mut burn_id = [0u8; 32];
        for (index, byte) in burn_id.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&burn_id_hex[index * 2..index * 2 + 2], 16)
                .expect("burn id is lower hex");
        }
        STANDARD.encode(
            ipc::control_messages::serialize_revocation_ack(
                &ipc::control_messages::RevocationAck { burn_id, applied },
            )
            .unwrap(),
        )
    }

    /// Queued is not acknowledged, and the product has to be able to say so.
    ///
    /// `queue_scope_revocations_locked` promises the operator "must be shown
    /// `Not acknowledged` — never a success" while a notice is only queued, and
    /// `HubScopeBurnResult::revocations_queued` is documented "Queued, not
    /// delivered — see `revocation_status`". `revocation_status` was never
    /// wired to anything, so that promise had no live surface at all.
    ///
    /// This drives the whole delivery lifecycle through
    /// [`revocation_status_for_storage_key`] — the exact entry point the
    /// `get_hub_revocation_status` Tauri command calls — and pins the one thing
    /// the UI must never get wrong: a conversation only reads acknowledged when
    /// every peer has actually acknowledged.
    #[test]
    fn revocation_status_reports_queued_notices_as_not_acknowledged_until_every_peer_acks() {
        let _harness = FileBackedSecurityHarness::new("revocation-status-lifecycle");
        let core = HubCoreState::default();
        install_self_identity(&core);
        let security = HubSecurityState::default();
        let storage_key = Scope::dm("peer-conversation-1").storage_key();

        // A key shape that is not a canonical scope key is refused, not guessed.
        assert!(revocation_status_for_storage_key(&security, "not-a-scope-key").is_err());

        let recipients = RevocationRecipients {
            peers: vec![
                ("osl-peer-a".to_owned(), [0x11; X25519_PUBLIC_BYTES]),
                ("osl-peer-b".to_owned(), [0x22; X25519_PUBLIC_BYTES]),
            ],
            skipped: 0,
            resolver_failed: false,
        };
        assert_eq!(
            queue_scope_revocations(
                &core,
                &security,
                &storage_key,
                &storage_key,
                &recipients,
                &[],
                1_700_000_000,
            )
            .unwrap(),
            (2, true),
            "both peers are addressable, so both notices queue"
        );

        // Queued for two peers, acknowledged by neither. This is the state the
        // shipping build could not express.
        let queued = revocation_status_for_storage_key(&security, &storage_key).unwrap();
        assert_eq!(queued.storage_key, storage_key);
        assert_eq!(queued.status, ipc::revocation::STATUS_NOT_ACKNOWLEDGED);
        assert_eq!((queued.peers_pending, queued.peers_acknowledged), (2, 0));

        let burn_ids = queued_burn_ids(&storage_key);
        assert_eq!(burn_ids.len(), 2);

        // A delivery attempt is not an acknowledgement.
        record_revocation_attempt(&security, &burn_ids[0], 1_700_000_100).unwrap();
        let attempted = revocation_status_for_storage_key(&security, &storage_key).unwrap();
        assert_eq!(attempted.status, ipc::revocation::STATUS_SENT_REQUEST);
        assert_eq!(
            (attempted.peers_pending, attempted.peers_acknowledged),
            (2, 0)
        );

        // One peer acknowledges; the conversation as a whole still may not read
        // complete while the other is outstanding.
        assert!(record_revocation_ack(&security, &ack_b64(&burn_ids[0], true)).unwrap());
        let half = revocation_status_for_storage_key(&security, &storage_key).unwrap();
        assert_eq!(half.status, ipc::revocation::STATUS_SENT_REQUEST);
        assert_eq!((half.peers_pending, half.peers_acknowledged), (1, 1));

        // A refusal (`applied == false`) never clears the queue.
        assert!(!record_revocation_ack(&security, &ack_b64(&burn_ids[1], false)).unwrap());
        let refused = revocation_status_for_storage_key(&security, &storage_key).unwrap();
        assert_eq!(refused.status, ipc::revocation::STATUS_SENT_REQUEST);
        assert_eq!((refused.peers_pending, refused.peers_acknowledged), (1, 1));

        // Both acknowledged: now, and only now, the conversation is complete.
        assert!(record_revocation_ack(&security, &ack_b64(&burn_ids[1], true)).unwrap());
        let complete = revocation_status_for_storage_key(&security, &storage_key).unwrap();
        assert_eq!(complete.status, ipc::revocation::STATUS_ACKNOWLEDGED);
        assert_eq!((complete.peers_pending, complete.peers_acknowledged), (0, 2));
        assert_eq!(complete.claims, burn_claims());

        // Another conversation is never covered by this one's acknowledgements.
        let untouched = revocation_status_for_storage_key(
            &security,
            &Scope::dm("peer-conversation-2").storage_key(),
        )
        .unwrap();
        assert_eq!(untouched.status, ipc::revocation::STATUS_NOT_ACKNOWLEDGED);
        assert_eq!(
            (untouched.peers_pending, untouched.peers_acknowledged),
            (0, 0)
        );
    }

    #[test]
    fn roster_projects_only_live_manual_approvals_for_the_attributed_friend() {
        let storage_key = Scope::dm("manual-scope-a").storage_key();
        let mut prefs = SecurityPreferences::default();
        prefs.manual_approved_scopes.insert(storage_key.clone());
        prefs
            .manual_approved_scope_people
            .insert(storage_key.clone(), "person-a".to_owned());

        let projected = manual_approved_scopes_for_person(&prefs, "person-a");
        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].storage_key, storage_key);
        assert!(manual_approved_scopes_for_person(&prefs, "person-b").is_empty());

        prefs.burned_manual_scopes.insert(storage_key);
        assert!(manual_approved_scopes_for_person(&prefs, "person-a").is_empty());
    }

    #[test]
    fn withdrawing_friend_grants_preserves_other_people_and_terminal_burns() {
        let harness = FileBackedSecurityHarness::new("withdraw-friend-grants");
        let core = HubCoreState::default();
        install_self_identity(&core);
        let security = HubSecurityState::default();
        let (person_a, metadata_a, peer_a) = test_friend(21);
        let (person_b, metadata_b, peer_b) = test_friend(22);
        write_encrypted_json(
            &harness.path().join(PEOPLE_FILE),
            &PeopleFile {
                version: 1,
                people: BTreeMap::from([
                    (person_a.clone(), metadata_a),
                    (person_b.clone(), metadata_b),
                ]),
            },
        )
        .unwrap();
        let peers =
            ipc::peer_map::PeerMap::from([(person_a.clone(), peer_a), (person_b.clone(), peer_b)]);
        write_encrypted_json(&harness.path().join("peer_map.json"), &peers).unwrap();
        *core.osl.peer_map.lock().unwrap() = peers;
        write_encrypted_json(
            &harness.path().join(SECURITY_PREFS_FILE),
            &SecurityPreferences {
                version: 2,
                ..SecurityPreferences::default()
            },
        )
        .unwrap();

        let binding_a = manual_peer_binding(&core, person_a.clone()).unwrap();
        let grant_a = ScopedTrustGrant::for_manual_peer(
            &binding_a,
            "osl-chat",
            "osl-main",
            dm_scope_input(manual_peer_scope_id("osl-chat", "osl-main", &person_a).unwrap()),
            ScopedTrustConsent::ExplicitUserAction,
        )
        .unwrap();
        apply_scoped_trust_grant(&security, &binding_a, &grant_a).unwrap();
        let binding_b = manual_peer_binding(&core, person_b.clone()).unwrap();
        let grant_b = ScopedTrustGrant::for_manual_peer(
            &binding_b,
            "osl-chat",
            "osl-main",
            dm_scope_input(manual_peer_scope_id("osl-chat", "osl-main", &person_b).unwrap()),
            ScopedTrustConsent::ExplicitUserAction,
        )
        .unwrap();
        apply_scoped_trust_grant(&security, &binding_b, &grant_b).unwrap();

        let terminal_burn = "dm:terminal-burn".to_owned();
        let mut prefs: SecurityPreferences =
            load_encrypted_json(&harness.path().join(SECURITY_PREFS_FILE)).unwrap();
        prefs
            .decrypt_display_by_scope
            .insert(grant_a.storage_key().to_owned(), true);
        prefs
            .decrypt_display_by_scope
            .insert(grant_b.storage_key().to_owned(), true);
        prefs
            .reach_narrowed_scopes
            .insert(person_a.clone(), BTreeSet::from(["gc:a".to_owned()]));
        prefs
            .reach_narrowed_scopes
            .insert(person_b.clone(), BTreeSet::from(["gc:b".to_owned()]));
        prefs.burned_manual_scopes.insert(terminal_burn.clone());
        write_encrypted_json(&harness.path().join(SECURITY_PREFS_FILE), &prefs).unwrap();

        let result = remove_friend(&core, &security, person_a.clone()).unwrap();
        assert_eq!(result.approvals_withdrawn, 1);
        assert!(result.peer_key_removed);

        let stored: SecurityPreferences =
            load_encrypted_json(&harness.path().join(SECURITY_PREFS_FILE)).unwrap();
        assert!(!stored
            .manual_approved_scopes
            .contains(grant_a.storage_key()));
        assert!(!stored
            .manual_approved_scope_people
            .contains_key(grant_a.storage_key()));
        assert!(!stored
            .decrypt_display_by_scope
            .contains_key(grant_a.storage_key()));
        assert!(stored
            .manual_approved_scopes
            .contains(grant_b.storage_key()));
        assert_eq!(
            stored
                .manual_approved_scope_people
                .get(grant_b.storage_key())
                .map(String::as_str),
            Some(person_b.as_str())
        );
        assert_eq!(
            stored.decrypt_display_by_scope.get(grant_b.storage_key()),
            Some(&true)
        );
        assert!(!stored.reach_narrowed_scopes.contains_key(&person_a));
        assert!(stored.reach_narrowed_scopes.contains_key(&person_b));
        assert_eq!(stored.burned_manual_scopes, BTreeSet::from([terminal_burn]));

        assert_eq!(
            manual_peer_scope_approved(
                &core,
                "osl-chat",
                "osl-main",
                person_a,
                dm_scope_input(grant_a.scope().id.clone())
            )
            .unwrap_err(),
            "OSL friend is unknown"
        );
        assert!(manual_peer_scope_approved(
            &core,
            "osl-chat",
            "osl-main",
            person_b,
            dm_scope_input(grant_b.scope().id.clone())
        )
        .unwrap());
    }

    #[test]
    fn remove_friend_preserves_burned_manual_scopes_on_disk() {
        let harness = FileBackedSecurityHarness::new("remove-preserves-burn");
        let core = HubCoreState::default();
        let security = HubSecurityState::default();
        let (person_id, metadata, peer) = test_friend(31);
        write_people(harness.path(), &person_id, metadata);
        install_peer_map(&core, harness.path(), &person_id, peer);

        let burned_scope = Scope::dm("burned-terminal").storage_key();
        let approved_scope = Scope::gc("approved-before-removal").storage_key();
        let narrowed_scope = Scope::server_channel("space-a", "channel-a").storage_key();
        let mut prefs = SecurityPreferences {
            version: 2,
            ..SecurityPreferences::default()
        };
        prefs
            .manual_approved_scopes
            .extend([burned_scope.clone(), approved_scope.clone()]);
        prefs.manual_approved_scope_people.extend([
            (burned_scope.clone(), person_id.clone()),
            (approved_scope.clone(), person_id.clone()),
        ]);
        prefs
            .decrypt_display_by_scope
            .extend([(burned_scope.clone(), true), (approved_scope.clone(), true)]);
        prefs
            .reach_narrowed_scopes
            .insert(person_id.clone(), BTreeSet::from([narrowed_scope]));
        prefs.burned_manual_scopes.insert(burned_scope.clone());
        let withdrawn_before: Vec<String> = prefs
            .manual_approved_scope_people
            .iter()
            .filter(|(_, approved_person_id)| *approved_person_id == &person_id)
            .map(|(storage_key, _)| storage_key.clone())
            .collect();
        assert!(
            !withdrawn_before.is_empty(),
            "fixture must withdraw at least one grant"
        );
        write_encrypted_json(&harness.path().join(SECURITY_PREFS_FILE), &prefs).unwrap();

        let result = remove_friend(&core, &security, person_id.clone()).unwrap();
        assert_eq!(result.approvals_withdrawn, withdrawn_before.len());
        assert!(result.peer_key_removed);

        let stored: SecurityPreferences =
            load_encrypted_json(&harness.path().join(SECURITY_PREFS_FILE)).unwrap();
        for withdrawn in &withdrawn_before {
            assert!(!stored.manual_approved_scopes.contains(withdrawn));
            assert!(!stored.manual_approved_scope_people.contains_key(withdrawn));
            assert!(!stored.decrypt_display_by_scope.contains_key(withdrawn));
        }
        assert!(!stored.reach_narrowed_scopes.contains_key(&person_id));
        assert!(stored.burned_manual_scopes.contains(&burned_scope));
    }

    #[test]
    fn remove_friend_rolls_back_peer_map_on_people_write_failure() {
        let harness = FileBackedSecurityHarness::new("remove-rollback-peer-map");
        let core = HubCoreState::default();
        let security = HubSecurityState::default();
        let (person_id, metadata, peer) = test_friend(41);
        write_people(harness.path(), &person_id, metadata);
        install_peer_map(&core, harness.path(), &person_id, peer);
        write_encrypted_json(
            &harness.path().join(SECURITY_PREFS_FILE),
            &SecurityPreferences::default(),
        )
        .unwrap();

        assert!(
            load_peer_map(harness.path()).contains_key(&person_id),
            "fixture must start with the peer persisted"
        );
        std::fs::create_dir(harness.path().join("hub_people.tmp")).unwrap();

        assert!(remove_friend(&core, &security, person_id.clone()).is_err());
        assert!(
            load_peer_map(harness.path()).contains_key(&person_id),
            "failed People write must restore the peer map on disk"
        );
    }

    #[test]
    fn person_reach_is_never_implied_by_an_ordinary_approval() {
        let gc = Scope::gc("gc-1");
        let channel = Scope::server_channel("space-1", "channel-1");
        let space = Scope::server_full("space-1");
        let dm = Scope::dm("peer-1");

        // Absent entry stays fail-closed for every scope kind.
        assert!(!whitelist_matches(&[], &dm, true, None));
        assert!(!whitelist_matches(&[], &gc, false, None));
        assert!(!whitelist_matches(&[], &channel, false, None));
        assert!(!whitelist_matches(&[], &space, false, None));

        // A plain DM approval covers the DM and nothing else.
        let plain = [dm_entry(false)];
        assert!(whitelist_matches(&plain, &dm, true, None));
        assert!(!whitelist_matches(&plain, &gc, false, None));
        assert!(!whitelist_matches(&plain, &channel, false, None));
        assert!(!whitelist_matches(&plain, &space, false, None));

        // Explicitly broadened reach covers the other scope kinds.
        let broadened = [dm_entry(true)];
        assert!(whitelist_matches(&broadened, &dm, true, None));
        assert!(whitelist_matches(&broadened, &gc, false, None));
        assert!(whitelist_matches(&broadened, &channel, false, None));
        assert!(whitelist_matches(&broadened, &space, false, None));
    }

    #[test]
    fn recorded_narrowing_beats_broadened_reach() {
        let gc = Scope::gc("gc-1");
        let other_gc = Scope::gc("gc-2");
        let broadened = [dm_entry(true)];
        let narrowed = BTreeSet::from([gc.storage_key()]);

        assert!(!whitelist_matches(&broadened, &gc, false, Some(&narrowed)));
        // Narrowing is per scope, not per person.
        assert!(whitelist_matches(
            &broadened,
            &other_gc,
            false,
            Some(&narrowed)
        ));

        // An explicit approval of the exact scope is the later, narrower
        // decision and wins; the ordinary approve path clears the narrowing.
        let approved = [
            dm_entry(true),
            WhitelistEntry::Gc {
                id: "gc-1".to_owned(),
                user_specific: true,
            },
        ];
        assert!(whitelist_matches(&approved, &gc, false, Some(&narrowed)));

        // Narrowing never grants anything on its own.
        assert!(!whitelist_matches(&[], &gc, false, Some(&narrowed)));
    }

    #[test]
    fn per_scope_entries_do_not_leak_across_scope_kinds() {
        let channel = Scope::server_channel("space-1", "channel-1");
        let other_channel = Scope::server_channel("space-1", "channel-2");
        let space = Scope::server_full("space-1");
        let gc = Scope::gc("gc-1");

        let channel_entry = [WhitelistEntry::ServerChannel {
            server_id: "space-1".to_owned(),
            channel_id: "channel-1".to_owned(),
            user_specific: true,
        }];
        assert!(whitelist_matches(&channel_entry, &channel, false, None));
        assert!(!whitelist_matches(
            &channel_entry,
            &other_channel,
            false,
            None
        ));
        assert!(!whitelist_matches(&channel_entry, &space, false, None));

        let space_entry = [WhitelistEntry::ServerFull {
            server_id: "space-1".to_owned(),
            user_specific: true,
        }];
        assert!(whitelist_matches(&space_entry, &space, false, None));
        assert!(!whitelist_matches(&space_entry, &channel, false, None));

        let gc_entry = [WhitelistEntry::Gc {
            id: "gc-1".to_owned(),
            user_specific: true,
        }];
        assert!(whitelist_matches(&gc_entry, &gc, false, None));
        assert!(!whitelist_matches(
            &gc_entry,
            &Scope::gc("gc-2"),
            false,
            None
        ));
        assert!(!whitelist_matches(
            &gc_entry,
            &Scope::dm("peer-1"),
            true,
            None
        ));
        // A group approval alone never widens into other scope kinds.
        assert!(!whitelist_matches(&gc_entry, &space, false, None));
    }

    #[test]
    fn reach_records_when_trust_was_widened_and_collapses_without_losing_scopes() {
        let mut entries = vec![
            dm_entry(true),
            WhitelistEntry::Gc {
                id: "gc-1".to_owned(),
                user_specific: true,
            },
        ];
        assert_eq!(
            person_reach_broadened_at(&entries),
            Some(Some("1770000000".to_owned()))
        );
        collapse_person_reach(&mut entries);
        assert!(person_reach_broadened_at(&entries).is_none());
        // Withdrawing reach keeps every per-scope approval, including the DM.
        assert_eq!(entries.len(), 2);
        assert!(whitelist_matches(
            &entries,
            &Scope::dm("peer-1"),
            true,
            None
        ));
        assert!(whitelist_matches(&entries, &Scope::gc("gc-1"), false, None));
        assert!(!whitelist_matches(
            &entries,
            &Scope::gc("gc-2"),
            false,
            None
        ));
    }

    #[test]
    fn roster_keys_round_trip_to_the_scope_they_describe() {
        assert_eq!(whitelist_entry_storage_key(&dm_entry(true)), "dm");
        assert_eq!(
            whitelist_entry_storage_key(&WhitelistEntry::Gc {
                id: "gc-1".to_owned(),
                user_specific: true
            }),
            Scope::gc("gc-1").storage_key()
        );
        assert_eq!(
            whitelist_entry_storage_key(&WhitelistEntry::ServerChannel {
                server_id: "space-1".to_owned(),
                channel_id: "channel-1".to_owned(),
                user_specific: true
            }),
            Scope::server_channel("space-1", "channel-1").storage_key()
        );
        assert_eq!(
            whitelist_entry_storage_key(&WhitelistEntry::ServerFull {
                server_id: "space-1".to_owned(),
                user_specific: false
            }),
            Scope::server_full("space-1").storage_key()
        );
        // The person-level DM sentinel can never name a real conversation.
        assert!(Scope::parse("dm").is_none());
        assert!(validate_storage_key("").is_err());
        assert!(validate_storage_key(
            "gc:bad
key"
        )
        .is_err());
        assert!(validate_storage_key(&"x".repeat(MAX_STORAGE_KEY_BYTES + 1)).is_err());
        assert!(validate_storage_key("gc:gc-1").is_ok());
    }

    #[test]
    fn narrowing_ledger_is_bounded_and_only_cleared_by_an_explicit_approval() {
        let mut prefs = SecurityPreferences::default();
        assert!(record_reach_narrowing(
            &mut prefs,
            "hub-person-a",
            "gc:gc-1"
        ));
        assert!(record_reach_narrowing(
            &mut prefs,
            "hub-person-a",
            "gc:gc-1"
        ));
        assert_eq!(
            prefs.reach_narrowed_scopes.get("hub-person-a"),
            Some(&BTreeSet::from(["gc:gc-1".to_owned()]))
        );
        clear_reach_narrowing(&mut prefs, "hub-person-a", "gc:gc-2");
        assert!(prefs.reach_narrowed_scopes.contains_key("hub-person-a"));
        clear_reach_narrowing(&mut prefs, "hub-person-a", "gc:gc-1");
        assert!(!prefs.reach_narrowed_scopes.contains_key("hub-person-a"));

        for index in 0..MAX_REACH_NARROWED_SCOPES_PER_PERSON {
            assert!(record_reach_narrowing(
                &mut prefs,
                "hub-person-b",
                &format!("gc:gc-{index}")
            ));
        }
        assert!(!record_reach_narrowing(
            &mut prefs,
            "hub-person-b",
            "gc:overflow"
        ));
    }

    #[test]
    fn full_space_burn_rejects_partial_channel_coverage() {
        let scope = Scope::server_full("space");
        assert!(burn_channels(&scope, vec!["one".to_owned()], false).is_err());
        assert_eq!(
            burn_channels(
                &scope,
                vec!["two".to_owned(), "one".to_owned(), "one".to_owned()],
                true,
            )
            .unwrap(),
            vec!["one".to_owned(), "two".to_owned()]
        );
    }

    #[test]
    fn enabling_friend_scope_requires_verified_stable_keys() {
        let mut metadata = PersonMetadata::default();
        assert!(ensure_friend_can_be_enabled(&metadata).is_err());
        metadata.safety_number_verified = true;
        assert!(ensure_friend_can_be_enabled(&metadata).is_ok());
        metadata.pending_ed25519_public = Some("changed-key".to_owned());
        assert!(ensure_friend_can_be_enabled(&metadata).is_err());
    }

    #[test]
    fn manual_peer_rejects_unverified_pending_and_missing_key_state() {
        let mut metadata = PersonMetadata {
            osl_user_id: "peer-osl".to_owned(),
            ..Default::default()
        };
        assert!(ensure_manual_peer_available(&metadata, true).is_err());
        metadata.safety_number_verified = true;
        assert!(ensure_manual_peer_available(&metadata, false).is_err());
        assert!(ensure_manual_peer_available(&metadata, true).is_ok());
        metadata.pending_key_bundle = Some(FriendCodeUnsigned {
            version: FRIEND_CODE_VERSION,
            osl_user_id: "peer-osl".to_owned(),
            x25519_public: String::new(),
            ed25519_public: String::new(),
            mlkem768_public: String::new(),
            ratchet_initial_public: None,
        });
        assert!(ensure_manual_peer_available(&metadata, true).is_err());
    }

    #[test]
    fn manual_approval_is_exact_to_app_and_friend_and_burn_is_terminal() {
        let discord_a =
            Scope::dm(manual_peer_scope_id("discord", "account-one", "hub-person-a").unwrap())
                .storage_key();
        let discord_a_other_account =
            Scope::dm(manual_peer_scope_id("discord", "account-two", "hub-person-a").unwrap())
                .storage_key();
        let instagram_a =
            Scope::dm(manual_peer_scope_id("instagram", "account-one", "hub-person-a").unwrap())
                .storage_key();
        let discord_b =
            Scope::dm(manual_peer_scope_id("discord", "account-one", "hub-person-b").unwrap())
                .storage_key();
        let mut prefs = SecurityPreferences::default();
        prefs.manual_approved_scopes.insert(discord_a.clone());
        assert!(manual_scope_preference_approved(&prefs, &discord_a));
        assert!(!manual_scope_preference_approved(&prefs, &instagram_a));
        assert!(!manual_scope_preference_approved(&prefs, &discord_b));
        assert!(!manual_scope_preference_approved(
            &prefs,
            &discord_a_other_account
        ));
        prefs.burned_manual_scopes.insert(discord_a.clone());
        assert!(!manual_scope_preference_approved(&prefs, &discord_a));
    }

    #[test]
    fn manual_burn_preserves_other_apps_friends_and_generic_peer_state() {
        let discord_a =
            Scope::dm(manual_peer_scope_id("discord", "account-one", "hub-person-a").unwrap())
                .storage_key();
        let instagram_a =
            Scope::dm(manual_peer_scope_id("instagram", "account-one", "hub-person-a").unwrap())
                .storage_key();
        let discord_b =
            Scope::dm(manual_peer_scope_id("discord", "account-one", "hub-person-b").unwrap())
                .storage_key();
        let discord_a_other_account =
            Scope::dm(manual_peer_scope_id("discord", "account-two", "hub-person-a").unwrap())
                .storage_key();
        let mut prefs = SecurityPreferences::default();
        prefs.manual_approved_scopes.extend([
            discord_a.clone(),
            instagram_a.clone(),
            discord_b.clone(),
            discord_a_other_account.clone(),
        ]);
        prefs.decrypt_display_by_scope.extend([
            (discord_a.clone(), true),
            (instagram_a.clone(), true),
            (discord_b.clone(), true),
            (discord_a_other_account.clone(), true),
        ]);
        let mut ttl = ipc::scope_ttl_file::ScopeTtlFile::default();
        ttl.entries.insert(discord_a.clone(), 3_600);
        ttl.entries.insert(instagram_a.clone(), 86_400);
        ttl.entries.insert(discord_b.clone(), 259_200);
        ttl.entries.insert(discord_a_other_account.clone(), 604_800);
        let mut blobs = ipc::scope_blobs_file::ScopeBlobsFile::default();
        ipc::scope_blobs_file::record_blob(
            &mut blobs,
            discord_a.clone(),
            "0011223344556677".to_owned(),
        );
        ipc::scope_blobs_file::record_blob(
            &mut blobs,
            instagram_a.clone(),
            "1122334455667788".to_owned(),
        );
        ipc::scope_blobs_file::record_blob(
            &mut blobs,
            discord_b.clone(),
            "2233445566778899".to_owned(),
        );
        ipc::scope_blobs_file::record_blob(
            &mut blobs,
            discord_a_other_account.clone(),
            "3344556677889900".to_owned(),
        );
        let generic_peer = PeerEntry {
            outgoing_whitelists: vec![WhitelistEntry::Dm {
                broadened: true,
                enabled_at: None,
            }],
            ..PeerEntry::default()
        };
        let generic_before = generic_peer.clone();
        let indexed_manual = crate::service_scope_index::IndexedServiceScope {
            storage_key: discord_a.clone(),
            scope: ScopeInput {
                kind: ScopeKind::Dm,
                id: discord_a.trim_start_matches("dm:").to_owned(),
                server_id: None,
                channel_id: Some("manual-dm-indexed-shared".to_owned()),
            },
            canonical_channel_ids: vec!["manual-dm-indexed-shared".to_owned()],
            local_context_binding_sha256: "a".repeat(64),
            manual_peer_person_id: Some("hub-person-a".to_owned()),
        };
        assert_eq!(
            indexed_manual.manual_peer_person_id.as_deref(),
            Some("hub-person-a")
        );
        assert_eq!(
            revoke_manual_scope_state(
                &mut prefs,
                &mut ttl,
                &mut blobs,
                &indexed_manual.storage_key,
            ),
            ["0011223344556677"]
        );
        assert!(!manual_scope_preference_approved(&prefs, &discord_a));
        assert!(manual_scope_preference_approved(&prefs, &instagram_a));
        assert!(manual_scope_preference_approved(&prefs, &discord_b));
        assert!(manual_scope_preference_approved(
            &prefs,
            &discord_a_other_account
        ));
        assert!(!ttl.entries.contains_key(&discord_a));
        assert_eq!(ttl.entries.get(&instagram_a), Some(&86_400));
        assert_eq!(ttl.entries.get(&discord_b), Some(&259_200));
        assert_eq!(ttl.entries.get(&discord_a_other_account), Some(&604_800));
        assert_eq!(ipc::scope_blobs_file::count_for(&blobs, &discord_a), 0);
        assert_eq!(ipc::scope_blobs_file::count_for(&blobs, &instagram_a), 1);
        assert_eq!(ipc::scope_blobs_file::count_for(&blobs, &discord_b), 1);
        assert_eq!(
            ipc::scope_blobs_file::count_for(&blobs, &discord_a_other_account),
            1
        );
        assert_eq!(generic_peer, generic_before);

        let source = include_str!("security.rs");
        let manual_burn = source
            .split("pub fn burn_manual_peer_scope")
            .nth(1)
            .unwrap()
            .split("fn revoke_manual_scope_state")
            .next()
            .unwrap();
        assert!(!manual_burn.contains("delete_messages_in_channel"));
        assert!(manual_burn.contains("let rows_destroyed = 0"));
    }

    #[test]
    fn peer_x25519_key_requires_exact_canonical_base64() {
        let canonical = STANDARD.encode([7u8; X25519_PUBLIC_BYTES]);
        let peer = PeerEntry {
            pubkey: Some(canonical.clone()),
            ..PeerEntry::default()
        };
        assert_eq!(strict_peer_x25519_public(&peer).unwrap(), [7u8; 32]);
        let noncanonical = PeerEntry {
            pubkey: Some(canonical.trim_end_matches('=').to_owned()),
            ..PeerEntry::default()
        };
        assert!(strict_peer_x25519_public(&noncanonical).is_err());
        let wrong_length = PeerEntry {
            pubkey: Some(STANDARD.encode([7u8; 31])),
            ..PeerEntry::default()
        };
        assert!(strict_peer_x25519_public(&wrong_length).is_err());
        let mlkem = PeerEntry {
            ik_mlkem768_pub: Some(STANDARD.encode([9u8; MLKEM768_PUBLIC_BYTES])),
            ..PeerEntry::default()
        };
        assert_eq!(
            strict_peer_mlkem768_public(&mlkem).unwrap(),
            [9u8; MLKEM768_PUBLIC_BYTES]
        );
    }

    #[test]
    fn hub_ttl_accepts_only_the_four_presented_lifetimes() {
        for accepted in [3_600, 86_400, 259_200, 604_800] {
            assert!(validate_hub_ttl(accepted).is_ok());
        }
        for rejected in [0, 3_599, 3_601, 604_799, 604_801] {
            assert!(validate_hub_ttl(rejected).is_err());
        }
    }

    #[test]
    fn repeated_ttl_saves_use_encrypted_recoverable_replacement() {
        let file_key = [0x5d; 32];
        let path = std::env::temp_dir().join(format!(
            "osl-scope-ttl-replacement-{}-{}.json",
            std::process::id(),
            ipc::main_password::now_unix_secs_pub()
        ));
        let mut ttl = ipc::scope_ttl_file::ScopeTtlFile::default();
        ipc::scope_ttl_file::set_scope_ttl(&mut ttl, "dm:test".to_owned(), 3_600);
        write_encrypted_json_with_key(&path, &ttl, &file_key).unwrap();
        ipc::scope_ttl_file::set_scope_ttl(&mut ttl, "dm:test".to_owned(), 86_400);
        write_encrypted_json_with_key(&path, &ttl, &file_key).unwrap();
        let on_disk = std::fs::read(&path).unwrap();
        assert!(ipc::main_password::has_enc_magic(&on_disk));
        let loaded: ipc::scope_ttl_file::ScopeTtlFile =
            load_encrypted_json_with_key(&path, &file_key).unwrap();
        assert_eq!(
            ipc::scope_ttl_file::get_scope_ttl(&loaded, "dm:test"),
            86_400
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn repeated_peer_prose_records_and_burns_use_windows_safe_encrypted_replacement() {
        let file_key = [0x6c; 32];
        let path = std::env::temp_dir().join(format!(
            "osl-peer-prose-blobs-{}-{}.json",
            std::process::id(),
            ipc::main_password::now_unix_secs_pub()
        ));
        let security = HubSecurityState::default();
        let scope = Scope {
            kind: ScopeKind::Dm,
            id: "hub-person-peer".to_owned(),
            server_id: None,
            channel_id: Some("manual-dm-shared".to_owned()),
        };
        record_peer_prose_blob_at_path(
            &security,
            &path,
            scope.clone(),
            "0011223344556677".to_owned(),
            &file_key,
        )
        .unwrap();
        record_peer_prose_blob_at_path(
            &security,
            &path,
            scope.clone(),
            "8899aabbccddeeff".to_owned(),
            &file_key,
        )
        .unwrap();
        let on_disk = std::fs::read(&path).unwrap();
        assert!(ipc::main_password::has_enc_magic(&on_disk));
        let ledger = load_scope_blobs_strict_with_key(&path, &file_key).unwrap();
        assert_eq!(
            ipc::scope_blobs_file::count_for(&ledger, &scope.storage_key()),
            2
        );
        let mut burned = ledger;
        assert_eq!(
            ipc::scope_blobs_file::take_blobs(&mut burned, &scope.storage_key()).len(),
            2
        );
        write_scope_blobs_with_key(&path, &burned, &file_key).unwrap();
        let cleared = load_scope_blobs_strict_with_key(&path, &file_key).unwrap();
        assert_eq!(
            ipc::scope_blobs_file::count_for(&cleared, &scope.storage_key()),
            0
        );
        let mut retried = cleared;
        ipc::scope_blobs_file::record_blob(
            &mut retried,
            scope.storage_key(),
            "fedcba9876543210".to_owned(),
        );
        write_scope_blobs_with_key(&path, &retried, &file_key).unwrap();
        let final_ledger = load_scope_blobs_strict_with_key(&path, &file_key).unwrap();
        assert_eq!(
            ipc::scope_blobs_file::count_for(&final_ledger, &scope.storage_key()),
            1
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn peer_replay_ledger_is_encrypted_scope_bound_pruned_and_burnable() {
        let file_key = [0x73; 32];
        let path = std::env::temp_dir().join(format!(
            "osl-peer-replay-{}-{}.json",
            std::process::id(),
            ipc::main_password::now_unix_secs_pub()
        ));
        let security = HubSecurityState::default();
        let first_id = "peer-00112233445566778899aabbccddeeff";
        let second_id = "peer-ffeeddccbbaa99887766554433221100";
        let now = 1_700_000_000;

        consume_peer_message_at_path(
            &security,
            &path,
            "dm:discord-a",
            first_id,
            now + 60,
            now,
            &file_key,
        )
        .unwrap();
        assert!(ipc::main_password::has_enc_magic(
            &std::fs::read(&path).unwrap()
        ));
        assert_eq!(
            consume_peer_message_at_path(
                &security,
                &path,
                "dm:discord-a",
                first_id,
                now + 60,
                now,
                &file_key,
            )
            .unwrap_err(),
            PEER_OPEN_ERROR
        );
        consume_peer_message_at_path(
            &security,
            &path,
            "dm:discord-b",
            first_id,
            now + 180,
            now,
            &file_key,
        )
        .unwrap();

        // Once the original record expires, pruning permits the same random
        // identifier again without retaining stale state forever.
        consume_peer_message_at_path(
            &security,
            &path,
            "dm:discord-a",
            first_id,
            now + 120,
            now + 61,
            &file_key,
        )
        .unwrap();
        consume_peer_message_at_path(
            &security,
            &path,
            "dm:discord-a",
            second_id,
            now + 120,
            now + 61,
            &file_key,
        )
        .unwrap();
        remove_peer_replay_scope_at_path(&path, "dm:discord-a", &file_key).unwrap();
        let ledger: PeerReplayLedger = load_encrypted_json_with_key(&path, &file_key).unwrap();
        assert!(!ledger.consumed_by_scope.contains_key("dm:discord-a"));
        assert!(ledger.consumed_by_scope.contains_key("dm:discord-b"));

        let mut bounded = PeerReplayLedger {
            version: 1,
            ..PeerReplayLedger::default()
        };
        let scope_entries = bounded
            .consumed_by_scope
            .entry("dm:bounded".to_owned())
            .or_default();
        for index in 0..MAX_PEER_REPLAY_ENTRIES_PER_SCOPE {
            scope_entries.insert(format!("peer-{index:032x}"), now + 300);
        }
        write_encrypted_json_with_key(&path, &bounded, &file_key).unwrap();
        assert_eq!(
            consume_peer_message_at_path(
                &security,
                &path,
                "dm:bounded",
                "peer-ffffffffffffffffffffffffffffffff",
                now + 300,
                now,
                &file_key,
            )
            .unwrap_err(),
            PEER_OPEN_ERROR
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("json.bak"));
    }

    #[test]
    fn attachment_burn_ledger_is_encrypted_bounded_expiring_and_retryable() {
        let file_key = [0x42; 32];
        let path = std::env::temp_dir().join(format!(
            "osl-attachment-burn-{}-{}.json",
            std::process::id(),
            ipc::main_password::now_unix_secs_pub()
        ));
        let security = HubSecurityState::default();
        let now = 1_700_000_000;
        let token_a = "00112233445566778899aabbccddeeff";
        let token_b = "ffeeddccbbaa99887766554433221100";
        record_peer_attachment_burn_capability_at_path(
            &security,
            &path,
            "dm:a".to_owned(),
            "00112233445566778899aabbccddeeff".to_owned(),
            token_a,
            now + 60,
            now,
            &file_key,
        )
        .unwrap();
        record_peer_attachment_burn_capability_at_path(
            &security,
            &path,
            "dm:b".to_owned(),
            "ffeeddccbbaa99887766554433221100".to_owned(),
            token_b,
            now + 120,
            now,
            &file_key,
        )
        .unwrap();
        let encrypted = std::fs::read(&path).unwrap();
        assert!(ipc::main_password::has_enc_magic(&encrypted));
        assert!(!encrypted
            .windows(token_a.len())
            .any(|window| window == token_a.as_bytes()));

        let mut ledger = load_attachment_burn_ledger_with_key(&path, &file_key).unwrap();
        prune_expired_attachment_burn_entries(&mut ledger, now + 61);
        assert!(!ledger.entries_by_scope.contains_key("dm:a"));
        let failed = take_attachment_burn_entries(&mut ledger, "dm:b");
        assert_eq!(failed.len(), 1);
        ledger.entries_by_scope.insert("dm:b".to_owned(), failed);
        write_encrypted_json_with_key(&path, &ledger, &file_key).unwrap();
        let retry = load_attachment_burn_ledger_with_key(&path, &file_key).unwrap();
        assert_eq!(retry.entries_by_scope["dm:b"].len(), 1);

        assert!(validate_attachment_burn_entry(
            "00112233445566778899AABBCCDDEEFF",
            token_a,
            now + 60,
            now
        )
        .is_err());

        let mut oversized = AttachmentBurnLedger {
            version: 1,
            ..AttachmentBurnLedger::default()
        };
        oversized.entries_by_scope.insert(
            "dm:oversized".to_owned(),
            (0..=MAX_ATTACHMENT_BURN_ENTRIES_PER_SCOPE)
                .map(|index| AttachmentBurnEntry {
                    object_id: format!("{index:032x}"),
                    fetch_token: token_a.to_owned(),
                    expires_at: now + 120,
                })
                .collect(),
        );
        write_encrypted_json_with_key(&path, &oversized, &file_key).unwrap();
        assert!(load_attachment_burn_ledger_with_key(&path, &file_key).is_err());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("json.bak"));
    }

    #[test]
    fn removing_last_auto_approved_friend_can_disable_scope_without_erasing_manual_choice() {
        let mut scopes = ipc::whitelist_state::WhitelistState::new();
        scopes.insert(
            "dm:friend".to_owned(),
            ipc::whitelist_state::ScopeState {
                encrypt_toggle: true,
                auto_enabled: true,
                ..Default::default()
            },
        );
        revoke_auto_scope_if_uncovered(&mut scopes, "dm:friend", false);
        assert!(!scopes["dm:friend"].encrypt_toggle);
        assert!(!scopes["dm:friend"].auto_enabled);

        scopes.insert(
            "dm:manual".to_owned(),
            ipc::whitelist_state::ScopeState {
                encrypt_toggle: true,
                auto_enabled: false,
                ..Default::default()
            },
        );
        revoke_auto_scope_if_uncovered(&mut scopes, "dm:manual", false);
        assert!(scopes["dm:manual"].encrypt_toggle);
    }

    // ---- Bilateral burn ----

    fn burn_peer(osl_user_id: &str, key_byte: u8, scope: &Scope) -> PeerEntry {
        PeerEntry {
            osl_user_id: Some(osl_user_id.to_owned()),
            pubkey: Some(STANDARD.encode([key_byte; X25519_PUBLIC_BYTES])),
            outgoing_whitelists: vec![whitelist_entry(scope, false)],
            ..PeerEntry::default()
        }
    }

    #[test]
    fn burn_notice_recipients_are_resolved_from_approvals_not_guessed() {
        let scope = Scope::gc("conversation-1");
        let other = Scope::gc("conversation-2");
        let mut peers = ipc::peer_map::PeerMap::new();
        peers.insert("person-b".to_owned(), burn_peer("osl-b", 2, &scope));
        peers.insert("person-a".to_owned(), burn_peer("osl-a", 1, &scope));
        // Approved for a different conversation: not notified.
        peers.insert("person-c".to_owned(), burn_peer("osl-c", 3, &other));
        // Approved here but no usable key: skipped rather than guessed at.
        peers.insert(
            "person-d".to_owned(),
            PeerEntry {
                osl_user_id: Some("osl-d".to_owned()),
                pubkey: None,
                outgoing_whitelists: vec![whitelist_entry(&scope, false)],
                ..PeerEntry::default()
            },
        );
        // Approved here but no OSL identifier: nothing to address.
        peers.insert(
            "person-e".to_owned(),
            PeerEntry {
                osl_user_id: None,
                pubkey: Some(STANDARD.encode([5u8; X25519_PUBLIC_BYTES])),
                outgoing_whitelists: vec![whitelist_entry(&scope, false)],
                ..PeerEntry::default()
            },
        );

        let resolved = revocation_peers_from_map(&peers, &scope, None);
        assert_eq!(
            resolved
                .peers
                .iter()
                .map(|(id, _)| id.as_str())
                .collect::<Vec<_>>(),
            vec!["osl-a", "osl-b"]
        );
        assert_eq!(resolved.peers[0].1, [1u8; X25519_PUBLIC_BYTES]);
        // person-d and person-e were approved here and cannot be addressed. The
        // conversation is therefore NOT fully notified, and person-c — approved
        // somewhere else entirely — is not counted as a miss.
        assert_eq!(resolved.skipped, 2);
        assert_eq!(resolved.expected(), 4);
        assert!(!resolved.fully_addressed());
    }

    #[test]
    fn a_malformed_peer_key_never_yields_a_placeholder_recipient() {
        let scope = Scope::dm("peer-1");
        let mut peers = ipc::peer_map::PeerMap::new();
        peers.insert(
            "person-a".to_owned(),
            PeerEntry {
                osl_user_id: Some("osl-a".to_owned()),
                // Valid base64, wrong length.
                pubkey: Some(STANDARD.encode([7u8; 16])),
                outgoing_whitelists: vec![whitelist_entry(&scope, false)],
                ..PeerEntry::default()
            },
        );
        let resolved = revocation_peers_from_map(&peers, &scope, Some("person-a"));
        assert!(resolved.peers.is_empty());
        assert_eq!(resolved.skipped, 1);
        assert!(!resolved.fully_addressed());
    }

    /// The three claims are distinct, ordered strongest-first, and none of them
    /// says a platform copy was removed. `osl-gui-final-plan.md:494` forbids
    /// ever rendering a burn as "Deleted".
    #[test]
    fn burn_claims_are_three_separate_honest_statements() {
        let claims = burn_claims();
        assert_eq!(claims.len(), 3);
        assert_eq!(claims[0], ipc::revocation::CLAIM_CONTENT_EXPIRY);
        assert_eq!(claims[1], ipc::revocation::CLAIM_LOCAL_REMOVAL);
        assert_eq!(claims[2], ipc::revocation::CLAIM_RECIPIENT_COPIES);
        assert!(claims[2].contains("no guarantee"));
        for claim in &claims {
            let lowered = claim.to_lowercase();
            assert!(!lowered.contains("discord"));
            assert!(!lowered.contains("their copies go dark"));
        }
    }

    /// A burn floor refusal must be indistinguishable from any other failure to
    /// open, or the UI becomes a "was that message burned?" oracle.
    #[test]
    fn a_burn_refusal_reads_exactly_like_any_other_open_failure() {
        assert_eq!(REVOCATION_REFUSED_ERROR, PEER_OPEN_ERROR);
        assert!(!REVOCATION_REFUSED_ERROR.to_lowercase().contains("burn"));
    }

    #[test]
    fn burn_identifiers_render_as_canonical_lower_hex() {
        let rendered = lower_hex(&[0x0fu8; 32]);
        assert_eq!(rendered.len(), 64);
        assert!(canonical_lower_hex(&rendered, 64));
        assert_eq!(lower_hex(&[0u8; 32]), "0".repeat(64));
        assert_eq!(lower_hex(&[0xffu8; 32]), "f".repeat(64));
    }

    /// The ledger loader is strict and fails closed. Every gate treats an `Err`
    /// as "already burned", so a ledger that has been truncated, re-keyed or
    /// hand-edited must produce an error rather than an empty ledger.
    #[test]
    fn a_malformed_burn_ledger_is_an_error_not_an_empty_one() {
        use ipc::revocation::{JournalEntry, RevocationLedger, ScopeRevocationState};
        let dir = std::env::temp_dir().join(format!(
            "osl-burn-ledger-{}-{}",
            std::process::id(),
            ipc::main_password::now_unix_secs_pub()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("ledger.json");
        let key = [9u8; 32];

        // A version from the future is refused rather than partially trusted.
        let mut future = RevocationLedger::default();
        future.version = 7;
        write_encrypted_json_with_key(&path, &future, &key).unwrap();
        assert!(load_revocation_ledger(&path, &key).is_err());

        // A slot key that is not a 32-byte commitment is refused: it cannot have
        // been written by this code, so the file is not ours to trust.
        let mut bogus = RevocationLedger::default();
        bogus.version = 1;
        bogus.scopes.insert(
            "dm:plaintext-scope-name".to_owned(),
            ScopeRevocationState::default(),
        );
        write_encrypted_json_with_key(&path, &bogus, &key).unwrap();
        assert!(load_revocation_ledger(&path, &key).is_err());

        // Same for a journal key.
        let mut bad_journal = RevocationLedger::default();
        bad_journal.version = 1;
        bad_journal.scopes.insert(
            lower_hex(&[1u8; 32]),
            ScopeRevocationState {
                high_water: 3,
                last_burn_epoch: 1,
                burn_floor: 3,
                journal: BTreeMap::from([("not-a-burn-id".to_owned(), JournalEntry::default())]),
            },
        );
        write_encrypted_json_with_key(&path, &bad_journal, &key).unwrap();
        assert!(load_revocation_ledger(&path, &key).is_err());

        // A plaintext (unencrypted) file is refused, not read.
        std::fs::write(&path, b"{\"version\":1,\"scopes\":{}}").unwrap();
        assert!(load_revocation_ledger(&path, &key).is_err());

        // The wrong key is refused, not treated as a fresh ledger.
        let mut good = RevocationLedger::default();
        good.version = 1;
        good.scopes
            .insert(lower_hex(&[2u8; 32]), ScopeRevocationState::default());
        write_encrypted_json_with_key(&path, &good, &key).unwrap();
        assert!(load_revocation_ledger(&path, &[8u8; 32]).is_err());
        // And the round trip works with the right key.
        assert_eq!(load_revocation_ledger(&path, &key).unwrap(), good);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_malformed_burn_queue_or_counter_file_is_an_error() {
        let dir = std::env::temp_dir().join(format!(
            "osl-burn-queue-{}-{}",
            std::process::id(),
            ipc::main_password::now_unix_secs_pub()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let key = [4u8; 32];

        let outbox_path = dir.join("outbox.json");
        let mut outbox = ipc::revocation::RevocationOutbox::default();
        outbox.version = 9;
        write_encrypted_json_with_key(&outbox_path, &outbox, &key).unwrap();
        assert!(load_revocation_outbox(&outbox_path, &key).is_err());

        let counters_path = dir.join("counters.json");
        let mut counters = ipc::revocation::SendCounters::default();
        counters.version = 9;
        write_encrypted_json_with_key(&counters_path, &counters, &key).unwrap();
        assert!(load_revocation_counters(&counters_path, &key).is_err());

        // Absent files are legitimately empty; the strictness is about content.
        assert_eq!(
            load_revocation_outbox(&dir.join("missing.json"), &key).unwrap(),
            ipc::revocation::RevocationOutbox::default()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
