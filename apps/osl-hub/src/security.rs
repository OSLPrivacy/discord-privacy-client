//! Trusted-local People, friend-code, and scope-security backend for the app.
//!
//! The platform webviews do not receive this API. Friend codes contain public
//! identity material only and are signed by the exporting identity. A valid
//! signature proves that the code is internally authentic; the separate
//! `safety_number_verified` bit records the user's out-of-band confirmation.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Mutex;

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use ipc::peer_map::{PeerEntry, WhitelistEntry};
use ipc::scope::{Scope, ScopeInput, ScopeKind};
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
const MAX_ATTACHMENT_BURN_ENTRIES_PER_SCOPE: usize = 256;
const MAX_ATTACHMENT_BURN_ENTRIES_TOTAL: usize = 2_048;
const MAX_PEER_REPLAY_SCOPES: usize = 512;
const MAX_PEER_REPLAY_ENTRIES_PER_SCOPE: usize = 4_096;
const MAX_PEER_REPLAY_ENTRIES_TOTAL: usize = 32_768;
const PEER_OPEN_ERROR: &str = "This encrypted message could not be opened";
const MAX_ALIAS_BYTES: usize = 80;
const MAX_ALIAS_CHARS: usize = 48;
const MAX_VISIBLE_WHITELIST_SCOPES: usize = 512;
const X25519_PUBLIC_BYTES: usize = 32;
const ED25519_PUBLIC_BYTES: usize = 32;
const ED25519_SIGNATURE_BYTES: usize = 64;
const MLKEM768_PUBLIC_BYTES: usize = 1184;
const RATCHET_PUBLIC_BYTES: usize = 32;

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
}

/// A local-only description of one approved encryption scope. It deliberately
/// contains no service or account handle: current friend codes do not prove
/// either relationship, so OSL must not infer one from a conversation id.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonWhitelistScopeDto {
    pub kind: String,
    pub context_id: Option<String>,
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
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ManualPeerBinding {
    pub person_id: String,
    pub peer_osl_user_id: String,
    pub peer_x25519_public: [u8; X25519_PUBLIC_BYTES],
    pub peer_mlkem768_public: [u8; MLKEM768_PUBLIC_BYTES],
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
}

#[derive(Clone, Serialize, Deserialize)]
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

