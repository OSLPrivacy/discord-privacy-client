//! The one shipping catalogue and strict resolver for OSL-owned interface text.
//!
//! Both the Windows desktop and the standalone service embed
//! [`PACKAGED_ENGLISH_CATALOGUE`] and enter through [`EnglishCatalogue::load`].
//! Values are deliberately stored as an array so duplicate JSON keys cannot be
//! collapsed by a map parser before validation sees them.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const CATALOGUE_SCHEMA: &str = "osl.english-catalogue.v1";
pub const CATALOGUE_VERSION: &str = "2026.08.1";
pub const ENGLISH_LOCALE: &str = "en-US";
pub const RESOLVER_ID: &str = "osl.strict-english-catalogue.v1";
pub const PACKAGED_ENGLISH_CATALOGUE: &str = include_str!("../catalogues/en-US.v1.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductionKey {
    pub key: &'static str,
    pub placeholders: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceReasonSpec {
    pub reason_code: &'static str,
    pub catalogue_key: &'static str,
}

/// The only person-facing service result codes accepted by the packaged
/// client. Each code has exactly one catalogue key; services never provide a
/// literal fallback.
pub const SERVICE_REASON_CODES: &[ServiceReasonSpec] = &[
    ServiceReasonSpec {
        reason_code: "relay_succeeded",
        catalogue_key: "service.relay.succeeded",
    },
    ServiceReasonSpec {
        reason_code: "relay_queued_offline",
        catalogue_key: "service.relay.queued_offline",
    },
    ServiceReasonSpec {
        reason_code: "relay_recipient_inbox_full",
        catalogue_key: "service.relay.recipient_inbox_full",
    },
    ServiceReasonSpec {
        reason_code: "relay_rate_limited",
        catalogue_key: "service.relay.rate_limited",
    },
    ServiceReasonSpec {
        reason_code: "relay_failed",
        catalogue_key: "service.relay.failed",
    },
    ServiceReasonSpec {
        reason_code: "key_server_succeeded",
        catalogue_key: "service.key_server.succeeded",
    },
    ServiceReasonSpec {
        reason_code: "key_server_rate_limited",
        catalogue_key: "service.key_server.rate_limited",
    },
    ServiceReasonSpec {
        reason_code: "key_server_failed",
        catalogue_key: "service.key_server.failed",
    },
    ServiceReasonSpec {
        reason_code: "storage_succeeded",
        catalogue_key: "service.storage.succeeded",
    },
    ServiceReasonSpec {
        reason_code: "storage_capacity",
        catalogue_key: "service.storage.capacity",
    },
    ServiceReasonSpec {
        reason_code: "storage_rate_limited",
        catalogue_key: "service.storage.rate_limited",
    },
    ServiceReasonSpec {
        reason_code: "storage_failed",
        catalogue_key: "service.storage.failed",
    },
    ServiceReasonSpec {
        reason_code: "storage_upload_unreachable",
        catalogue_key: "service.storage.upload.unreachable",
    },
    ServiceReasonSpec {
        reason_code: "storage_upload_timed_out",
        catalogue_key: "service.storage.upload.timed_out",
    },
    ServiceReasonSpec {
        reason_code: "storage_upload_rate_limited",
        catalogue_key: "service.storage.upload.rate_limited",
    },
    ServiceReasonSpec {
        reason_code: "storage_upload_capability_rejected",
        catalogue_key: "service.storage.upload.capability_rejected",
    },
    ServiceReasonSpec {
        reason_code: "storage_upload_gone",
        catalogue_key: "service.storage.upload.gone",
    },
    ServiceReasonSpec {
        reason_code: "storage_upload_too_large",
        catalogue_key: "service.storage.upload.too_large",
    },
    ServiceReasonSpec {
        reason_code: "storage_upload_unsupported_lifetime",
        catalogue_key: "service.storage.upload.unsupported_lifetime",
    },
    ServiceReasonSpec {
        reason_code: "storage_upload_server_fault",
        catalogue_key: "service.storage.upload.server_fault",
    },
    ServiceReasonSpec {
        reason_code: "storage_upload_malformed_response",
        catalogue_key: "service.storage.upload.malformed_response",
    },
    ServiceReasonSpec {
        reason_code: "storage_upload_local_io",
        catalogue_key: "service.storage.upload.local_io",
    },
    ServiceReasonSpec {
        reason_code: "storage_upload_refused",
        catalogue_key: "service.storage.upload.refused",
    },
    ServiceReasonSpec {
        reason_code: "storage_upload_route_unavailable",
        catalogue_key: "service.storage.upload.route_unavailable",
    },
    ServiceReasonSpec {
        reason_code: "storage_fetch_unreachable",
        catalogue_key: "service.storage.fetch.unreachable",
    },
    ServiceReasonSpec {
        reason_code: "storage_fetch_timed_out",
        catalogue_key: "service.storage.fetch.timed_out",
    },
    ServiceReasonSpec {
        reason_code: "storage_fetch_rate_limited",
        catalogue_key: "service.storage.fetch.rate_limited",
    },
    ServiceReasonSpec {
        reason_code: "storage_fetch_capability_rejected",
        catalogue_key: "service.storage.fetch.capability_rejected",
    },
    ServiceReasonSpec {
        reason_code: "storage_fetch_gone",
        catalogue_key: "service.storage.fetch.gone",
    },
    ServiceReasonSpec {
        reason_code: "storage_fetch_too_large",
        catalogue_key: "service.storage.fetch.too_large",
    },
    ServiceReasonSpec {
        reason_code: "storage_fetch_unsupported_lifetime",
        catalogue_key: "service.storage.fetch.unsupported_lifetime",
    },
    ServiceReasonSpec {
        reason_code: "storage_fetch_server_fault",
        catalogue_key: "service.storage.fetch.server_fault",
    },
    ServiceReasonSpec {
        reason_code: "storage_fetch_malformed_response",
        catalogue_key: "service.storage.fetch.malformed_response",
    },
    ServiceReasonSpec {
        reason_code: "storage_fetch_local_io",
        catalogue_key: "service.storage.fetch.local_io",
    },
    ServiceReasonSpec {
        reason_code: "storage_fetch_refused",
        catalogue_key: "service.storage.fetch.refused",
    },
    ServiceReasonSpec {
        reason_code: "storage_fetch_route_unavailable",
        catalogue_key: "service.storage.fetch.route_unavailable",
    },
    ServiceReasonSpec {
        reason_code: "storage_delete_unreachable",
        catalogue_key: "service.storage.delete.unreachable",
    },
    ServiceReasonSpec {
        reason_code: "storage_delete_timed_out",
        catalogue_key: "service.storage.delete.timed_out",
    },
    ServiceReasonSpec {
        reason_code: "storage_delete_rate_limited",
        catalogue_key: "service.storage.delete.rate_limited",
    },
    ServiceReasonSpec {
        reason_code: "storage_delete_capability_rejected",
        catalogue_key: "service.storage.delete.capability_rejected",
    },
    ServiceReasonSpec {
        reason_code: "storage_delete_gone",
        catalogue_key: "service.storage.delete.gone",
    },
    ServiceReasonSpec {
        reason_code: "storage_delete_too_large",
        catalogue_key: "service.storage.delete.too_large",
    },
    ServiceReasonSpec {
        reason_code: "storage_delete_unsupported_lifetime",
        catalogue_key: "service.storage.delete.unsupported_lifetime",
    },
    ServiceReasonSpec {
        reason_code: "storage_delete_server_fault",
        catalogue_key: "service.storage.delete.server_fault",
    },
    ServiceReasonSpec {
        reason_code: "storage_delete_malformed_response",
        catalogue_key: "service.storage.delete.malformed_response",
    },
    ServiceReasonSpec {
        reason_code: "storage_delete_local_io",
        catalogue_key: "service.storage.delete.local_io",
    },
    ServiceReasonSpec {
        reason_code: "storage_delete_refused",
        catalogue_key: "service.storage.delete.refused",
    },
    ServiceReasonSpec {
        reason_code: "storage_delete_route_unavailable",
        catalogue_key: "service.storage.delete.route_unavailable",
    },
    ServiceReasonSpec {
        reason_code: "payment_voucher_active",
        catalogue_key: "service.payment_voucher.active",
    },
    ServiceReasonSpec {
        reason_code: "payment_voucher_revoked",
        catalogue_key: "service.payment_voucher.revoked",
    },
    ServiceReasonSpec {
        reason_code: "payment_voucher_expired",
        catalogue_key: "service.payment_voucher.expired",
    },
    ServiceReasonSpec {
        reason_code: "payment_voucher_unknown",
        catalogue_key: "service.payment_voucher.unknown",
    },
    ServiceReasonSpec {
        reason_code: "payment_voucher_unredeemed",
        catalogue_key: "service.payment_voucher.unredeemed",
    },
    ServiceReasonSpec {
        reason_code: "payment_voucher_already_redeemed",
        catalogue_key: "service.payment_voucher.already_redeemed",
    },
    ServiceReasonSpec {
        reason_code: "payment_voucher_rate_limited",
        catalogue_key: "service.payment_voucher.rate_limited",
    },
    ServiceReasonSpec {
        reason_code: "payment_voucher_failed",
        catalogue_key: "service.payment_voucher.failed",
    },
];

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ServiceResultEnvelope {
    pub reason_code: String,
    #[serde(default)]
    pub parameters: BTreeMap<String, String>,
}

/// The production inventory is independent of the JSON entries. A catalogue
/// cannot make its own missing entry disappear by editing the data file.
pub const PRODUCTION_KEYS: &[ProductionKey] = &[
    ProductionKey {
        key: "welcome.title",
        placeholders: &[],
    },
    ProductionKey {
        key: "welcome.body",
        placeholders: &[],
    },
    ProductionKey {
        key: "welcome.primary_button",
        placeholders: &[],
    },
    ProductionKey {
        key: "common.action.cancel",
        placeholders: &[],
    },
    ProductionKey {
        key: "common.action.close",
        placeholders: &[],
    },
    ProductionKey {
        key: "common.action.continue",
        placeholders: &[],
    },
    ProductionKey {
        key: "windows.catalogue.loaded",
        placeholders: &["caller", "version"],
    },
    ProductionKey {
        key: "service.catalogue.loaded",
        placeholders: &["caller", "version"],
    },
    ProductionKey {
        key: "service.status.ready",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.relay.succeeded",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.relay.queued_offline",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.relay.recipient_inbox_full",
        placeholders: &["scope"],
    },
    ProductionKey {
        key: "service.relay.rate_limited",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.relay.failed",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.key_server.succeeded",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.key_server.rate_limited",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.key_server.failed",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.succeeded",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.capacity",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.rate_limited",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.failed",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.upload.unreachable",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.upload.timed_out",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.upload.rate_limited",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.upload.capability_rejected",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.upload.gone",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.upload.too_large",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.upload.unsupported_lifetime",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.upload.server_fault",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.upload.malformed_response",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.upload.local_io",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.upload.refused",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.upload.route_unavailable",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.fetch.unreachable",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.fetch.timed_out",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.fetch.rate_limited",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.fetch.capability_rejected",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.fetch.gone",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.fetch.too_large",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.fetch.unsupported_lifetime",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.fetch.server_fault",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.fetch.malformed_response",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.fetch.local_io",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.fetch.refused",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.fetch.route_unavailable",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.delete.unreachable",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.delete.timed_out",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.delete.rate_limited",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.delete.capability_rejected",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.delete.gone",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.delete.too_large",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.delete.unsupported_lifetime",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.delete.server_fault",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.delete.malformed_response",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.delete.local_io",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.delete.refused",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.storage.delete.route_unavailable",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.payment_voucher.active",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.payment_voucher.revoked",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.payment_voucher.expired",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.payment_voucher.unknown",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.payment_voucher.unredeemed",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.payment_voucher.already_redeemed",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.payment_voucher.rate_limited",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.payment_voucher.failed",
        placeholders: &[],
    },
];

/// Q7 registers English only. This is intentionally a fixed one-element list,
/// not locale negotiation or a language selector.
pub const fn registered_locales() -> [&'static str; 1] {
    [ENGLISH_LOCALE]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogueError {
    caller: String,
    key: String,
    problem: String,
    fallback: String,
}

impl CatalogueError {
    fn strict(caller: &str, key: impl Into<String>, problem: impl Into<String>) -> Self {
        Self {
            caller: caller.to_owned(),
            key: key.into(),
            problem: problem.into(),
            fallback: "disabled".to_owned(),
        }
    }

    pub fn caller(&self) -> &str {
        &self.caller
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn fallback(&self) -> &str {
        &self.fallback
    }
}

impl fmt::Display for CatalogueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "strict English catalogue refused before rendering: caller={} key={} fallback={}: {}",
            self.caller, self.key, self.fallback, self.problem
        )
    }
}

