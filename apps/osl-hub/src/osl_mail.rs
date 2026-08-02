//! T4-N2's signed OSL Mail provisioning bridge.
//!
//! The server deliberately has no status endpoint.  Availability comes from
//! its public capability document; whether this identity has a mailbox is
//! local state established only after a signed provision response succeeds.

use crate::core_bridge::HubCoreState;
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine as _,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::sync::Mutex;

const MAIL_DOMAIN: &str = "oslprivacy.com";
const RETENTION_SECONDS: u32 = 7 * 24 * 60 * 60;

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
        .insert(identity.user_id, provisioned.address.clone());
    Ok(status_from_address(Some(provisioned.address)))
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
    reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|_| "OSL Mail network client is unavailable".to_owned())
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
        available: true,
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
    use super::{signed_message, status_from_address};
    use serde_json::{Map, Value};

    #[test]
    fn unprovisioned_identity_has_a_valid_empty_mailbox_status() {
        assert_eq!(
            status_from_address(None),
            super::OslMailStatus {
                available: true,
                provisioned: false,
                address: None,
                unread_count: 0,
                retention_seconds: 604_800,
            }
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
}