#[derive(Default, Serialize, Deserialize)]
struct SecurityPreferences {
    version: u32,
    #[serde(default)]
    decrypt_display_by_scope: BTreeMap<String, bool>,
    #[serde(default)]
    manual_approved_scopes: BTreeSet<String>,
    #[serde(default)]
    burned_manual_scopes: BTreeSet<String>,
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
    let encoded = serde_json::to_vec(&signed)
        .map_err(|_| "OSL friend code could not be encoded".to_owned())?;
    Ok(FriendCodeExport {
        friend_code: format!("{FRIEND_CODE_PREFIX}{}", URL_SAFE_NO_PAD.encode(encoded)),
        osl_user_id: identity.user_id,
        safety_number: ipc::tofu::safety_number(&signed.payload.ed25519_public),
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
        let existing = people
            .people
            .get_mut(&existing_id)
            .ok_or_else(|| "OSL friend is unknown".to_owned())?;
        if existing.ed25519_public != parsed.payload.ed25519_public {
            existing.pending_ed25519_public = Some(parsed.payload.ed25519_public.clone());
            existing.pending_key_bundle = Some(parsed.payload.clone());
            existing.safety_number_verified = false;
            write_encrypted_json(&dir.join(PEOPLE_FILE), &people)?;
            return Ok(AddFriendResult {
                disposition: AddFriendDisposition::KeyChangeRequiresVerification,
                person_id: existing_id,
                osl_user_id: parsed.payload.osl_user_id.clone(),
                safety_number: ipc::tofu::safety_number(&parsed.payload.ed25519_public),
                code_signature_valid: true,
                safety_number_verified: false,
            });
        }
        let safety_number_verified = existing.safety_number_verified;
        if alias.is_some() {
            existing.alias = alias;
            write_encrypted_json(&dir.join(PEOPLE_FILE), &people)?;
        }
        return Ok(AddFriendResult {
            disposition: AddFriendDisposition::AlreadyPresent,
            person_id: existing_id,
            osl_user_id: parsed.payload.osl_user_id.clone(),
            safety_number: ipc::tofu::safety_number(&parsed.payload.ed25519_public),
            code_signature_valid: true,
            safety_number_verified,
        });
    }

    let person_id = person_id(&parsed.payload.ed25519_public);
    let peer = peer_entry(&parsed.payload)?;
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
        safety_number: ipc::tofu::safety_number(&parsed.payload.ed25519_public),
        code_signature_valid: true,
        safety_number_verified: false,
    })
}

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
        .get_mut(&person_id)
        .ok_or_else(|| "OSL friend is unknown".to_owned())?;
    let expected_key = metadata
        .pending_key_bundle
        .as_ref()
        .map(|payload| payload.ed25519_public.as_str())
        .or(metadata.pending_ed25519_public.as_deref())
        .unwrap_or(&metadata.ed25519_public);
    let expected = ipc::tofu::safety_number(expected_key);
    if normalise_safety_number(&safety_number) != normalise_safety_number(&expected) {
        return Err("OSL safety number does not match".to_owned());
    }
    if metadata.pending_key_bundle.is_none() && metadata.pending_ed25519_public.is_some() {
        return Err(
            "Re-import this friend's current signed code before accepting the key change"
                .to_owned(),
        );
    }
    let pending = metadata.pending_key_bundle.clone();
    if let Some(payload) = pending.as_ref() {
        let previous_peers = core
            .osl
            .peer_map
            .lock()
            .map_err(|_| "OSL peer state is unavailable".to_owned())?
            .clone();
        let mut peers = previous_peers.clone();
        let previous = peers
            .get(&person_id)
            .cloned()
            .ok_or_else(|| "OSL friend key state is missing".to_owned())?;
        let mut replacement = peer_entry(payload)?;
        replacement.outgoing_whitelists = previous.outgoing_whitelists;
        replacement.burned_scopes = previous.burned_scopes;
        replacement.discord_id = previous.discord_id;
        replacement.is_self = previous.is_self;
        replacement.first_seen = previous.first_seen;
        peers.insert(person_id.clone(), replacement);
        write_encrypted_json(&dir.join("peer_map.json"), &peers)
            .map_err(|_| "OSL changed friend keys could not be persisted".to_owned())?;
        metadata.ed25519_public = payload.ed25519_public.clone();
        metadata.pending_ed25519_public = None;
        metadata.pending_key_bundle = None;
        metadata.safety_number_verified = true;
        if let Err(error) = write_encrypted_json(&dir.join(PEOPLE_FILE), &people) {
            let _ = write_encrypted_json(&dir.join("peer_map.json"), &previous_peers);
            return Err(error);
        }
        *core
            .osl
            .peer_map
            .lock()
            .map_err(|_| "OSL peer state is unavailable".to_owned())? = peers;
        return person_dto(core, &person_id, &people.people[&person_id]);
    }
    metadata.safety_number_verified = true;
    let metadata = metadata.clone();
    write_encrypted_json(&dir.join(PEOPLE_FILE), &people)?;
    person_dto(core, &person_id, &metadata)
}

pub fn list_people(core: &HubCoreState) -> Result<Vec<PersonDto>, String> {
    require_unlocked()?;
    let people = load_encrypted_json::<PeopleFile>(&config_dir()?.join(PEOPLE_FILE))?;
    people
        .people
        .iter()
        .map(|(person_id, metadata)| person_dto(core, person_id, metadata))
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
    person_dto(core, &person_id, &updated)
}

