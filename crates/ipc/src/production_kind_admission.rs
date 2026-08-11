//! Fail-closed provider-kind admission for production whitelist discovery.
//!
//! Provider JSON reaches these functions before any kind normalization, rule
//! lookup, allowed-row creation, prompt, notice, or provider action.  The
//! authenticated connector sessions cannot be built from the JSON itself.

use crate::allowed_places::AllowedPlaceRecord;
use crate::commands::cmd_osl_new_place;
use crate::shipping_email::{
    run_shipping_email_action, EmailRuleTarget, ReceivedProviderEmail, ShippingEmailActionReceipt,
    VerifiedShippingEmailAccount,
};
use crate::state::AppState;
use serde_json::{Map, Value};
use std::fmt;
use std::path::Path;

pub const PRODUCTION_KIND_DESERIALIZER: &str = "osl.provider-kind-admission.v1";
pub const DISCORD_DECLARED_KINDS: [&str; 5] = [
    "direct_message",
    "group_chat",
    "server",
    "server_channel",
    "thread",
];
pub const EMAIL_DECLARED_KINDS: [&str; 2] = ["address", "domain"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShippingDiscordConnector {
    pub connector_id: &'static str,
    pub supports_signed_in_discovery: bool,
    pub supports_provider_action: bool,
}

pub const SHIPPING_DISCORD_CONNECTORS: [ShippingDiscordConnector; 1] = [ShippingDiscordConnector {
    connector_id: "native_discord",
    supports_signed_in_discovery: true,
    supports_provider_action: true,
}];

#[derive(Debug, Clone, Copy, Default)]
pub struct ShippingDiscordProviderRegistry;

impl ShippingDiscordProviderRegistry {
    pub const fn authoritative() -> Self {
        Self
    }

    pub fn signed_in_connector(&self, connector_id: &str) -> Option<ShippingDiscordConnector> {
        SHIPPING_DISCORD_CONNECTORS.into_iter().find(|connector| {
            connector.connector_id == connector_id
                && connector.supports_signed_in_discovery
                && connector.supports_provider_action
        })
    }

    pub fn verify_signed_in_account(
        &self,
        connector_id: &str,
        account_id: &str,
        account_binding_sha256: &str,
        connector_revision: &str,
    ) -> Result<VerifiedShippingDiscordAccount, String> {
        let connector = self
            .signed_in_connector(connector_id)
            .ok_or_else(|| format!("OSL: Discord connector '{connector_id}' is not shipping"))?;
        if account_id.is_empty() || account_id != account_id.trim() {
            return Err("OSL: signed-in Discord account id is invalid".to_owned());
        }
        if account_binding_sha256.len() != 64
            || !account_binding_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err("OSL: signed-in Discord account binding is invalid".to_owned());
        }
        validate_revision(connector_revision)?;
        Ok(VerifiedShippingDiscordAccount {
            connector,
            account_id: account_id.to_owned(),
            account_binding_sha256: account_binding_sha256.to_owned(),
            connector_revision: connector_revision.to_owned(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedShippingDiscordAccount {
    connector: ShippingDiscordConnector,
    account_id: String,
    account_binding_sha256: String,
    connector_revision: String,
}

impl VerifiedShippingDiscordAccount {
    pub fn connector_id(&self) -> &'static str {
        self.connector.connector_id
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn account_binding_sha256(&self) -> &str {
        &self.account_binding_sha256
    }

    pub fn connector_revision(&self) -> &str {
        &self.connector_revision
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProductionAdmissionObserver {
    pub deserializer_entries: usize,
    pub refusals: usize,
    pub normalized_kinds: usize,
    pub rule_lookups: usize,
    pub allowed_rows: usize,
    pub pending_rows: usize,
    pub prompts: usize,
    pub notices: usize,
    pub provider_actions: usize,
    pub output_surface_rows: usize,
}

impl ProductionAdmissionObserver {
    pub fn downstream_total(&self) -> usize {
        self.normalized_kinds
            + self.rule_lookups
            + self.allowed_rows
            + self.pending_rows
            + self.prompts
            + self.notices
            + self.provider_actions
            + self.output_surface_rows
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductionKindRefusal {
    pub carrier: &'static str,
    pub raw_kind: String,
    pub reason: &'static str,
}

impl fmt::Display for ProductionKindRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "OSL: {} raw kind {:?} refused at {}: {}",
            self.carrier, self.raw_kind, PRODUCTION_KIND_DESERIALIZER, self.reason
        )
    }
}

impl std::error::Error for ProductionKindRefusal {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductionDiscoveryReceipt {
    pub carrier: &'static str,
    pub raw_kind: String,
    pub normalized_kind: String,
    pub rule_key: String,
    pub rule_choice: String,
    pub outcome: String,
    pub stable_id: String,
}

pub fn admit_discord_provider_discovery(
    state: &AppState,
    app_data_dir: impl AsRef<Path>,
    account: &VerifiedShippingDiscordAccount,
    provider_json: &[u8],
    observer: &mut ProductionAdmissionObserver,
) -> Result<ProductionDiscoveryReceipt, ProductionKindRefusal> {
    observer.deserializer_entries += 1;
    let object = deserialize_object("discord", provider_json, observer)?;
    let raw_kind = require_exact_kind("discord", &object, &DISCORD_DECLARED_KINDS, observer)?;
    require_revision(
        "discord",
        &object,
        &raw_kind,
        account.connector_revision(),
        observer,
    )?;
    require_exact_keys(
        "discord",
        &object,
        &raw_kind,
        &["connectorRevision", "kind", "personName", "providerPlaceId"],
        observer,
    )?;
    let place_id =
        require_payload_string("discord", &object, &raw_kind, "providerPlaceId", observer)?;
    let person_name =
        require_payload_string("discord", &object, &raw_kind, "personName", observer)?;

    observer.normalized_kinds += 1;
    observer.rule_lookups += 1;
    observer.provider_actions += 1;
    let mut place = AllowedPlaceRecord::from_parts(
        "discord",
        account.account_id(),
        &raw_kind,
        format!("discord:{}:{raw_kind}:{place_id}", account.account_id()),
    );
    place.person_name = person_name;
    let decision = cmd_osl_new_place(state, place, Some(app_data_dir.as_ref().to_path_buf()))
        .map_err(|_| {
            refuse(
                "discord",
                raw_kind.clone(),
                "downstream action failed",
                observer,
            )
        })?;
    observe_decision(&decision.status, decision.prompt, observer);
    Ok(ProductionDiscoveryReceipt {
        carrier: "discord",
        raw_kind: raw_kind.clone(),
        normalized_kind: raw_kind,
        rule_key: decision.app_kind,
        rule_choice: decision.rule_choice,
        outcome: decision.status,
        stable_id: decision.stable_id,
    })
}

pub fn admit_email_provider_discovery(
    state: &AppState,
    app_data_dir: impl AsRef<Path>,
    account: &VerifiedShippingEmailAccount,
    provider_json: &[u8],
    observer: &mut ProductionAdmissionObserver,
) -> Result<ProductionDiscoveryReceipt, ProductionKindRefusal> {
    observer.deserializer_entries += 1;
    let object = deserialize_object("email", provider_json, observer)?;
    let raw_kind = require_exact_kind("email", &object, &EMAIL_DECLARED_KINDS, observer)?;
    require_revision(
        "email",
        &object,
        &raw_kind,
        account.connector_revision(),
        observer,
    )?;
    require_exact_keys(
        "email",
        &object,
        &raw_kind,
        &[
            "connectorRevision",
            "kind",
            "providerMessageId",
            "senderHeader",
        ],
        observer,
    )?;
    let provider_message_id =
        require_payload_string("email", &object, &raw_kind, "providerMessageId", observer)?;
    let sender_header =
        require_payload_string("email", &object, &raw_kind, "senderHeader", observer)?;
    let target = match raw_kind.as_str() {
        "address" => EmailRuleTarget::SenderAddress,
        "domain" => EmailRuleTarget::SenderDomain,
        _ => unreachable!("exact registry checked before typed mapping"),
    };

    observer.normalized_kinds += 1;
    observer.rule_lookups += 1;
    observer.provider_actions += 1;
    let action = run_shipping_email_action(
        state,
        app_data_dir,
        account,
        &ReceivedProviderEmail {
            provider_message_id,
            sender_header,
        },
        target,
    )
    .map_err(|_| {
        refuse(
            "email",
            raw_kind.clone(),
            "downstream action failed",
            observer,
        )
    })?;
    observe_email_action(&action, observer);
    Ok(ProductionDiscoveryReceipt {
        carrier: "email",
        normalized_kind: action.target_kind.clone(),
        rule_key: target.whitelist_kind().rule_key().to_owned(),
        raw_kind,
        rule_choice: action.rule_choice,
        outcome: action.outcome,
        stable_id: action.stable_id,
    })
}

fn deserialize_object(
    carrier: &'static str,
    raw: &[u8],
    observer: &mut ProductionAdmissionObserver,
) -> Result<Map<String, Value>, ProductionKindRefusal> {
    let value: Value = serde_json::from_slice(raw).map_err(|_| {
        refuse(
            carrier,
            "<unparseable>".to_owned(),
            "malformed JSON",
            observer,
        )
    })?;
    value.as_object().cloned().ok_or_else(|| {
        refuse(
            carrier,
            "<absent>".to_owned(),
            "provider discovery is not an object",
            observer,
        )
    })
}

fn require_exact_kind(
    carrier: &'static str,
    object: &Map<String, Value>,
    registry: &[&str],
    observer: &mut ProductionAdmissionObserver,
) -> Result<String, ProductionKindRefusal> {
    let raw_kind = match object.get("kind") {
        None => {
            return Err(refuse(
                carrier,
                "<absent>".to_owned(),
                "kind is absent",
                observer,
            ))
        }
        Some(Value::String(value)) => value.clone(),
        Some(value) => {
            return Err(refuse(
                carrier,
                format!("<malformed:{}>", json_type(value)),
                "kind is not a string",
                observer,
            ))
        }
    };
    if !registry.contains(&raw_kind.as_str()) {
        return Err(refuse(
            carrier,
            raw_kind,
            "kind is not in the exact declared registry",
            observer,
        ));
    }
    Ok(raw_kind)
}

fn require_revision(
    carrier: &'static str,
    object: &Map<String, Value>,
    raw_kind: &str,
    expected: &str,
    observer: &mut ProductionAdmissionObserver,
) -> Result<(), ProductionKindRefusal> {
    if object.get("connectorRevision").and_then(Value::as_str) != Some(expected) {
        return Err(refuse(
            carrier,
            raw_kind.to_owned(),
            "connector revision is absent, malformed, or stale",
            observer,
        ));
    }
    Ok(())
}

fn require_exact_keys(
    carrier: &'static str,
    object: &Map<String, Value>,
    raw_kind: &str,
    expected: &[&str],
    observer: &mut ProductionAdmissionObserver,
) -> Result<(), ProductionKindRefusal> {
    if object.len() != expected.len() || !expected.iter().all(|key| object.contains_key(*key)) {
        return Err(refuse(
            carrier,
            raw_kind.to_owned(),
            "provider discovery fields are malformed",
            observer,
        ));
    }
    Ok(())
}

fn require_payload_string(
    carrier: &'static str,
    object: &Map<String, Value>,
    raw_kind: &str,
    key: &str,
    observer: &mut ProductionAdmissionObserver,
) -> Result<String, ProductionKindRefusal> {
    let Some(value) = object.get(key).and_then(Value::as_str) else {
        return Err(refuse(
            carrier,
            raw_kind.to_owned(),
            "provider discovery payload is malformed",
            observer,
        ));
    };
    if value.is_empty() || value != value.trim() || value.len() > 512 || value.contains(['\0', ':'])
    {
        return Err(refuse(
            carrier,
            raw_kind.to_owned(),
            "provider discovery payload is malformed",
            observer,
        ));
    }
    Ok(value.to_owned())
}

fn refuse(
    carrier: &'static str,
    raw_kind: String,
    reason: &'static str,
    observer: &mut ProductionAdmissionObserver,
) -> ProductionKindRefusal {
    observer.refusals += 1;
    ProductionKindRefusal {
        carrier,
        raw_kind,
        reason,
    }
}

fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn observe_decision(status: &str, prompt: bool, observer: &mut ProductionAdmissionObserver) {
    match status {
        "allowed" => observer.allowed_rows += 1,
        "pending_allow_request" => observer.pending_rows += 1,
        _ => {}
    }
    if prompt {
        observer.prompts += 1;
        observer.notices += 1;
    }
    observer.output_surface_rows += 1;
}

fn observe_email_action(
    action: &ShippingEmailActionReceipt,
    observer: &mut ProductionAdmissionObserver,
) {
    if action.outcome == "provider_allowed" {
        observer.allowed_rows += 1;
    }
    observer.output_surface_rows += 1;
}

fn validate_revision(revision: &str) -> Result<(), String> {
    if revision.len() < 16
        || revision.len() > 128
        || revision != revision.trim()
        || !revision
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err("OSL: connector revision is invalid".to_owned());
    }
    Ok(())
}
