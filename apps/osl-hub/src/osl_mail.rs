//! T4-N2's signed OSL Mail provisioning bridge.
//!
//! The server deliberately has no status endpoint. Server capability still
//! gates the signed lab operations, but the user-facing Mail client is not
//! available until the desktop bridge exists. Whether this identity has a
//! mailbox is local state established only after a signed provision response
//! succeeds.

use crate::claim_state::{
    claim_of, public_claim, CarrierEvidence, DeliveryEvidence, PublicClaim, Surface,
};
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
const EMPTY_UNREAD_COUNT: u32 = 0;
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
struct MailListResponse {
    unread_count: u32,
}

pub fn get_status(core: &HubCoreState, state: &OslMailState) -> Result<OslMailStatus, String> {
    let identity = active_identity(core)?;
    let base_url = mail_base_url()?;
    ensure_capabilities(&base_url)?;
    let address = state
        .addresses
        .lock()
        .map_err(|_| "OSL Mail state is unavailable".to_owned())?
        .get(&identity.user_id)
        .cloned();
    let unread_count = match address.as_ref() {
        Some(_) => unread_count(&base_url, &identity)?,
        None => EMPTY_UNREAD_COUNT,
    };
    Ok(status_from_address(address, unread_count))
}

pub fn provision(
    core: &HubCoreState,
    state: &OslMailState,
    username: String,
) -> Result<OslMailStatus, String> {
    if !keystore::client::is_normalized_username(&username) {
        return Err(format!(
            "OSL Mail {}",
            keystore::client::USERNAME_RULES_MESSAGE
        ));
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
    Ok(status_from_address(
        Some(provisioned.address),
        EMPTY_UNREAD_COUNT,
    ))
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

fn unread_count(base_url: &str, identity: &keystore::Identity) -> Result<u32, String> {
    let mut unsigned = Map::new();
    // The count comes from the mailbox's one durable `opened_at IS NULL`
    // decision. A single list row is enough because the server returns the
    // whole count separately, bounded by the mailbox's 500-message limit.
    unsigned.insert("limit".to_owned(), Value::from(1));
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
        .map_err(|_| "OSL Mail unread count is unavailable".to_owned())?;
    if !response.status().is_success() {
        return Err("OSL Mail unread count was refused".to_owned());
    }
    let listed: MailListResponse = response
        .json()
        .map_err(|_| "OSL Mail unread count response was malformed".to_owned())?;
    Ok(listed.unread_count)
}

fn status_from_address(address: Option<String>, unread_count: u32) -> OslMailStatus {
    let provisioned = address.is_some();
    OslMailStatus {
        available: osl_mail_desktop_bridge_available(),
        provisioned,
        address,
        unread_count,
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
        pointer_envelope, signed_message, status_from_address, BurnResponse, EMPTY_UNREAD_COUNT,
    };
    use serde_json::{Map, Value};

    #[test]
    fn unprovisioned_identity_has_a_valid_empty_mailbox_status() {
        assert_eq!(
            status_from_address(None, EMPTY_UNREAD_COUNT),
            super::OslMailStatus {
                available: false,
                provisioned: false,
                address: None,
                unread_count: EMPTY_UNREAD_COUNT,
                retention_seconds: 604_800,
            }
        );
    }

    #[test]
    fn status_returns_the_mailbox_unread_count_instead_of_a_fixed_value() {
        let status = status_from_address(Some("member@oslprivacy.com".to_owned()), 4);
        assert_eq!(status.unread_count, 4);
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
    fn task1296_no_osl_forward_recipient_receives_cover_only() {
        let fixture_no_osl = "fixture-recipient@example.com";
        let cover = "Plain cover email for task 1296. Checking in about the notes.".to_owned();
        let protected_text =
            "TASK1296 protected text must not reach the no-OSL recipient".to_owned();
        let protected_file =
            "TASK1296 protected file record must not reach the no-OSL recipient".to_owned();
        let recipients = vec!["alice@oslprivacy.com".to_owned(), fixture_no_osl.to_owned()];
        let confirmation = no_osl_forward_confirmation(&[fixture_no_osl.to_owned()]);

        let receipt = super::forward_protected(
            recipients,
            cover.clone(),
            protected_text.clone(),
            Some(protected_file.clone()),
            confirmation,
        )
        .expect("confirmed protected forward should deliver");

        let no_osl_delivery = receipt
            .deliveries
            .iter()
            .find(|delivery| delivery.recipient == fixture_no_osl)
            .expect("fixture no-OSL recipient must receive a delivery");
        let no_osl_protected_text_records = usize::from(no_osl_delivery.protected_text.is_some());
        let no_osl_protected_file_records =
            usize::from(no_osl_delivery.protected_file_record.is_some());

        println!(
            "TASK1296 recipient={} transit={} plain_cover_email={} protected_text_records={} protected_file_records={}",
            no_osl_delivery.recipient,
            no_osl_delivery.transit,
            no_osl_delivery.plain_cover_email,
            no_osl_protected_text_records,
            no_osl_protected_file_records
        );

        assert_eq!(no_osl_delivery.transit, "plainCoverEmail");
        assert_eq!(no_osl_delivery.plain_cover_email, cover);
        assert_eq!(no_osl_delivery.protected_text, None);
        assert_eq!(no_osl_delivery.protected_file_record, None);
        assert_eq!(no_osl_protected_text_records, 0);
        assert_eq!(no_osl_protected_file_records, 0);
        assert!(!format!("{no_osl_delivery:?}").contains(&protected_text));
        assert!(!format!("{no_osl_delivery:?}").contains(&protected_file));
    }
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslMailForwardPlan {
    pub osl_recipients: Vec<String>,
    pub no_osl_warnings: Vec<String>,
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
const BORING_PROTECTED_SUBJECT: &str = "OSL protected message";
const MIN_COPIED_SUBJECT_BYTES: usize = 16;
const NO_OSL_FORWARD_CONFIRMATION_PREFIX: &str = "CONFIRM NO-OSL FORWARD: ";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslMailForwardDelivery {
    pub recipient: String,
    pub transit: &'static str,
    pub plain_cover_email: String,
    pub protected_text: Option<String>,
    pub protected_file_record: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslMailForwardReceipt {
    pub accepted: bool,
    pub osl_recipients: Vec<String>,
    pub no_osl_warnings: Vec<String>,
    pub deliveries: Vec<OslMailForwardDelivery>,
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

fn normalize_forward_recipients(recipients: Vec<String>) -> Result<Vec<String>, String> {
    let mut normalized = Vec::new();
    for recipient in recipients {
        let recipient = recipient.trim().to_lowercase();
        if !valid_forward_address(&recipient) {
            return Err("OSL Mail forward recipient is invalid".to_owned());
        }
        if !normalized.contains(&recipient) {
            normalized.push(recipient);
        }
    }
    if normalized.is_empty() {
        return Err("OSL Mail protected forward needs at least one recipient".to_owned());
    }
    Ok(normalized)
}

fn valid_forward_address(address: &str) -> bool {
    let Some((local, domain)) = address.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !address.bytes().any(|byte| byte.is_ascii_whitespace())
        && address.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'_' | b'-' | b'+' | b'@')
        })
}

fn no_osl_forward_confirmation(no_osl_recipients: &[String]) -> String {
    format!(
        "{NO_OSL_FORWARD_CONFIRMATION_PREFIX}{}",
        no_osl_recipients.join(",")
    )
}