pub fn set_friend_scope_permission(
    core: &HubCoreState,
    security: &HubSecurityState,
    person_id: String,
    scope_input: ScopeInput,
    enabled: bool,
    broadened: bool,
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
    peer.outgoing_whitelists
        .retain(|entry| !whitelist_matches(entry, &scope));
    if enabled {
        peer.outgoing_whitelists
            .push(whitelist_entry(&scope, broadened));
    }
    let mut whitelist_state = core
        .osl
        .whitelist_state
        .lock()
        .map_err(|_| "OSL whitelist state is unavailable".to_owned())?
        .clone();
    if enabled {
        let scope_state = whitelist_state.entry(scope.storage_key()).or_default();
        scope_state.encrypt_toggle = true;
        scope_state.auto_enabled = true;
    } else {
        let another_approved_friend = peers.values().any(|candidate| {
            candidate
                .outgoing_whitelists
                .iter()
                .any(|entry| whitelist_matches(entry, &scope))
        });
        revoke_auto_scope_if_uncovered(
            &mut whitelist_state,
            &scope.storage_key(),
            another_approved_friend,
        );
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
        server_defaults,
    };
    write_encrypted_json(&dir.join("peer_map.json"), &peers)
        .map_err(|_| "OSL whitelist could not be persisted".to_owned())?;
    // The legacy IPC convenience writer drops server_defaults and its raw
    // rename cannot replace an existing destination on Windows. OSL Privacy writes
    // the complete envelope through its authenticated, recoverable path.
    if write_encrypted_json(&dir.join("whitelist_state.json"), &whitelist_document).is_err() {
        let _ = write_encrypted_json(&dir.join("peer_map.json"), &previous_peers);
        return Err("OSL whitelist could not be persisted".to_owned());
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
    if peer.osl_user_id.as_deref() != Some(metadata.osl_user_id.as_str()) {
        return Err("OSL friend identity state does not match".to_owned());
    }
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
    let scope: Scope = scope_input
        .try_into()
        .map_err(|_| "OSL manual peer scope is invalid".to_owned())?;
    if scope.kind != ScopeKind::Dm
        || scope.id != manual_peer_scope_id(service_id, account_id, &binding.person_id)?
        || scope.channel_id.as_deref().is_none_or(str::is_empty)
    {
        return Err("OSL manual peer scope is invalid".to_owned());
    }
    let dir = config_dir()?;
    let prefs = load_encrypted_json::<SecurityPreferences>(&dir.join(SECURITY_PREFS_FILE))?;
    let storage_key = scope.storage_key();
    Ok(manual_scope_preference_approved(&prefs, &storage_key))
}

fn manual_scope_preference_approved(prefs: &SecurityPreferences, storage_key: &str) -> bool {
    prefs.manual_approved_scopes.contains(storage_key)
        && !prefs.burned_manual_scopes.contains(storage_key)
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
    if service_id != "osl-chat" || account_id != "osl-main" {
        crate::service_host::service_manifest(service_id).map_err(|error| error.to_string())?;
    }
    crate::service_host::validate_opaque_id(account_id).map_err(|error| error.to_string())?;
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
    let scope: Scope = scope_input
        .try_into()
        .map_err(|_| "OSL manual peer scope is invalid".to_owned())?;
    if scope.kind != ScopeKind::Dm
        || scope.id != manual_peer_scope_id(service_id, account_id, &binding.person_id)?
        || scope.channel_id.as_deref().is_none_or(str::is_empty)
    {
        return Err("OSL manual peer scope is invalid".to_owned());
    }
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL manual peer settings are unavailable".to_owned())?;
    let dir = config_dir()?;
    let path = dir.join(SECURITY_PREFS_FILE);
    let mut prefs = load_encrypted_json::<SecurityPreferences>(&path)?;
    let storage_key = scope.storage_key();
    if enabled && prefs.burned_manual_scopes.contains(&storage_key) {
        return Err("This manual conversation was burned and cannot be reapproved".to_owned());
    }
    prefs.version = 2;
    if enabled {
        prefs.manual_approved_scopes.insert(storage_key);
    } else {
        prefs.manual_approved_scopes.remove(&storage_key);
    }
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
    let _transition = security
        .transition
        .lock()
        .map_err(|_| "OSL security state is unavailable".to_owned())?;
    let dir = config_dir()?;
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
    for peer in peers.values_mut() {
        let before = peer.outgoing_whitelists.len();
        peer.outgoing_whitelists
            .retain(|entry| !whitelist_matches(entry, &scope));
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
    })
}

