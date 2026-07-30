//! Native Discord overlay attachment orchestration.
//!
//! The renderer can request selection/open by opaque attachment ID only. File
//! paths, capabilities, keys, ciphertext, and plaintext bytes stay in Rust.

use osl_privacy_hub::attachment_formats;
use osl_privacy_hub::broker::{
    self, HubBrokerState, PendingNativeOverlayAttachment, PreparedNativeOverlayAttachment,
};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::peer_attachment_io;
use osl_privacy_hub::security::HubSecurityState;
use osl_privacy_hub::service_host::ActiveServiceHost;
use std::fs::File;
use std::io::{Seek, SeekFrom};
use std::path::Path;
#[cfg(windows)]
use std::process::Command;
use std::time::Duration;
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use zeroize::Zeroize;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpenedNativeOverlayAttachment {
    attachment_id: String,
    original_filename: String,
    mime_type: String,
    plaintext_size: u64,
    view_once_consumed: bool,
    opened_in_native_viewer: bool,
}

fn require_active_pro(core: &HubCoreState) -> Result<(), String> {
    if ipc::tier_gate::is_paid_equivalent(&core.osl) {
        Ok(())
    } else {
        Err("Encrypted attachments require OSL Pro".to_owned())
    }
}

pub(crate) fn select_encrypt_upload_deliver(
    app: &tauri::AppHandle,
    core: &HubCoreState,
    security: &HubSecurityState,
    broker_state: &HubBrokerState,
    context_epoch: u64,
    expected_host: &ActiveServiceHost,
    view_once: bool,
) -> Result<Option<PreparedNativeOverlayAttachment>, String> {
    select_encrypt_upload_deliver_inner(
        app,
        core,
        security,
        broker_state,
        Some((context_epoch, expected_host)),
        view_once,
    )
}

pub(crate) fn select_osl_chat_attachment(
    app: &tauri::AppHandle,
    core: &HubCoreState,
    security: &HubSecurityState,
    broker_state: &HubBrokerState,
    view_once: bool,
) -> Result<Option<PreparedNativeOverlayAttachment>, String> {
    select_encrypt_upload_deliver_inner(app, core, security, broker_state, None, view_once)
}

fn validate_surface(
    app: &tauri::AppHandle,
    broker: &HubBrokerState,
    overlay: Option<(u64, &ActiveServiceHost)>,
) -> Result<(), String> {
    if let Some((epoch, host)) = overlay {
        super::require_same_overlay_context(app, epoch, host)
    } else {
        broker.active_osl_chat_context_token().map(|_| ())
    }
}

