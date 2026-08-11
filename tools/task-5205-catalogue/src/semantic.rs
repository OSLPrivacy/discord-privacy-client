//! Independently authored, compiled semantic oracle for the 5205b gate.
//!
//! This module is deliberately separate from the production catalogue JSON.
//! Candidate bytes can never add, remove, or rewrite these contracts.

use crate::{
    load_external_service_catalogue, load_external_windows_catalogue, INDEPENDENT_PRODUCTION_KEYS,
    SERVICE_CATALOGUE_CALLER, WINDOWS_CATALOGUE_CALLER,
};
use osl_english_catalogue::{
    CatalogueError, EnglishCatalogue, PACKAGED_ENGLISH_CATALOGUE, PRODUCTION_KEYS,
};
use std::collections::{BTreeMap, BTreeSet};

pub const FIXED_ORACLE_ID: &str = "osl.english-semantics.5205b.v1";
pub const TASK_5212_LIMIT: &str = "Keyboard and screen-reader operation is verified only for sending a message and changing a two-state setting; other controls may require a pointer.";
const STRUCTURAL_ONLY_KEYS: &[&str] = &[
    "welcome.title",
    "welcome.body",
    "welcome.primary_button",
    "common.action.cancel",
    "common.action.close",
    "common.action.continue",
    "windows.catalogue.loaded",
    "service.catalogue.loaded",
    "service.status.ready",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticContract {
    pub task: u16,
    pub semantic_owner: &'static str,
    pub key: &'static str,
    pub meaning_id: &'static str,
    pub template: &'static str,
    pub parameters: &'static [&'static str],
    pub severity: &'static str,
    pub disposition: &'static str,
    pub equivalence: &'static str,
}

macro_rules! contract {
    ($task:literal, $owner:literal, $key:literal, $meaning:literal, $template:expr, $parameters:expr, $severity:literal, $disposition:literal) => {
        SemanticContract {
            task: $task,
            semantic_owner: $owner,
            key: $key,
            meaning_id: $meaning,
            template: $template,
            parameters: $parameters,
            severity: $severity,
            disposition: $disposition,
            equivalence: $meaning,
        }
    };
}

