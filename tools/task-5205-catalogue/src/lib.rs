use osl_english_catalogue::{
    all_production_keys, production_key_count, registered_locales, EnglishCatalogue,
    PACKAGED_ENGLISH_CATALOGUE, RESOLVER_ID,
};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub mod semantic;

// Compile the exact production entry-point files, rather than checker-only
// copies, without pulling either shipping binary's unrelated dependency graph
// into this focused release check.
#[path = "../../../services/crypto-watcher/src/english_catalogue_entry.rs"]
mod service_production_entry;
#[path = "../../../apps/osl-hub/src/english_catalogue_entry.rs"]
mod windows_production_entry;

pub use service_production_entry::{load_external_service_catalogue, SERVICE_CATALOGUE_CALLER};
pub use windows_production_entry::{load_external_windows_catalogue, WINDOWS_CATALOGUE_CALLER};

/// Independently authored acceptance inventory. It is deliberately not
/// generated from either the packaged JSON or the resolver registry.
pub const INDEPENDENT_PRODUCTION_KEYS: &[&str] = &[
    "welcome.title",
    "welcome.body",
    "welcome.primary_button",
    "common.action.cancel",
    "common.action.close",
    "common.action.continue",
    "windows.catalogue.loaded",
    "service.catalogue.loaded",
    "service.status.ready",
    "dialog.account_delete.confirm",
    "dialog.account_delete.cancel",
    "notification.friend_key_change.windows_toast.title",
    "notification.friend_key_change.windows_toast.body",
    "notification.friend_key_change.windows_toast.action",
    "notification.friend_key_change.in_app_banner.title",
    "notification.friend_key_change.in_app_banner.body",
    "notification.friend_key_change.in_app_banner.action",
    "notification.friend_key_change.notice_history.title",
    "notification.friend_key_change.notice_history.body",
    "notification.friend_key_change.notice_history.action",
    "notification.encrypted_chat_message.windows_toast.title",
    "notification.encrypted_chat_message.windows_toast.body",
    "notification.encrypted_chat_message.windows_toast.action",
    "notification.encrypted_chat_message.in_app_banner.title",
    "notification.encrypted_chat_message.in_app_banner.body",
    "notification.encrypted_chat_message.in_app_banner.action",
    "notification.encrypted_chat_message.notice_history.title",
    "notification.encrypted_chat_message.notice_history.body",
    "notification.encrypted_chat_message.notice_history.action",
    "local.adapter.accessibility_unavailable",
    "local.adapter.authorization_rejected",
    "local.adapter.canary_mismatch",
    "local.adapter.capability_not_granted",
    "local.adapter.composer_ambiguous",
    "local.adapter.composer_not_found",
    "local.adapter.destination_changed",
    "local.adapter.destination_unattested",
    "local.adapter.generation_stale",
    "local.adapter.not_focused",
    "local.adapter.occluded",
    "local.adapter.password_field",
    "local.adapter.platform_unsupported",
    "local.adapter.profile_expired",
    "local.adapter.profile_not_usable",
    "local.adapter.read_incomplete",
    "local.adapter.timeout",
    "local.adapter.transcript_not_found",
    "local.adapter.whatsapp.app_root_ambiguous",
    "local.adapter.whatsapp.app_root_missing",
    "local.adapter.whatsapp.body_candidate_blocked",
    "local.adapter.whatsapp.body_candidate_missing",
    "local.adapter.whatsapp.carrier_row_ambiguous",
    "local.adapter.whatsapp.carrier_row_missing",
    "local.adapter.whatsapp.composer_ambiguous",
    "local.adapter.whatsapp.composer_missing",
    "local.adapter.whatsapp.content_root_ambiguous",
    "local.adapter.whatsapp.content_root_missing",
    "local.adapter.whatsapp.invalid_carrier",
    "local.adapter.whatsapp.transcript_ambiguous",
    "local.adapter.whatsapp.transcript_missing",
    "local.adapter.window_gone",
    "local.command.argument_missing",
    "local.command.argument_unexpected",
    "local.command.integer_invalid",
    "local.command.required_missing",
    "local.command.state_unavailable",
    "local.command.storage_unavailable",
    "local.command.store_missing",
    "local.command.usage",
    "local.command.value_missing",
    "local.security.friend_bundle_invalid",
    "local.security.friend_identity_invalid",
    "local.security.key_change_incomplete",
    "local.security.message_open_refused",
    "local.security.safety_number_mismatch",
    "local.validation.switch_missing",
    "local.validation.switch_mixed",
    "local.validation.switch_unknown",
    "accessibility.verified_scope.limit",
    "service.payment_voucher.active",
    "service.relay.succeeded",
    "service.relay.queued_offline",
    "service.relay.recipient_inbox_full",
    "service.relay.rate_limited",
    "service.relay.failed",
    "service.key_server.succeeded",
    "service.key_server.rate_limited",
    "service.key_server.failed",
    "service.storage.succeeded",
    "service.storage.capacity",
    "service.storage.rate_limited",
    "service.storage.failed",
    "service.storage.upload.unreachable",
    "service.storage.upload.timed_out",
    "service.storage.upload.rate_limited",
    "service.storage.upload.capability_rejected",
    "service.storage.upload.gone",
    "service.storage.upload.too_large",
    "service.storage.upload.unsupported_lifetime",
    "service.storage.upload.server_fault",
    "service.storage.upload.malformed_response",
    "service.storage.upload.local_io",
    "service.storage.upload.refused",
    "service.storage.upload.route_unavailable",
    "service.storage.fetch.unreachable",
    "service.storage.fetch.timed_out",
    "service.storage.fetch.rate_limited",
    "service.storage.fetch.capability_rejected",
    "service.storage.fetch.gone",
    "service.storage.fetch.too_large",
    "service.storage.fetch.unsupported_lifetime",
    "service.storage.fetch.server_fault",
    "service.storage.fetch.malformed_response",
    "service.storage.fetch.local_io",
    "service.storage.fetch.refused",
    "service.storage.fetch.route_unavailable",
    "service.storage.delete.unreachable",
    "service.storage.delete.timed_out",
    "service.storage.delete.rate_limited",
    "service.storage.delete.capability_rejected",
    "service.storage.delete.gone",
    "service.storage.delete.too_large",
    "service.storage.delete.unsupported_lifetime",
    "service.storage.delete.server_fault",
    "service.storage.delete.malformed_response",
    "service.storage.delete.local_io",
    "service.storage.delete.refused",
    "service.storage.delete.route_unavailable",
    "service.payment_voucher.revoked",
    "service.payment_voucher.expired",
    "service.payment_voucher.unknown",
    "service.payment_voucher.unredeemed",
    "service.payment_voucher.already_redeemed",
    "service.payment_voucher.rate_limited",
    "service.payment_voucher.failed",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceReport {
    pub version: String,
    pub packaged_bytes: usize,
    pub production_keys: usize,
    pub second_locale_registrations: usize,
    pub production_entry_points: usize,
    pub invalid_refusals_before_rendering: usize,
    pub changed_values: usize,
    pub changed_production_results: usize,
    pub resolver: String,
}

pub fn run_acceptance() -> Result<AcceptanceReport, String> {
    let windows = windows_production_entry::load_packaged_windows_catalogue()
        .map_err(|error| error.to_string())?;
    let service = service_production_entry::load_packaged_service_catalogue()
        .map_err(|error| error.to_string())?;
    check_inventory(&windows)?;
    check_inventory(&service)?;
    if windows.version() != service.version()
        || windows.locale() != service.locale()
        || windows.resolver_id() != service.resolver_id()
    {
        return Err(
            "shipping Windows and service entry points disagree on catalogue identity".into(),
        );
    }

    let temp = ExternalCopies::create()?;
    let missing = mutate_remove("welcome.title")?;
    let duplicate = mutate_duplicate("welcome.title")?;
    let malformed = mutate_value(
        "windows.catalogue.loaded",
        "Loaded English catalogue {version for {caller}.",
    )?;
    let invalid = [
        ("missing-welcome-title.json", missing, "welcome.title"),
        ("duplicate-welcome-title.json", duplicate, "welcome.title"),
        (
            "malformed-windows-interpolation.json",
            malformed,
            "windows.catalogue.loaded",
        ),
    ];
    let mut invalid_refusals = 0;
    for (name, source, expected_key) in invalid {
        let path = temp.path().join(name);
        fs::write(&path, source).map_err(|error| error.to_string())?;
        let external = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        for (caller, result) in [
            (
                WINDOWS_CATALOGUE_CALLER,
                load_external_windows_catalogue(&external),
            ),
            (
                SERVICE_CATALOGUE_CALLER,
                load_external_service_catalogue(&external),
            ),
        ] {
            let error = result
                .map(|_| ())
                .expect_err("invalid catalogue must refuse");
            if error.key() != expected_key
                || error.caller() != caller
                || error.fallback() != "disabled"
                || !error.to_string().contains("before rendering")
            {
                return Err(format!(
                    "invalid catalogue refusal lost detail: expected key={expected_key} caller={caller}, got {error}"
                ));
            }
            invalid_refusals += 1;
        }
    }

    let baseline = capture_results(&windows)?;
    let mut changed: Value =
        serde_json::from_str(PACKAGED_ENGLISH_CATALOGUE).map_err(|error| error.to_string())?;
    let changed_keys = &INDEPENDENT_PRODUCTION_KEYS[..5];
    let entries = changed["entries"]
        .as_array_mut()
        .ok_or("catalogue entries are not an array")?;
    for key in changed_keys {
        let entry = entries
            .iter_mut()
            .find(|entry| entry["key"].as_str() == Some(key))
            .ok_or_else(|| format!("change target {key} missing"))?;
        let old = entry["value"]
            .as_str()
            .ok_or_else(|| format!("change target {key} has no string value"))?;
        entry["value"] = Value::String(format!("5205 changed: {old}"));
    }
    let changed_path = temp.path().join("five-values-changed.json");
    fs::write(
        &changed_path,
        serde_json::to_string_pretty(&changed).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let external = fs::read_to_string(&changed_path).map_err(|error| error.to_string())?;
    let changed_windows =
        load_external_windows_catalogue(&external).map_err(|error| error.to_string())?;
    let changed_service =
        load_external_service_catalogue(&external).map_err(|error| error.to_string())?;
    check_inventory(&changed_service)?;
    let changed_capture = capture_results(&changed_windows)?;
    let result_changes = baseline
        .iter()
        .zip(&changed_capture)
        .filter(|(before, after)| before != after)
        .count();
    if result_changes != 5 {
        return Err(format!(
            "changing 5 real values changed {result_changes} captured production results"
        ));
    }

    let locale_count = registered_locales().len();
    if locale_count != 1 || registered_locales()[0] != "en-US" {
        return Err(format!(
            "expected exactly English registration, found {:?}",
            registered_locales()
        ));
    }

    temp.discard()?;

    Ok(AcceptanceReport {
        version: windows.version().to_owned(),
        packaged_bytes: PACKAGED_ENGLISH_CATALOGUE.len(),
        production_keys: production_key_count(),
        second_locale_registrations: locale_count - 1,
        production_entry_points: 2,
        invalid_refusals_before_rendering: invalid_refusals,
        changed_values: changed_keys.len(),
        changed_production_results: result_changes,
        resolver: RESOLVER_ID.to_owned(),
    })
}

fn check_inventory(catalogue: &EnglishCatalogue) -> Result<(), String> {
    let mut independent: BTreeSet<_> = INDEPENDENT_PRODUCTION_KEYS.iter().copied().collect();
    independent.extend(catalogue.keys().filter(|key| key.starts_with("screen.")));
    let resolver: BTreeSet<_> = all_production_keys().map(|spec| spec.key).collect();
    let loaded: BTreeSet<_> = catalogue.keys().collect();
    if independent != resolver || independent != loaded {
        return Err(format!(
            "production key inventory mismatch: independent={independent:?} resolver={resolver:?} loaded={loaded:?}"
        ));
    }
    for key in INDEPENDENT_PRODUCTION_KEYS {
        let variables = interpolation_variables(catalogue, key);
        let result = catalogue
            .resolve(key, variables)
            .map_err(|error| error.to_string())?;
        if result.key != *key
            || result.version != catalogue.version()
            || result.resolver != RESOLVER_ID
            || result.fallback.is_some()
        {
            return Err(format!(
                "resolved production result is malformed for key={key}"
            ));
        }
    }
    Ok(())
}

fn capture_results(catalogue: &EnglishCatalogue) -> Result<Vec<String>, String> {
    INDEPENDENT_PRODUCTION_KEYS
        .iter()
        .map(|key| {
            catalogue
                .resolve(key, interpolation_variables(catalogue, key))
                .map(|resolved| resolved.value)
                .map_err(|error| error.to_string())
        })
        .collect()
}

fn interpolation_variables<'a>(
    catalogue: &'a EnglishCatalogue,
    key: &str,
) -> Vec<(&'static str, &'a str)> {
    match key {
        "windows.catalogue.loaded" | "service.catalogue.loaded" => vec![
            ("caller", catalogue.caller()),
            ("version", catalogue.version()),
        ],
        key if key.starts_with("local.adapter.") && !key.starts_with("local.adapter.whatsapp.") => {
            vec![("adapter", "ADAPTER-ORACLE")]
        }
        "local.command.argument_unexpected"
        | "local.command.integer_invalid"
        | "local.command.required_missing"
        | "local.command.value_missing" => vec![("argument", "--ORACLE")],
        "local.validation.switch_missing" | "local.validation.switch_unknown" => {
            vec![("names", "NAMES-ORACLE")]
        }
        "local.validation.switch_mixed" => {
            vec![("missing", "MISSING-ORACLE"), ("unknown", "UNKNOWN-ORACLE")]
        }
        "service.relay.recipient_inbox_full" => vec![("scope", "SCOPE-ORACLE")],
        _ => Vec::new(),
    }
}

fn parsed() -> Result<Value, String> {
    serde_json::from_str(PACKAGED_ENGLISH_CATALOGUE).map_err(|error| error.to_string())
}

fn mutate_remove(key: &str) -> Result<String, String> {
    let mut value = parsed()?;
    let entries = value["entries"]
        .as_array_mut()
        .ok_or("catalogue entries are not an array")?;
    let before = entries.len();
    entries.retain(|entry| entry["key"].as_str() != Some(key));
    if entries.len() + 1 != before {
        return Err(format!("could not remove exactly one key={key}"));
    }
    serde_json::to_string_pretty(&value).map_err(|error| error.to_string())
}

fn mutate_duplicate(key: &str) -> Result<String, String> {
    let mut value = parsed()?;
    let entries = value["entries"]
        .as_array_mut()
        .ok_or("catalogue entries are not an array")?;
    let entry = entries
        .iter()
        .find(|entry| entry["key"].as_str() == Some(key))
        .cloned()
        .ok_or_else(|| format!("duplicate target key={key} missing"))?;
    entries.push(entry);
    serde_json::to_string_pretty(&value).map_err(|error| error.to_string())
}

fn mutate_value(key: &str, replacement: &str) -> Result<String, String> {
    let mut value = parsed()?;
    let entries = value["entries"]
        .as_array_mut()
        .ok_or("catalogue entries are not an array")?;
    let entry = entries
        .iter_mut()
        .find(|entry| entry["key"].as_str() == Some(key))
        .ok_or_else(|| format!("mutation target key={key} missing"))?;
    entry["value"] = Value::String(replacement.to_owned());
    serde_json::to_string_pretty(&value).map_err(|error| error.to_string())
}

struct ExternalCopies(PathBuf);

impl ExternalCopies {
    fn create() -> Result<Self, String> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("osl-task-5205-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn discard(mut self) -> Result<(), String> {
        fs::remove_dir_all(&self.0).map_err(|error| format!("{}: {error}", self.0.display()))?;
        self.0 = PathBuf::new();
        Ok(())
    }
}

impl Drop for ExternalCopies {
    fn drop(&mut self) {
        if self.0.as_os_str().is_empty() {
            return;
        }
        if let Err(error) = fs::remove_dir_all(&self.0) {
            eprintln!(
                "5205 release check could not discard external copies at {}: {error}",
                self.0.display()
            );
        }
    }
}