fn select_encrypt_upload_deliver_inner(
    app: &tauri::AppHandle,
    core: &HubCoreState,
    security: &HubSecurityState,
    broker_state: &HubBrokerState,
    overlay_context: Option<(u64, &ActiveServiceHost)>,
    view_once: bool,
) -> Result<Option<PreparedNativeOverlayAttachment>, String> {
    require_active_pro(core)?;
    let parent_label = if overlay_context.is_some() {
        super::native_discord_overlay::OVERLAY_LABEL
    } else {
        "main"
    };
    let parent = app
        .get_webview_window(parent_label)
        .ok_or_else(|| "The trusted attachment picker is unavailable".to_owned())?;
    // Offer exactly what the recipient can open. The filter is derived from
    // `attachment_formats::accepted_attachment_mime`, never restated here: a
    // second hardcoded list is how the picker came to offer GIF and WebP that
    // the protected viewer then refused, so the operator could send an
    // attachment nobody could ever open.
    let offered_extensions = attachment_formats::offered_attachment_extensions();
    let selected = app
        .dialog()
        .file()
        .set_parent(&parent)
        .set_title("Choose a private OSL attachment")
        .add_filter("Supported private files", &offered_extensions)
        .blocking_pick_file();
    let Some(selected) = selected else {
        return Ok(None);
    };
    require_active_pro(core)?;
    validate_surface(app, broker_state, overlay_context)?;
    let selected_path = selected
        .into_path()
        .map_err(|_| "The selected attachment path is unavailable".to_owned())?;
    let filename = selected_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "The selected attachment filename is invalid".to_owned())?
        .to_owned();
    // Refuse before anything is read, encrypted, staged or uploaded. A dialog
    // filter is a hint, not a gate: the operator can type a name, drag a file
    // in, or hit a shell handler that ignores the filter. Refusing later would
    // leave remote ciphertext to roll back, which is exactly the path that
    // needed the deletion outbox in the first place.
    if attachment_formats::accepted_attachment_mime(&filename).is_none() {
        return Err(attachment_formats::unsupported_selection_message());
    }
    let mut source = File::open(&selected_path)
        .map_err(|_| "The selected attachment could not be opened".to_owned())?;
    let metadata = source
        .metadata()
        .map_err(|_| "The selected attachment could not be checked".to_owned())?;
    if !metadata.is_file() {
        return Err("The selected attachment is not a regular file".to_owned());
    }
    let config_root = app
        .path()
        .app_config_dir()
        .map_err(|_| "OSL attachment transport is unavailable".to_owned())?;
    let client = ipc::cipher_store_client::CipherStoreClient::new(
        ipc::cipher_store_client::resolve_cipher_store_base_url(&config_root),
    )
    .map_err(|_| "OSL attachment transport is unavailable".to_owned())?;
    // Finish deletions an earlier rollback could not complete before adding
    // more remote ciphertext. Done before anything is staged locally so a
    // stuck outbox cannot also leave a sealed copy on this device.
    retry_pending_deletions(&client)?;
    let plan = if overlay_context.is_some() {
        broker::begin_native_overlay_attachment(
            core,
            broker_state,
            filename,
            metadata.len(),
            view_once,
        )?
    } else {
        broker::begin_osl_chat_attachment(core, broker_state, filename, metadata.len(), view_once)?
    };
    let local_root = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "OSL attachment storage is unavailable".to_owned())?;
    let staged = peer_attachment_io::encrypt_file(
        &local_root,
        &mut source,
        &plan.original_filename,
        &plan.mime_type,
        crypto::aead::Key::from_bytes(plan.attachment_key),
        plan.content_id.to_vec(),
        0,
    )?;
    let (digest, sealed_size) = match peer_attachment_io::sha256_file(staged.path()) {
        Ok(value) => value,
        Err(error) => {
            return Err(with_staging_cleanup(
                error,
                peer_attachment_io::remove_staged_file(staged),
            ));
        }
    };
    let mut fetch_token = [0u8; ipc::cipher_store_client::FETCH_TOKEN_BYTES];
    let token_len = fetch_token.len();
    fetch_token.copy_from_slice(&crypto::random::random_bytes(token_len));
    let ttl = u32::try_from(plan.expires_at.saturating_sub(plan.created_at))
        .map_err(|_| "OSL attachment expiry is invalid".to_owned())?;
    let sealed_file = File::open(staged.path())
        .map_err(|_| "OSL sealed attachment could not be reopened".to_owned())?;
    let upload = match client.upload_attachment_file(sealed_file, ttl, &fetch_token) {
        Ok(upload) => upload,
        Err(error) => {
            fetch_token.zeroize();
            // A bad TTL, a 413, a 429, a 403 capability rejection and being
            // offline are separate facts and must not read alike.
            let message = peer_attachment_io::describe_cipher_store_error(
                &error,
                peer_attachment_io::TransportPhase::Upload,
            );
            return Err(with_staging_cleanup(
                message,
                peer_attachment_io::remove_staged_file(staged),
            ));
        }
    };
    if let Err(error) = peer_attachment_io::remove_staged_file(staged) {
        let rollback = delete_remote_ciphertext(
            &client,
            &upload.id_hex,
            &fetch_token,
            upload.expires_at,
            view_once,
        );
        fetch_token.zeroize();
        return Err(with_rollback(error, rollback));
    }
    if let Err(error) = validate_surface(app, broker_state, overlay_context) {
        let rollback = delete_remote_ciphertext(
            &client,
            &upload.id_hex,
            &fetch_token,
            upload.expires_at,
            view_once,
        );
        fetch_token.zeroize();
        return Err(with_rollback(error, rollback));
    }
    if let Err(error) = require_active_pro(core) {
        let rollback = delete_remote_ciphertext(
            &client,
            &upload.id_hex,
            &fetch_token,
            upload.expires_at,
            view_once,
        );
        fetch_token.zeroize();
        return Err(with_rollback(error, rollback));
    }
    let burn_scope = plan.burn_scope.clone();
    let mut fetch_token_hex = lower_hex(&fetch_token);
    if osl_privacy_hub::security::record_peer_attachment_burn_capability(
        security,
        burn_scope.clone(),
        upload.id_hex.clone(),
        fetch_token_hex.clone(),
        upload.expires_at,
    )
    .is_err()
    {
        let rollback = delete_remote_ciphertext(
            &client,
            &upload.id_hex,
            &fetch_token,
            upload.expires_at,
            view_once,
        );
        fetch_token_hex.zeroize();
        fetch_token.zeroize();
        return Err(with_rollback(
            "OSL could not retain the attachment burn capability".to_owned(),
            rollback,
        ));
    }
    let digest_hex = lower_hex(&digest);
    let delivered = if overlay_context.is_some() {
        broker::deliver_native_overlay_attachment(
            core,
            broker_state,
            plan,
            sealed_size,
            digest_hex,
            upload.id_hex.clone(),
            fetch_token_hex,
        )
    } else {
        broker::deliver_osl_chat_attachment(
            core,
            broker_state,
            plan,
            sealed_size,
            digest_hex,
            upload.id_hex.clone(),
            fetch_token_hex,
        )
    };
    match delivered {
        Ok(prepared) => {
            fetch_token.zeroize();
            Ok(Some(prepared))
        }
        Err(error) => {
            let rollback = delete_remote_ciphertext(
                &client,
                &upload.id_hex,
                &fetch_token,
                upload.expires_at,
                view_once,
            );
            fetch_token.zeroize();
            // The burn capability may only be forgotten once the object is
            // confirmed gone. While a retry is still queued the capability is
            // what the retry needs, so it stays.
            let capability = if matches!(rollback, Ok(RollbackOutcome::Deleted)) {
                osl_privacy_hub::security::remove_peer_attachment_burn_capability(
                    security,
                    burn_scope,
                    &upload.id_hex,
                )
            } else {
                Ok(())
            };
            // The real delivery failure is reported, not replaced by a generic
            // sentence that hides which stage failed.
            let mut message = with_rollback(error, rollback);
            if capability.is_err() {
                message.push_str(" OSL also could not clear the retained burn capability.");
            }
            Err(message)
        }
    }
}