/// Fixed by review, not generated from `en-US.v1.json` or `PRODUCTION_KEYS`.
pub const SEMANTIC_CONTRACTS: &[SemanticContract] = &[
    contract!(
        5209,
        "windows.dialog.account-delete",
        "dialog.account_delete.confirm",
        "5209.account-delete.confirm",
        "Delete account",
        &[],
        "destructive",
        "confirm"
    ),
    contract!(
        5209,
        "windows.dialog.account-delete",
        "dialog.account_delete.cancel",
        "5209.account-delete.cancel",
        "Keep account",
        &[],
        "protective",
        "cancel"
    ),
    contract!(
        5210,
        "windows.desktop.local_notifications.construct_windows_toast",
        "notification.friend_key_change.windows_toast.title",
        "5210.friend-key-change.title",
        "Friend encryption key changed",
        &[],
        "security-critical",
        "review-and-verify"
    ),
    contract!(
        5210,
        "windows.desktop.local_notifications.construct_windows_toast",
        "notification.friend_key_change.windows_toast.body",
        "5210.friend-key-change.body",
        "Verify the new safety number outside this chat before allowing encrypted messages.",
        &[],
        "security-critical",
        "review-and-verify"
    ),
    contract!(
        5210,
        "windows.desktop.local_notifications.construct_windows_toast",
        "notification.friend_key_change.windows_toast.action",
        "5210.friend-key-change.action",
        "Review",
        &[],
        "security-critical",
        "review-and-verify"
    ),
    contract!(
        5210,
        "windows.desktop.local_notifications.construct_in_app_banner",
        "notification.friend_key_change.in_app_banner.title",
        "5210.friend-key-change.title",
        "Friend encryption key changed",
        &[],
        "security-critical",
        "review-and-verify"
    ),
    contract!(
        5210,
        "windows.desktop.local_notifications.construct_in_app_banner",
        "notification.friend_key_change.in_app_banner.body",
        "5210.friend-key-change.body",
        "Verify the new safety number outside this chat before allowing encrypted messages.",
        &[],
        "security-critical",
        "review-and-verify"
    ),
    contract!(
        5210,
        "windows.desktop.local_notifications.construct_in_app_banner",
        "notification.friend_key_change.in_app_banner.action",
        "5210.friend-key-change.action",
        "Review",
        &[],
        "security-critical",
        "review-and-verify"
    ),
    contract!(
        5210,
        "windows.desktop.local_notifications.construct_notice_history",
        "notification.friend_key_change.notice_history.title",
        "5210.friend-key-change.title",
        "Friend encryption key changed",
        &[],
        "security-critical",
        "review-and-verify"
    ),
    contract!(
        5210,
        "windows.desktop.local_notifications.construct_notice_history",
        "notification.friend_key_change.notice_history.body",
        "5210.friend-key-change.body",
        "Verify the new safety number outside this chat before allowing encrypted messages.",
        &[],
        "security-critical",
        "review-and-verify"
    ),
    contract!(
        5210,
        "windows.desktop.local_notifications.construct_notice_history",
        "notification.friend_key_change.notice_history.action",
        "5210.friend-key-change.action",
        "Review",
        &[],
        "security-critical",
        "review-and-verify"
    ),
    contract!(
        5210,
        "windows.desktop.local_notifications.construct_windows_toast",
        "notification.encrypted_chat_message.windows_toast.title",
        "5210.encrypted-chat.title",
        "OSL Chat",
        &[],
        "informational",
        "open"
    ),
    contract!(
        5210,
        "windows.desktop.local_notifications.construct_windows_toast",
        "notification.encrypted_chat_message.windows_toast.body",
        "5210.encrypted-chat.body",
        "New encrypted message",
        &[],
        "informational",
        "open"
    ),
    contract!(
        5210,
        "windows.desktop.local_notifications.construct_windows_toast",
        "notification.encrypted_chat_message.windows_toast.action",
        "5210.encrypted-chat.action",
        "Open",
        &[],
        "informational",
        "open"
    ),
    contract!(
        5210,
        "windows.desktop.local_notifications.construct_in_app_banner",
        "notification.encrypted_chat_message.in_app_banner.title",
        "5210.encrypted-chat.title",
        "OSL Chat",
        &[],
        "informational",
        "open"
    ),
    contract!(
        5210,
        "windows.desktop.local_notifications.construct_in_app_banner",
        "notification.encrypted_chat_message.in_app_banner.body",
        "5210.encrypted-chat.body",
        "New encrypted message",
        &[],
        "informational",
        "open"
    ),
    contract!(
        5210,
        "windows.desktop.local_notifications.construct_in_app_banner",
        "notification.encrypted_chat_message.in_app_banner.action",
        "5210.encrypted-chat.action",
        "Open",
        &[],
        "informational",
        "open"
    ),
    contract!(
        5210,
        "windows.desktop.local_notifications.construct_notice_history",
        "notification.encrypted_chat_message.notice_history.title",
        "5210.encrypted-chat.title",
        "OSL Chat",
        &[],
        "informational",
        "open"
    ),
    contract!(
        5210,
        "windows.desktop.local_notifications.construct_notice_history",
        "notification.encrypted_chat_message.notice_history.body",
        "5210.encrypted-chat.body",
        "New encrypted message",
        &[],
        "informational",
        "open"
    ),
    contract!(
        5210,
        "windows.desktop.local_notifications.construct_notice_history",
        "notification.encrypted_chat_message.notice_history.action",
        "5210.encrypted-chat.action",
        "Open",
        &[],
        "informational",
        "open"
    ),
    contract!(5211, "windows.local-results.adapter", "local.adapter.accessibility_unavailable", "5211.adapter.accessibility-unavailable", "{adapter} accessibility support is unavailable.", &["adapter"], "operational-error", "refuse"),
    contract!(5211, "windows.local-results.adapter", "local.adapter.authorization_rejected", "5211.adapter.authorization-rejected", "{adapter} rejected the requested authorization.", &["adapter"], "security-critical", "refuse"),
    contract!(5211, "windows.local-results.adapter", "local.adapter.canary_mismatch", "5211.adapter.canary-mismatch", "{adapter} no longer matches its verified application.", &["adapter"], "security-critical", "refuse"),
    contract!(5211, "windows.local-results.adapter", "local.adapter.capability_not_granted", "5211.adapter.capability-not-granted", "{adapter} does not have the required capability.", &["adapter"], "security-critical", "refuse"),
    contract!(5211, "windows.local-results.adapter", "local.adapter.composer_ambiguous", "5211.adapter.composer-ambiguous", "More than one {adapter} message box was found.", &["adapter"], "safety-blocking", "refuse"),
    contract!(5211, "windows.local-results.adapter", "local.adapter.composer_not_found", "5211.adapter.composer-not-found", "The {adapter} message box was not found.", &["adapter"], "safety-blocking", "refuse"),
    contract!(5211, "windows.local-results.adapter", "local.adapter.destination_changed", "5211.adapter.destination-changed", "The selected {adapter} destination changed.", &["adapter"], "security-critical", "refuse"),
    contract!(5211, "windows.local-results.adapter", "local.adapter.destination_unattested", "5211.adapter.destination-unattested", "The {adapter} destination could not be verified.", &["adapter"], "security-critical", "refuse"),
    contract!(5211, "windows.local-results.adapter", "local.adapter.generation_stale", "5211.adapter.generation-stale", "The observed {adapter} window is out of date.", &["adapter"], "safety-blocking", "refuse"),
    contract!(5211, "windows.local-results.adapter", "local.adapter.not_focused", "5211.adapter.not-focused", "{adapter} is not the focused application.", &["adapter"], "safety-blocking", "refuse"),
    contract!(5211, "windows.local-results.adapter", "local.adapter.occluded", "5211.adapter.occluded", "The required {adapter} controls are covered.", &["adapter"], "safety-blocking", "refuse"),
    contract!(5211, "windows.local-results.adapter", "local.adapter.password_field", "5211.adapter.password-field", "The selected {adapter} field is a password field.", &["adapter"], "security-critical", "refuse"),
    contract!(5211, "windows.local-results.adapter", "local.adapter.platform_unsupported", "5211.adapter.platform-unsupported", "This platform does not support the {adapter} adapter.", &["adapter"], "operational-error", "refuse"),
    contract!(5211, "windows.local-results.adapter", "local.adapter.profile_expired", "5211.adapter.profile-expired", "The {adapter} verification profile has expired.", &["adapter"], "security-critical", "refuse"),
    contract!(5211, "windows.local-results.adapter", "local.adapter.profile_not_usable", "5211.adapter.profile-not-usable", "The {adapter} verification profile cannot be used.", &["adapter"], "security-critical", "refuse"),
    contract!(5211, "windows.local-results.adapter", "local.adapter.read_incomplete", "5211.adapter.read-incomplete", "The {adapter} conversation could not be read completely.", &["adapter"], "safety-blocking", "refuse"),
    contract!(5211, "windows.local-results.adapter", "local.adapter.timeout", "5211.adapter.timeout", "{adapter} did not become ready in time.", &["adapter"], "operational-error", "refuse"),
    contract!(5211, "windows.local-results.adapter", "local.adapter.transcript_not_found", "5211.adapter.transcript-not-found", "The {adapter} conversation was not found.", &["adapter"], "safety-blocking", "refuse"),
    contract!(5211, "windows.local-results.adapter.whatsapp", "local.adapter.whatsapp.app_root_ambiguous", "5211.adapter.whatsapp.app-root-ambiguous", "More than one verified WhatsApp application window was found.", &[], "safety-blocking", "refuse"),
    contract!(5211, "windows.local-results.adapter.whatsapp", "local.adapter.whatsapp.app_root_missing", "5211.adapter.whatsapp.app-root-missing", "A verified WhatsApp application window was not found.", &[], "safety-blocking", "refuse"),
    contract!(5211, "windows.local-results.adapter.whatsapp", "local.adapter.whatsapp.body_candidate_blocked", "5211.adapter.whatsapp.body-candidate-blocked", "The WhatsApp message row contains unsupported content.", &[], "security-critical", "refuse"),
    contract!(5211, "windows.local-results.adapter.whatsapp", "local.adapter.whatsapp.body_candidate_missing", "5211.adapter.whatsapp.body-candidate-missing", "The exact WhatsApp message body was not found.", &[], "safety-blocking", "refuse"),
    contract!(5211, "windows.local-results.adapter.whatsapp", "local.adapter.whatsapp.carrier_row_ambiguous", "5211.adapter.whatsapp.carrier-row-ambiguous", "More than one matching WhatsApp message row was found.", &[], "safety-blocking", "refuse"),
    contract!(5211, "windows.local-results.adapter.whatsapp", "local.adapter.whatsapp.carrier_row_missing", "5211.adapter.whatsapp.carrier-row-missing", "The matching WhatsApp message row was not found.", &[], "safety-blocking", "refuse"),
    contract!(5211, "windows.local-results.adapter.whatsapp", "local.adapter.whatsapp.composer_ambiguous", "5211.adapter.whatsapp.composer-ambiguous", "More than one WhatsApp message box was found.", &[], "safety-blocking", "refuse"),
    contract!(5211, "windows.local-results.adapter.whatsapp", "local.adapter.whatsapp.composer_missing", "5211.adapter.whatsapp.composer-missing", "The WhatsApp message box was not found.", &[], "safety-blocking", "refuse"),
    contract!(5211, "windows.local-results.adapter.whatsapp", "local.adapter.whatsapp.content_root_ambiguous", "5211.adapter.whatsapp.content-root-ambiguous", "More than one verified WhatsApp content area was found.", &[], "safety-blocking", "refuse"),
    contract!(5211, "windows.local-results.adapter.whatsapp", "local.adapter.whatsapp.content_root_missing", "5211.adapter.whatsapp.content-root-missing", "A verified WhatsApp content area was not found.", &[], "safety-blocking", "refuse"),
    contract!(5211, "windows.local-results.adapter.whatsapp", "local.adapter.whatsapp.invalid_carrier", "5211.adapter.whatsapp.invalid-carrier", "The WhatsApp carrier is invalid.", &[], "security-critical", "refuse"),
    contract!(5211, "windows.local-results.adapter.whatsapp", "local.adapter.whatsapp.transcript_ambiguous", "5211.adapter.whatsapp.transcript-ambiguous", "More than one WhatsApp conversation area was found.", &[], "safety-blocking", "refuse"),
    contract!(5211, "windows.local-results.adapter.whatsapp", "local.adapter.whatsapp.transcript_missing", "5211.adapter.whatsapp.transcript-missing", "The WhatsApp conversation area was not found.", &[], "safety-blocking", "refuse"),
    contract!(5211, "windows.local-results.adapter", "local.adapter.window_gone", "5211.adapter.window-gone", "The {adapter} window is no longer available.", &["adapter"], "operational-error", "refuse"),
    contract!(5211, "windows.allowed-place-cli", "local.command.argument_missing", "5211.command.argument-missing", "An allowed-place command argument is missing.", &[], "input-error", "correct-input"),
    contract!(5211, "windows.allowed-place-cli", "local.command.argument_unexpected", "5211.command.argument-unexpected", "The argument {argument} is not allowed here.", &["argument"], "input-error", "correct-input"),
    contract!(5211, "windows.allowed-place-cli", "local.command.integer_invalid", "5211.command.integer-invalid", "The value for {argument} must be a whole number.", &["argument"], "input-error", "correct-input"),
    contract!(5211, "windows.allowed-place-cli", "local.command.required_missing", "5211.command.required-missing", "The required argument {argument} is missing.", &["argument"], "input-error", "correct-input"),
    contract!(5211, "windows.allowed-place-cli", "local.command.state_unavailable", "5211.command.state-unavailable", "The allowed-place command state is unavailable.", &[], "operational-error", "refuse"),
    contract!(5211, "windows.allowed-place-cli", "local.command.storage_unavailable", "5211.command.storage-unavailable", "Allowed-place storage is unavailable.", &[], "operational-error", "refuse"),
    contract!(5211, "windows.allowed-place-cli", "local.command.store_missing", "5211.command.store-missing", "The required storage directory is missing.", &[], "operational-error", "refuse"),
    contract!(5211, "windows.allowed-place-cli", "local.command.usage", "5211.command.usage", "Use an allowed-place command with its required arguments.", &[], "input-error", "correct-input"),
    contract!(5211, "windows.allowed-place-cli", "local.command.value_missing", "5211.command.value-missing", "The argument {argument} requires a value.", &["argument"], "input-error", "correct-input"),
    contract!(5211, "windows.local-results.security", "local.security.friend_bundle_invalid", "5211.security.friend-bundle-invalid", "The friend key bundle is invalid.", &[], "security-critical", "refuse"),
    contract!(5211, "windows.local-results.security", "local.security.friend_identity_invalid", "5211.security.friend-identity-invalid", "A Discord identifier cannot be used as this friend identity.", &[], "security-critical", "refuse"),
    contract!(5211, "windows.local-results.security", "local.security.key_change_incomplete", "5211.security.key-change-incomplete", "The friend key change is incomplete.", &[], "security-critical", "refuse"),
    contract!(5211, "windows.local-results.security", "local.security.message_open_refused", "5211.security.message-open-refused", "This encrypted message could not be opened.", &[], "security-critical", "refuse"),
    contract!(5211, "windows.local-results.security", "local.security.safety_number_mismatch", "5211.security.safety-number-mismatch", "The safety number does not match.", &[], "security-critical", "refuse"),
    contract!(5211, "windows.runtime-validation", "local.validation.switch_missing", "5211.validation.switch-missing", "Required runtime switches are missing: {names}.", &["names"], "input-error", "correct-input"),
    contract!(5211, "windows.runtime-validation", "local.validation.switch_mixed", "5211.validation.switch-mixed", "Runtime switches are missing ({missing}) and unknown ({unknown}).", &["missing", "unknown"], "input-error", "correct-input"),
    contract!(5211, "windows.runtime-validation", "local.validation.switch_unknown", "5211.validation.switch-unknown", "Unknown runtime switches were supplied: {names}.", &["names"], "input-error", "correct-input"),
    contract!(
        5212,
        "windows.accessibility.release-limit",
        "accessibility.verified_scope.limit",
        "5212.accessibility.verified-scope-limit",
        TASK_5212_LIMIT,
        &[],
        "release-limit",
        "disclose"
    ),
    contract!(5213, "windows.service-result.relay", "service.relay.succeeded", "5213.relay.succeeded", "Sent privately.", &[], "delivery-success", "deliver"),
    contract!(5213, "windows.service-result.relay", "service.relay.queued_offline", "5213.relay.queued-offline", "OSL could not reach the key server. This encrypted message is saved and will finish sending by itself when you are back online.", &[], "delivery-pending", "queue"),
    contract!(5213, "windows.service-result.relay", "service.relay.recipient_inbox_full", "5213.relay.recipient-inbox-full", "That person cannot receive this yet ({scope}).", &["scope"], "warning", "retry-later"),
    contract!(5213, "windows.service-result.relay", "service.relay.rate_limited", "5213.relay.rate-limited", "Too many attempts. Try again shortly.", &[], "warning", "retry-later"),
    contract!(5213, "windows.service-result.relay", "service.relay.failed", "5213.relay.failed", "The private relay could not complete that request.", &[], "delivery-error", "refuse"),
    contract!(5213, "windows.service-result.key-server", "service.key_server.succeeded", "5213.key-server.succeeded", "Account keys are ready.", &[], "security-success", "grant"),
    contract!(5213, "windows.service-result.key-server", "service.key_server.rate_limited", "5213.key-server.rate-limited", "Too many account-key requests. Try again shortly.", &[], "warning", "retry-later"),
    contract!(5213, "windows.service-result.key-server", "service.key_server.failed", "5213.key-server.failed", "The account-key service could not complete that request.", &[], "security-critical", "refuse"),
    contract!(5213, "windows.service-result.storage", "service.storage.succeeded", "5213.storage.succeeded", "Encrypted storage completed the request.", &[], "storage-success", "complete"),
    contract!(5213, "windows.service-result.storage", "service.storage.capacity", "5213.storage.capacity", "Encrypted storage is temporarily full.", &[], "warning", "retry-later"),
    contract!(5213, "windows.service-result.storage", "service.storage.rate_limited", "5213.storage.rate-limited", "Too many storage attempts. Try again shortly.", &[], "warning", "retry-later"),
    contract!(5213, "windows.service-result.storage", "service.storage.failed", "5213.storage.failed", "Encrypted storage could not complete that request.", &[], "storage-error", "refuse"),
    contract!(5213, "windows.service-result.payment", "service.payment_voucher.active", "5213.payment-voucher.active", "Your activation code is active.", &[], "payment-success", "grant"),
    contract!(5213, "windows.service-result.storage.upload", "service.storage.upload.unreachable", "5213.storage.upload.unreachable", "This private attachment could not be uploaded: OSL could not reach its encrypted attachment storage.", &[], "warning", "retry-later"),
    contract!(5213, "windows.service-result.storage.upload", "service.storage.upload.timed_out", "5213.storage.upload.timed-out", "This private attachment could not be uploaded: the encrypted attachment storage did not answer in time.", &[], "warning", "retry-later"),
    contract!(5213, "windows.service-result.storage.upload", "service.storage.upload.rate_limited", "5213.storage.upload.rate-limited", "This private attachment could not be uploaded: the encrypted attachment storage is rate limiting this device, so wait and retry.", &[], "warning", "retry-later"),
    contract!(5213, "windows.service-result.storage.upload", "service.storage.upload.capability_rejected", "5213.storage.upload.capability-rejected", "This private attachment could not be uploaded: the encrypted attachment storage rejected this device's capability for it.", &[], "security-critical", "refuse"),
    contract!(5213, "windows.service-result.storage.upload", "service.storage.upload.gone", "5213.storage.upload.gone", "This private attachment could not be uploaded: it has already expired or been burned.", &[], "storage-terminal", "stop"),
    contract!(5213, "windows.service-result.storage.upload", "service.storage.upload.too_large", "5213.storage.upload.too-large", "This private attachment could not be uploaded: it exceeds the encrypted attachment size limit.", &[], "input-error", "reduce-size"),
    contract!(5213, "windows.service-result.storage.upload", "service.storage.upload.unsupported_lifetime", "5213.storage.upload.unsupported-lifetime", "This private attachment could not be uploaded: its requested lifetime is not one OSL storage accepts.", &[], "input-error", "change-lifetime"),
    contract!(5213, "windows.service-result.storage.upload", "service.storage.upload.server_fault", "5213.storage.upload.server-fault", "This private attachment could not be uploaded: the encrypted attachment storage reported a fault on its side.", &[], "warning", "retry-later"),
    contract!(5213, "windows.service-result.storage.upload", "service.storage.upload.malformed_response", "5213.storage.upload.malformed-response", "This private attachment could not be uploaded: the encrypted attachment storage returned an unexpected response.", &[], "storage-error", "refuse"),
    contract!(5213, "windows.service-result.storage.upload", "service.storage.upload.local_io", "5213.storage.upload.local-io", "This private attachment could not be uploaded: OSL could not read the sealed copy on this device.", &[], "local-error", "refuse"),
    contract!(5213, "windows.service-result.storage.upload", "service.storage.upload.refused", "5213.storage.upload.refused", "This private attachment could not be uploaded: the encrypted attachment storage refused the request.", &[], "security-critical", "refuse"),
    contract!(5213, "windows.service-result.storage.upload", "service.storage.upload.route_unavailable", "5213.storage.upload.route-unavailable", "This private attachment could not be uploaded: Tor is selected and its tunnel is unavailable, so OSL refused rather than using a direct connection.", &[], "privacy-critical", "refuse"),
    contract!(5213, "windows.service-result.storage.fetch", "service.storage.fetch.unreachable", "5213.storage.fetch.unreachable", "This private attachment could not be retrieved: OSL could not reach its encrypted attachment storage.", &[], "warning", "retry-later"),
    contract!(5213, "windows.service-result.storage.fetch", "service.storage.fetch.timed_out", "5213.storage.fetch.timed-out", "This private attachment could not be retrieved: the encrypted attachment storage did not answer in time.", &[], "warning", "retry-later"),
    contract!(5213, "windows.service-result.storage.fetch", "service.storage.fetch.rate_limited", "5213.storage.fetch.rate-limited", "This private attachment could not be retrieved: the encrypted attachment storage is rate limiting this device, so wait and retry.", &[], "warning", "retry-later"),
    contract!(5213, "windows.service-result.storage.fetch", "service.storage.fetch.capability_rejected", "5213.storage.fetch.capability-rejected", "This private attachment could not be retrieved: the encrypted attachment storage rejected this device's capability for it.", &[], "security-critical", "refuse"),
    contract!(5213, "windows.service-result.storage.fetch", "service.storage.fetch.gone", "5213.storage.fetch.gone", "This private attachment could not be retrieved: it has already expired or been burned.", &[], "storage-terminal", "stop"),
    contract!(5213, "windows.service-result.storage.fetch", "service.storage.fetch.too_large", "5213.storage.fetch.too-large", "This private attachment could not be retrieved: it exceeds the encrypted attachment size limit.", &[], "input-error", "refuse"),
    contract!(5213, "windows.service-result.storage.fetch", "service.storage.fetch.unsupported_lifetime", "5213.storage.fetch.unsupported-lifetime", "This private attachment could not be retrieved: its requested lifetime is not one OSL storage accepts.", &[], "input-error", "refuse"),
    contract!(5213, "windows.service-result.storage.fetch", "service.storage.fetch.server_fault", "5213.storage.fetch.server-fault", "This private attachment could not be retrieved: the encrypted attachment storage reported a fault on its side.", &[], "warning", "retry-later"),
    contract!(5213, "windows.service-result.storage.fetch", "service.storage.fetch.malformed_response", "5213.storage.fetch.malformed-response", "This private attachment could not be retrieved: the encrypted attachment storage returned an unexpected response.", &[], "storage-error", "refuse"),
    contract!(5213, "windows.service-result.storage.fetch", "service.storage.fetch.local_io", "5213.storage.fetch.local-io", "This private attachment could not be retrieved: OSL could not read the sealed copy on this device.", &[], "local-error", "refuse"),
    contract!(5213, "windows.service-result.storage.fetch", "service.storage.fetch.refused", "5213.storage.fetch.refused", "This private attachment could not be retrieved: the encrypted attachment storage refused the request.", &[], "security-critical", "refuse"),
    contract!(5213, "windows.service-result.storage.fetch", "service.storage.fetch.route_unavailable", "5213.storage.fetch.route-unavailable", "This private attachment could not be retrieved: Tor is selected and its tunnel is unavailable, so OSL refused rather than using a direct connection.", &[], "privacy-critical", "refuse"),
    contract!(5213, "windows.service-result.storage.delete", "service.storage.delete.unreachable", "5213.storage.delete.unreachable", "OSL could not delete this private attachment from its storage: OSL could not reach its encrypted attachment storage.", &[], "warning", "retry-later"),
    contract!(5213, "windows.service-result.storage.delete", "service.storage.delete.timed_out", "5213.storage.delete.timed-out", "OSL could not delete this private attachment from its storage: the encrypted attachment storage did not answer in time.", &[], "warning", "retry-later"),
    contract!(5213, "windows.service-result.storage.delete", "service.storage.delete.rate_limited", "5213.storage.delete.rate-limited", "OSL could not delete this private attachment from its storage: the encrypted attachment storage is rate limiting this device, so wait and retry.", &[], "warning", "retry-later"),
    contract!(5213, "windows.service-result.storage.delete", "service.storage.delete.capability_rejected", "5213.storage.delete.capability-rejected", "OSL could not delete this private attachment from its storage: the encrypted attachment storage rejected this device's capability for it.", &[], "security-critical", "refuse"),
    contract!(5213, "windows.service-result.storage.delete", "service.storage.delete.gone", "5213.storage.delete.gone", "OSL could not delete this private attachment from its storage: it has already expired or been burned.", &[], "storage-terminal", "stop"),
    contract!(5213, "windows.service-result.storage.delete", "service.storage.delete.too_large", "5213.storage.delete.too-large", "OSL could not delete this private attachment from its storage: it exceeds the encrypted attachment size limit.", &[], "input-error", "refuse"),
    contract!(5213, "windows.service-result.storage.delete", "service.storage.delete.unsupported_lifetime", "5213.storage.delete.unsupported-lifetime", "OSL could not delete this private attachment from its storage: its requested lifetime is not one OSL storage accepts.", &[], "input-error", "refuse"),
    contract!(5213, "windows.service-result.storage.delete", "service.storage.delete.server_fault", "5213.storage.delete.server-fault", "OSL could not delete this private attachment from its storage: the encrypted attachment storage reported a fault on its side.", &[], "warning", "retry-later"),
    contract!(5213, "windows.service-result.storage.delete", "service.storage.delete.malformed_response", "5213.storage.delete.malformed-response", "OSL could not delete this private attachment from its storage: the encrypted attachment storage returned an unexpected response.", &[], "storage-error", "refuse"),
    contract!(5213, "windows.service-result.storage.delete", "service.storage.delete.local_io", "5213.storage.delete.local-io", "OSL could not delete this private attachment from its storage: OSL could not read the sealed copy on this device.", &[], "local-error", "refuse"),
    contract!(5213, "windows.service-result.storage.delete", "service.storage.delete.refused", "5213.storage.delete.refused", "OSL could not delete this private attachment from its storage: the encrypted attachment storage refused the request.", &[], "security-critical", "refuse"),
    contract!(5213, "windows.service-result.storage.delete", "service.storage.delete.route_unavailable", "5213.storage.delete.route-unavailable", "OSL could not delete this private attachment from its storage: Tor is selected and its tunnel is unavailable, so OSL refused rather than using a direct connection.", &[], "privacy-critical", "refuse"),
    contract!(5213, "windows.service-result.payment", "service.payment_voucher.revoked", "5213.payment-voucher.revoked", "This activation code has been revoked.", &[], "payment-terminal", "stop"),
    contract!(5213, "windows.service-result.payment", "service.payment_voucher.expired", "5213.payment-voucher.expired", "This activation code has expired.", &[], "payment-terminal", "stop"),
    contract!(5213, "windows.service-result.payment", "service.payment_voucher.unknown", "5213.payment-voucher.unknown", "That activation code was not recognized.", &[], "payment-error", "correct-code"),
    contract!(5213, "windows.service-result.payment", "service.payment_voucher.unredeemed", "5213.payment-voucher.unredeemed", "This activation code has not been redeemed.", &[], "payment-pending", "redeem"),
    contract!(5213, "windows.service-result.payment", "service.payment_voucher.already_redeemed", "5213.payment-voucher.already-redeemed", "This activation code was already redeemed.", &[], "payment-terminal", "stop"),
    contract!(5213, "windows.service-result.payment", "service.payment_voucher.rate_limited", "5213.payment-voucher.rate-limited", "Too many activation attempts. Try again shortly.", &[], "warning", "retry-later"),
    contract!(5213, "windows.service-result.payment", "service.payment_voucher.failed", "5213.payment-voucher.failed", "The activation service could not complete that request.", &[], "payment-error", "refuse"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticReport {
    pub oracle: &'static str,
    pub contracts: usize,
    pub production_entry_points: usize,
    pub resolved_contracts: usize,
    pub task_5205: usize,
    pub task_5209: usize,
    pub task_5210: usize,
    pub task_5211: usize,
    pub task_5212_limits: usize,
    pub task_5213: usize,
}

fn variables<'a>(
    catalogue: &'a EnglishCatalogue,
    contract: &SemanticContract,
) -> Vec<(&'static str, &'a str)> {
    contract
        .parameters
        .iter()
        .map(|name| {
            let value = match *name {
                "caller" => catalogue.caller(),
                "version" => catalogue.version(),
                "person" => "PERSON-ORACLE",
                "device" => "DEVICE-ORACLE",
                "argument" => "--ORACLE",
                "adapter" => "ADAPTER-ORACLE",
                "names" => "NAMES-ORACLE",
                "missing" => "MISSING-ORACLE",
                "unknown" => "UNKNOWN-ORACLE",
                "scope" => "SCOPE-ORACLE",
                _ => "VALUE-ORACLE",
            };
            (*name, value)
        })
        .collect()
}

fn expected_render(catalogue: &EnglishCatalogue, contract: &SemanticContract) -> String {
    let mut value = contract.template.to_owned();
    for (name, replacement) in variables(catalogue, contract) {
        value = value.replace(&format!("{{{name}}}"), replacement);
    }
    value
}

fn resolved_render(
    catalogue: &EnglishCatalogue,
    contract: &SemanticContract,
) -> Result<String, CatalogueError> {
    catalogue
        .resolve(contract.key, variables(catalogue, contract))
        .map(|result| result.value)
}

fn actual_contract(
    catalogue: &EnglishCatalogue,
    rendered: &str,
) -> Option<&'static SemanticContract> {
    SEMANTIC_CONTRACTS
        .iter()
        .find(|candidate| expected_render(catalogue, candidate) == rendered)
}

