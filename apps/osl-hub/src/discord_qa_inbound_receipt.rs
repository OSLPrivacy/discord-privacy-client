//! Hash-only receiver evidence for disposable Discord QA builds.
//!
//! The normal overlay renderer keeps decrypted Discord text only in memory.
//! UI Automation over WebView2 is not a reliable way to prove that the native
//! receiver authenticated and opened a deterministic QA message, so the QA
//! build emits a bounded receipt at that exact native boundary. Plaintext is
//! never written to disk.

use crate::broker::{OpenedNativeOverlayTextBatch, PreparedNativeOverlayText};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const RECEIPT_FILE: &str = "discord-qa-inbound-receipt.json";
pub(crate) const OUTBOUND_RECEIPT_FILE: &str = "discord-qa-outbound-receipt.json";
pub(crate) const POLL_RECEIPT_FILE: &str = "discord-qa-inbound-poll-receipt.json";
pub(crate) const SEND_STAGE_RECEIPT_FILE: &str = "discord-qa-send-stage-receipt.json";
pub(crate) const OVERLAY_OPEN_RECEIPT_FILE: &str = "discord-qa-overlay-open-receipt.json";
const MAX_RECEIPT_MESSAGES: usize = 64;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiscordQaInboundMessageReceipt {
    plaintext_sha256: String,
    utf8_bytes: usize,
    lines: usize,
    context_verified: bool,
    person_to_person_e2ee: bool,
    view_once_consumed: bool,
    expires_at: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiscordQaInboundReceipt {
    schema_version: u8,
    observed_at_unix_ms: u128,
    messages: Vec<DiscordQaInboundMessageReceipt>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiscordQaOutboundReceipt {
    schema_version: u8,
    observed_at_unix_ms: u128,
    plaintext_sha256: String,
    message_id_sha256: String,
    utf8_bytes: usize,
    lines: usize,
    person_to_person_e2ee: bool,
    view_once: bool,
    delivered_to_osl_inbox: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiscordQaInboundPollReceipt {
    schema_version: u8,
    observed_at_unix_ms: u128,
    outcome: &'static str,
    error_class: Option<&'static str>,
    opened_count: usize,
    pending_view_once_count: usize,
    acknowledgment_count: usize,
    fetched: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiscordQaSendStageReceipt {
    schema_version: u8,
    observed_at_unix_ms: u128,
    plaintext_sha256: String,
    entered: bool,
    registration_terminal_state: &'static str,
    keyserver_available: bool,
    identity_unchanged: bool,
    overlay_context_verified: bool,
    post_succeeded: bool,
    phase: &'static str,
    phase_outcome: &'static str,
    error_class: Option<&'static str>,
    /// Fixed, site-specific diagnostic label, additive to `error_class`. Unlike
    /// `error_class` (which collapses every "post" phase failure to
    /// `post_rejected`), this distinguishes *which* post-phase site refused,
    /// e.g. `carrier_not_confirmed` vs `carrier_geometry_unproven`. Absent on
    /// success. Always a fixed label supplied by the call site — never derived
    /// from, and never containing, raw error text.
    error_detail: Option<&'static str>,
    /// Structural facts (never content) that explain a `carrier_not_confirmed`
    /// `error_detail`: which conjunct of the post-send confirmation check
    /// failed. Only populated for the `post` phase; `None` everywhere else.
    carrier_status: Option<&'static str>,
    carrier_placed: Option<bool>,
    carrier_enter_sent: Option<bool>,
    overlay_context_unchanged: Option<bool>,
}

/// Structural (never content) evidence about the Discord carrier at the moment
/// a post-phase send confirmation failed. `carrier_status` must be a fixed
/// label (e.g. the `DiscordCarrierStatus` variant name) — never carrier or
/// message text.
pub struct PostCarrierDiagnostics {
    pub carrier_status: &'static str,
    pub carrier_placed: bool,
    pub carrier_enter_sent: bool,
    pub overlay_context_unchanged: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiscordQaOverlayOpenReceipt {
    schema_version: u8,
    observed_at_unix_ms: u128,
    outcome: &'static str,
    error_class: Option<&'static str>,
}

fn sha256_hex(value: &[u8]) -> String {
    let digest = Sha256::digest(value);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn receipt_for(batch: &OpenedNativeOverlayTextBatch) -> Option<DiscordQaInboundReceipt> {
    if batch.messages.is_empty() {
        return None;
    }
    let observed_at_unix_ms = now_unix_ms();
    let messages = batch
        .messages
        .iter()
        .take(MAX_RECEIPT_MESSAGES)
        .map(|message| DiscordQaInboundMessageReceipt {
            plaintext_sha256: sha256_hex(message.plaintext.as_bytes()),
            utf8_bytes: message.plaintext.len(),
            lines: message
                .plaintext
                .as_bytes()
                .iter()
                .filter(|byte| **byte == b'\n')
                .count()
                .saturating_add(1),
            context_verified: message.context_verified,
            person_to_person_e2ee: message.person_to_person_e2ee,
            view_once_consumed: message.view_once_consumed,
            expires_at: message.expires_at,
        })
        .collect();
    Some(DiscordQaInboundReceipt {
        schema_version: 1,
        observed_at_unix_ms,
        messages,
    })
}

fn receipt_path() -> Result<PathBuf, String> {
    keystore::osl_base_dir()
        .map(|dir| dir.join(RECEIPT_FILE))
        .map_err(|_| "Discord QA receipt storage is unavailable".to_owned())
}

fn fixed_receipt_path(filename: &str) -> Result<PathBuf, String> {
    keystore::osl_base_dir()
        .map(|dir| dir.join(filename))
        .map_err(|_| "Discord QA receipt storage is unavailable".to_owned())
}

fn write_receipt(path: &Path, batch: &OpenedNativeOverlayTextBatch) -> Result<(), String> {
    let Some(receipt) = receipt_for(batch) else {
        return Ok(());
    };
    let encoded = serde_json::to_vec(&receipt)
        .map_err(|_| "Discord QA inbound receipt could not be encoded".to_owned())?;
    crate::atomic_file::write_recoverable(path, &encoded, "Discord QA inbound receipt")
}

pub fn record(batch: &OpenedNativeOverlayTextBatch) -> Result<(), String> {
    write_receipt(&receipt_path()?, batch)
}

pub fn record_outbound(
    plaintext: &str,
    prepared: &PreparedNativeOverlayText,
) -> Result<(), String> {
    let receipt = DiscordQaOutboundReceipt {
        schema_version: 1,
        observed_at_unix_ms: now_unix_ms(),
        plaintext_sha256: sha256_hex(plaintext.as_bytes()),
        message_id_sha256: sha256_hex(prepared.message_id.as_bytes()),
        utf8_bytes: plaintext.len(),
        lines: plaintext
            .as_bytes()
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
            .saturating_add(1),
        person_to_person_e2ee: prepared.person_to_person_e2ee,
        view_once: prepared.view_once,
        delivered_to_osl_inbox: prepared.delivered_to_osl_inbox,
    };
    let encoded = serde_json::to_vec(&receipt)
        .map_err(|_| "Discord QA outbound receipt could not be encoded".to_owned())?;
    crate::atomic_file::write_recoverable(
        &fixed_receipt_path(OUTBOUND_RECEIPT_FILE)?,
        &encoded,
        "Discord QA outbound receipt",
    )
}

pub fn record_send_stage(
    plaintext: &str,
    registration_terminal_state: &'static str,
    keyserver_available: bool,
    identity_unchanged: bool,
    overlay_context_verified: bool,
    post_succeeded: bool,
) -> Result<(), String> {
    if !matches!(
        registration_terminal_state,
        "pending"
            | "registered"
            | "timeout"
            | "identity_changed"
            | "identity_unavailable"
            | "conflict"
            | "offline"
            | "unavailable"
    ) {
        return Err("Discord QA send stage is invalid".to_owned());
    }
    let receipt = DiscordQaSendStageReceipt {
        schema_version: 2,
        observed_at_unix_ms: now_unix_ms(),
        plaintext_sha256: sha256_hex(plaintext.as_bytes()),
        entered: true,
        registration_terminal_state,
        keyserver_available,
        identity_unchanged,
        overlay_context_verified,
        post_succeeded,
        phase: if post_succeeded {
            "post"
        } else if overlay_context_verified {
            "context"
        } else {
            "registration"
        },
        phase_outcome: if post_succeeded { "ready" } else { "entered" },
        error_class: None,
        error_detail: None,
        carrier_status: None,
        carrier_placed: None,
        carrier_enter_sent: None,
        overlay_context_unchanged: None,
    };
    let encoded = serde_json::to_vec(&receipt)
        .map_err(|_| "Discord QA send-stage receipt could not be encoded".to_owned())?;
    crate::atomic_file::write_recoverable(
        &fixed_receipt_path(SEND_STAGE_RECEIPT_FILE)?,
        &encoded,
        "Discord QA send-stage receipt",
    )
}

fn headless_phase_error_class(phase: &str, error: &str) -> &'static str {
    match phase {
        "host" => "host_unavailable",
        "pairing" if error.contains("unavailable") => "pairing_unavailable",
        "pairing" => "pairing_mismatch",
        "binding" => "peer_binding_rejected",
        "activation" => "activation_rejected",
        "permission" => "permission_rejected",
        "context" => "context_changed",
        "pre_send_drain" => "pre_send_drain_rejected",
        "scope" => "scope_rejected",
        "verified_peer" => "verified_peer_rejected",
        "record" => "record_rejected",
        "encrypt" => "encrypt_rejected",
        "keyserver_client" => "keyserver_client_unavailable",
        "recipient_registration" => "recipient_registration_rejected",
        "post_control_inbox" => "post_control_inbox_rejected",
        "post" if error.contains("key server") || error.contains("inbox") => {
            "transport_unavailable"
        }
        "post" => "post_rejected",
        _ => "rejected",
    }
}

/// Pure builder for the headless send-phase receipt. Split out from
/// `record_headless_send_phase` so the field-mapping and validation logic can
/// be unit tested without touching disk.
fn headless_send_phase_receipt(
    plaintext: &str,
    registration_terminal_state: &'static str,
    keyserver_available: bool,
    identity_unchanged: bool,
    phase: &'static str,
    outcome: &'static str,
    error: Option<&str>,
    error_detail: Option<&'static str>,
    carrier_diagnostics: Option<PostCarrierDiagnostics>,
) -> Result<DiscordQaSendStageReceipt, String> {
    if !matches!(
        phase,
        "host"
            | "pairing"
            | "binding"
            | "activation"
            | "permission"
            | "context"
            | "pre_send_drain"
            | "scope"
            | "verified_peer"
            | "record"
            | "encrypt"
            | "keyserver_client"
            | "recipient_registration"
            | "post_control_inbox"
            | "post"
    ) || !matches!(outcome, "entered" | "ready" | "error")
        || (outcome == "error") != error.is_some()
        || (error_detail.is_some() && outcome != "error")
        || (carrier_diagnostics.is_some() && phase != "post")
    {
        return Err("Discord QA headless send phase is invalid".to_owned());
    }
    Ok(DiscordQaSendStageReceipt {
        schema_version: 2,
        observed_at_unix_ms: now_unix_ms(),
        plaintext_sha256: sha256_hex(plaintext.as_bytes()),
        entered: true,
        registration_terminal_state,
        keyserver_available,
        identity_unchanged,
        overlay_context_verified: matches!(phase, "context" | "post") && outcome == "ready",
        post_succeeded: phase == "post" && outcome == "ready",
        phase,
        phase_outcome: outcome,
        error_class: error.map(|value| headless_phase_error_class(phase, value)),
        error_detail,
        carrier_status: carrier_diagnostics.as_ref().map(|d| d.carrier_status),
        carrier_placed: carrier_diagnostics.as_ref().map(|d| d.carrier_placed),
        carrier_enter_sent: carrier_diagnostics.as_ref().map(|d| d.carrier_enter_sent),
        overlay_context_unchanged: carrier_diagnostics.map(|d| d.overlay_context_unchanged),
    })
}

/// Original 7-argument entry point, unchanged for existing callers (e.g.
/// `broker.rs`) that have no site-specific diagnostic to attach. Delegates to
/// [`record_headless_send_phase_detailed`] with `error_detail` and
/// `carrier_diagnostics` both absent.
pub fn record_headless_send_phase(
    plaintext: &str,
    registration_terminal_state: &'static str,
    keyserver_available: bool,
    identity_unchanged: bool,
    phase: &'static str,
    outcome: &'static str,
    error: Option<&str>,
) -> Result<(), String> {
    record_headless_send_phase_detailed(
        plaintext,
        registration_terminal_state,
        keyserver_available,
        identity_unchanged,
        phase,
        outcome,
        error,
        None,
        None,
    )
}

/// As [`record_headless_send_phase`], plus a fixed site-specific `error_detail`
/// label and, for the `post` phase only, structural (never content) carrier
/// diagnostics — so distinct "post" phase refusals no longer collapse to one
/// indistinguishable `error_class`.
#[allow(clippy::too_many_arguments)]
pub fn record_headless_send_phase_detailed(
    plaintext: &str,
    registration_terminal_state: &'static str,
    keyserver_available: bool,
    identity_unchanged: bool,
    phase: &'static str,
    outcome: &'static str,
    error: Option<&str>,
    error_detail: Option<&'static str>,
    carrier_diagnostics: Option<PostCarrierDiagnostics>,
) -> Result<(), String> {
    let receipt = headless_send_phase_receipt(
        plaintext,
        registration_terminal_state,
        keyserver_available,
        identity_unchanged,
        phase,
        outcome,
        error,
        error_detail,
        carrier_diagnostics,
    )?;
    let encoded = serde_json::to_vec(&receipt)
        .map_err(|_| "Discord QA send-stage receipt could not be encoded".to_owned())?;
    crate::atomic_file::write_recoverable(
        &fixed_receipt_path(SEND_STAGE_RECEIPT_FILE)?,
        &encoded,
        "Discord QA send-stage receipt",
    )
}

/// Same classification, exposed for the broker's QA refusal breadcrumb.
#[cfg(feature = "discord-qa-shell")]
pub(crate) fn keyserver_post_error_class_pub(error: &keystore::Error) -> &'static str {
    keyserver_post_error_class(error)
}

#[deny(unreachable_patterns)]
fn keyserver_post_error_class(error: &keystore::Error) -> &'static str {
    match error {
        keystore::Error::HttpStatus { status: 400, .. } => "http_400",
        keystore::Error::HttpStatus { status: 401, .. } => "http_401",
        keystore::Error::HttpStatus { status: 403, .. } => "http_403",
        keystore::Error::HttpStatus { status: 404, .. } => "http_404",
        keystore::Error::HttpStatus { status: 409, .. } => "http_409",
        keystore::Error::HttpStatus { status: 429, .. } => "http_429",
        keystore::Error::HttpStatus { status, .. } if *status >= 500 => "http_5xx",
        keystore::Error::HttpStatus { .. } => "http_other",
        keystore::Error::Transport(_) | keystore::Error::Io(_) => "transport",
        keystore::Error::Json(_) | keystore::Error::Base64(_) => "encode_or_response",
        keystore::Error::Crypto(_) => "crypto",
        // Its own class on purpose, and never folded into `crypto` or `transport`.
        // This is the C1 full-bundle proof refusing a peer bundle whose identity
        // signature did not verify, i.e. the one symptom a keyserver attempting
        // key substitution produces. Sharing a label with a decode failure would
        // make an active attack indistinguishable from a glitch.
        keystore::Error::PeerBundleProofInvalid => "peer_bundle_proof_invalid",
        // Also its own class. A one-time prekey the handshake named is absent from
        // local state, which is a key-availability fact, not a crypto failure and
        // not local blob damage. Folding it into "crypto" would hide prekey
        // exhaustion (a peer that can no longer be reached) behind a label that
        // reads like a broken ciphertext.
        keystore::Error::PrekeyMissing => "prekey_missing",
        keystore::Error::Sealer(_)
        | keystore::Error::BlobVersionMismatch { .. }
        | keystore::Error::BlobFieldLength { .. }
        | keystore::Error::BlobMethodMismatch { .. } => "local_state",
    }
}

pub fn record_headless_post_control_stage(
    outcome: &'static str,
    error: Option<&keystore::Error>,
) -> Result<(), String> {
    let receipt = headless_post_control_stage_receipt(outcome, error)?;
    let encoded = serde_json::to_vec(&receipt)
        .map_err(|_| "Discord QA control-inbox receipt could not be encoded".to_owned())?;
    crate::atomic_file::write_recoverable(
        &fixed_receipt_path(SEND_STAGE_RECEIPT_FILE)?,
        &encoded,
        "Discord QA control-inbox receipt",
    )
}

fn headless_post_control_stage_receipt(
    outcome: &'static str,
    error: Option<&keystore::Error>,
) -> Result<DiscordQaSendStageReceipt, String> {
    if !matches!(outcome, "entered" | "ready" | "error") || (outcome == "error") != error.is_some()
    {
        return Err("Discord QA control-inbox stage is invalid".to_owned());
    }
    Ok(DiscordQaSendStageReceipt {
        schema_version: 2,
        observed_at_unix_ms: now_unix_ms(),
        plaintext_sha256: sha256_hex(b"OSL Discord QA probe"),
        entered: true,
        registration_terminal_state: "registered",
        keyserver_available: true,
        identity_unchanged: true,
        overlay_context_verified: true,
        post_succeeded: outcome == "ready",
        phase: "post_control_inbox",
        phase_outcome: outcome,
        error_class: error.map(keyserver_post_error_class),
        error_detail: None,
        carrier_status: None,
        carrier_placed: None,
        carrier_enter_sent: None,
        overlay_context_unchanged: None,
    })
}

fn overlay_open_error_class(error: &str) -> &'static str {
    if error.contains("focus") || error.contains("foreground") {
        "focus_unavailable"
    } else if error.contains("trusted OSL overlay owner")
        || error.contains("protected window owner")
        || error.contains("overlay owner")
    {
        "owner_changed"
    } else if error.contains("verified Discord composer bounds")
        || error.contains("composer bounds")
    {
        "composer_bounds_unavailable"
    } else if error.contains("scope binding") || error.contains("binding changed") {
        "scope_binding_changed"
    } else if error.contains("locked") || error.contains("identity is not loaded") {
        "identity_unavailable"
    } else if error.contains("friend") || error.contains("peer") {
        "friend_unavailable"
    } else if error.contains("scope") || error.contains("approval") {
        "scope_unavailable"
    } else if error.contains("one exact visible message composer") {
        "composer_unavailable"
    } else if error.contains("selected conversation") {
        "conversation_unavailable"
    } else if error.contains("matching visible conversation header") {
        "conversation_header_unavailable"
    // NOTE: no site in this repo produces "Clear the visible Discord composer";
    // this branch is unreachable and kept only until the producing side exists.
    } else if error.contains("Clear the visible Discord composer") {
        "composer_not_empty"
    } else if error.contains("Windows accessibility") {
        "windows_accessibility_unavailable"
    } else if error.contains("accessibility exceeded") {
        "accessibility_limit_exceeded"
    } else if error.contains("accessibility timed out") {
        "accessibility_timeout"
    } else if error.contains("host state is missing") {
        "native_host_state_missing"
    } else if error.contains("owner binding") {
        "native_host_owner_mismatch"
    } else if error.contains("window PID") {
        "native_host_window_pid_changed"
    } else if error.contains("parent is unavailable") {
        "native_host_parent_unavailable"
    } else if error.contains("tether is unavailable") {
        "native_host_tether_unavailable"
    } else if error.contains("Windows session") {
        "native_host_session_changed"
    } else if error.contains("process identity") {
        "native_host_process_changed"
    } else if error.contains("account is unavailable") {
        "native_host_account_unavailable"
    } else if error.contains("window validation") {
        "native_host_window_invalid"
    } else if error.contains("native Discord") || error.contains("native host") {
        "native_host_unavailable"
    } else if error.contains("capture resistance") || error.contains("capture shield") {
        "capture_protection_unavailable"
    } else if error.contains("geometry") || error.contains("position") || error.contains("size") {
        "geometry_unavailable"
    } else if error.contains("context") || error.contains("generation") {
        "context_changed"
    } else if error.contains("created") || error.contains("closed") || error.contains("shown") {
        "window_unavailable"
    } else if error.contains("in time") {
        "timeout"
    } else {
        "rejected"
    }
}

pub fn record_overlay_open_stage(outcome: &'static str, error: Option<&str>) -> Result<(), String> {
    if !matches!(outcome, "guarding" | "ready" | "error") {
        return Err("Discord QA overlay-open stage is invalid".to_owned());
    }
    let receipt = DiscordQaOverlayOpenReceipt {
        schema_version: 1,
        observed_at_unix_ms: now_unix_ms(),
        outcome,
        error_class: error.map(overlay_open_error_class),
    };
    let encoded = serde_json::to_vec(&receipt)
        .map_err(|_| "Discord QA overlay-open receipt could not be encoded".to_owned())?;
    crate::atomic_file::write_recoverable(
        &fixed_receipt_path(OVERLAY_OPEN_RECEIPT_FILE)?,
        &encoded,
        "Discord QA overlay-open receipt",
    )
}

fn poll_error_class(error: &str) -> &'static str {
    if error.contains("context") || error.contains("overlay") {
        "context_unavailable"
    } else if error.contains("receive protected") || error.contains("key server") {
        "transport_unavailable"
    } else {
        "rejected"
    }
}

fn poll_receipt_for(
    result: Result<&OpenedNativeOverlayTextBatch, &str>,
) -> DiscordQaInboundPollReceipt {
    match result {
        Ok(batch) => DiscordQaInboundPollReceipt {
            schema_version: 1,
            observed_at_unix_ms: now_unix_ms(),
            outcome: if batch.messages.is_empty() {
                "success_empty"
            } else {
                "opened"
            },
            error_class: None,
            opened_count: batch.messages.len(),
            pending_view_once_count: batch.pending_view_once.len(),
            acknowledgment_count: batch.acknowledgments.len(),
            fetched: batch.fetched,
        },
        Err(error) => DiscordQaInboundPollReceipt {
            schema_version: 1,
            observed_at_unix_ms: now_unix_ms(),
            outcome: "error",
            error_class: Some(poll_error_class(error)),
            opened_count: 0,
            pending_view_once_count: 0,
            acknowledgment_count: 0,
            fetched: 0,
        },
    }
}

pub fn record_poll(result: Result<&OpenedNativeOverlayTextBatch, &str>) -> Result<(), String> {
    let receipt = poll_receipt_for(result);
    let encoded = serde_json::to_vec(&receipt)
        .map_err(|_| "Discord QA poll receipt could not be encoded".to_owned())?;
    crate::atomic_file::write_recoverable(
        &fixed_receipt_path(POLL_RECEIPT_FILE)?,
        &encoded,
        "Discord QA inbound poll receipt",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::OpenedNativeOverlayText;

    fn batch(plaintext: &str) -> OpenedNativeOverlayTextBatch {
        OpenedNativeOverlayTextBatch {
            messages: vec![OpenedNativeOverlayText {
                message_id: "peer-00001111222233334444555566667777".to_owned(),
                cover_pointer: Some("qa cover prose".to_owned()),
                plaintext: plaintext.to_owned(),
                context_verified: true,
                person_to_person_e2ee: true,
                view_once_consumed: false,
                expires_at: 1_900_000_000,
            }],
            pending_view_once: Vec::new(),
            acknowledgments: Vec::new(),
            fetched: 1,
            decrypt_display_enabled: true,
            deferred_rows: 0,
        }
    }

    #[test]
    fn receipt_contains_only_hash_and_bounded_metadata() {
        let plaintext = "qa secret first line\nsecond line";
        let receipt = receipt_for(&batch(plaintext)).expect("receipt");
        let encoded = serde_json::to_string(&receipt).expect("encode");
        assert!(!encoded.contains(plaintext));
        assert!(!encoded.contains("qa secret"));
        assert!(encoded.contains(&sha256_hex(plaintext.as_bytes())));
        assert!(encoded.contains("\"utf8Bytes\":32"));
        assert!(encoded.contains("\"lines\":2"));
        assert!(encoded.contains("\"contextVerified\":true"));
        assert!(encoded.contains("\"personToPersonE2ee\":true"));
        // The received batch now carries a correlation handle so the renderer can
        // paint over the right Discord row. It is routing metadata for that
        // renderer and nothing else: neither the raw message id nor the public
        // cover may reach this on-disk receipt.
        assert!(!encoded.contains("peer-00001111222233334444555566667777"));
        assert!(!encoded.contains("qa cover prose"));
    }

    #[test]
    fn empty_poll_does_not_replace_prior_receipt() {
        let root = std::env::temp_dir().join(format!(
            "osl-discord-qa-receipt-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("root");
        let path = root.join(RECEIPT_FILE);
        write_receipt(&path, &batch("first")).expect("first receipt");
        let before = std::fs::read(&path).expect("read first");
        let empty = OpenedNativeOverlayTextBatch {
            messages: Vec::new(),
            pending_view_once: Vec::new(),
            acknowledgments: Vec::new(),
            fetched: 0,
            decrypt_display_enabled: true,
            deferred_rows: 0,
        };
        write_receipt(&path, &empty).expect("empty poll");
        assert_eq!(std::fs::read(&path).expect("read after"), before);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn outbound_receipt_contains_hashes_but_no_plaintext_or_raw_message_id() {
        let plaintext = "OSL Discord QA probe";
        let prepared = PreparedNativeOverlayText {
            message_id: "raw-message-id-forbidden".to_owned(),
            expires_at: 1_900_000_000,
            person_to_person_e2ee: true,
            view_once: false,
            delivered_to_osl_inbox: true,
        };
        let receipt = DiscordQaOutboundReceipt {
            schema_version: 1,
            observed_at_unix_ms: 1,
            plaintext_sha256: sha256_hex(plaintext.as_bytes()),
            message_id_sha256: sha256_hex(prepared.message_id.as_bytes()),
            utf8_bytes: plaintext.len(),
            lines: 1,
            person_to_person_e2ee: prepared.person_to_person_e2ee,
            view_once: prepared.view_once,
            delivered_to_osl_inbox: prepared.delivered_to_osl_inbox,
        };
        let encoded = serde_json::to_string(&receipt).unwrap();
        assert!(!encoded.contains(plaintext));
        assert!(!encoded.contains(&prepared.message_id));
        assert!(encoded.contains(&sha256_hex(plaintext.as_bytes())));
        assert!(encoded.contains("\"deliveredToOslInbox\":true"));
    }

    #[test]
    fn poll_receipt_uses_only_bounded_error_classes() {
        assert_eq!(
            poll_error_class("secret raw transport error: key server unavailable"),
            "transport_unavailable"
        );
        assert_eq!(poll_error_class("secret raw unknown failure"), "rejected");
        let receipt = DiscordQaInboundPollReceipt {
            schema_version: 1,
            observed_at_unix_ms: 1,
            outcome: "error",
            error_class: Some(poll_error_class("secret raw unknown failure")),
            opened_count: 0,
            pending_view_once_count: 0,
            acknowledgment_count: 0,
            fetched: 0,
        };
        let encoded = serde_json::to_string(&receipt).unwrap();
        assert!(!encoded.contains("secret raw"));
        assert!(encoded.contains("\"errorClass\":\"rejected\""));
    }

    #[test]
    fn poll_receipt_distinguishes_empty_and_authenticated_open() {
        let empty = OpenedNativeOverlayTextBatch {
            messages: Vec::new(),
            pending_view_once: Vec::new(),
            acknowledgments: Vec::new(),
            fetched: 0,
            decrypt_display_enabled: true,
            deferred_rows: 0,
        };
        let empty_receipt = poll_receipt_for(Ok(&empty));
        assert_eq!(empty_receipt.outcome, "success_empty");
        assert_eq!(empty_receipt.opened_count, 0);
        assert!(empty_receipt.error_class.is_none());

        let opened = batch("OSL Discord QA probe");
        let opened_receipt = poll_receipt_for(Ok(&opened));
        assert_eq!(opened_receipt.outcome, "opened");
        assert_eq!(opened_receipt.opened_count, 1);
        assert_eq!(opened_receipt.fetched, 1);
        assert!(opened_receipt.error_class.is_none());
    }

    #[test]
    fn send_stage_receipt_is_bounded_and_contains_no_raw_identity_or_error() {
        // Struct literal extended with the errorDetail/carrier-diagnostic
        // fields (schemaVersion bumped 1 -> 2); this is a field addition to an
        // existing literal, not a re-baselined assertion.
        let receipt = DiscordQaSendStageReceipt {
            schema_version: 2,
            observed_at_unix_ms: 1,
            plaintext_sha256: sha256_hex(b"OSL Discord QA probe"),
            entered: true,
            registration_terminal_state: "timeout",
            keyserver_available: true,
            identity_unchanged: true,
            overlay_context_verified: false,
            post_succeeded: false,
            phase: "pairing",
            phase_outcome: "error",
            error_class: Some("pairing_mismatch"),
            error_detail: None,
            carrier_status: None,
            carrier_placed: None,
            carrier_enter_sent: None,
            overlay_context_unchanged: None,
        };
        let encoded = serde_json::to_string(&receipt).unwrap();
        assert!(!encoded.contains("OSL Discord QA probe"));
        assert!(!encoded.contains("user_id"));
        assert!(!encoded.contains("raw error"));
        assert!(encoded.contains("\"schemaVersion\":2"));
        assert!(encoded.contains("\"registrationTerminalState\":\"timeout\""));
        assert!(encoded.contains("\"postSucceeded\":false"));
        assert!(encoded.contains("\"phase\":\"pairing\""));
        assert!(encoded.contains("\"errorClass\":\"pairing_mismatch\""));
    }

    #[test]
    fn headless_send_phase_receipt_gives_each_post_failure_site_a_distinct_error_detail() {
        // These four labels are the fixed, site-specific diagnostics main.rs
        // attaches at its four distinct "post" phase failure sites, which all
        // previously collapsed to the single errorClass "post_rejected".
        let sites: [(&'static str, &'static str); 4] = [
            (
                "Discord carrier geometry could not be proven",
                "carrier_geometry_unproven",
            ),
            (
                "The native Discord overlay context changed",
                "overlay_context_changed",
            ),
            (
                "A native Discord carrier placement is already in flight",
                "placement_already_in_flight",
            ),
            (
                "The native Discord carrier was not confirmed as sent",
                "carrier_not_confirmed",
            ),
        ];
        let mut seen_details = std::collections::HashSet::new();
        for (raw_error, detail) in sites {
            let receipt = headless_send_phase_receipt(
                "OSL Discord QA atomic send",
                "registered",
                true,
                true,
                "post",
                "error",
                Some(raw_error),
                Some(detail),
                None,
            )
            .expect("valid headless send phase");
            assert_eq!(receipt.error_class, Some("post_rejected"));
            assert_eq!(receipt.error_detail, Some(detail));
            let encoded = serde_json::to_string(&receipt).unwrap();
            assert!(!encoded.contains(raw_error));
            assert!(encoded.contains(&format!("\"errorDetail\":\"{detail}\"")));
            assert!(seen_details.insert(detail), "duplicate errorDetail label");
        }
        assert_eq!(seen_details.len(), 4);
    }

    #[test]
    fn headless_send_phase_receipt_carries_carrier_diagnostics_only_for_post_phase() {
        let diagnostics = PostCarrierDiagnostics {
            carrier_status: "ContextChanged",
            carrier_placed: true,
            carrier_enter_sent: false,
            overlay_context_unchanged: true,
        };
        let receipt = headless_send_phase_receipt(
            "OSL Discord QA atomic send",
            "registered",
            true,
            true,
            "post",
            "error",
            Some("The native Discord carrier was not confirmed as sent"),
            Some("carrier_not_confirmed"),
            Some(diagnostics),
        )
        .expect("valid headless send phase");
        assert_eq!(receipt.carrier_status, Some("ContextChanged"));
        assert_eq!(receipt.carrier_placed, Some(true));
        assert_eq!(receipt.carrier_enter_sent, Some(false));
        assert_eq!(receipt.overlay_context_unchanged, Some(true));

        // Carrier diagnostics attached to a non-"post" phase is refused: those
        // structural facts only exist once a carrier placement was attempted.
        let rejected = headless_send_phase_receipt(
            "OSL Discord QA atomic send",
            "registered",
            true,
            true,
            "context",
            "error",
            Some("raw context error"),
            None,
            Some(PostCarrierDiagnostics {
                carrier_status: "Sent",
                carrier_placed: true,
                carrier_enter_sent: true,
                overlay_context_unchanged: true,
            }),
        );
        assert!(rejected.is_err());
    }

    #[test]
    fn headless_send_phase_maps_raw_failures_to_fixed_classes() {
        assert_eq!(
            headless_phase_error_class("pairing", "raw peer offer unavailable at private path"),
            "pairing_unavailable"
        );
        assert_eq!(
            headless_phase_error_class("pairing", "raw secret hash mismatch"),
            "pairing_mismatch"
        );
        assert_eq!(
            headless_phase_error_class("permission", "raw private identity detail"),
            "permission_rejected"
        );
    }

    #[test]
    fn peer_bundle_proof_invalid_is_an_explicit_terminal_control_inbox_refusal() {
        let error = keystore::Error::PeerBundleProofInvalid;
        let receipt = headless_post_control_stage_receipt("error", Some(&error))
            .expect("proof-invalid produces a bounded refusal receipt");

        assert_eq!(receipt.phase, "post_control_inbox");
        assert_eq!(receipt.phase_outcome, "error");
        assert!(!receipt.post_succeeded);
        assert_eq!(
            receipt.error_class,
            Some("peer_bundle_proof_invalid"),
            "an invalid peer-bundle proof must never be retried or accepted as transport success"
        );

        let entered = headless_post_control_stage_receipt("entered", None)
            .expect("entered without an error is a valid non-success state");
        assert!(!entered.post_succeeded);
        assert_eq!(entered.error_class, None);

        let ready = headless_post_control_stage_receipt("ready", None)
            .expect("ready without an error is the only successful state");
        assert!(ready.post_succeeded);
        assert_eq!(ready.error_class, None);

        for (outcome, error, reason) in [
            (
                "entered",
                Some(&error),
                "entered must not accept an error as a default outcome",
            ),
            (
                "ready",
                Some(&error),
                "proof-invalid must not coexist with transport success",
            ),
            (
                "error",
                None,
                "an error outcome must carry the typed refusal",
            ),
            (
                "unknown",
                None,
                "an unknown outcome must not gain a default accepted state",
            ),
            (
                "unknown",
                Some(&error),
                "an unknown error outcome must not gain a default refusal state",
            ),
        ] {
            assert!(
                headless_post_control_stage_receipt(outcome, error).is_err(),
                "{reason}"
            );
        }
    }

    #[test]
    fn keyserver_error_classifier_semantically_separates_proof_invalid() {
        let proof_invalid = keyserver_post_error_class(&keystore::Error::PeerBundleProofInvalid);
        let transport =
            keyserver_post_error_class(&keystore::Error::Transport("network down".to_owned()));
        let local_state = keyserver_post_error_class(&keystore::Error::BlobVersionMismatch {
            got: 7,
            expected: 1,
        });

        assert_eq!(proof_invalid, "peer_bundle_proof_invalid");
        assert_eq!(transport, "transport");
        assert_eq!(local_state, "local_state");
        assert_ne!(proof_invalid, transport);
        assert_ne!(proof_invalid, local_state);
    }

    #[test]
    fn control_inbox_post_uses_only_typed_fixed_error_classes() {
        assert_eq!(
            keyserver_post_error_class(&keystore::Error::HttpStatus {
                status: 429,
                body: "raw private quota detail".to_owned(),
            }),
            "http_429"
        );
        assert_eq!(
            keyserver_post_error_class(&keystore::Error::HttpStatus {
                status: 503,
                body: "raw upstream detail".to_owned(),
            }),
            "http_5xx"
        );
        assert_eq!(
            keyserver_post_error_class(&keystore::Error::Transport(
                "raw endpoint detail".to_owned()
            )),
            "transport"
        );
        // Struct literal extended with the errorDetail/carrier-diagnostic
        // fields (schemaVersion bumped 1 -> 2); this is a field addition to an
        // existing literal, not a re-baselined assertion.
        let receipt = DiscordQaSendStageReceipt {
            schema_version: 2,
            observed_at_unix_ms: 1,
            plaintext_sha256: sha256_hex(b"OSL Discord QA probe"),
            entered: true,
            registration_terminal_state: "registered",
            keyserver_available: true,
            identity_unchanged: true,
            overlay_context_verified: true,
            post_succeeded: false,
            phase: "post_control_inbox",
            phase_outcome: "error",
            error_class: Some("http_429"),
            error_detail: None,
            carrier_status: None,
            carrier_placed: None,
            carrier_enter_sent: None,
            overlay_context_unchanged: None,
        };
        let encoded = serde_json::to_string(&receipt).unwrap();
        assert!(encoded.contains("\"errorClass\":\"http_429\""));
        assert!(!encoded.contains("raw private quota detail"));
        assert!(!encoded.contains("recipient"));
    }

    #[test]
    fn overlay_open_receipt_uses_only_fixed_outcomes_and_error_classes() {
        let receipt = DiscordQaOverlayOpenReceipt {
            schema_version: 1,
            observed_at_unix_ms: 1,
            outcome: "error",
            error_class: Some(overlay_open_error_class(
                "raw secret: could not receive trusted input focus",
            )),
        };
        let encoded = serde_json::to_string(&receipt).unwrap();
        assert!(!encoded.contains("raw secret"));
        assert!(encoded.contains("\"outcome\":\"error\""));
        assert!(encoded.contains("\"errorClass\":\"focus_unavailable\""));
        assert_eq!(
            overlay_open_error_class("native protection context changed"),
            "context_changed"
        );
        assert_eq!(
            overlay_open_error_class("OSL friend key state is missing"),
            "friend_unavailable"
        );
        assert_eq!(
            overlay_open_error_class("OSL identity is not loaded"),
            "identity_unavailable"
        );
        assert_eq!(
            overlay_open_error_class("native Discord host is unavailable"),
            "native_host_unavailable"
        );
        assert_eq!(
            overlay_open_error_class("native Discord window validation failed"),
            "native_host_window_invalid"
        );
        assert_eq!(overlay_open_error_class("unknown raw secret"), "rejected");
    }
}