/// Whether a rollback removed the remote object or only queued the removal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RollbackOutcome {
    Deleted,
    Queued,
}

/// Delete remote ciphertext now, or durably record the deletion so a later pass
/// finishes it. Only failing both is an error: that is the case where OSL has
/// silently left ciphertext in storage for the full TTL, which for a view-once
/// attachment breaks the privacy promise outright.
fn delete_remote_ciphertext(
    client: &ipc::cipher_store_client::CipherStoreClient,
    object_id: &str,
    token: &[u8; ipc::cipher_store_client::FETCH_TOKEN_BYTES],
    expires_at: i64,
    view_once: bool,
) -> Result<RollbackOutcome, String> {
    match client.delete_attachment(object_id, token) {
        Ok(()) => return Ok(RollbackOutcome::Deleted),
        Err(error) => {
            if peer_attachment_io::classify_cipher_store_error(&error)
                == peer_attachment_io::TransportOutcome::Gone
            {
                return Ok(RollbackOutcome::Deleted);
            }
        }
    }
    // Clamp the recorded lifetime: server clock skew must not make an
    // otherwise valid retry record inadmissible, and an object whose TTL has
    // already passed is gone without any retry at all.
    let now = ipc::main_password::now_unix_secs_pub();
    let bounded = expires_at.min(now.saturating_add(MAX_ATTACHMENT_TTL_SECONDS));
    if bounded <= now {
        return Ok(RollbackOutcome::Deleted);
    }
    let mut token_hex = lower_hex(token);
    let queued = peer_attachment_io::enqueue_attachment_deletion(
        object_id,
        &token_hex,
        bounded,
        view_once,
    );
    token_hex.zeroize();
    queued.map(|()| RollbackOutcome::Queued)
}

/// Bound one opportunistic retry pass. Each delete is a network round trip on
/// the calling thread, so a pass is capped and stops at the first hard failure
/// rather than stalling an attachment behind a long-dead store.
const MAX_DELETION_RETRIES_PER_PASS: usize = 8;

pub(crate) fn retry_pending_deletions(
    client: &ipc::cipher_store_client::CipherStoreClient,
) -> Result<(), String> {
    let mut attempted = 0usize;
    let mut transport_down = false;
    peer_attachment_io::drain_attachment_deletions(|pending| {
        if transport_down || attempted >= MAX_DELETION_RETRIES_PER_PASS {
            return peer_attachment_io::DeletionAttempt::Retry;
        }
        attempted += 1;
        match client.delete_attachment(&pending.object_id, &pending.fetch_token) {
            Ok(()) => peer_attachment_io::DeletionAttempt::Deleted,
            Err(error) => {
                if peer_attachment_io::classify_cipher_store_error(&error)
                    == peer_attachment_io::TransportOutcome::Gone
                {
                    peer_attachment_io::DeletionAttempt::AlreadyGone
                } else {
                    transport_down = true;
                    peer_attachment_io::DeletionAttempt::Retry
                }
            }
        }
    })
    .map(|_report| ())
}

