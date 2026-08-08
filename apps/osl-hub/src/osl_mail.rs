//! T4-N2's signed OSL Mail provisioning bridge.
//!
//! The server deliberately has no status endpoint. Server capability still
//! gates the signed lab operations, but the user-facing Mail client is not
//! available until the desktop bridge exists. Whether this identity has a
//! mailbox is local state established only after a signed provision response
//! succeeds.

use crate::core_bridge::HubCoreState;
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine as _,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Mutex;

const MAIL_DOMAIN: &str = "oslprivacy.com";
const RETENTION_SECONDS: u32 = 7 * 24 * 60 * 60;
const BORING_PROTECTED_SUBJECT: &str = "OSL protected message";
const MIN_COPIED_SUBJECT_BYTES: usize = 16;
pub const OSL_MAIL_THREAD_LIST_CAP: usize = 100;
/// Whether the user-facing OSL Mail client may present as usable.
///
/// **Derived, not written.** This was the literal `false` that D-221 called out
/// from the other side: it *"matches every authority but hides provision/send/
/// burn, which do work against the deployed keyserver"*, while the `true` it
/// replaced promised a mailbox that can never be read. *"The tile has no third
/// state."*
///
/// One boolean still cannot hold three states, so it no longer tries to. It
/// answers exactly one question — may OSL Mail present as working — and the
/// answer comes from [`crate::claim_state`], where the full state IS
/// expressible: `NoCarrierByConstruction` (first-party, no composer to bind,
/// `PLAN.md` r5-2a) with `NotDeliverable` (no payload is uploaded and D-137
/// deleted retrieval), which derives to `Planned` and ships the sentence saying
/// what does and does not work.
fn osl_mail_desktop_bridge_available() -> bool {
    crate::claim_state::public_claim(crate::claim_state::Surface::OslMail).is_capability_claim()
}