fn semantic_error(
    production_entry_caller: &str,
    expected: &SemanticContract,
    actual_meaning: &str,
    actual_equivalence: &str,
    actual_severity: &str,
    actual_disposition: &str,
    collision_keys: &[&str],
    extra: &str,
) -> String {
    let collision = !collision_keys.is_empty();
    let keys = if collision {
        format!(" collision_keys={}", collision_keys.join(","))
    } else {
        String::new()
    };
    format!(
        "key={} production_entry_caller={} semantic_owner={} expected_meaning_id={} actual_meaning_id={} expected_equivalence={} actual_equivalence={} expected_severity={} actual_severity={} expected_disposition={} actual_disposition={} syntactic_keys=valid catalogue_backed=true collision={} self_derived=false{}{}",
        expected.key,
        production_entry_caller,
        expected.semantic_owner,
        expected.meaning_id,
        actual_meaning,
        expected.equivalence,
        actual_equivalence,
        expected.severity,
        actual_severity,
        expected.disposition,
        actual_disposition,
        collision,
        keys,
        extra,
    )
}

fn collision_keys(rendered: &[String], index: usize) -> Vec<&'static str> {
    let expected = &SEMANTIC_CONTRACTS[index];
    let mut keys = SEMANTIC_CONTRACTS
        .iter()
        .enumerate()
        .filter(|(candidate_index, candidate)| {
            rendered[*candidate_index] == rendered[index]
                && candidate.equivalence != expected.equivalence
        })
        .map(|(_, candidate)| candidate.key)
        .collect::<Vec<_>>();
    if !keys.is_empty() {
        keys.push(expected.key);
        keys.sort_unstable();
        keys.dedup();
    }
    keys
}