/// Burn only one Hub-owned manual app+friend scope. This deliberately avoids
/// the generic DM peer-map and burned-scope machinery, whose DM keys are
/// friend-global and cannot represent an app-specific manual conversation.
pub fn burn_manual_peer_scope(
    _core: &HubCoreState,
    security: &HubSecurityState,
    service_id: &str,
    account_id: &str,
    person_id: &str,
    scope_input: ScopeInput,
) -> Result<HubScopeBurnResult, String> {
    require_unlocked()?;
    let scope: Scope = scope_input
        .clone()
        .try_into()
        .map_err(|_| "OSL manual peer scope is invalid".to_owned())?;
    if scope.kind != ScopeKind::Dm
        || scope.id != manual_peer_scope_id(service_id, account_id, person_id)?
        || scope.channel_id.as_deref().is_none_or(str::is_empty)
    {
        return Err("OSL manual peer scope is invalid".to_owned());
    }
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
    prefs.burned_manual_scopes.insert(storage_key.to_owned());
    prefs
        .decrypt_display_by_scope
        .insert(storage_key.to_owned(), false);
    ttl.entries.remove(storage_key);
    ipc::scope_blobs_file::take_blobs(blobs, storage_key)
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
    Ok(PeerEntry {
        osl_user_id: Some(payload.osl_user_id.clone()),
        pubkey: Some(standard_base64(&payload.x25519_public)?),
        ik_mlkem768_pub: Some(standard_base64(&payload.mlkem768_public)?),
        ik_ratchet_initial_pub: payload
            .ratchet_initial_public
            .as_deref()
            .map(standard_base64)
            .transpose()?,
        tofu_ed25519_pub: Some(standard_base64(&payload.ed25519_public)?),
        first_seen: Some(ipc::main_password::now_unix_secs_pub().to_string()),
        ..PeerEntry::default()
    })
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

fn person_dto(
    core: &HubCoreState,
    person_id: &str,
    metadata: &PersonMetadata,
) -> Result<PersonDto, String> {
    let peer_map = core
        .osl
        .peer_map
        .lock()
        .map_err(|_| "OSL peer state is unavailable".to_owned())?;
    let whitelists = peer_map
        .get(person_id)
        .map(|entry| entry.outgoing_whitelists.as_slice())
        .unwrap_or(&[]);
    let whitelist_count = whitelists.len();
    let whitelisted_scopes = whitelists
        .iter()
        .take(MAX_VISIBLE_WHITELIST_SCOPES)
        .map(whitelist_scope_dto)
        .collect();
    Ok(PersonDto {
        person_id: person_id.to_owned(),
        osl_user_id: metadata.osl_user_id.clone(),
        alias: metadata.alias.clone(),
        safety_number: ipc::tofu::safety_number(
            metadata
                .pending_key_bundle
                .as_ref()
                .map(|payload| payload.ed25519_public.as_str())
                .or(metadata.pending_ed25519_public.as_deref())
                .unwrap_or(&metadata.ed25519_public),
        ),
        safety_number_verified: metadata.safety_number_verified,
        whitelist_count,
        whitelisted_scopes,
        whitelisted_scopes_truncated: whitelist_count > MAX_VISIBLE_WHITELIST_SCOPES,
        pending_key_change: metadata.pending_ed25519_public.is_some()
            || metadata.pending_key_bundle.is_some(),
    })
}

fn whitelist_scope_dto(entry: &WhitelistEntry) -> PersonWhitelistScopeDto {
    match entry {
        WhitelistEntry::Dm { .. } => PersonWhitelistScopeDto {
            kind: "dm".to_owned(),
            context_id: None,
        },
        WhitelistEntry::Gc { id, .. } => PersonWhitelistScopeDto {
            kind: "group".to_owned(),
            context_id: bounded_context_id(id),
        },
        WhitelistEntry::ServerChannel {
            server_id,
            channel_id,
            ..
        } => PersonWhitelistScopeDto {
            kind: "channel".to_owned(),
            context_id: bounded_context_id(&format!("{server_id}:{channel_id}")),
        },
        WhitelistEntry::ServerFull { server_id, .. } => PersonWhitelistScopeDto {
            kind: "space".to_owned(),
            context_id: bounded_context_id(server_id),
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

fn whitelist_matches(entry: &WhitelistEntry, scope: &Scope) -> bool {
    match (entry, scope.kind) {
        (WhitelistEntry::Dm { .. }, ScopeKind::Dm) => true,
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
    fn scope_match_is_service_neutral() {
        let scope = Scope::server_channel("space", "conversation");
        let entry = whitelist_entry(&scope, false);
        assert!(whitelist_matches(&entry, &scope));
        assert!(!whitelist_matches(&entry, &Scope::gc("conversation")));
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
                context_id: None
            }
        );
        assert_eq!(
            whitelist_scope_dto(&WhitelistEntry::Gc {
                id: "gc-1".to_owned(),
                user_specific: true
            }),
            PersonWhitelistScopeDto {
                kind: "group".to_owned(),
                context_id: Some("gc-1".to_owned())
            }
        );
        assert_eq!(bounded_context_id(&"x".repeat(513)), None);
        assert_eq!(bounded_context_id("unsafe\ncontext"), None);
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
}