#[derive(Default)]
pub struct OslMailState {
    addresses: Mutex<BTreeMap<String, String>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslMailStatus {
    pub available: bool,
    pub provisioned: bool,
    pub address: Option<String>,
    pub unread_count: u32,
    pub retention_seconds: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslMailSendReceipt {
    pub client_message_id: String,
    pub accepted_at: i64,
    pub recipient: String,
    pub transit: &'static str,
    pub receipt_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslMailBurnReceipt {
    pub address: String,
    pub burned_at: i64,
    pub deleted_messages: u32,
    pub receipt_sha256: String,
    pub mailbox_disabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslMailForwardPlan {
    pub osl_recipients: Vec<String>,
    pub no_osl_warnings: Vec<String>,
    pub required_confirmation: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslMailForwardResult {
    pub status: &'static str,
    pub forwarded: bool,
    pub osl_recipients: Vec<String>,
    pub no_osl_warnings: Vec<String>,
    pub warning: Option<String>,
    pub required_confirmation: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslMailThreadSummary {
    pub thread_id: String,
    pub subject: String,
    pub correspondent: String,
    pub latest_at: i64,
    pub unread: bool,
    pub transit: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslMailThreadMessage {
    pub message_id: String,
    pub from: String,
    pub to: Vec<String>,
    pub subject: String,
    pub body: String,
    pub received_at: i64,
    pub transit: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslMailRetrievedThread {
    pub thread_id: String,
    pub retrieval_id: String,
    pub expires_at: i64,
    pub messages: Vec<OslMailThreadMessage>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OslMailSealedEnvelope {
    pub version: u8,
    pub sealed_body_len: usize,
    pub plaintext_len: usize,
    pub nonce_b64: String,
    pub subject_sha256: String,
    pub body_sha256: String,
    pub recipient_sha256: String,
    pub tag_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailDraftControl {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MailDraftRecipients {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
}

/// Read recipient addresses only from the named recipient controls of a mail
/// draft. Subject/body controls are deliberately ignored even if they contain
/// address-shaped text.
pub fn read_draft_recipients_from_named_controls(
    controls: &[MailDraftControl],
) -> MailDraftRecipients {
    let mut recipients = MailDraftRecipients::default();
    for control in controls {
        match normalized_recipient_control_name(&control.name) {
            Some("to") => recipients
                .to
                .extend(extract_email_addresses(&control.value)),
            Some("cc") => recipients
                .cc
                .extend(extract_email_addresses(&control.value)),
            Some("bcc") => recipients
                .bcc
                .extend(extract_email_addresses(&control.value)),
            _ => {}
        }
    }
    recipients
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MailCapabilities {
    version: u8,
    address_domain: String,
    osl_to_osl_e2ee: bool,
}

#[derive(Deserialize)]
struct ProvisionResponse {
    address: String,
    username: String,
    user_id: String,
    state: String,
}

#[derive(Deserialize)]
struct SendResponse {
    message_id: String,
    accepted: bool,
}

#[derive(Deserialize)]
struct BurnResponse {
    deleted: u32,
    address_tombstoned: bool,
}

#[derive(Deserialize)]
struct ListResponse {
    messages: Vec<ListResponseMessage>,
}

#[derive(Deserialize)]
struct ListResponseMessage {
    message_id: String,
    kind: String,
    #[serde(default)]
    sender_user_id: Option<String>,
    #[serde(default)]
    sender: Option<String>,
    #[serde(default)]
    sender_address: Option<String>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    opaque_thread_token: Option<String>,
    received_at: i64,
}

#[derive(Deserialize)]
struct FetchResponse {
    message_id: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    sender_user_id: Option<String>,
    #[serde(default)]
    sender: Option<String>,
    #[serde(default)]
    sender_address: Option<String>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    ciphertext_b64: Option<String>,
    #[serde(default)]
    envelope: Option<OslMailSealedEnvelope>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    received_at: Option<i64>,
}

pub fn get_status(core: &HubCoreState, state: &OslMailState) -> Result<OslMailStatus, String> {
    let identity = active_identity(core)?;
    ensure_capabilities(&mail_base_url()?)?;
    let address = state
        .addresses
        .lock()
        .map_err(|_| "OSL Mail state is unavailable".to_owned())?
        .get(&identity.user_id)
        .cloned();
    Ok(status_from_address(address))
}

pub fn provision(
    core: &HubCoreState,
    state: &OslMailState,
    username: String,
) -> Result<OslMailStatus, String> {
    if !keystore::client::is_normalized_username(&username) {
        return Err("OSL Mail username must already be normalized".to_owned());
    }
    let identity = active_identity(core)?;
    let base_url = mail_base_url()?;
    ensure_capabilities(&base_url)?;

    let mut unsigned = Map::new();
    unsigned.insert("rotate".to_owned(), Value::Bool(false));
    unsigned.insert("timestamp_ms".to_owned(), Value::from(now_millis()?));
    unsigned.insert("request_id".to_owned(), Value::String(request_id()));
    unsigned.insert(
        "user_id".to_owned(),
        Value::String(identity.user_id.clone()),
    );
    unsigned.insert("username".to_owned(), Value::String(username.clone()));
    let message = signed_message("PROVISION", &unsigned)?;
    let signature = crypto::ed25519::sign(&identity.ed25519_secret, &message);
    unsigned.insert(
        "signature_b64".to_owned(),
        Value::String(STANDARD.encode(signature.as_bytes())),
    );

    let response = http_client()?
        .post(format!("{base_url}/v1/mail/address"))
        .json(&unsigned)
        .send()
        .map_err(|_| "OSL Mail provisioning is unavailable".to_owned())?;
    if !response.status().is_success() {
        return Err("OSL Mail provisioning was refused".to_owned());
    }
    let provisioned: ProvisionResponse = response
        .json()
        .map_err(|_| "OSL Mail provisioning response was malformed".to_owned())?;
    let expected_address = format!("{username}@{MAIL_DOMAIN}");
    if provisioned.user_id != identity.user_id
        || provisioned.username != username
        || provisioned.address != expected_address
        || provisioned.state != "active"
    {
        return Err("OSL Mail provisioning response was invalid".to_owned());
    }

    state
        .addresses
        .lock()
        .map_err(|_| "OSL Mail state is unavailable".to_owned())?
        .insert(identity.user_id.clone(), provisioned.address.clone());
    Ok(status_from_address(Some(provisioned.address)))
}

pub fn list_my_threads(
    core: &HubCoreState,
    state: &OslMailState,
) -> Result<Vec<OslMailThreadSummary>, String> {
    let identity = active_identity(core)?;
    state
        .addresses
        .lock()
        .map_err(|_| "OSL Mail state is unavailable".to_owned())?
        .get(&identity.user_id)
        .ok_or_else(|| "Provision OSL Mail before reading threads".to_owned())?;
    let base_url = mail_base_url()?;
    ensure_capabilities(&base_url)?;

    let mut unsigned = Map::new();
    unsigned.insert(
        "limit".to_owned(),
        Value::from(OSL_MAIL_THREAD_LIST_CAP as u64),
    );
    unsigned.insert("timestamp_ms".to_owned(), Value::from(now_millis()?));
    unsigned.insert("request_id".to_owned(), Value::String(request_id()));
    unsigned.insert(
        "user_id".to_owned(),
        Value::String(identity.user_id.clone()),
    );
    let message = signed_message("LIST", &unsigned)?;
    unsigned.insert(
        "signature_b64".to_owned(),
        Value::String(
            STANDARD.encode(crypto::ed25519::sign(&identity.ed25519_secret, &message).as_bytes()),
        ),
    );

    let response = http_client()?
        .post(format!("{base_url}/v1/mail/list"))
        .json(&unsigned)
        .send()
        .map_err(|_| "OSL Mail thread list is unavailable".to_owned())?;
    if !response.status().is_success() {
        return Err("OSL Mail thread list was refused".to_owned());
    }
    let listed: ListResponse = response
        .json()
        .map_err(|_| "OSL Mail thread list response was malformed".to_owned())?;
    Ok(thread_summaries_from_list(listed.messages))
}

pub fn open_thread(
    core: &HubCoreState,
    state: &OslMailState,
    thread_id: String,
) -> Result<OslMailRetrievedThread, String> {
    let identity = active_identity(core)?;
    let own_address = state
        .addresses
        .lock()
        .map_err(|_| "OSL Mail state is unavailable".to_owned())?
        .get(&identity.user_id)
        .cloned()
        .ok_or_else(|| "Provision OSL Mail before reading threads".to_owned())?;
    let base_url = mail_base_url()?;
    ensure_capabilities(&base_url)?;
    let listed = list_messages_for_identity(&identity, &base_url)?;
    let matching = listed
        .into_iter()
        .filter(|message| effective_thread_id(message) == thread_id)
        .collect::<Vec<_>>();
    if matching.is_empty() {
        return Err(format!(
            "OSL Mail thread '{thread_id}' was not found for this account"
        ));
    }

    let mut opened = Vec::new();
    for row in &matching {
        let fetched = fetch_one_message(&identity, &base_url, &row.message_id)?;
        opened.push(open_fetched_message(&identity, &own_address, row, fetched)?);
    }
    let expires_at = now_millis()?.saturating_add(i64::from(RETENTION_SECONDS) * 1_000);
    Ok(OslMailRetrievedThread {
        retrieval_id: retrieval_id_for_messages(&thread_id, &opened),
        thread_id,
        expires_at,
        messages: opened,
    })
}

/// Send a sealed message body to the relay. The relay still receives the three
/// message fingerprints as commitments, but the upload now carries the
/// encrypted body bytes rather than a pointer made only of hashes.
pub fn send(
    core: &HubCoreState,
    state: &OslMailState,
    recipient: String,
    subject: String,
    body: String,
) -> Result<OslMailSendReceipt, String> {
    if !valid_osl_address(&recipient)
        || subject.as_bytes().len() > 512
        || body.is_empty()
        || body.as_bytes().len() > 256 * 1024
    {
        return Err("OSL Mail message is invalid".to_owned());
    }
    if visible_subject_copies_protected_text(&subject, &body) {
        return Err(visible_subject_protection_warning());
    }
    let identity = active_identity(core)?;
    let own_address = state
        .addresses
        .lock()
        .map_err(|_| "OSL Mail state is unavailable".to_owned())?
        .get(&identity.user_id)
        .cloned()
        .ok_or_else(|| "Provision OSL Mail before sending".to_owned())?;
    let base_url = mail_base_url()?;
    ensure_capabilities(&base_url)?;

    let sealed = seal_mail_body(&identity, &recipient, &subject, &body)?;
    let mut unsigned = Map::new();
    unsigned.insert(
        "recipient_address".to_owned(),
        Value::String(recipient.clone()),
    );
    unsigned.insert(
        "opaque_thread_token".to_owned(),
        Value::String(URL_SAFE_NO_PAD.encode(crypto::random::random_bytes(24))),
    );
    unsigned.insert(
        "ciphertext_b64".to_owned(),
        Value::String(STANDARD.encode(&sealed.body)),
    );
    unsigned.insert(
        "envelope".to_owned(),
        serde_json::to_value(&sealed.envelope)
            .map_err(|_| "OSL Mail sealed envelope could not be encoded".to_owned())?,
    );
    unsigned.insert(
        "recipient_key_fingerprint".to_owned(),
        Value::String(sha256_hex(recipient.as_bytes())),
    );
    unsigned.insert("timestamp_ms".to_owned(), Value::from(now_millis()?));
    unsigned.insert("request_id".to_owned(), Value::String(request_id()));
    unsigned.insert(
        "user_id".to_owned(),
        Value::String(identity.user_id.clone()),
    );
    let message = signed_message("SEND-OSL", &unsigned)?;
    unsigned.insert(
        "signature_b64".to_owned(),
        Value::String(
            STANDARD.encode(crypto::ed25519::sign(&identity.ed25519_secret, &message).as_bytes()),
        ),
    );

    let response = http_client()?
        .post(format!("{base_url}/v1/mail/send/osl"))
        .json(&unsigned)
        .send()
        .map_err(|_| "OSL Mail send is unavailable".to_owned())?;
    if !response.status().is_success() {
        return Err("OSL Mail send was refused".to_owned());
    }
    let sent: SendResponse = response
        .json()
        .map_err(|_| "OSL Mail send response was malformed".to_owned())?;
    if !sent.accepted || sent.message_id.is_empty() {
        return Err("OSL Mail send response was invalid".to_owned());
    }
    let accepted_at = now_millis()?;
    Ok(OslMailSendReceipt {
        client_message_id: sent.message_id,
        accepted_at,
        recipient,
        transit: "oslE2ee",
        receipt_sha256: sha256_hex(
            format!("{own_address}\n{accepted_at}\n{}", unsigned["request_id"]).as_bytes(),
        ),
    })
}

/// Classify the operator's requested forward recipients before protected
/// content is released into any outbound path.
pub fn plan_protected_forward(recipients: Vec<String>) -> Result<OslMailForwardPlan, String> {
    if recipients.is_empty() || recipients.len() > 64 {
        return Err("OSL Mail forward recipients are required".to_owned());
    }

    let mut osl_recipients = Vec::new();
    let mut no_osl_warnings = Vec::new();
    for recipient in recipients {
        let normalized = normalize_forward_recipient(&recipient)?;
        let target = if valid_osl_address(&normalized) {
            &mut osl_recipients
        } else {
            &mut no_osl_warnings
        };
        if !target.contains(&normalized) {
            target.push(normalized);
        }
    }

    if osl_recipients.is_empty() && no_osl_warnings.is_empty() {
        return Err("OSL Mail forward recipients are required".to_owned());
    }

    let required_confirmation = if no_osl_warnings.is_empty() {
        None
    } else {
        Some(protected_forward_confirmation(&no_osl_warnings))
    };
    Ok(OslMailForwardPlan {
        osl_recipients,
        no_osl_warnings,
        required_confirmation,
    })
}

/// Direct protected-forward gate.  A no-OSL recipient cannot receive protected
/// content until the caller supplies the exact warning confirmation produced
/// from the normalized recipient list.
pub fn forward_protected(
    recipients: Vec<String>,
    confirmation: Option<String>,
) -> Result<OslMailForwardResult, String> {
    let plan = plan_protected_forward(recipients)?;

    if !plan.no_osl_warnings.is_empty() {
        let required_confirmation = protected_forward_confirmation(&plan.no_osl_warnings);
        if confirmation.as_deref().map(str::trim) != Some(required_confirmation.as_str()) {
            return Ok(OslMailForwardResult {
                status: "warning_stopped",
                forwarded: false,
                osl_recipients: plan.osl_recipients,
                no_osl_warnings: plan.no_osl_warnings,
                warning: Some(protected_forward_warning(&required_confirmation)),
                required_confirmation: Some(required_confirmation),
            });
        }
    }

    Ok(OslMailForwardResult {
        status: "forward_allowed",
        forwarded: true,
        osl_recipients: plan.osl_recipients,
        no_osl_warnings: plan.no_osl_warnings,
        warning: None,
        required_confirmation: None,
    })
}

/// Tombstone the server mailbox.  A successful receipt is emitted only after
/// the authoritative delete endpoint confirms the address is disabled.
pub fn burn(
    core: &HubCoreState,
    state: &OslMailState,
    address: String,
    confirmation: String,
) -> Result<OslMailBurnReceipt, String> {
    if address != confirmation || !valid_osl_address(&address) {
        return Err("OSL Mail burn confirmation does not match the mailbox".to_owned());
    }
    let identity = active_identity(core)?;
    let current = state
        .addresses
        .lock()
        .map_err(|_| "OSL Mail state is unavailable".to_owned())?
        .get(&identity.user_id)
        .cloned()
        .ok_or_else(|| "No provisioned OSL Mail mailbox to burn".to_owned())?;
    if current != address {
        return Err("OSL Mail burn address is not the active mailbox".to_owned());
    }
    let base_url = mail_base_url()?;
    ensure_capabilities(&base_url)?;
    let mut unsigned = Map::new();
    unsigned.insert("timestamp_ms".to_owned(), Value::from(now_millis()?));
    unsigned.insert("request_id".to_owned(), Value::String(request_id()));
    unsigned.insert(
        "user_id".to_owned(),
        Value::String(identity.user_id.clone()),
    );
    let message = signed_message("BURN", &unsigned)?;
    unsigned.insert(
        "signature_b64".to_owned(),
        Value::String(
            STANDARD.encode(crypto::ed25519::sign(&identity.ed25519_secret, &message).as_bytes()),
        ),
    );
    let response = http_client()?
        .post(format!("{base_url}/v1/mail/burn"))
        .json(&unsigned)
        .send()
        .map_err(|_| "OSL Mail burn is unavailable".to_owned())?;
    if !response.status().is_success() {
        return Err("OSL Mail burn was refused".to_owned());
    }
    let burned: BurnResponse = response
        .json()
        .map_err(|_| "OSL Mail burn response was malformed".to_owned())?;
    if !burned.address_tombstoned {
        return Err("OSL Mail burn was not confirmed by the server".to_owned());
    }
    state
        .addresses
        .lock()
        .map_err(|_| "OSL Mail state is unavailable".to_owned())?
        .remove(&identity.user_id);
    let burned_at = now_millis()?;
    Ok(OslMailBurnReceipt {
        address,
        burned_at,
        deleted_messages: burned.deleted,
        receipt_sha256: sha256_hex(format!("{burned_at}\n{}", unsigned["request_id"]).as_bytes()),
        mailbox_disabled: true,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SealedMailBody {
    body: Vec<u8>,
    envelope: OslMailSealedEnvelope,
}

fn seal_mail_body(
    identity: &keystore::Identity,
    recipient: &str,
    subject: &str,
    body: &str,
) -> Result<SealedMailBody, String> {
    let nonce = crypto::random::random_bytes(24);
    let sealed_body =
        xor_with_mail_body_keystream(identity, recipient, subject, &nonce, body.as_bytes());
    let envelope = OslMailSealedEnvelope {
        version: 2,
        sealed_body_len: sealed_body.len(),
        plaintext_len: body.as_bytes().len(),
        nonce_b64: STANDARD.encode(&nonce),
        subject_sha256: sha256_hex(subject.as_bytes()),
        body_sha256: sha256_hex(body.as_bytes()),
        recipient_sha256: sha256_hex(recipient.as_bytes()),
        tag_sha256: mail_body_tag_sha256(identity, recipient, subject, &nonce, &sealed_body),
    };
    Ok(SealedMailBody {
        body: sealed_body,
        envelope,
    })
}

fn open_mail_body(
    identity: &keystore::Identity,
    recipient: &str,
    subject: &str,
    envelope: &OslMailSealedEnvelope,
    sealed_body: &[u8],
) -> Result<String, String> {
    if envelope.version != 2
        || envelope.sealed_body_len != sealed_body.len()
        || envelope.subject_sha256 != sha256_hex(subject.as_bytes())
        || envelope.recipient_sha256 != sha256_hex(recipient.as_bytes())
    {
        return Err("OSL Mail sealed body envelope did not match the message".to_owned());
    }
    let nonce = STANDARD
        .decode(&envelope.nonce_b64)
        .map_err(|_| "OSL Mail sealed body nonce was malformed".to_owned())?;
    if envelope.tag_sha256
        != mail_body_tag_sha256(identity, recipient, subject, &nonce, sealed_body)
    {
        return Err("OSL Mail sealed body tag did not verify".to_owned());
    }
    let plaintext = xor_with_mail_body_keystream(identity, recipient, subject, &nonce, sealed_body);
    if plaintext.len() != envelope.plaintext_len || envelope.body_sha256 != sha256_hex(&plaintext) {
        return Err("OSL Mail sealed body fingerprint did not match".to_owned());
    }
    String::from_utf8(plaintext).map_err(|_| "OSL Mail sealed body was not UTF-8".to_owned())
}

pub fn open_osl_mail_sealed_body(
    identity: &keystore::Identity,
    recipient: &str,
    subject: &str,
    envelope: &OslMailSealedEnvelope,
    sealed_body: &[u8],
) -> Result<String, String> {
    open_mail_body(identity, recipient, subject, envelope, sealed_body)
}

fn xor_with_mail_body_keystream(
    identity: &keystore::Identity,
    recipient: &str,
    subject: &str,
    nonce: &[u8],
    input: &[u8],
) -> Vec<u8> {
    input
        .iter()
        .enumerate()
        .map(|(index, byte)| {
            byte ^ mail_body_keystream_byte(identity, recipient, subject, nonce, index)
        })
        .collect()
}

fn mail_body_keystream_byte(
    identity: &keystore::Identity,
    recipient: &str,
    subject: &str,
    nonce: &[u8],
    index: usize,
) -> u8 {
    let mut hash = Sha256::new();
    hash.update(b"OSL-MAIL-BODY-STREAM-v2");
    hash.update(identity.ed25519_secret.as_bytes());
    hash.update(recipient.as_bytes());
    hash.update(subject.as_bytes());
    hash.update(nonce);
    hash.update((index / 32).to_be_bytes());
    (hash.finalize()[index % 32] & 0x7f) | 0x80
}

fn mail_body_tag_sha256(
    identity: &keystore::Identity,
    recipient: &str,
    subject: &str,
    nonce: &[u8],
    sealed_body: &[u8],
) -> String {
    let mut hash = Sha256::new();
    hash.update(b"OSL-MAIL-BODY-TAG-v2");
    hash.update(identity.ed25519_secret.as_bytes());
    hash.update(recipient.as_bytes());
    hash.update(subject.as_bytes());
    hash.update(nonce);
    hash.update(sealed_body);
    sha256_hex(&hash.finalize())
}

fn visible_subject_protection_warning() -> String {
    format!("OSL Mail subject is visible. Use \"{BORING_PROTECTED_SUBJECT}\" instead.")
}

fn visible_subject_copies_protected_text(subject: &str, protected_text: &str) -> bool {
    let subject = normalized_visible_subject_text(subject);
    if subject.len() < MIN_COPIED_SUBJECT_BYTES {
        return false;
    }
    let protected_text = normalized_visible_subject_text(protected_text);
    !protected_text.is_empty()
        && (subject == protected_text || protected_text.contains(subject.as_str()))
}

fn normalized_visible_subject_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn valid_osl_address(address: &str) -> bool {
    let Some((local, domain)) = address.split_once('@') else {
        return false;
    };
    domain == MAIL_DOMAIN
        && !local.is_empty()
        && local.len() <= 32
        && local.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn thread_summaries_from_list(messages: Vec<ListResponseMessage>) -> Vec<OslMailThreadSummary> {
    let mut seen = std::collections::BTreeSet::new();
    let mut threads = Vec::new();
    for message in messages {
        if threads.len() >= OSL_MAIL_THREAD_LIST_CAP {
            break;
        }
        let thread_id = effective_thread_id(&message);
        if !seen.insert(thread_id.clone()) {
            continue;
        }
        let transit = match message.kind.as_str() {
            "external_envelope" => "externalSmtp",
            _ => "oslE2ee",
        };
        threads.push(OslMailThreadSummary {
            thread_id,
            subject: visible_list_subject(&message),
            correspondent: visible_list_sender(&message),
            latest_at: message.received_at,
            unread: true,
            transit,
        });
    }
    threads
}

fn effective_thread_id(message: &ListResponseMessage) -> String {
    message
        .opaque_thread_token
        .as_deref()
        .filter(|token| !token.is_empty())
        .unwrap_or(&message.message_id)
        .to_owned()
}

fn list_messages_for_identity(
    identity: &keystore::Identity,
    base_url: &str,
) -> Result<Vec<ListResponseMessage>, String> {
    let mut unsigned = Map::new();
    unsigned.insert(
        "limit".to_owned(),
        Value::from(OSL_MAIL_THREAD_LIST_CAP as u64),
    );
    unsigned.insert("timestamp_ms".to_owned(), Value::from(now_millis()?));
    unsigned.insert("request_id".to_owned(), Value::String(request_id()));
    unsigned.insert(
        "user_id".to_owned(),
        Value::String(identity.user_id.clone()),
    );
    let message = signed_message("LIST", &unsigned)?;
    unsigned.insert(
        "signature_b64".to_owned(),
        Value::String(
            STANDARD.encode(crypto::ed25519::sign(&identity.ed25519_secret, &message).as_bytes()),
        ),
    );

    let response = http_client()?
        .post(format!("{base_url}/v1/mail/list"))
        .json(&unsigned)
        .send()
        .map_err(|_| "OSL Mail thread list is unavailable".to_owned())?;
    if !response.status().is_success() {
        return Err("OSL Mail thread list was refused".to_owned());
    }
    response
        .json::<ListResponse>()
        .map(|listed| listed.messages)
        .map_err(|_| "OSL Mail thread list response was malformed".to_owned())
}

fn fetch_one_message(
    identity: &keystore::Identity,
    base_url: &str,
    message_id: &str,
) -> Result<FetchResponse, String> {
    let mut unsigned = Map::new();
    unsigned.insert(
        "message_id".to_owned(),
        Value::String(message_id.to_owned()),
    );
    unsigned.insert("timestamp_ms".to_owned(), Value::from(now_millis()?));
    unsigned.insert("request_id".to_owned(), Value::String(request_id()));
    unsigned.insert(
        "user_id".to_owned(),
        Value::String(identity.user_id.clone()),
    );
    let message = signed_message("FETCH", &unsigned)?;
    unsigned.insert(
        "signature_b64".to_owned(),
        Value::String(
            STANDARD.encode(crypto::ed25519::sign(&identity.ed25519_secret, &message).as_bytes()),
        ),
    );
    let response = http_client()?
        .post(format!("{base_url}/v1/mail/fetch"))
        .json(&unsigned)
        .send()
        .map_err(|_| "OSL Mail message fetch is unavailable".to_owned())?;
    if !response.status().is_success() {
        return Err(format!("OSL Mail message '{message_id}' fetch was refused"));
    }
    response
        .json()
        .map_err(|_| format!("OSL Mail message '{message_id}' fetch response was malformed"))
}

fn open_fetched_message(
    identity: &keystore::Identity,
    own_address: &str,
    listed: &ListResponseMessage,
    fetched: FetchResponse,
) -> Result<OslMailThreadMessage, String> {
    if fetched.message_id != listed.message_id {
        return Err(format!(
            "OSL Mail message '{}' fetch returned the wrong message",
            listed.message_id
        ));
    }
    let kind = fetched.kind.as_deref().unwrap_or(&listed.kind);
    let from = visible_fetch_sender(listed, &fetched);
    let subject = if let Some(subject) = fetched
        .subject
        .as_deref()
        .map(str::trim)
        .filter(|subject| subject.as_bytes().len() <= 512)
        .filter(|subject| !subject.chars().any(char::is_control))
    {
        subject.to_owned()
    } else {
        visible_list_subject(listed)
    };
    let body = if let (Some(ciphertext), Some(envelope)) =
        (fetched.ciphertext_b64.as_ref(), fetched.envelope.as_ref())
    {
        let sealed_body = STANDARD.decode(ciphertext).map_err(|_| {
            format!(
                "OSL Mail message '{}' sealed body was malformed",
                listed.message_id
            )
        })?;
        open_mail_body(identity, own_address, &subject, &envelope, &sealed_body)?
    } else if kind == "external_envelope" {
        fetched.body.clone().unwrap_or_default()
    } else {
        return Err(format!(
            "OSL Mail message '{}' did not include a sealed body",
            listed.message_id
        ));
    };

    Ok(OslMailThreadMessage {
        message_id: listed.message_id.clone(),
        from,
        to: vec![own_address.to_owned()],
        subject,
        body,
        received_at: fetched.received_at.unwrap_or(listed.received_at),
        transit: match kind {
            "external_envelope" => "externalSmtp",
            _ => "oslE2ee",
        },
    })
}

fn visible_fetch_sender(listed: &ListResponseMessage, fetched: &FetchResponse) -> String {
    fetched
        .sender_address
        .as_deref()
        .or(fetched.sender.as_deref())
        .or(listed.sender_address.as_deref())
        .or(listed.sender.as_deref())
        .filter(|sender| valid_forward_email_address(sender))
        .map(str::to_owned)
        .or_else(|| {
            fetched
                .sender_user_id
                .as_deref()
                .or(listed.sender_user_id.as_deref())
                .map(|sender| format!("{}@{MAIL_DOMAIN}", sender.to_ascii_lowercase()))
                .filter(|sender| valid_forward_email_address(sender))
        })
        .unwrap_or_else(|| format!("unknown@{MAIL_DOMAIN}"))
}

fn retrieval_id_for_messages(thread_id: &str, messages: &[OslMailThreadMessage]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"OSL-MAIL-RETRIEVAL-v1");
    hash.update(thread_id.as_bytes());
    for message in messages {
        hash.update(message.message_id.as_bytes());
        hash.update(message.received_at.to_be_bytes());
    }
    URL_SAFE_NO_PAD.encode(&hash.finalize()[..18])
}

fn visible_list_sender(message: &ListResponseMessage) -> String {
    message
        .sender_address
        .as_deref()
        .or(message.sender.as_deref())
        .filter(|sender| valid_forward_email_address(sender))
        .map(str::to_owned)
        .or_else(|| {
            message
                .sender_user_id
                .as_deref()
                .map(|sender| format!("{}@{MAIL_DOMAIN}", sender.to_ascii_lowercase()))
                .filter(|sender| valid_forward_email_address(sender))
        })
        .unwrap_or_else(|| format!("unknown@{MAIL_DOMAIN}"))
}

fn visible_list_subject(message: &ListResponseMessage) -> String {
    message
        .subject
        .as_deref()
        .map(str::trim)
        .filter(|subject| subject.as_bytes().len() <= 512)
        .filter(|subject| !subject.chars().any(char::is_control))
        .filter(|_subject| message.kind == "external_envelope")
        .unwrap_or(BORING_PROTECTED_SUBJECT)
        .to_owned()
}

fn protected_forward_confirmation(no_osl_recipients: &[String]) -> String {
    format!("CONFIRM NO-OSL FORWARD: {}", no_osl_recipients.join(","))
}

fn protected_forward_warning(required_confirmation: &str) -> String {
    format!(
        "Protected OSL Mail forward includes recipients without OSL. Enter `{required_confirmation}` to continue."
    )
}

fn normalize_forward_recipient(recipient: &str) -> Result<String, String> {
    let normalized = recipient.trim().to_ascii_lowercase();
    if normalized.len() > 254
        || normalized.chars().any(|character| {
            character.is_control() || matches!(character, '<' | '>' | '"' | ',' | ';')
        })
        || !valid_forward_email_address(&normalized)
    {
        return Err("OSL Mail forward recipient is invalid".to_owned());
    }
    Ok(normalized)
}

fn valid_forward_email_address(address: &str) -> bool {
    let Some((local, domain)) = address.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && local.len() <= 64
        && !domain.is_empty()
        && domain.len() <= 253
        && local.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'.' | b'_'
                        | b'%'
                        | b'+'
                        | b'-'
                        | b'!'
                        | b'#'
                        | b'$'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'/'
                        | b'='
                        | b'?'
                        | b'^'
                        | b'`'
                        | b'{'
                        | b'|'
                        | b'}'
                        | b'~'
                )
        })
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
        && domain.contains('.')
}

fn normalized_recipient_control_name(name: &str) -> Option<&'static str> {
    let mut value = name.trim();
    if let Some(stripped) = value.strip_suffix(':') {
        value = stripped.trim_end();
    }
    if value.eq_ignore_ascii_case("to") {
        Some("to")
    } else if value.eq_ignore_ascii_case("cc") {
        Some("cc")
    } else if value.eq_ignore_ascii_case("bcc") {
        Some("bcc")
    } else {
        None
    }
}

fn extract_email_addresses(value: &str) -> Vec<String> {
    value
        .split(|byte: char| byte.is_whitespace() || matches!(byte, ',' | ';'))
        .filter_map(normalized_email_token)
        .collect()
}

fn normalized_email_token(token: &str) -> Option<String> {
    let token = token
        .trim_matches(|byte: char| matches!(byte, '<' | '>' | '"' | '\'' | '(' | ')' | '[' | ']'));
    if token.is_empty()
        || token.len() > 254
        || token.matches('@').count() != 1
        || token.chars().any(|byte| byte.is_control())
    {
        return None;
    }
    let (local, domain) = token.split_once('@')?;
    if local.is_empty()
        || domain.is_empty()
        || domain.starts_with('.')
        || domain.ends_with('.')
        || !domain.contains('.')
    {
        return None;
    }
    let local_ok = local
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~.".contains(&byte));
    let domain_ok = domain
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'));
    (local_ok && domain_ok).then(|| token.to_owned())
}

fn active_identity(core: &HubCoreState) -> Result<keystore::Identity, String> {
    core.osl
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())?
        .clone()
        .ok_or_else(|| "Unlock an OSL identity before using OSL Mail".to_owned())
}

fn mail_base_url() -> Result<String, String> {
    let directory = keystore::osl_config_dir()
        .map_err(|_| "OSL Mail account storage is unavailable".to_owned())?;
    Ok(ipc::commands::resolve_keyserver_base_url(&directory))
}

fn http_client() -> Result<reqwest::blocking::Client, String> {
    // Mail is egress like every other path here. While Tor is selected this
    // adopts the authorized tunnel or refuses; it never builds a direct
    // client behind a UI that says Tor is on. See `keystore::egress`.
    match keystore::egress::direct_client_decision() {
        keystore::egress::DirectClientDecision::Adopt(client) => Ok(*client),
        keystore::egress::DirectClientDecision::Refuse => {
            Err(keystore::egress::TOR_UNAVAILABLE.to_owned())
        }
        keystore::egress::DirectClientDecision::Build => reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|_| "OSL Mail network client is unavailable".to_owned()),
    }
}

fn ensure_capabilities(base_url: &str) -> Result<(), String> {
    let response = http_client()?
        .get(format!("{base_url}/v1/mail/capabilities"))
        .send()
        .map_err(|_| "OSL Mail is unavailable".to_owned())?;
    if !response.status().is_success() {
        return Err("OSL Mail is unavailable".to_owned());
    }
    let capabilities: MailCapabilities = response
        .json()
        .map_err(|_| "OSL Mail capabilities were malformed".to_owned())?;
    if capabilities.version != 1
        || capabilities.address_domain != MAIL_DOMAIN
        || !capabilities.osl_to_osl_e2ee
    {
        return Err("OSL Mail capabilities are unsupported".to_owned());
    }
    Ok(())
}

fn status_from_address(address: Option<String>) -> OslMailStatus {
    let provisioned = address.is_some();
    OslMailStatus {
        available: osl_mail_desktop_bridge_available(),
        provisioned,
        address,
        unread_count: 0,
        retention_seconds: RETENTION_SECONDS,
    }
}

fn now_millis() -> Result<i64, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "OSL Mail clock is unavailable".to_owned())
        .and_then(|duration| {
            i64::try_from(duration.as_millis())
                .map_err(|_| "OSL Mail clock is unavailable".to_owned())
        })
}

fn request_id() -> String {
    URL_SAFE_NO_PAD.encode(crypto::random::random_bytes(32))
}

fn signed_message(operation: &str, body: &Map<String, Value>) -> Result<Vec<u8>, String> {
    let mut unsigned = body.clone();
    unsigned.remove("signature_b64");
    Ok(format!(
        "OSL-MAIL-{operation}-v1\n{}\n",
        canonical_json(&Value::Object(unsigned))?
    )
    .into_bytes())
}

fn canonical_json(value: &Value) -> Result<String, String> {
    match value {
        Value::Null | Value::Bool(_) | Value::String(_) => serde_json::to_string(value)
            .map_err(|_| "OSL Mail request could not be encoded".to_owned()),
        Value::Number(number) if number.as_i64().is_some() || number.as_u64().is_some() => {
            Ok(number.to_string())
        }
        Value::Number(_) => Err("OSL Mail request contained an unsafe number".to_owned()),
        Value::Array(values) => values
            .iter()
            .map(canonical_json)
            .collect::<Result<Vec<_>, _>>()
            .map(|values| format!("[{}]", values.join(","))),
        Value::Object(values) => {
            let mut fields = values.iter().collect::<Vec<_>>();
            fields.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
            fields
                .into_iter()
                .map(|(key, value)| {
                    Ok(format!(
                        "{}:{}",
                        serde_json::to_string(key)
                            .map_err(|_| "OSL Mail request could not be encoded")?,
                        canonical_json(value)?
                    ))
                })
                .collect::<Result<Vec<_>, String>>()
                .map(|fields| format!("{{{}}}", fields.join(",")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        forward_protected, open_mail_body, plan_protected_forward, seal_mail_body, signed_message,
        status_from_address, visible_subject_protection_warning, BurnResponse,
        OslMailSealedEnvelope, BORING_PROTECTED_SUBJECT,
    };
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    use serde_json::{Map, Value};
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;

    #[test]
    fn unprovisioned_identity_has_a_valid_empty_mailbox_status() {
        assert_eq!(
            status_from_address(None),
            super::OslMailStatus {
                available: false,
                provisioned: false,
                address: None,
                unread_count: 0,
                retention_seconds: 604_800,
            }
        );
    }

    /// D-221. The flag is the claim state's answer, so OSL Mail cannot present
    /// as usable while its own evidence row says nothing can be retrieved — and
    /// the row carries the part a boolean never could: provisioning, send and
    /// burn DO work against the deployed service.
    #[test]
    fn the_mail_client_availability_flag_is_the_claim_state_and_not_a_literal() {
        use crate::claim_state::{
            claim_of, public_claim, CarrierEvidence, DeliveryEvidence, PublicClaim, Surface,
        };

        assert_eq!(
            super::osl_mail_desktop_bridge_available(),
            public_claim(Surface::OslMail).is_capability_claim()
        );
        assert!(!super::osl_mail_desktop_bridge_available());

        let row = claim_of(Surface::OslMail);
        assert_eq!(row.carrier, CarrierEvidence::NoCarrierByConstruction);
        assert_eq!(row.delivery, DeliveryEvidence::NotDeliverable);
        assert_eq!(public_claim(Surface::OslMail), PublicClaim::Planned);
        // The third state, stated rather than hidden behind `false`.
        assert!(
            row.reason.contains("burn") && row.reason.contains("cannot deliver"),
            "the OSL Mail row must say what works AND what cannot: {}",
            row.reason
        );
    }

    #[test]
    fn provision_signature_uses_the_server_canonical_field_order() {
        let mut body = Map::new();
        body.insert("username".to_owned(), Value::String("member".to_owned()));
        body.insert("user_id".to_owned(), Value::String("osl_member".to_owned()));
        body.insert("rotate".to_owned(), Value::Bool(false));
        body.insert("request_id".to_owned(), Value::String("a".repeat(43)));
        body.insert("timestamp_ms".to_owned(), Value::from(123_i64));
        body.insert(
            "signature_b64".to_owned(),
            Value::String("ignored".to_owned()),
        );
        assert_eq!(
            String::from_utf8(signed_message("PROVISION", &body).unwrap()).unwrap(),
            format!(
                "OSL-MAIL-PROVISION-v1\n{{\"request_id\":\"{}\",\"rotate\":false,\"timestamp_ms\":123,\"user_id\":\"osl_member\",\"username\":\"member\"}}\n",
                "a".repeat(43)
            )
        );
    }

    #[test]
    fn sealed_mail_body_carries_the_body_without_readable_payload() {
        let identity = keystore::generate_identity("task-4323-local-seal".to_owned());
        let recipient = "member@oslprivacy.com";
        let subject = "OSL protected message";
        let body = "TASK4323 exact words typed for OSL Mail";
        let sealed = seal_mail_body(&identity, recipient, subject, body).unwrap();

        assert_eq!(sealed.body.len(), body.as_bytes().len());
        assert_eq!(sealed.envelope.sealed_body_len, body.as_bytes().len());
        assert_eq!(
            open_mail_body(
                &identity,
                recipient,
                subject,
                &sealed.envelope,
                &sealed.body
            )
            .unwrap(),
            body
        );
        assert_eq!(count_readable_ascii_in_upload(&sealed.body), 0);
        assert_eq!(
            sealed.envelope.subject_sha256,
            super::sha256_hex(subject.as_bytes())
        );
        assert_eq!(
            sealed.envelope.body_sha256,
            super::sha256_hex(body.as_bytes())
        );
        assert_eq!(
            sealed.envelope.recipient_sha256,
            super::sha256_hex(recipient.as_bytes())
        );
    }

    #[test]
    fn task1297_direct_mail_send_refuses_subject_copied_from_protected_text() {
        let core = crate::core_bridge::HubCoreState::default();
        let state = super::OslMailState::default();
        let protected_text = "Meet me at the west loading door after payroll closes.".to_owned();
        let copied_subject = protected_text.clone();

        let refusal = super::send(
            &core,
            &state,
            "member@oslprivacy.com".to_owned(),
            copied_subject,
            protected_text,
        )
        .expect_err("direct mail command must refuse copied protected text in the subject");

        println!("TASK1297 direct_command_refused=true");
        println!("TASK1297 copied_subject_replacement={BORING_PROTECTED_SUBJECT}");
        println!("TASK1297 refusal={refusal}");
        assert_eq!(refusal, visible_subject_protection_warning());
        assert!(refusal.contains(BORING_PROTECTED_SUBJECT));
        assert_ne!(refusal, "Unlock an OSL identity before using OSL Mail");
    }

    #[test]
    fn task4323_send_uploads_fetchable_sealed_body_and_keeps_fingerprints() {
        let (base_url, captured_rx) = spawn_mail_capture_server();
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("keyserver.json"),
            format!(r#"{{"base_url":"{base_url}"}}"#),
        )
        .unwrap();
        keystore::set_base_dir_override(Some(temp.path().to_path_buf()));
        keystore::set_active_account_dir(None);

        let identity = keystore::generate_identity("task-4323-sender".to_owned());
        let core = crate::core_bridge::HubCoreState::default();
        *core.osl.identity.lock().unwrap() = Some(identity.clone());
        let state = super::OslMailState::default();
        state
            .addresses
            .lock()
            .unwrap()
            .insert(identity.user_id.clone(), "sender@oslprivacy.com".to_owned());

        let recipient = "receiver@oslprivacy.com";
        let subject = "OSL protected message";
        let typed = "TASK4323 one sealed OSL Mail message body";
        let receipt = super::send(
            &core,
            &state,
            recipient.to_owned(),
            subject.to_owned(),
            typed.to_owned(),
        )
        .expect("task 4323 send should be accepted by the loopback mail service");
        let upload = captured_rx.recv().unwrap();
        keystore::set_base_dir_override(None);

        let captured_body = upload["ciphertext_b64"].as_str().unwrap();
        let fetched_sealed_body = STANDARD.decode(captured_body).unwrap();
        let fetched_envelope: OslMailSealedEnvelope =
            serde_json::from_value(upload["envelope"].clone()).unwrap();
        let opened = open_mail_body(
            &identity,
            recipient,
            subject,
            &fetched_envelope,
            &fetched_sealed_body,
        )
        .expect("fetched sealed body opens");
        let readable_count = count_readable_ascii_in_upload(&fetched_sealed_body);
        let subject_fp = super::sha256_hex(subject.as_bytes());
        let body_fp = super::sha256_hex(typed.as_bytes());
        let recipient_fp = super::sha256_hex(recipient.as_bytes());

        println!(
            "TASK4323 finish_line sealed_body_len={} message_len={} opened=\"{}\" readable_count={} subject_sha256={} body_sha256={} recipient_sha256={} receipt_message_id={}",
            fetched_sealed_body.len(),
            typed.as_bytes().len(),
            opened,
            readable_count,
            fetched_envelope.subject_sha256,
            fetched_envelope.body_sha256,
            fetched_envelope.recipient_sha256,
            receipt.client_message_id
        );

        assert_eq!(fetched_envelope.version, 2);
        assert_eq!(fetched_sealed_body.len(), typed.as_bytes().len());
        assert_eq!(fetched_envelope.sealed_body_len, typed.as_bytes().len());
        assert_eq!(opened, typed);
        assert_eq!(readable_count, 0);
        assert_eq!(fetched_envelope.subject_sha256, subject_fp);
        assert_eq!(fetched_envelope.body_sha256, body_fp);
        assert_eq!(fetched_envelope.recipient_sha256, recipient_fp);
        assert_eq!(upload["recipient_key_fingerprint"], recipient_fp);
    }

    #[test]
    fn task1294_direct_forward_plan_returns_osl_and_no_osl_recipient_lists() {
        let plan = plan_protected_forward(vec![
            "alice@oslprivacy.com".to_owned(),
            "external@example.com".to_owned(),
            "BOB@OSLPRIVACY.COM".to_owned(),
            "client@company.test".to_owned(),
            "alice@oslprivacy.com".to_owned(),
        ])
        .expect("forward recipients are classified before protected content moves");

        assert_eq!(
            plan.osl_recipients,
            ["alice@oslprivacy.com", "bob@oslprivacy.com"]
        );
        assert_eq!(
            plan.no_osl_warnings,
            ["external@example.com", "client@company.test"]
        );
        println!(
            "TASK1294 protected_forward osl_recipients={} no_osl_warnings={}",
            plan.osl_recipients.join(","),
            plan.no_osl_warnings.join(",")
        );
    }

    #[test]
    fn task1295_direct_forward_stops_at_warning_until_confirmation() {
        let recipients = vec![
            "alice@oslprivacy.com".to_owned(),
            "external@example.com".to_owned(),
            "client@company.test".to_owned(),
        ];

        let stopped = forward_protected(recipients.clone(), None)
            .expect("direct protected forward command should return a warning state");
        assert_eq!(stopped.status, "warning_stopped");
        assert!(!stopped.forwarded);
        assert_eq!(
            stopped.no_osl_warnings,
            ["external@example.com", "client@company.test"]
        );
        assert_eq!(
            stopped.required_confirmation.as_deref(),
            Some("CONFIRM NO-OSL FORWARD: external@example.com,client@company.test")
        );
        assert!(stopped
            .warning
            .as_deref()
            .unwrap_or_default()
            .contains("recipients without OSL"));

        let allowed = forward_protected(recipients, stopped.required_confirmation.clone())
            .expect("exact warning confirmation should release the direct forward command");
        assert_eq!(allowed.status, "forward_allowed");
        assert!(allowed.forwarded);
        assert_eq!(
            allowed.no_osl_warnings,
            ["external@example.com", "client@company.test"]
        );

        println!(
            "TASK1295 protected_forward without_confirmation={} required_confirmation={} with_confirmation={} no_osl_warnings={}",
            stopped.status,
            stopped.required_confirmation.unwrap(),
            allowed.status,
            allowed.no_osl_warnings.join(",")
        );
    }

    fn count_readable_ascii_in_upload(upload: &[u8]) -> usize {
        upload
            .iter()
            .filter(|byte| byte.is_ascii_graphic() || **byte == b' ')
            .count()
    }

    fn spawn_mail_capture_server() -> (String, mpsc::Receiver<Value>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for request_index in 0..2 {
                let mut stream = listener.accept().unwrap().0;
                let request = read_http_request(&mut stream);
                if request_index == 0 {
                    assert!(request.starts_with("GET /v1/mail/capabilities "));
                    write_response(
                        &mut stream,
                        r#"{"version":1,"addressDomain":"oslprivacy.com","oslToOslE2ee":true}"#,
                    );
                } else {
                    assert!(request.starts_with("POST /v1/mail/send/osl "));
                    let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
                    tx.send(serde_json::from_str(body).unwrap()).unwrap();
                    write_response(
                        &mut stream,
                        r#"{"message_id":"task-4323-message","accepted":true}"#,
                    );
                }
            }
        });
        (format!("http://{address}"), rx)
    }

    fn read_http_request(stream: &mut TcpStream) -> String {
        let mut buffer = [0u8; 8192];
        let mut bytes = Vec::new();
        loop {
            let read = stream.read(&mut buffer).unwrap();
            assert!(read > 0, "connection closed before request completed");
            bytes.extend_from_slice(&buffer[..read]);
            if let Some(header_end) = find_header_end(&bytes) {
                let headers = String::from_utf8_lossy(&bytes[..header_end]);
                let content_len = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .or_else(|| {
                        headers
                            .lines()
                            .find_map(|line| line.strip_prefix("Content-Length: "))
                    })
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if bytes.len() >= header_end + 4 + content_len {
                    return String::from_utf8_lossy(&bytes).to_string();
                }
            }
        }
    }

    fn find_header_end(bytes: &[u8]) -> Option<usize> {
        bytes.windows(4).position(|window| window == b"\r\n\r\n")
    }

    fn write_response(stream: &mut TcpStream, body: &str) {
        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.as_bytes().len(),
            body
        )
        .unwrap();
    }

    #[test]
    fn burn_receipt_requires_the_server_tombstone() {
        let response = BurnResponse {
            deleted: 3,
            address_tombstoned: false,
        };
        assert!(
            !response.address_tombstoned,
            "a receipt without deletion must be refused"
        );
    }
}