impl std::error::Error for CatalogueError {}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCatalogue {
    schema: String,
    version: String,
    locale: String,
    entries: Vec<RawEntry>,
    /// This field is recognized only so an attempted literal fallback can be
    /// rejected with the exact key and fallback named, rather than as an
    /// unhelpful unknown-field parse error.
    #[serde(default)]
    missing_key_fallbacks: Vec<RawFallback>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    key: String,
    value: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFallback {
    key: String,
    literal: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResolvedString {
    pub version: String,
    pub locale: String,
    pub key: String,
    pub value: String,
    pub caller: String,
    pub resolver: String,
    pub fallback: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EnglishCatalogue {
    caller: String,
    entries: BTreeMap<String, String>,
}

impl EnglishCatalogue {
    pub fn packaged(caller: &str) -> Result<Self, CatalogueError> {
        Self::load(PACKAGED_ENGLISH_CATALOGUE, caller)
    }

    /// Strict production loader. It validates the complete catalogue before it
    /// returns, so no caller can render a partial result.
    pub fn load(source: &str, caller: &str) -> Result<Self, CatalogueError> {
        if !valid_caller(caller) {
            return Err(CatalogueError::strict(
                caller,
                "<catalogue>",
                "caller is empty or malformed",
            ));
        }
        let raw: RawCatalogue = serde_json::from_str(source).map_err(|error| {
            CatalogueError::strict(caller, "<catalogue>", format!("malformed JSON: {error}"))
        })?;
        if raw.schema != CATALOGUE_SCHEMA {
            return Err(CatalogueError::strict(
                caller,
                "<catalogue>",
                format!("schema must be {CATALOGUE_SCHEMA}, got {}", raw.schema),
            ));
        }
        if raw.version != CATALOGUE_VERSION {
            return Err(CatalogueError::strict(
                caller,
                "<catalogue>",
                format!("version must be {CATALOGUE_VERSION}, got {}", raw.version),
            ));
        }
        if raw.locale != ENGLISH_LOCALE {
            return Err(CatalogueError::strict(
                caller,
                "<catalogue>",
                format!("second-locale registration refused: locale={}", raw.locale),
            ));
        }
        if let Some(fallback) = raw.missing_key_fallbacks.first() {
            return Err(CatalogueError {
                caller: caller.to_owned(),
                key: fallback.key.clone(),
                problem: format!(
                    "hard-coded missing-key literal fallback was enabled ({:?})",
                    fallback.literal
                ),
                fallback: "literal".to_owned(),
            });
        }

        let specs: BTreeMap<&str, &ProductionKey> = PRODUCTION_KEYS
            .iter()
            .map(|spec| (spec.key, spec))
            .collect();
        let mut entries = BTreeMap::new();
        for entry in raw.entries {
            if !valid_key(&entry.key) {
                return Err(CatalogueError::strict(
                    caller,
                    &entry.key,
                    "key is malformed; expected stable lower-case dotted segments",
                ));
            }
            let Some(spec) = specs.get(entry.key.as_str()) else {
                return Err(CatalogueError::strict(
                    caller,
                    &entry.key,
                    "key is not registered in the production inventory",
                ));
            };
            if entries.contains_key(&entry.key) {
                return Err(CatalogueError::strict(caller, &entry.key, "duplicate key"));
            }
            if entry.value.is_empty()
                || entry
                    .value
                    .chars()
                    .any(|ch| ch.is_control() && ch != '\n' && ch != '\t')
            {
                return Err(CatalogueError::strict(
                    caller,
                    &entry.key,
                    "value is empty or contains a forbidden control character",
                ));
            }
            let actual = placeholders(&entry.value).map_err(|problem| {
                CatalogueError::strict(
                    caller,
                    &entry.key,
                    format!("malformed interpolation: {problem}"),
                )
            })?;
            let expected: BTreeSet<&str> = spec.placeholders.iter().copied().collect();
            if actual != expected {
                return Err(CatalogueError::strict(
                    caller,
                    &entry.key,
                    format!(
                        "malformed interpolation: expected {:?}, found {:?}",
                        expected, actual
                    ),
                ));
            }
            entries.insert(entry.key, entry.value);
        }
        for spec in PRODUCTION_KEYS {
            if !entries.contains_key(spec.key) {
                return Err(CatalogueError::strict(caller, spec.key, "missing key"));
            }
        }
        if entries.len() != PRODUCTION_KEYS.len() {
            return Err(CatalogueError::strict(
                caller,
                "<catalogue>",
                format!(
                    "expected {} production keys, found {}",
                    PRODUCTION_KEYS.len(),
                    entries.len()
                ),
            ));
        }
        Ok(Self {
            caller: caller.to_owned(),
            entries,
        })
    }

    pub fn version(&self) -> &'static str {
        CATALOGUE_VERSION
    }

    pub fn locale(&self) -> &'static str {
        ENGLISH_LOCALE
    }

    pub fn resolver_id(&self) -> &'static str {
        RESOLVER_ID
    }