/// How often the background drain retries the deletion outbox.
///
/// There is no general scheduler in this app, so this interval is chosen to be
/// defensible on its own:
///
/// * An idle pass costs one read of a file that usually does not exist —
///   `drain_attachment_deletions` returns early on an empty outbox without
///   writing it back — and makes **no network call at all**. So the tick is not
///   a periodic beacon: traffic appears only when OSL genuinely owes a deletion.
/// * A pass that does have work is already capped at
///   [`MAX_DELETION_RETRIES_PER_PASS`] round trips and stops at the first hard
///   transport failure, so a dead cipher store costs one request per tick rather
///   than 512 sequential timeouts. This tick adds no loop around that.
/// * The deadline it races is the object's TTL, which the outbox admits up to
///   seven days. Fifteen minutes is three orders of magnitude tighter than that
///   while still being far too slow to look like polling, and it bounds how long
///   view-once ciphertext can outlive its promise after a transient failure to
///   roughly one tick plus one retry pass.
pub(crate) const DELETION_DRAIN_INTERVAL: Duration = Duration::from_secs(900);

/// Renderer-visible advisory that OSL still owes a remote deletion.
///
/// Carries a fixed non-secret sentence only: never a filename, object id,
/// capability token or path. Targeted at the trusted main webview by label, so
/// it is never delivered into a service child view hosting remote content.
const DELETION_DRAIN_NOTICE_EVENT: &str = "osl://attachment-deletion-drain";
const DELETION_DRAIN_NOTICE_TARGET: &str = "main";

/// Last advisory emitted, so a store that stays down does not emit the same
/// sentence every tick. Cleared by a clean pass, so a later failure is reported.
static LAST_DELETION_DRAIN_NOTICE: std::sync::Mutex<Option<String>> =
    std::sync::Mutex::new(None);

fn deletion_drain_client(
    app: &tauri::AppHandle,
) -> Result<ipc::cipher_store_client::CipherStoreClient, String> {
    let config_root = app
        .path()
        .app_config_dir()
        .map_err(|_| "OSL attachment transport is unavailable".to_owned())?;
    ipc::cipher_store_client::CipherStoreClient::new(
        ipc::cipher_store_client::resolve_cipher_store_base_url(&config_root),
    )
    .map_err(|_| "OSL attachment transport is unavailable".to_owned())
}

/// Surface a drain outcome without letting it reach the caller's result.
fn report_deletion_drain(app: &tauri::AppHandle, outcome: Result<(), String>) {
    let notice = outcome.err();
    let Ok(mut last) = LAST_DELETION_DRAIN_NOTICE.lock() else {
        return;
    };
    if *last == notice {
        return;
    }
    *last = notice.clone();
    if let Some(message) = notice {
        let _ = app.emit_to(
            DELETION_DRAIN_NOTICE_TARGET,
            DELETION_DRAIN_NOTICE_EVENT,
            message,
        );
    }
}

/// Run one drain pass off the caller's thread, and never in the caller's result.
///
/// Used by the password gate. Two reasons it must be detached rather than
/// `retry_pending_deletions(&client)?`: a pass can spend up to
/// [`MAX_DELETION_RETRIES_PER_PASS`] network round trips, so awaiting it would
/// hold the unlock behind a possibly-dead store, and `?` inside
/// `unlock_hub_password_gate` would turn a failed deletion retry into a refusal
/// to let the operator into their own app. A deletion OSL could not finish is
/// still owed either way; the outbox is durable and the periodic tick retries.
pub(crate) fn drain_pending_deletions_detached(app: &tauri::AppHandle) {
    let worker = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let outcome = deletion_drain_client(&worker).and_then(|client| {
            // The outbox is sealed with the file storage key, so this is only
            // ever meaningful after the gate has opened. Before that
            // `drain_attachment_deletions` reports an empty pass by design.
            retry_pending_deletions(&client)
        });
        report_deletion_drain(&worker, outcome);
    });
}

/// Retry the deletion outbox for the life of the process on
/// [`DELETION_DRAIN_INTERVAL`].
///
/// Started from `setup`, but deliberately *not* equivalent to draining at
/// startup: the first pass happens one interval in, by which time the password
/// gate has normally opened and the encrypted outbox is readable. A pass taken
/// while OSL is still locked reads nothing and reports nothing, so this cannot
/// substitute for the post-unlock drain — it is the thing that keeps a long
/// session, and an outbox that only fails intermittently, from waiting for the
/// operator's next attachment action.
pub(crate) fn spawn_deletion_outbox_drain(app: tauri::AppHandle) {
    let _ = std::thread::Builder::new()
        .name("osl-attachment-deletion-drain".to_owned())
        .spawn(move || loop {
            std::thread::sleep(DELETION_DRAIN_INTERVAL);
            let outcome =
                deletion_drain_client(&app).and_then(|client| retry_pending_deletions(&client));
            report_deletion_drain(&app, outcome);
        });
}