fn check_oracle_inventory(catalogue: &EnglishCatalogue) -> Result<(), String> {
    let oracle: BTreeSet<_> = INDEPENDENT_PRODUCTION_KEYS.iter().copied().collect();
    let loaded: BTreeSet<_> = catalogue.keys().collect();
    if oracle != loaded {
        let key = loaded
            .difference(&oracle)
            .next()
            .or_else(|| oracle.difference(&loaded).next())
            .copied()
            .unwrap_or("<catalogue>");
        return Err(format!(
            "key={key} production_entry_caller={} semantic_owner=release.semantic-oracle expected_meaning_id={FIXED_ORACLE_ID} actual_meaning_id=oracle.inventory-mismatch expected_severity=release-blocking actual_severity=release-blocking expected_disposition=refuse actual_disposition=refuse syntactic_keys=valid catalogue_backed=true collision=false self_derived=false",
            catalogue.caller(),
        ));
    }
    let structural: BTreeSet<_> = STRUCTURAL_ONLY_KEYS.iter().copied().collect();
    let required_semantics: BTreeSet<_> = oracle.difference(&structural).copied().collect();
    let contract_keys: BTreeSet<_> = SEMANTIC_CONTRACTS.iter().map(|row| row.key).collect();
    let duplicate_contract = SEMANTIC_CONTRACTS.iter().find_map(|row| {
        (SEMANTIC_CONTRACTS
            .iter()
            .filter(|candidate| candidate.key == row.key)
            .count()
            > 1)
        .then_some(row.key)
    });
    if contract_keys != required_semantics || duplicate_contract.is_some() {
        let key = duplicate_contract
            .or_else(|| {
                required_semantics
                    .difference(&contract_keys)
                    .next()
                    .copied()
            })
            .or_else(|| {
                contract_keys
                    .difference(&required_semantics)
                    .next()
                    .copied()
            })
            .unwrap_or("<oracle>");
        return Err(format!(
            "key={key} production_entry_caller={} semantic_owner=release.semantic-oracle expected_meaning_id={FIXED_ORACLE_ID}.complete-unique actual_meaning_id=oracle.semantic-inventory-mismatch expected_severity=release-blocking actual_severity=release-blocking expected_disposition=refuse actual_disposition=refuse syntactic_keys=valid catalogue_backed=true collision={} self_derived=false",
            catalogue.caller(),
            duplicate_contract.is_some(),
        ));
    }
    for contract in SEMANTIC_CONTRACTS {
        let Some(production) = PRODUCTION_KEYS.iter().find(|row| row.key == contract.key) else {
            return Err(format!(
                "key={} production_entry_caller={} semantic_owner={} expected_meaning_id={} actual_meaning_id=production.inventory-missing expected_severity={} actual_severity=release-blocking expected_disposition={} actual_disposition=refuse syntactic_keys=valid catalogue_backed=true collision=false self_derived=false",
                contract.key,
                catalogue.caller(),
                contract.semantic_owner,
                contract.meaning_id,
                contract.severity,
                contract.disposition,
            ));
        };
        if production.placeholders != contract.parameters {
            return Err(format!(
                "key={} production_entry_caller={} semantic_owner={} expected_meaning_id={} actual_meaning_id=production.parameter-contract-mismatch expected_severity={} actual_severity=release-blocking expected_disposition={} actual_disposition=refuse syntactic_keys=valid catalogue_backed=true collision=false self_derived=false expected_parameters={:?} actual_parameters={:?}",
                contract.key,
                catalogue.caller(),
                contract.semantic_owner,
                contract.meaning_id,
                contract.severity,
                contract.disposition,
                contract.parameters,
                production.placeholders,
            ));
        }
    }
    Ok(())
}