    pub fn caller(&self) -> &str {
        &self.caller
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    pub fn resolve<I, K, V>(
        &self,
        key: &str,
        variables: I,
    ) -> Result<ResolvedString, CatalogueError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let Some(template) = self.entries.get(key) else {
            return Err(CatalogueError::strict(&self.caller, key, "missing key"));
        };
        let supplied: BTreeMap<String, String> = variables
            .into_iter()
            .map(|(name, value)| (name.as_ref().to_owned(), value.as_ref().to_owned()))
            .collect();
        let expected = placeholders(template).map_err(|problem| {
            CatalogueError::strict(
                &self.caller,
                key,
                format!("malformed interpolation: {problem}"),
            )
        })?;
        let supplied_names: BTreeSet<&str> = supplied.keys().map(String::as_str).collect();
        if supplied_names != expected {
            return Err(CatalogueError::strict(
                &self.caller,
                key,
                format!(
                    "interpolation arguments do not match: expected {:?}, found {:?}",
                    expected, supplied_names
                ),
            ));
        }
        // Substitute in one pass. Replacement text is opaque: a parameter
        // containing another `{placeholder}` can never trigger a second
        // expansion.
        let value = interpolate_once(template, &supplied).map_err(|problem| {
            CatalogueError::strict(
                &self.caller,
                key,
                format!("malformed interpolation: {problem}"),
            )
        })?;
        Ok(ResolvedString {
            version: CATALOGUE_VERSION.to_owned(),
            locale: ENGLISH_LOCALE.to_owned(),
            key: key.to_owned(),
            value,
            caller: self.caller.clone(),
            resolver: RESOLVER_ID.to_owned(),
            fallback: None,
        })
    }