/// Report the original failure, and say so when the remote ciphertext could not
/// even be queued for deletion. A silent orphan is the defect being fixed.
fn with_rollback<T>(primary: String, rollback: Result<T, String>) -> String {
    match rollback {
        Ok(_) => primary,
        Err(rollback_error) => format!("{primary} ({rollback_error})"),
    }
}

/// Report the original failure, plus a local sealed copy OSL could not clear.
fn with_staging_cleanup(primary: String, cleanup: Result<(), String>) -> String {
    match cleanup {
        Ok(()) => primary,
        Err(_) => format!("{primary} OSL also could not clear its local sealed copy."),
    }
}

/// Report the original failure, plus a decrypted copy OSL could not remove.
/// Leaving plaintext at rest must never be invisible to the user.
fn with_plaintext_removal(primary: String, removal: Result<(), String>) -> String {
    match removal {
        Ok(()) => primary,
        Err(_) => format!(
            "{primary} OSL could not remove its decrypted copy; it is removed at the next start."
        ),
    }
}

pub(crate) fn list_pending(
    core: &HubCoreState,
    security: &HubSecurityState,
    broker: &HubBrokerState,
) -> Result<Vec<PendingNativeOverlayAttachment>, String> {
    require_active_pro(core)?;
    broker::list_native_overlay_attachments(core, security, broker)
}

pub(crate) fn list_osl_chat_pending(
    core: &HubCoreState,
    security: &HubSecurityState,
    broker: &HubBrokerState,
) -> Result<Vec<PendingNativeOverlayAttachment>, String> {
    require_active_pro(core)?;
    broker::list_osl_chat_attachments(core, security, broker)
}

pub(crate) fn open_pending(
    app: &tauri::AppHandle,
    core: &HubCoreState,
    security: &HubSecurityState,
    broker_state: &HubBrokerState,
    context_epoch: u64,
    expected_host: &ActiveServiceHost,
    attachment_id: &str,
) -> Result<OpenedNativeOverlayAttachment, String> {
    open_pending_inner(
        app,
        core,
        security,
        broker_state,
        Some((context_epoch, expected_host)),
        attachment_id,
    )
}

pub(crate) fn open_osl_chat_pending(
    app: &tauri::AppHandle,
    core: &HubCoreState,
    security: &HubSecurityState,
    broker_state: &HubBrokerState,
    attachment_id: &str,
) -> Result<OpenedNativeOverlayAttachment, String> {
    open_pending_inner(app, core, security, broker_state, None, attachment_id)
}