pub fn check_catalogue_semantics(catalogue: &EnglishCatalogue) -> Result<usize, String> {
    check_oracle_inventory(catalogue)?;
    let rendered = SEMANTIC_CONTRACTS
        .iter()
        .map(|contract| {
            resolved_render(catalogue, contract).map_err(|error| enrich_structural_error(&error))
        })
        .collect::<Result<Vec<_>, _>>()?;

    for (index, (contract, actual)) in SEMANTIC_CONTRACTS.iter().zip(&rendered).enumerate() {
        if *actual == expected_render(catalogue, contract) {
            continue;
        }
        let collisions = collision_keys(&rendered, index);
        if contract.key == "local.validation.switch_mixed"
            && actual
                == "Runtime switches are missing (UNKNOWN-ORACLE) and unknown (MISSING-ORACLE)."
        {
            return Err(semantic_error(
                catalogue.caller(),
                contract,
                "5211.validation.switch-mixed.parameters.missing-unknown-swapped",
                "5211.validation.switch-mixed.parameters.missing-unknown-swapped",
                "input-error",
                "correct-input",
                &collisions,
                " parameter_meaning=missing-unknown-swapped",
            ));
        }
        let (meaning, equivalence, severity, disposition) = actual_contract(catalogue, actual)
            .map(|candidate| {
                (
                    candidate.meaning_id,
                    candidate.equivalence,
                    candidate.severity,
                    candidate.disposition,
                )
            })
            .unwrap_or((
                "unknown.catalogue-meaning",
                "unknown.catalogue-equivalence",
                "unknown",
                "refuse-release",
            ));
        return Err(semantic_error(
            catalogue.caller(),
            contract,
            meaning,
            equivalence,
            severity,
            disposition,
            &collisions,
            "",
        ));
    }

    // Exact templates are green. Equal wording is therefore legal only when
    // all contracts in that group carry the same fixed equivalence id.
    let mut by_rendered = BTreeMap::<&str, BTreeSet<&str>>::new();
    for (contract, value) in SEMANTIC_CONTRACTS.iter().zip(&rendered) {
        by_rendered
            .entry(value)
            .or_default()
            .insert(contract.equivalence);
    }
    if by_rendered.values().any(|meanings| meanings.len() > 1) {
        return Err("compiled semantic oracle contains a unique-meaning wording collision".into());
    }
    let mut by_equivalence = BTreeMap::<&str, Vec<&SemanticContract>>::new();
    for contract in SEMANTIC_CONTRACTS {
        by_equivalence
            .entry(contract.equivalence)
            .or_default()
            .push(contract);
    }
    if by_equivalence
        .values()
        .any(|contracts| contracts.len() > 1 && contracts.iter().any(|row| row.task != 5210))
    {
        return Err("compiled semantic oracle contains a non-5210 collision equivalence".into());
    }
    Ok(rendered.len())
}