    /// Resolve one service wire result through its sole catalogue key. Values
    /// are escaped before interpolation so the returned string is safe at an
    /// HTML sink as well as a textContent sink.
    pub fn resolve_service_result(
        &self,
        result: &ServiceResultEnvelope,
    ) -> Result<ResolvedString, CatalogueError> {
        let matches: Vec<&ServiceReasonSpec> = SERVICE_REASON_CODES
            .iter()
            .filter(|spec| spec.reason_code == result.reason_code)
            .collect();
        if matches.len() != 1 {
            return Err(CatalogueError::strict(
                &self.caller,
                &result.reason_code,
                format!(
                    "unknown or ambiguous service reason code; catalogue matches={}",
                    matches.len()
                ),
            ));
        }
        let escaped = result
            .parameters
            .iter()
            .map(|(name, value)| (name.clone(), escape_html(value)))
            .collect::<BTreeMap<_, _>>();
        self.resolve(matches[0].catalogue_key, escaped)
    }
}

fn interpolate_once(template: &str, supplied: &BTreeMap<String, String>) -> Result<String, String> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after
            .find('}')
            .ok_or_else(|| "opening brace has no closing brace".to_owned())?;
        let name = &after[..end];
        let replacement = supplied
            .get(name)
            .ok_or_else(|| format!("missing {{{name}}}"))?;
        out.push_str(replacement);
        rest = &after[end + 1..];
    }
    if rest.contains('}') {
        return Err("closing brace has no opening brace".to_owned());
    }
    out.push_str(rest);
    Ok(out)
}

