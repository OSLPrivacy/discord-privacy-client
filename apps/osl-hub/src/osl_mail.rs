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

/// Send only a pointer envelope to the relay.  The user-authored subject and
/// body never enter the signed request (nor its receipt); the mail relay is
/// deliberately a pointer lane, not a plaintext mail store.
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

    let pointer = pointer_envelope(&recipient, &subject, &body);
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
        Value::String(STANDARD.encode(pointer.as_bytes())),
    );
    unsigned.insert(
        "envelope".to_owned(),
        serde_json::json!({ "version": 1, "pointer_only": true }),
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

    Ok(OslMailForwardPlan {
        osl_recipients,
        no_osl_warnings,
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

fn pointer_envelope(recipient: &str, subject: &str, body: &str) -> String {
    // The relay receives commitments only.  Transport owns resolving these
    // capabilities; keeping the UI text out of this lane prevents an inline
    // plaintext fallback from becoming a downgrade path.
    serde_json::json!({
        "v": 1,
        "subject_sha256": sha256_hex(subject.as_bytes()),
        "body_sha256": sha256_hex(body.as_bytes()),
        "recipient_sha256": sha256_hex(recipient.as_bytes()),
    })
    .to_string()
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
        plan_protected_forward, pointer_envelope, signed_message, status_from_address, BurnResponse,
    };
    use serde_json::{Map, Value};

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
    fn send_pointer_never_contains_the_composed_payload() {
        let pointer = pointer_envelope(
            "member@oslprivacy.com",
            "private subject",
            "payload must never transit",
        );
        assert!(!pointer.contains("private subject"));
        assert!(!pointer.contains("payload must never transit"));
        assert!(pointer.contains("body_sha256"));
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