fn open_pending_inner(
    app: &tauri::AppHandle,
    core: &HubCoreState,
    security: &HubSecurityState,
    broker_state: &HubBrokerState,
    overlay_context: Option<(u64, &ActiveServiceHost)>,
    attachment_id: &str,
) -> Result<OpenedNativeOverlayAttachment, String> {
    require_active_pro(core)?;
    let local_root = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "OSL attachment storage is unavailable".to_owned())?;
    let config_root = app
        .path()
        .app_config_dir()
        .map_err(|_| "OSL attachment transport is unavailable".to_owned())?;
    let client = ipc::cipher_store_client::CipherStoreClient::new(
        ipc::cipher_store_client::resolve_cipher_store_base_url(&config_root),
    )
    .map_err(|_| "OSL attachment transport is unavailable".to_owned())?;
    // Retry outstanding deletions before consuming the pending record, so a
    // stuck outbox can never burn a replay slot for an attachment that then
    // fails to open.
    retry_pending_deletions(&client)?;
    let plan = if overlay_context.is_some() {
        broker::take_native_overlay_attachment(core, security, broker_state, attachment_id)?
    } else {
        broker::take_osl_chat_attachment(core, security, broker_state, attachment_id)?
    };
    // Keep every network, durable-I/O, decrypt, replay, reveal, and burn step
    // behind the production viewer-policy seam. A non-image refusal therefore
    // occurs before the fetch token is parsed or a staging file is created.
    peer_attachment_io::require_protected_attachment_viewer(&plan.mime_type)?;
    let is_image = plan.mime_type.starts_with("image/");
    // Inbound filenames come from the sender's wire payload, not from this
    // device's picker, so a peer running an older build can still deliver a
    // format the viewer cannot decode. Keep failing closed, but say which
    // formats do work, from the same derivation the picker uses.
    if is_image && !peer_attachment_io::supported_protected_image_mime(&plan.mime_type) {
        return Err(attachment_formats::unsupported_protected_image_message());
    }
    let token = parse_token(&plan.fetch_token)?;
    let (download_path, mut download) = peer_attachment_io::create_download_file(&local_root)?;
    let cleanup_download = |path: &Path| {
        let _ = peer_attachment_io::remove_staging_path_in_root(&local_root, path);
    };
    let fetched = match client.fetch_attachment_to_writer(&plan.object_id, &token, &mut download) {
        Ok(size) => size,
        Err(error) => {
            drop(download);
            cleanup_download(&download_path);
            // A rejected capability is not an expiry, and being offline is
            // neither. Report which one actually happened.
            return Err(peer_attachment_io::describe_cipher_store_error(
                &error,
                peer_attachment_io::TransportPhase::Fetch,
            ));
        }
    };
    if fetched != plan.sealed_size || download.sync_all().is_err() {
        drop(download);
        cleanup_download(&download_path);
        return Err("This private attachment has an invalid size".to_owned());
    }
    drop(download);
    let (digest, size) = peer_attachment_io::sha256_file(&download_path)?;
    if size != plan.sealed_size || lower_hex(&digest) != plan.ciphertext_sha256 {
        cleanup_download(&download_path);
        return Err("This private attachment failed authentication".to_owned());
    }
    let mut sealed = File::open(&download_path)
        .map_err(|_| "This private attachment could not be opened".to_owned())?;
    sealed
        .seek(SeekFrom::Start(0))
        .map_err(|_| "This private attachment could not be opened".to_owned())?;
    if is_image {
        let opened = match peer_attachment_io::decrypt_file_to_memory(
            &mut sealed,
            &plan.original_filename,
            &plan.mime_type,
            crypto::aead::Key::from_bytes(plan.attachment_key),
        ) {
            Ok(opened) => opened,
            Err(error) => {
                cleanup_download(&download_path);
                return Err(error);
            }
        };
        cleanup_download(&download_path);
        if opened.len() as u64 != plan.plaintext_size {
            return Err("This private attachment has an invalid plaintext size".to_owned());
        }
        if let Err(error) = validate_surface(app, broker_state, overlay_context) {
            return Err(error);
        }
        require_active_pro(core)?;
        // Decode and create the window while it is hidden. `prepare` applies
        // and reads back capture exclusion before returning; Drop closes the
        // hidden window and zeroizes its pixels on every later failure.
        let viewer = super::native_image_viewer::prepare(app, opened)?;
        validate_surface(app, broker_state, overlay_context)?;
        require_active_pro(core)?;
        if overlay_context.is_some() {
            broker::commit_native_overlay_attachment_open(core, security, broker_state, &plan)?;
        } else {
            broker::commit_osl_chat_attachment_open(core, security, broker_state, &plan)?;
        }
        // Reveal before the burn. The replay slot is already spent, so a burn
        // that needs a retry must not also cost the user the content.
        viewer.show()?;
        if plan.view_once {
            burn_view_once(&client, &plan.object_id, &token)?;
        }
        return Ok(OpenedNativeOverlayAttachment {
            attachment_id: plan.attachment_id.clone(),
            original_filename: plan.original_filename.clone(),
            mime_type: plan.mime_type.clone(),
            plaintext_size: plan.plaintext_size,
            view_once_consumed: plan.view_once,
            opened_in_native_viewer: true,
        });
    }
    let opened = match peer_attachment_io::decrypt_file(
        &local_root,
        &mut sealed,
        &plan.original_filename,
        &plan.mime_type,
        crypto::aead::Key::from_bytes(plan.attachment_key),
    ) {
        Ok(opened) => opened,
        Err(error) => {
            cleanup_download(&download_path);
            return Err(error);
        }
    };
    cleanup_download(&download_path);
    // `opened` is an RAII guard: every path below removes the decrypted file,
    // and the explicit `remove_now` calls also surface a removal that failed.
    if opened.plaintext_len() != plan.plaintext_size {
        return Err(with_plaintext_removal(
            "This private attachment has an invalid plaintext size".to_owned(),
            opened.remove_now(),
        ));
    }
    if let Err(error) = validate_surface(app, broker_state, overlay_context) {
        return Err(with_plaintext_removal(error, opened.remove_now()));
    }
    if let Err(error) = require_active_pro(core) {
        return Err(with_plaintext_removal(error, opened.remove_now()));
    }
    let committed = if overlay_context.is_some() {
        broker::commit_native_overlay_attachment_open(core, security, broker_state, &plan)
    } else {
        broker::commit_osl_chat_attachment_open(core, security, broker_state, &plan)
    };
    if let Err(error) = committed {
        return Err(with_plaintext_removal(error, opened.remove_now()));
    }
    let response = OpenedNativeOverlayAttachment {
        attachment_id: plan.attachment_id.clone(),
        original_filename: plan.original_filename.clone(),
        mime_type: plan.mime_type.clone(),
        plaintext_size: plan.plaintext_size,
        view_once_consumed: plan.view_once,
        opened_in_native_viewer: true,
    };
    // Replay is committed and the encrypted inbox capability is already gone, so
    // the remote burn cannot reopen this attachment. Hand the decrypted copy to
    // the external reader first, then burn: the replay slot is spent either way.
    launch_and_scavenge(opened)?;
    if plan.view_once {
        burn_view_once(&client, &plan.object_id, &token)?;
    }
    Ok(response)
}

