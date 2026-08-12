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

mod screen_keys;
pub use screen_keys::SCREEN_PRODUCTION_KEYS;

pub fn all_production_keys() -> impl Iterator<Item = &'static ProductionKey> {
    PRODUCTION_KEYS.iter().chain(SCREEN_PRODUCTION_KEYS.iter())
}

pub fn production_key_count() -> usize {
    PRODUCTION_KEYS.len() + SCREEN_PRODUCTION_KEYS.len()
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
        key: "dialog.account_delete.confirm",
        placeholders: &[],
    },
    ProductionKey {
        key: "dialog.account_delete.cancel",
        placeholders: &[],
    },
    ProductionKey {
        key: "notification.friend_key_change.windows_toast.title",
        placeholders: &[],
    },
    ProductionKey {
        key: "notification.friend_key_change.windows_toast.body",
        placeholders: &[],
    },
    ProductionKey {
        key: "notification.friend_key_change.windows_toast.action",
        placeholders: &[],
    },
    ProductionKey {
        key: "notification.friend_key_change.in_app_banner.title",
        placeholders: &[],
    },
    ProductionKey {
        key: "notification.friend_key_change.in_app_banner.body",
        placeholders: &[],
    },
    ProductionKey {
        key: "notification.friend_key_change.in_app_banner.action",
        placeholders: &[],
    },
    ProductionKey {
        key: "notification.friend_key_change.notice_history.title",
        placeholders: &[],
    },
    ProductionKey {
        key: "notification.friend_key_change.notice_history.body",
        placeholders: &[],
    },
    ProductionKey {
        key: "notification.friend_key_change.notice_history.action",
        placeholders: &[],
    },
    ProductionKey {
        key: "notification.encrypted_chat_message.windows_toast.title",
        placeholders: &[],
    },
    ProductionKey {
        key: "notification.encrypted_chat_message.windows_toast.body",
        placeholders: &[],
    },
    ProductionKey {
        key: "notification.encrypted_chat_message.windows_toast.action",
        placeholders: &[],
    },
    ProductionKey {
        key: "notification.encrypted_chat_message.in_app_banner.title",
        placeholders: &[],
    },
    ProductionKey {
        key: "notification.encrypted_chat_message.in_app_banner.body",
        placeholders: &[],
    },
    ProductionKey {
        key: "notification.encrypted_chat_message.in_app_banner.action",
        placeholders: &[],
    },
    ProductionKey {
        key: "notification.encrypted_chat_message.notice_history.title",
        placeholders: &[],
    },
    ProductionKey {
        key: "notification.encrypted_chat_message.notice_history.body",
        placeholders: &[],
    },
    ProductionKey {
        key: "notification.encrypted_chat_message.notice_history.action",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.adapter.accessibility_unavailable",
        placeholders: &["adapter"],
    },
    ProductionKey {
        key: "local.adapter.authorization_rejected",
        placeholders: &["adapter"],
    },
    ProductionKey {
        key: "local.adapter.canary_mismatch",
        placeholders: &["adapter"],
    },
    ProductionKey {
        key: "local.adapter.capability_not_granted",
        placeholders: &["adapter"],
    },
    ProductionKey {
        key: "local.adapter.composer_ambiguous",
        placeholders: &["adapter"],
    },
    ProductionKey {
        key: "local.adapter.composer_not_found",
        placeholders: &["adapter"],
    },
    ProductionKey {
        key: "local.adapter.destination_changed",
        placeholders: &["adapter"],
    },
    ProductionKey {
        key: "local.adapter.destination_unattested",
        placeholders: &["adapter"],
    },
    ProductionKey {
        key: "local.adapter.generation_stale",
        placeholders: &["adapter"],
    },
    ProductionKey {
        key: "local.adapter.not_focused",
        placeholders: &["adapter"],
    },
    ProductionKey {
        key: "local.adapter.occluded",
        placeholders: &["adapter"],
    },
    ProductionKey {
        key: "local.adapter.password_field",
        placeholders: &["adapter"],
    },
    ProductionKey {
        key: "local.adapter.platform_unsupported",
        placeholders: &["adapter"],
    },
    ProductionKey {
        key: "local.adapter.profile_expired",
        placeholders: &["adapter"],
    },
    ProductionKey {
        key: "local.adapter.profile_not_usable",
        placeholders: &["adapter"],
    },
    ProductionKey {
        key: "local.adapter.read_incomplete",
        placeholders: &["adapter"],
    },
    ProductionKey {
        key: "local.adapter.timeout",
        placeholders: &["adapter"],
    },
    ProductionKey {
        key: "local.adapter.transcript_not_found",
        placeholders: &["adapter"],
    },
    ProductionKey {
        key: "local.adapter.whatsapp.app_root_ambiguous",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.adapter.whatsapp.app_root_missing",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.adapter.whatsapp.body_candidate_blocked",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.adapter.whatsapp.body_candidate_missing",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.adapter.whatsapp.carrier_row_ambiguous",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.adapter.whatsapp.carrier_row_missing",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.adapter.whatsapp.composer_ambiguous",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.adapter.whatsapp.composer_missing",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.adapter.whatsapp.content_root_ambiguous",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.adapter.whatsapp.content_root_missing",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.adapter.whatsapp.invalid_carrier",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.adapter.whatsapp.transcript_ambiguous",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.adapter.whatsapp.transcript_missing",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.adapter.window_gone",
        placeholders: &["adapter"],
    },
    ProductionKey {
        key: "local.command.argument_missing",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.command.argument_unexpected",
        placeholders: &["argument"],
    },
    ProductionKey {
        key: "local.command.integer_invalid",
        placeholders: &["argument"],
    },
    ProductionKey {
        key: "local.command.required_missing",
        placeholders: &["argument"],
    },
    ProductionKey {
        key: "local.command.state_unavailable",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.command.storage_unavailable",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.command.store_missing",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.command.usage",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.command.value_missing",
        placeholders: &["argument"],
    },
    ProductionKey {
        key: "local.security.friend_bundle_invalid",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.security.friend_identity_invalid",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.security.key_change_incomplete",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.security.message_open_refused",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.security.safety_number_mismatch",
        placeholders: &[],
    },
    ProductionKey {
        key: "local.validation.switch_missing",
        placeholders: &["names"],
    },
    ProductionKey {
        key: "local.validation.switch_mixed",
        placeholders: &["missing", "unknown"],
    },
    ProductionKey {
        key: "local.validation.switch_unknown",
        placeholders: &["names"],
    },
    ProductionKey {
        key: "accessibility.verified_scope.limit",
        placeholders: &[],
    },
    ProductionKey {
        key: "service.payment_voucher.active",
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
    ProductionKey {
        key: "release.capability.carrier_list",
        placeholders: &[],
    },
    ProductionKey {
        key: "release.capability.matrix_note",
        placeholders: &[],
    },
    ProductionKey {
        key: "strip.help",
        placeholders: &[],
    },
    ProductionKey {
        key: "notification.help",
        placeholders: &[],
    },
    ProductionKey {
        key: "account.export.independent_copy_warning",
        placeholders: &[],
    },
    ProductionKey {
        key: "account.export.key_storage_warning",
        placeholders: &[],
    },
    ProductionKey {
        key: "recovery.kit.theft_warning",
        placeholders: &[],
    },
    ProductionKey {
        key: "succession.warning.period_successor",
        placeholders: &["period", "successor"],
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

        let specs: BTreeMap<&str, &ProductionKey> =
            all_production_keys().map(|spec| (spec.key, spec)).collect();
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
        for spec in all_production_keys() {
            if !entries.contains_key(spec.key) {
                return Err(CatalogueError::strict(caller, spec.key, "missing key"));
            }
        }
        if entries.len() != production_key_count() {
            return Err(CatalogueError::strict(
                caller,
                "<catalogue>",
                format!(
                    "expected {} production keys, found {}",
                    production_key_count(),
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
        let mut value = template.clone();
        for (name, replacement) in supplied {
            value = value.replace(&format!("{{{name}}}"), &replacement);
        }
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
        assert_eq!(catalogue.keys().count(), production_key_count());
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