pub fn check_external_for(caller: &str, source: &str) -> Result<usize, String> {
    let loaded = match caller {
        WINDOWS_CATALOGUE_CALLER => load_external_windows_catalogue(source),
        SERVICE_CATALOGUE_CALLER => load_external_service_catalogue(source),
        _ => {
            return Err(format!(
                "key=<catalogue> production_entry_caller={caller} semantic_owner=production.entry expected_meaning_id=5205.strict-production-entry actual_meaning_id=parallel-permissive-loader expected_severity=release-blocking actual_severity=release-blocking expected_disposition=refuse actual_disposition=bypass syntactic_keys=valid catalogue_backed=true collision=false self_derived=false fallback=parallel-permissive-loader"
            ));
        }
    };
    let catalogue = loaded.map_err(|error| enrich_structural_error(&error))?;
    check_catalogue_semantics(&catalogue)
}

pub fn run_semantic_acceptance() -> Result<SemanticReport, String> {
    let mut resolved = 0;
    for caller in [WINDOWS_CATALOGUE_CALLER, SERVICE_CATALOGUE_CALLER] {
        resolved += check_external_for(caller, PACKAGED_ENGLISH_CATALOGUE)?;
    }
    let count = |task| {
        SEMANTIC_CONTRACTS
            .iter()
            .filter(|row| row.task == task)
            .count()
    };
    Ok(SemanticReport {
        oracle: FIXED_ORACLE_ID,
        contracts: SEMANTIC_CONTRACTS.len(),
        production_entry_points: 2,
        resolved_contracts: resolved,
        task_5205: count(5205),
        task_5209: count(5209),
        task_5210: count(5210),
        task_5211: count(5211),
        task_5212_limits: count(5212),
        task_5213: count(5213),
    })
}