/// Longest lifetime the cipher store honours. The taken plan does not expose its
/// absolute expiry, so a queued burn is admitted with this bound and therefore
/// outlives any object that could still exist.
const MAX_ATTACHMENT_TTL_SECONDS: i64 = 604_800;

/// Burn one view-once object. A remote copy OSL can neither delete nor even
/// queue for deletion is a broken privacy promise, so it is reported rather than
/// discarded.
fn burn_view_once(
    client: &ipc::cipher_store_client::CipherStoreClient,
    object_id: &str,
    token: &[u8; ipc::cipher_store_client::FETCH_TOKEN_BYTES],
) -> Result<(), String> {
    let expires_at =
        ipc::main_password::now_unix_secs_pub().saturating_add(MAX_ATTACHMENT_TTL_SECONDS);
    delete_remote_ciphertext(client, object_id, token, expires_at, true)
        .map(|_outcome| ())
        .map_err(|error| {
            format!(
                "This view-once attachment opened, but OSL could not delete or queue deletion of its stored copy ({error})."
            )
        })
}

fn lower_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn parse_token(value: &str) -> Result<[u8; ipc::cipher_store_client::FETCH_TOKEN_BYTES], String> {
    if value.len() != ipc::cipher_store_client::FETCH_TOKEN_BYTES * 2 {
        return Err("This private attachment capability is invalid".to_owned());
    }
    let mut output = [0u8; ipc::cipher_store_client::FETCH_TOKEN_BYTES];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk)
            .map_err(|_| "This private attachment capability is invalid".to_owned())?;
        output[index] = u8::from_str_radix(text, 16)
            .map_err(|_| "This private attachment capability is invalid".to_owned())?;
    }
    Ok(output)
}

/// Bounded read window granted to a Windows shell handler that exits after
/// handing off to an already running viewer.
#[cfg(windows)]
const PLAINTEXT_READ_WINDOW: Duration = Duration::from_secs(10);

/// Escalating waits, in seconds, for the detached reaper's three delete
/// attempts. The first is longer than [`PLAINTEXT_READ_WINDOW`] so OSL's own
/// cleanup normally wins; the later two cover a Windows viewer that still holds
/// the file open, which makes a delete fail rather than succeed.
#[cfg(windows)]
const DETACHED_REAPER_DELAYS_SECONDS: [u32; 3] = [21, 61, 301];

/// How long OSL's own thread keeps retrying removal once the read window has
/// passed, and how often. A viewer that holds the decrypted file open blocks
/// deletion on Windows, so a single attempt is not enough.
#[cfg(windows)]
const PLAINTEXT_REMOVAL_WINDOW: Duration = Duration::from_secs(600);
#[cfg(windows)]
const PLAINTEXT_REMOVAL_INTERVAL: Duration = Duration::from_secs(5);