pub fn escape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn valid_caller(caller: &str) -> bool {
    !caller.is_empty()
        && caller.len() <= 128
        && caller.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && key.split('.').all(|segment| {
            !segment.is_empty()
                && segment.as_bytes()[0].is_ascii_lowercase()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        })
}

fn placeholders(value: &str) -> Result<BTreeSet<&str>, String> {
    let bytes = value.as_bytes();
    let mut found = BTreeSet::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'{' => {
                let start = cursor + 1;
                let Some(relative_end) = bytes[start..].iter().position(|byte| *byte == b'}')
                else {
                    return Err("opening brace has no closing brace".to_owned());
                };
                let end = start + relative_end;
                let name = &value[start..end];
                if name.is_empty()
                    || !name.as_bytes()[0].is_ascii_lowercase()
                    || !name.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                    })
                {
                    return Err(format!("invalid placeholder {{{name}}}"));
                }
                found.insert(name);
                cursor = end + 1;
            }
            b'}' => return Err("closing brace has no opening brace".to_owned()),
            _ => cursor += 1,
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_catalogue_is_strict_and_complete() {
        let catalogue = EnglishCatalogue::packaged("catalogue.unit-test").unwrap();
        assert_eq!(catalogue.version(), CATALOGUE_VERSION);
        assert_eq!(catalogue.locale(), ENGLISH_LOCALE);
        assert_eq!(catalogue.keys().count(), PRODUCTION_KEYS.len());
        assert_eq!(registered_locales(), ["en-US"]);
    }

    #[test]
    fn missing_duplicate_and_malformed_interpolation_are_visible() {
        let missing = PACKAGED_ENGLISH_CATALOGUE.replace(
            "    {\n      \"key\": \"welcome.title\",\n      \"value\": \"Welcome to OSL\"\n    },\n",
            "",
        );
        let error = EnglishCatalogue::load(&missing, "catalogue.unit-test").unwrap_err();
        assert_eq!(error.key(), "welcome.title");
        assert!(error.to_string().contains("fallback=disabled"));

        let needle = "    {\n      \"key\": \"welcome.title\",\n      \"value\": \"Welcome to OSL\"\n    },\n";
        let duplicate =
            PACKAGED_ENGLISH_CATALOGUE.replacen(needle, &format!("{needle}{needle}"), 1);
        let error = EnglishCatalogue::load(&duplicate, "catalogue.unit-test").unwrap_err();
        assert_eq!(error.key(), "welcome.title");
        assert!(error.to_string().contains("duplicate key"));

        let malformed = PACKAGED_ENGLISH_CATALOGUE.replace("{version}", "{version");
        let error = EnglishCatalogue::load(&malformed, "catalogue.unit-test").unwrap_err();
        assert_eq!(error.key(), "windows.catalogue.loaded");
        assert!(error.to_string().contains("malformed interpolation"));
    }
}