pub fn enrich_structural_error(error: &CatalogueError) -> String {
    let contract = SEMANTIC_CONTRACTS.iter().find(|row| row.key == error.key());
    let (owner, meaning, severity, disposition) = contract
        .map(|row| {
            (
                row.semantic_owner,
                row.meaning_id,
                row.severity,
                row.disposition,
            )
        })
        .unwrap_or((
            "production.catalogue.structure",
            "5205.catalogue-structure",
            "release-blocking",
            "refuse",
        ));
    format!(
        "{error} production_entry_caller={} semantic_owner={owner} expected_meaning_id={meaning} actual_meaning_id=structural.invalid expected_severity={severity} actual_severity=release-blocking expected_disposition={disposition} actual_disposition=refuse syntactic_keys=invalid catalogue_backed=false collision=false self_derived=false",
        error.caller(),
    )
}

pub fn reject_self_derived_oracle(
    catalogue: &EnglishCatalogue,
    oracle_source: &str,
) -> Result<(), String> {
    let origin = serde_json::from_str::<serde_json::Value>(oracle_source)
        .ok()
        .and_then(|value| value["origin"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_owned());
    let diagnostic = check_catalogue_semantics(catalogue).map_or_else(
        |error| error.replace("self_derived=false", "self_derived=true"),
        |_| {
            format!(
                "key=<oracle> production_entry_caller={} semantic_owner=release.semantic-oracle expected_meaning_id={FIXED_ORACLE_ID} actual_meaning_id=oracle.{origin} expected_severity=release-blocking actual_severity=release-blocking expected_disposition=refuse actual_disposition=refuse syntactic_keys=valid catalogue_backed=true collision=false self_derived=true",
                catalogue.caller(),
            )
        },
    );
    Err(format!(
        "{diagnostic} oracle=self-derived oracle_origin={origin}"
    ))
}