fn launch_and_scavenge(staged: peer_attachment_io::StagedPlaintext) -> Result<(), String> {
    if ipc::attachment_wire::is_blocked_automatic_open_filename(staged.original_filename()) {
        return Err(with_plaintext_removal(
            "This attachment type cannot be opened automatically".to_owned(),
            staged.remove_now(),
        ));
    }
    #[cfg(windows)]
    {
        let path = match staged.path() {
            Some(path) => path.to_owned(),
            None => return Err("The decrypted attachment is unavailable".to_owned()),
        };
        let root = match staged.root() {
            Some(root) => root.to_owned(),
            None => return Err("The decrypted attachment is unavailable".to_owned()),
        };
        // Install the OS-owned deletion before anything can read the plaintext.
        // The reaper is a separate process, so the decrypted copy's lifetime no
        // longer depends on an in-process best-effort call winning a race with
        // OSL exiting, crashing, or being killed. Failing to install it fails
        // closed: the plaintext is removed and the attachment is not opened.
        if spawn_detached_plaintext_reaper(&path).is_err() {
            return Err(with_plaintext_removal(
                "OSL could not guarantee removal of the decrypted copy, so it did not open this attachment".to_owned(),
                staged.remove_now(),
            ));
        }
        let child = Command::new(system32_binary("rundll32.exe"))
            .arg("url.dll,FileProtocolHandler")
            .arg(&path)
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(_) => {
                return Err(with_plaintext_removal(
                    "The native attachment viewer could not be opened".to_owned(),
                    staged.remove_now(),
                ));
            }
        };
        // Removal now belongs to the reaper thread and to the detached process,
        // so the guard must not delete the file the viewer is about to read.
        if staged.release_to_external_reader().is_none() {
            return Err("The decrypted attachment is unavailable".to_owned());
        }
        std::thread::spawn(move || {
            let _ = child.wait();
            // Some Windows shell handlers hand off to an existing process and
            // exit immediately. Give that viewer a bounded read window, then
            // remove OSL's only plaintext staging file. A viewer that still
            // holds the file makes the delete fail on Windows, so retry across a
            // bounded window; only after that is the failure counted, and the
            // detached reaper plus the startup sweep remain independent
            // backstops.
            std::thread::sleep(PLAINTEXT_READ_WINDOW);
            let deadline = std::time::Instant::now() + PLAINTEXT_REMOVAL_WINDOW;
            loop {
                if peer_attachment_io::remove_staging_path_in_root(&root, &path).is_ok() {
                    return;
                }
                if std::time::Instant::now() >= deadline {
                    peer_attachment_io::note_unremoved_plaintext_file();
                    return;
                }
                std::thread::sleep(PLAINTEXT_REMOVAL_INTERVAL);
            }
        });
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err(with_plaintext_removal(
            "Native attachment viewing is available only on Windows".to_owned(),
            staged.remove_now(),
        ))
    }
}

/// Absolute System32 path for a Windows helper binary, so neither spawn can be
/// redirected by a hijacked `PATH`.
#[cfg(windows)]
fn system32_binary(name: &str) -> std::path::PathBuf {
    std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("C:\\Windows"))
        .join("System32")
        .join(name)
}

/// Start a short-lived process, independent of OSL, that deletes the decrypted
/// staging file after a bounded delay. It survives OSL exiting or being killed,
/// and it fails harmlessly when OSL's own cleanup already removed the file or a
/// viewer still holds it open.
#[cfg(windows)]
fn spawn_detached_plaintext_reaper(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    let text = path
        .to_str()
        .ok_or_else(|| "The decrypted attachment path is unsupported".to_owned())?;
    // The command line is assembled by hand. Inside double quotes the command
    // interpreter treats `&`, `|`, `<` and `>` literally, so only a quote, a
    // percent expansion, or a control character could escape the argument. OSL
    // generates this filename itself, so a rejection means a real defect rather
    // than hostile input. `/d` skips AutoRun and `/v:off` forces delayed
    // expansion off, so `!` cannot expand either.
    if text
        .chars()
        .any(|value| matches!(value, '"' | '%') || value.is_control())
    {
        return Err("The decrypted attachment path is unsupported".to_owned());
    }
    let [first, second, third] = DETACHED_REAPER_DELAYS_SECONDS;
    Command::new(system32_binary("cmd.exe"))
        .raw_arg(format!(
            "/d /v:off /c ping -n {first} 127.0.0.1 >nul & del /f /q \"{text}\" \
             & if exist \"{text}\" (ping -n {second} 127.0.0.1 >nul & del /f /q \"{text}\") \
             & if exist \"{text}\" (ping -n {third} 127.0.0.1 >nul & del /f /q \"{text}\")"
        ))
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .map(|_child| ())
        .map_err(|_| "OSL could not install its decrypted-copy cleanup".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_parser_is_exact_and_lowercase_agnostic_only_at_decode() {
        assert_eq!(
            parse_token("00112233445566778899aabbccddeeff").unwrap()[0],
            0
        );
        assert!(parse_token("0011").is_err());
        assert!(parse_token("00112233445566778899aabbccddeefg").is_err());
    }
}
